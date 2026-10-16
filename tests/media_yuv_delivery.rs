// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pictures in planes, with the real ffmpeg (phase 1.3d): what ffprobe says of how a video is stored and what colours it says it has; whether the planes
//! arrive as they were stored, to the byte, at odd sizes too; and whether the colours that the engine's arithmetic makes of the planes are the colours
//! that ffmpeg makes of them when it is asked for RGBA, which is what the preview always showed.
//!
//! The tolerances of the comparisons were measured (against ffmpeg 9 on Windows) and are stated where they are used. They are far smaller than what the
//! mistakes the tests are for make: the matrix of standard-definition video for high-definition, the wrong range, two planes the wrong way round, or a
//! colour put one pixel off its place each differ by a dozen levels or much more. (BT.709 and BT.2020 are so near each other that no comparison with
//! ffmpeg can tell them apart; the numbers of each are held to the standards' by the tests of the engine, and which one a video gets by its tags by
//! the tests of the probe.)
//!
//! The videos are tagged by saying what they are (`setparams`) and not by asking ffmpeg to convert them to what the tags say (which is what its `-colorspace`
//! does in the newest versions, and not in older ones): the planes that are stored are the ones that the picture's expressions make, whatever the tags.

mod support;

use std::path::Path;

use fastforwardrs::engine::{YuvColour, YuvMatrix, YuvRange, chroma_size, rgba_of_yuv420p_planes};
use fastforwardrs::media::{DecodedFrame, FfmpegToolchain, NativeMatrix, NativeYuv420p, probe_video_stream_format};
use fastforwardrs::playback::colour_for_the_engine;
use support::scratch_directory;
use support::yuv_videos::{
    BT470BG_FULL, BT709_FULL, BT709_LIMITED, BT2020_LIMITED, FOUR_COLOURS, HARD_CHROMA_STRIPES, SMPTE170M_LIMITED,
    Tags, UNTAGGED, ffmpeg, picture_as_rgba, picture_in_planes, planes_of_noise, run, tagging_filter,
    video_made_by_geq, video_of_planes,
};

/// Each way of tagging a video, and the colour that the engine must be given for it.
const TAGGINGS: [(&str, Tags, YuvColour); 6] = [
    ("untagged", UNTAGGED, YuvColour::UNSPECIFIED),
    (
        "bt709 limited",
        BT709_LIMITED,
        YuvColour {
            matrix: YuvMatrix::Bt709,
            range: YuvRange::Limited,
        },
    ),
    (
        "bt709 full",
        BT709_FULL,
        YuvColour {
            matrix: YuvMatrix::Bt709,
            range: YuvRange::Full,
        },
    ),
    ("smpte170m limited", SMPTE170M_LIMITED, YuvColour::UNSPECIFIED),
    (
        "bt470bg full",
        BT470BG_FULL,
        YuvColour {
            matrix: YuvMatrix::Bt601,
            range: YuvRange::Full,
        },
    ),
    (
        "bt2020nc limited",
        BT2020_LIMITED,
        YuvColour {
            matrix: YuvMatrix::Bt2020,
            range: YuvRange::Limited,
        },
    ),
];

/// The largest and the mean difference between two pictures of RGBA bytes, over the red, green and blue of every pixel. (Not over how opaque they are:
/// that is not what these tests are about, and what ffmpeg makes of it is looked at by a test of its own, in `media_frame_stream`.)
fn differences(ours: &[u8], theirs: &[u8]) -> (u8, f64) {
    assert_eq!(ours.len(), theirs.len());
    let (mut largest, mut sum, mut count) = (0u8, 0u64, 0u64);
    for (ours, theirs) in ours.as_chunks::<4>().0.iter().zip(theirs.as_chunks::<4>().0) {
        for channel in 0..3 {
            let difference = ours[channel].abs_diff(theirs[channel]);
            largest = largest.max(difference);
            sum += u64::from(difference);
            count += 1;
        }
    }
    (largest, sum as f64 / count as f64)
}

