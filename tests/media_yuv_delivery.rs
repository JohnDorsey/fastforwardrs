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

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use fastforwardrs::engine::{YuvColour, YuvMatrix, YuvRange, chroma_size, rgba_of_yuv420p_planes};
use fastforwardrs::media::{
    DecodedFrame, DecodedYuvFrame, FfmpegToolchain, FrameDelivery, FrameStream, FrameStreamEvent, FrameStreamRequest,
    NativeMatrix, NativeYuv420p, probe_video_stream_format,
};
use fastforwardrs::playback::colour_for_the_engine;
use fastforwardrs::project::{FrameRate, SourceFrameIndex, SpeedFactor};
use support::scratch_directory;

const TIME_ALLOWED_FOR_A_STREAM: Duration = Duration::from_secs(30);

/// What a video is made to say about its colours, as the options of ffmpeg's `setparams` filter: nothing, or a matrix and a range.
type Tags = &'static [&'static str];

const UNTAGGED: Tags = &[];
const BT709_LIMITED: Tags = &["colorspace=bt709", "range=tv"];
const BT709_FULL: Tags = &["colorspace=bt709", "range=pc"];
const SMPTE170M_LIMITED: Tags = &["colorspace=smpte170m", "range=tv"];
const BT470BG_FULL: Tags = &["colorspace=bt470bg", "range=pc"];
const BT2020_LIMITED: Tags = &["colorspace=bt2020nc", "range=tv"];

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

/// A picture of four flat colours, one in each quarter, with a ramp of brightness across: strong colour differences (more than 60 either side of grey),
/// and borders between the colours that are as hard as can be.
const FOUR_COLOURS: &str = "lum='60+120*X*SW/W':cb='if(lt(X*SW,W/2),if(lt(Y*SH,H/2),64,192),if(lt(Y*SH,H/2),128,224))':\
cr='if(lt(X*SW,W/2),if(lt(Y*SH,H/2),192,64),if(lt(Y*SH,H/2),224,96))'";

/// A picture of stripes of colour four pixels of the colour planes wide, which are 48 on one side of their edge and 208 on the other: the hardest edges
/// of colour there are, which the colour planes of a video have no room to smooth.
const HARD_CHROMA_STRIPES: &str =
    "lum='128+60*sin(X/23)*sin(Y/17)':cb='if(lt(mod(X,8),4),48,208)':cr='if(lt(mod(Y,8),4),48,208)'";

fn ffmpeg(toolchain: &FfmpegToolchain) -> Command {
    let mut command = Command::new(toolchain.ffmpeg_path());
    command.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"]);
    command
}

fn run(mut command: Command) {
    let output = command.output().expect("ffmpeg could not be run");
    assert!(
        output.status.success(),
        "ffmpeg failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The filter that makes a video say it has these colours, without changing a number of it.
fn tagging_filter(tags: Tags) -> String {
    if tags.is_empty() {
        "null".to_owned()
    } else {
        format!("setparams={}", tags.join(":"))
    }
}

/// How a picture in `pixel_format` is stored so that every number of it is kept, and what it says about its range with it: the numbers as they are,
/// uncompressed, for `yuv420p` (the lossless codec of ffmpeg, FFV1, keeps only the limited range of it, and changes the numbers of a picture that says
/// it is full range to fit), and FFV1 for the others (the tests that use them look at nothing but how they are stored).
fn stored_without_a_change(pixel_format: &str) -> Vec<&str> {
    if pixel_format == "yuv420p" {
        vec!["-c:v", "rawvideo"]
    } else {
        vec!["-c:v", "ffv1", "-pix_fmt", pixel_format]
    }
}

/// A video of one picture, with the three planes the `geq` filter makes of `expressions`, in `pixel_format`, tagged as `tags` say.
fn video_made_by_geq(
    toolchain: &FfmpegToolchain,
    folder: &Path,
    name: &str,
    (width, height): (u32, u32),
    expressions: &str,
    pixel_format: &str,
    tags: Tags,
) -> PathBuf {
    let path = folder.join(format!("{name}.mkv"));
    let mut command = ffmpeg(toolchain);
    command
        .args(["-f", "lavfi", "-i"])
        .arg(format!(
            "nullsrc=s={width}x{height}:r=1:d=2,format=yuv420p,geq={expressions},{}",
            tagging_filter(tags)
        ))
        .args(stored_without_a_change(pixel_format));
    command.args(["-frames:v", "1"]).arg(&path);
    run(command);
    path
}

/// The three planes of a picture: numbers that look random and are the same every time.
fn planes_of_noise(width: u32, height: u32, seed: u64) -> [Vec<u8>; 3] {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut noise = |count: usize| -> Vec<u8> {
        (0..count)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 24) as u8
            })
            .collect()
    };
    let (chroma_width, chroma_height) = chroma_size(width, height);
    let chroma_bytes = (chroma_width * chroma_height) as usize;
    [
        noise((width * height) as usize),
        noise(chroma_bytes),
        noise(chroma_bytes),
    ]
}

