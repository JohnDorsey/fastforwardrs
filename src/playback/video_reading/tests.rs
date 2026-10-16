// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::colour_space::{ChromaSiting, StandardPrimaries, TransferFunction, VideoMatrix};

fn format(pixel_format: &str, colour_space: Option<&str>, colour_range: Option<&str>) -> VideoStreamFormat {
    VideoStreamFormat {
        pixel_format: Some(pixel_format.to_owned()),
        colour_space: colour_space.map(str::to_owned),
        colour_range: colour_range.map(str::to_owned),
        ..VideoStreamFormat::default()
    }
}

const HD: (u32, u32) = (1920, 1080);

#[test]
fn a_video_that_is_native_and_wanted_in_planes_is_read_by_the_engine_and_ffmpeg_is_told_nothing() {
    let reading = reading_of_a_video(
        Some(&format("yuv420p", Some("bt709"), Some("tv"))),
        HD,
        &InputColourOverride::AUTOMATIC,
        true,
    );

    assert_eq!(
        reading.planes,
        Some(NativeYuv420p {
            matrix: VideoMatrix::Bt709,
            full_range: false
        })
    );
    assert_eq!(reading.ffmpeg_is_told, None);
    assert_eq!(reading.picture_colour, PictureColour::SRGB);
}

#[test]
fn a_video_that_is_not_wanted_in_planes_or_is_stored_another_way_is_made_into_colours_by_ffmpeg_which_is_told_the_matrix_and_the_range()
 {
    for (pixel_format, planes_wanted) in [
        ("yuv420p", false),
        ("yuv444p", true),
        ("yuv422p10le", true),
        ("yuvj420p", true),
    ] {
        let reading = reading_of_a_video(
            Some(&format(pixel_format, None, None)),
            HD,
            &InputColourOverride::AUTOMATIC,
            planes_wanted,
        );
        assert_eq!(reading.planes, None, "{pixel_format}");
        let told = reading
            .ffmpeg_is_told
            .unwrap_or_else(|| panic!("{pixel_format}: ffmpeg is told"));
        assert_eq!(
            told.matrix,
            Some(VideoMatrix::Bt709),
            "{pixel_format}: a file that says nothing is BT.709 when the picture is high definition"
        );
    }
}

#[test]
fn ffmpeg_is_told_the_matrix_by_the_size_of_the_picture_where_the_file_says_nothing_and_by_the_file_where_it_says() {
    let told = |format: VideoStreamFormat, size| {
        reading_of_a_video(Some(&format), size, &InputColourOverride::AUTOMATIC, false)
            .ffmpeg_is_told
            .unwrap()
    };
    assert_eq!(
        told(format("yuv444p", None, None), (320, 180)).matrix,
        Some(VideoMatrix::Bt601)
    );
    assert_eq!(
        told(format("yuv444p", None, None), (1280, 720)).matrix,
        Some(VideoMatrix::Bt709)
    );
    assert_eq!(
        told(format("yuv444p", Some("bt2020nc"), None), (320, 180)).matrix,
        Some(VideoMatrix::Bt2020)
    );
    assert_eq!(
        told(format("yuv444p", Some("bt470bg"), None), (3840, 2160)).matrix,
        Some(VideoMatrix::Bt601)
    );
    // And a matrix that the program has not got is left to ffmpeg.
    assert_eq!(told(format("yuv444p", Some("smpte240m"), None), HD).matrix, None);
}

#[test]
fn ffmpeg_is_told_the_range_the_file_says_and_the_limited_one_where_it_says_nothing() {
    let range = |colour_range: Option<&str>| {
        reading_of_a_video(
            Some(&format("yuv444p", Some("bt709"), colour_range)),
            HD,
            &InputColourOverride::AUTOMATIC,
            false,
        )
        .ffmpeg_is_told
        .unwrap()
        .range
    };
    assert_eq!(range(None), Some(VideoRange::Limited));
    assert_eq!(range(Some("tv")), Some(VideoRange::Limited));
    assert_eq!(range(Some("pc")), Some(VideoRange::Full));
    assert_eq!(
        range(Some("strange")),
        None,
        "a range that is not known is left to ffmpeg"
    );
}