/// Whether every pixel of ffmpeg's picture is opaque, as the picture of a video must be. (On ffmpeg for macOS on ARM it is not, for a picture of an odd width:
/// ffmpeg's own conversion leaves clear pixels there, and `media_ffmpeg_quirks` says so. The comparisons below are of the engine with what ffmpeg makes right,
/// so they leave a case alone where ffmpeg's own picture is broken, and only where the picture is of an odd size: everywhere else it must be right.)
fn ffmpeg_made_it_opaque(frame: &DecodedFrame) -> bool {
    frame.rgba_pixels.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255)
}

/// What the preview makes of a video it decodes in planes: the colour comes from what ffprobe says of the video, as in the program.
fn colour_the_preview_gives(toolchain: &FfmpegToolchain, video: &Path) -> YuvColour {
    let format = probe_video_stream_format(toolchain, video.to_path_buf()).expect("the video can be probed");
    colour_for_the_engine(
        format
            .as_native_yuv420p(64, 48)
            .expect("the preview can use the planes of this video"),
    )
}

#[test]
fn what_ffprobe_says_of_a_video_is_read_and_decides_whether_the_preview_can_use_its_planes_and_with_which_colours() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let size = (64, 48);

    let native = |matrix, full_range| Some(NativeYuv420p { matrix, full_range });
    for (name, tags, expected) in [
        ("untagged", UNTAGGED, native(NativeMatrix::Bt601, false)),
        ("bt709 limited", BT709_LIMITED, native(NativeMatrix::Bt709, false)),
        ("bt709 full", BT709_FULL, native(NativeMatrix::Bt709, true)),
        (
            "smpte170m limited",
            SMPTE170M_LIMITED,
            native(NativeMatrix::Bt601, false),
        ),
        ("bt470bg full", BT470BG_FULL, native(NativeMatrix::Bt601, true)),
        ("bt2020nc limited", BT2020_LIMITED, native(NativeMatrix::Bt2020, false)),
    ] {
        let video = video_made_by_geq(
            &toolchain,
            folder.path(),
            &format!("probe_{}", name.replace(' ', "_")),
            size,
            FOUR_COLOURS,
            "yuv420p",
            tags,
        );
        let format = probe_video_stream_format(&toolchain, video).expect("the video can be probed");
        assert_eq!(format.pixel_format.as_deref(), Some("yuv420p"), "{name}");
        assert_eq!(format.as_native_yuv420p(64, 48), expected, "{name}: {format:?}");
    }

    // What the words of a tagged video are, as ffprobe prints them.
    let video = video_made_by_geq(
        &toolchain,
        folder.path(),
        "probe_words",
        size,
        FOUR_COLOURS,
        "yuv420p",
        BT709_FULL,
    );
    let format = probe_video_stream_format(&toolchain, video).unwrap();
    assert_eq!(format.colour_space.as_deref(), Some("bt709"));
    assert_eq!(format.colour_range.as_deref(), Some("pc"));

    // A video that is stored another way is not one the preview takes in planes.
    for pixel_format in ["yuv422p", "yuv444p", "yuv420p10le", "gray"] {
        let video = video_made_by_geq(
            &toolchain,
            folder.path(),
            &format!("probe_{pixel_format}"),
            size,
            FOUR_COLOURS,
            pixel_format,
            BT709_LIMITED,
        );
        let format = probe_video_stream_format(&toolchain, video).expect("the video can be probed");
        assert_eq!(format.pixel_format.as_deref(), Some(pixel_format));
        assert_eq!(format.as_native_yuv420p(64, 48), None, "{pixel_format}");
    }
}