/// A video of one picture whose planes are exactly `planes`, tagged as `tags` say.
fn video_of_planes(
    toolchain: &FfmpegToolchain,
    folder: &Path,
    name: &str,
    (width, height): (u32, u32),
    planes: &[Vec<u8>; 3],
    tags: Tags,
) -> PathBuf {
    let raw = folder.join(format!("{name}.yuv"));
    std::fs::write(&raw, planes.concat()).expect("the planes could be written");
    let path = folder.join(format!("{name}.mkv"));
    let mut command = ffmpeg(toolchain);
    command
        .args(["-f", "rawvideo", "-pixel_format", "yuv420p", "-video_size"])
        .arg(format!("{width}x{height}"))
        .args(["-framerate", "1", "-i"])
        .arg(&raw)
        .args(["-vf", &tagging_filter(tags)])
        .args(stored_without_a_change("yuv420p"))
        .args(["-frames:v", "1"])
        .arg(&path);
    run(command);
    path
}

/// The first picture that a stream of `video` (which is `video_size` big) delivers when the preview picture is `picture_size`.
fn first_picture(
    toolchain: &FfmpegToolchain,
    video: &Path,
    video_size: (u32, u32),
    picture_size: (u32, u32),
    delivery: FrameDelivery,
) -> FrameStreamEvent {
    let request = FrameStreamRequest {
        video: video.to_path_buf().into(),
        project_frame_rate: FrameRate::new(1, 1).unwrap(),
        video_frame_rate: FrameRate::new(1, 1).unwrap(),
        first_source_frame: SourceFrameIndex::new(0),
        source_frame_count: 1,
        speed_factor: SpeedFactor::new(1).unwrap(),
        video_display_size_in_pixels: video_size,
        picture_width_in_pixels: picture_size.0,
        picture_height_in_pixels: picture_size.1,
        delivery,
    };
    let stream = FrameStream::start(toolchain, &request, Arc::new(|| {})).expect("the stream should start");
    let started = Instant::now();
    loop {
        match stream.try_next_event() {
            Some(FrameStreamEvent::Finished { error_output }) => panic!("no picture came: {error_output:?}"),
            Some(picture) => return picture,
            None => {
                assert!(started.elapsed() < TIME_ALLOWED_FOR_A_STREAM, "no picture came in time");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}

fn picture_as_rgba(
    toolchain: &FfmpegToolchain,
    video: &Path,
    video_size: (u32, u32),
    picture_size: (u32, u32),
) -> DecodedFrame {
    match first_picture(toolchain, video, video_size, picture_size, FrameDelivery::Rgba) {
        FrameStreamEvent::Frame(frame) => frame,
        other => panic!("RGBA was asked for: {other:?}"),
    }
}

fn picture_in_planes(
    toolchain: &FfmpegToolchain,
    video: &Path,
    video_size: (u32, u32),
    picture_size: (u32, u32),
) -> DecodedYuvFrame {
    match first_picture(toolchain, video, video_size, picture_size, FrameDelivery::Yuv420p) {
        FrameStreamEvent::YuvFrame(frame) => frame,
        other => panic!("planes were asked for: {other:?}"),
    }
}

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

/// What the preview makes of a video it decodes in planes: the colour comes from what ffprobe says of the video, as in the program.
fn colour_the_preview_gives(toolchain: &FfmpegToolchain, video: &Path) -> YuvColour {
    let format = probe_video_stream_format(toolchain, video.to_path_buf()).expect("the video can be probed");
    colour_for_the_engine(
        format
            .as_native_yuv420p()
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
        assert_eq!(format.as_native_yuv420p(), expected, "{name}: {format:?}");
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
        assert_eq!(format.as_native_yuv420p(), None, "{pixel_format}");
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
        assert_eq!(format.as_native_yuv420p(), Some(expected), "{name}: {format:?}");
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

            let (largest, mean) = differences(&ours, &theirs.rgba_pixels);
            let what = format!("{video_size:?} made {width}x{height} [{name}]");
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
            let (largest, mean) = differences(&ours, &theirs.rgba_pixels);
            let what = format!("{video_size:?} made {width}x{height} [{name}]");
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
}