#[test]
fn a_video_stored_in_a_pixel_format_of_full_range_that_does_not_say_its_range_is_told_to_be_full() {
    // (yuvj420p is the JPEG form of yuv420p: full range by its name; saying "limited" of it would wash every colour out.)
    let reading = reading_of_a_video(
        Some(&format("yuvj420p", Some("bt709"), None)),
        HD,
        &InputColourOverride::AUTOMATIC,
        true,
    );
    assert_eq!(reading.ffmpeg_is_told.unwrap().range, Some(VideoRange::Full));
    // And one that does say its range is believed.
    let says = reading_of_a_video(
        Some(&format("yuvj420p", Some("bt709"), Some("tv"))),
        HD,
        &InputColourOverride::AUTOMATIC,
        true,
    );
    assert_eq!(says.ffmpeg_is_told.unwrap().range, Some(VideoRange::Limited));
}

#[test]
fn what_the_user_says_is_what_ffmpeg_is_told_and_what_the_planes_stand_for_and_what_the_pictures_come_with() {
    let said = InputColourOverride {
        matrix: Some(VideoMatrix::Bt601),
        range: Some(VideoRange::Full),
        primaries: Some(StandardPrimaries::DisplayP3),
        curve: Some(TransferFunction::Linear),
    };
    let chosen_codes = PictureColour {
        primaries: StandardPrimaries::DisplayP3,
        curve: TransferFunction::Linear,
    };

    let in_planes = reading_of_a_video(Some(&format("yuv420p", Some("bt709"), Some("tv"))), HD, &said, true);
    assert_eq!(
        in_planes.planes,
        Some(NativeYuv420p {
            matrix: VideoMatrix::Bt601,
            full_range: true
        })
    );
    assert_eq!(in_planes.picture_colour, chosen_codes);

    let by_ffmpeg = reading_of_a_video(Some(&format("yuv422p", Some("bt709"), Some("tv"))), HD, &said, true);
    assert_eq!(
        by_ffmpeg.ffmpeg_is_told,
        Some(HowToReadColours {
            matrix: Some(VideoMatrix::Bt601),
            range: Some(VideoRange::Full)
        })
    );
    assert_eq!(by_ffmpeg.picture_colour, chosen_codes);
}

#[test]
fn a_video_whose_format_is_not_known_yet_is_srgb_and_ffmpeg_is_told_nothing_unless_the_user_chose_the_primaries_or_the_curve()
 {
    let nothing_known = reading_of_a_video(None, HD, &InputColourOverride::AUTOMATIC, true);
    assert_eq!(
        nothing_known,
        VideoReading {
            planes: None,
            picture_colour: PictureColour::SRGB,
            ffmpeg_is_told: None,
            chroma_siting: ChromaSiting::Left,
        }
    );
    let chosen = reading_of_a_video(
        None,
        HD,
        &InputColourOverride {
            curve: Some(TransferFunction::Gamma22),
            ..InputColourOverride::AUTOMATIC
        },
        true,
    );
    assert_eq!(chosen.picture_colour.curve, TransferFunction::Gamma22);
    assert_eq!(chosen.ffmpeg_is_told, None);
}

#[test]
fn where_the_colour_differences_sit_is_what_the_file_says_and_the_usual_place_where_it_does_not_say_or_is_not_known_yet()
 {
    let said = |location: Option<&str>| VideoStreamFormat {
        chroma_location: location.map(str::to_owned),
        ..format("yuv420p", Some("bt709"), None)
    };
    for (location, expected) in [
        (Some("center"), ChromaSiting::Center),
        (Some("topleft"), ChromaSiting::TopLeft),
        (Some("left"), ChromaSiting::Left),
        (None, ChromaSiting::Left),
    ] {
        let reading = reading_of_a_video(Some(&said(location)), HD, &InputColourOverride::AUTOMATIC, true);
        assert_eq!(reading.chroma_siting, expected, "{location:?}");
    }
    assert_eq!(
        reading_of_a_video(None, HD, &InputColourOverride::AUTOMATIC, true).chroma_siting,
        ChromaSiting::Left
    );
}