#[test]
fn an_ordinary_h264_file_is_probed_too_tagged_or_not() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    for (name, tags, expected) in [
        (
            "h264_untagged",
            UNTAGGED,
            NativeYuv420p {
                matrix: NativeMatrix::Bt601,
                full_range: false,
            },
        ),
        (
            "h264_bt709",
            &[
                "colorspace=bt709",
                "color_primaries=bt709",
                "color_trc=bt709",
                "range=tv",
            ][..],
            NativeYuv420p {
                matrix: NativeMatrix::Bt709,
                full_range: false,
            },
        ),
    ] {
        let path = folder.path().join(format!("{name}.mp4"));
        let mut command = ffmpeg(&toolchain);
        command
            .args(["-f", "lavfi", "-i", "color=c=0x406080:s=64x48:r=10:d=1"])
            .args(["-vf", &format!("format=yuv420p,{}", tagging_filter(tags))])
            .args(["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p"])
            .arg(&path);
        run(command);
        let format = probe_video_stream_format(&toolchain, path).expect("the video can be probed");
        assert_eq!(format.as_native_yuv420p(64, 48), Some(expected), "{name}: {format:?}");
    }
}

#[test]
fn the_planes_of_a_picture_arrive_as_they_were_stored_to_the_byte_at_every_size_even_and_odd_and_whatever_the_tags() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    for (width, height) in [
        (2, 2),
        (64, 48),
        (65, 48),
        (64, 49),
        (65, 49),
        (33, 17),
        (3, 3),
        (1, 1),
        (7, 2),
    ] {
        for (name, tags, _) in TAGGINGS {
            let planes = planes_of_noise(width, height, u64::from(width * 1000 + height));
            let video = video_of_planes(
                &toolchain,
                folder.path(),
                &format!("planes_{width}x{height}_{}", name.replace(' ', "_")),
                (width, height),
                &planes,
                tags,
            );
            let delivered = picture_in_planes(&toolchain, &video, (width, height), (width, height));
            assert_eq!((delivered.width_in_pixels, delivered.height_in_pixels), (width, height));
            assert_eq!(delivered.y, planes[0], "{width}x{height} [{name}]: brightness");
            assert_eq!(
                delivered.u, planes[1],
                "{width}x{height} [{name}]: blue colour difference"
            );
            assert_eq!(
                delivered.v, planes[2],
                "{width}x{height} [{name}]: red colour difference"
            );
        }
    }
}

#[test]
fn a_picture_that_is_made_another_size_by_ffmpeg_comes_in_planes_of_the_size_that_fits_the_picture() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let video = video_made_by_geq(
        &toolchain,
        folder.path(),
        "resized",
        (72, 128),
        FOUR_COLOURS,
        "yuv420p",
        UNTAGGED,
    );
    for picture_size in [(40, 72), (36, 64), (101, 101), (144, 256)] {
        let planes = picture_in_planes(&toolchain, &video, (72, 128), picture_size);
        let rgba = picture_as_rgba(&toolchain, &video, (72, 128), picture_size);
        let (width, height) = (planes.width_in_pixels, planes.height_in_pixels);
        assert_eq!(
            (rgba.width_in_pixels, rgba.height_in_pixels),
            (width, height),
            "{picture_size:?}"
        );
        let (chroma_width, chroma_height) = chroma_size(width, height);
        assert_eq!(planes.y.len(), (width * height) as usize, "{picture_size:?}");
        assert_eq!(
            planes.u.len(),
            (chroma_width * chroma_height) as usize,
            "{picture_size:?}"
        );
        assert_eq!(planes.v.len(), planes.u.len(), "{picture_size:?}");
    }
}

/// Where the picture the engine makes of the planes may be off from the one ffmpeg makes of them, in levels of red, green and blue. Measured (ffmpeg 9.0,
/// Windows) in all of the cases below, the largest difference was 4 and the mean never over 1.9: what is left is the whole numbers of ffmpeg's arithmetic,
/// which cuts off where the engine rounds (about a level, more for green, which has three terms), and the colour planes and brightness that it scales
/// in more bits than the 8 the planes have. The limits have two levels over what was measured, for other versions of ffmpeg.
const LARGEST_DIFFERENCE_FROM_FFMPEG_IN_FLAT_COLOURS: u8 = 6;
const LARGEST_MEAN_DIFFERENCE_FROM_FFMPEG_IN_FLAT_COLOURS: f64 = 3.0;

/// The same, for hard edges of colour where ffmpeg works at the size of the planes or at twice it: measured, the largest difference was 3 and the mean
/// never over 0.82 (where a smooth blend between colour pixels, which was the engine's way before this was measured, is off by 82 and 10).
const LARGEST_DIFFERENCE_FROM_FFMPEG_AT_HARD_EDGES: u8 = 6;
const LARGEST_MEAN_DIFFERENCE_FROM_FFMPEG_AT_HARD_EDGES: f64 = 1.5;

#[test]
fn the_colours_the_engine_makes_of_the_planes_are_those_ffmpeg_makes_for_every_tag_at_every_size_and_scale() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let (mut compared, mut skipped) = (0, 0);
    // (the size of the video, the size of the picture it is delivered for), as the preview asks for them: unscaled and scaled, halved, doubled, and at
    // odd sizes across, down, and both.
    for (video_size, picture_size) in [
        ((64, 48), (64, 48)),
        ((65, 48), (65, 48)),
        ((64, 49), (64, 49)),
        ((65, 49), (65, 49)),
        ((320, 180), (320, 180)),
        ((128, 96), (64, 48)),
        ((128, 96), (96, 72)),
        ((64, 48), (128, 96)),
        ((72, 128), (40, 72)),
    ] {
        for (name, tags, expected_colour) in TAGGINGS {
            let video = video_made_by_geq(
                &toolchain,
                folder.path(),
                &format!("flat_{}x{}_{}", video_size.0, video_size.1, name.replace(' ', "_")),
                video_size,
                FOUR_COLOURS,
                "yuv420p",
                tags,
            );
            let colour = colour_the_preview_gives(&toolchain, &video);
            assert_eq!(colour, expected_colour, "{name}: the colour that the video's tags give");

            let theirs = picture_as_rgba(&toolchain, &video, video_size, picture_size);
            let planes = picture_in_planes(&toolchain, &video, video_size, picture_size);
            let (width, height) = (planes.width_in_pixels, planes.height_in_pixels);
            assert_eq!((theirs.width_in_pixels, theirs.height_in_pixels), (width, height));
            let ours = rgba_of_yuv420p_planes(width, height, &planes.y, &planes.u, &planes.v, colour);

            let what = format!("{video_size:?} made {width}x{height} [{name}]");
            if !ffmpeg_made_it_opaque(&theirs) {
                assert!(
                    width % 2 == 1 || height % 2 == 1,
                    "{what}: ffmpeg's own picture has clear pixels"
                );
                eprintln!("SKIPPED {what}: ffmpeg's own picture of this odd size has clear pixels");
                skipped += 1;
                continue;
            }
            compared += 1;
            let (largest, mean) = differences(&ours, &theirs.rgba_pixels);
            assert!(
                largest <= LARGEST_DIFFERENCE_FROM_FFMPEG_IN_FLAT_COLOURS,
                "{what}: the engine's picture is off from ffmpeg's by {largest} levels"
            );
            assert!(
                mean <= LARGEST_MEAN_DIFFERENCE_FROM_FFMPEG_IN_FLAT_COLOURS,
                "{what}: the engine's picture is off from ffmpeg's by {mean} levels on average"
            );
        }
    }
    assert!(
        compared >= 2 * skipped,
        "{compared} cases were compared and {skipped} left alone: that is too few that were compared"
    );
}

#[test]
fn the_wrong_matrix_or_range_or_planes_the_wrong_way_round_would_be_far_off_so_the_comparison_above_can_tell() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let video = video_made_by_geq(
        &toolchain,
        folder.path(),
        "sensitivity",
        (64, 48),
        FOUR_COLOURS,
        "yuv420p",
        BT709_LIMITED,
    );
    let theirs = picture_as_rgba(&toolchain, &video, (64, 48), (64, 48));
    let planes = picture_in_planes(&toolchain, &video, (64, 48), (64, 48));
    let engine =
        |colour: YuvColour, blue: &[u8], red: &[u8]| rgba_of_yuv420p_planes(64, 48, &planes.y, blue, red, colour);

    let right = YuvColour {
        matrix: YuvMatrix::Bt709,
        range: YuvRange::Limited,
    };
    let (largest, _) = differences(&engine(right, &planes.u, &planes.v), &theirs.rgba_pixels);
    assert!(
        largest <= LARGEST_DIFFERENCE_FROM_FFMPEG_IN_FLAT_COLOURS,
        "the right colour: {largest}"
    );

    for (what, colour, blue, red) in [
        ("BT.601 for BT.709", YuvColour::UNSPECIFIED, &planes.u, &planes.v),
        (
            "full range for limited",
            YuvColour {
                matrix: YuvMatrix::Bt709,
                range: YuvRange::Full,
            },
            &planes.u,
            &planes.v,
        ),
        ("the colour planes the wrong way round", right, &planes.v, &planes.u),
    ] {
        let (largest, mean) = differences(&engine(colour, blue, red), &theirs.rgba_pixels);
        assert!(
            largest >= 12 && mean >= 3.5,
            "{what}: only {largest} levels off at most and {mean} on average, which is too little to be told from the right colour"
        );
    }
}

#[test]
fn hard_edges_of_colour_come_out_as_ffmpeg_makes_them_which_is_one_colour_difference_for_each_square_of_four_pixels() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let (mut compared, mut skipped) = (0, 0);
    // Where ffmpeg turns the planes into colours at the size they have, or at half of the size of the video (and the picture has an even number of
    // rows), it gives every pixel the colour difference of the square of four it is in. (At the other scales it scales the colour planes first by its
    // own way, which the planes that arrive here are not; see `docs/engine-phase-1-design.md`.)
    for (video_size, picture_size) in [((64, 48), (64, 48)), ((65, 48), (65, 48)), ((128, 96), (64, 48))] {
        for (name, tags, _) in TAGGINGS {
            let video = video_made_by_geq(
                &toolchain,
                folder.path(),
                &format!("stripes_{}x{}_{}", video_size.0, video_size.1, name.replace(' ', "_")),
                video_size,
                HARD_CHROMA_STRIPES,
                "yuv420p",
                tags,
            );
            let colour = colour_the_preview_gives(&toolchain, &video);
            let theirs = picture_as_rgba(&toolchain, &video, video_size, picture_size);
            let planes = picture_in_planes(&toolchain, &video, video_size, picture_size);
            let (width, height) = (planes.width_in_pixels, planes.height_in_pixels);
            // (The colours really do change by 160 between neighbours; this test shows nothing if they do not.)
            let (lowest, highest) = (planes.u.iter().min().unwrap(), planes.u.iter().max().unwrap());
            assert!(highest - lowest >= 100, "{video_size:?}: the stripes are not there");

            let ours = rgba_of_yuv420p_planes(width, height, &planes.y, &planes.u, &planes.v, colour);
            let what = format!("{video_size:?} made {width}x{height} [{name}]");
            if !ffmpeg_made_it_opaque(&theirs) {
                assert!(
                    width % 2 == 1 || height % 2 == 1,
                    "{what}: ffmpeg's own picture has clear pixels"
                );
                eprintln!("SKIPPED {what}: ffmpeg's own picture of this odd size has clear pixels");
                skipped += 1;
                continue;
            }
            compared += 1;
            let (largest, mean) = differences(&ours, &theirs.rgba_pixels);
            assert!(
                largest <= LARGEST_DIFFERENCE_FROM_FFMPEG_AT_HARD_EDGES,
                "{what}: off from ffmpeg's by {largest} levels at most"
            );
            assert!(
                mean <= LARGEST_MEAN_DIFFERENCE_FROM_FFMPEG_AT_HARD_EDGES,
                "{what}: off from ffmpeg's by {mean} levels on average"
            );
        }
    }
    assert!(
        compared >= 2 * skipped,
        "{compared} cases were compared and {skipped} left alone: that is too few that were compared"
    );
}
