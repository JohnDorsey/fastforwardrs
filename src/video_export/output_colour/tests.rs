// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which matrix the planes of the finished video are made with, and what the file says: the table of `docs/engine-phase-2-design.md`, section 5, row by row.

use super::*;
use crate::playback::colour_for_the_engine;

fn format_with(
    colour_space: Option<&str>,
    primaries: Option<&str>,
    transfer: Option<&str>,
    range: Option<&str>,
) -> VideoStreamFormat {
    VideoStreamFormat {
        pixel_format: Some("yuv420p".to_owned()),
        colour_space: colour_space.map(str::to_owned),
        colour_range: range.map(str::to_owned),
        colour_primaries: primaries.map(str::to_owned),
        colour_transfer: transfer.map(str::to_owned),
    }
}

fn limited(matrix: YuvMatrix) -> YuvColour {
    YuvColour {
        matrix,
        range: YuvRange::Limited,
    }
}

#[test]
fn a_project_without_a_video_is_bt601_limited_and_says_nothing() {
    let colour = output_colour_for(None);
    assert_eq!(colour.planes, limited(YuvMatrix::Bt601));
    assert!(colour.tags.is_empty());
    assert_eq!(colour.tags.setparams_filter(), None);
}

#[test]
fn a_first_video_that_says_nothing_is_bt601_limited_and_the_file_says_nothing_either() {
    let colour = output_colour_for(Some(&format_with(None, None, None, None)));
    assert_eq!(colour.planes, limited(YuvMatrix::Bt601));
    assert_eq!(colour.tags, OutputTags::default());
    assert_eq!(colour.tags.setparams_filter(), None);
}

#[test]
fn a_first_video_that_says_its_matrix_gets_that_matrix_and_the_file_says_it_too() {
    for (stated, matrix) in [
        ("bt709", YuvMatrix::Bt709),
        ("smpte170m", YuvMatrix::Bt601),
        ("bt470bg", YuvMatrix::Bt601),
        ("bt2020nc", YuvMatrix::Bt2020),
    ] {
        let colour = output_colour_for(Some(&format_with(Some(stated), None, None, Some("tv"))));
        assert_eq!(colour.planes, limited(matrix), "{stated}");
        assert_eq!(
            colour.tags.matrix.as_deref(),
            Some(stated),
            "the file says what the video said"
        );
        assert_eq!(
            colour.tags.setparams_filter(),
            Some(format!("setparams=colorspace={stated}:range=tv")),
            "{stated}"
        );
    }
}

#[test]
fn the_primaries_and_the_transfer_that_the_video_states_are_passed_on_as_they_are() {
    let colour = output_colour_for(Some(&format_with(
        Some("bt2020nc"),
        Some("bt2020"),
        Some("smpte2084"),
        Some("tv"),
    )));
    assert_eq!(
        colour.tags.setparams_filter(),
        Some("setparams=colorspace=bt2020nc:range=tv:color_primaries=bt2020:color_trc=smpte2084".to_owned())
    );
}

#[test]
fn a_video_that_states_only_its_primaries_is_still_bt601_and_the_file_says_what_was_stated_and_no_matrix() {
    let colour = output_colour_for(Some(&format_with(None, Some("bt709"), None, None)));
    assert_eq!(colour.planes, limited(YuvMatrix::Bt601));
    assert_eq!(colour.tags.matrix, None);
    assert_eq!(
        colour.tags.setparams_filter(),
        Some("setparams=range=tv:color_primaries=bt709".to_owned())
    );
}

#[test]
fn a_matrix_the_engine_cannot_make_planes_with_is_not_claimed_by_the_file() {
    for stated in ["smpte240m", "fcc", "bt2020c", "ycgco", "gbr"] {
        let colour = output_colour_for(Some(&format_with(Some(stated), Some("bt709"), None, None)));
        assert_eq!(colour.planes, limited(YuvMatrix::Bt601), "{stated}");
        assert_eq!(colour.tags.matrix, None, "{stated}: the planes are not made with it");
        assert_eq!(
            colour.tags.primaries.as_deref(),
            Some("bt709"),
            "{stated}: this is still true of the picture"
        );
    }
}

#[test]
fn the_range_of_the_finished_video_is_always_the_limited_one() {
    for range in [None, Some("tv"), Some("pc")] {
        let colour = output_colour_for(Some(&format_with(Some("bt709"), None, None, range)));
        assert_eq!(colour.planes.range, YuvRange::Limited, "{range:?}");
        assert_eq!(
            colour.tags.setparams_filter(),
            Some("setparams=colorspace=bt709:range=tv".to_owned()),
            "a video of full range is squeezed into the limited one, and the file says so: {range:?}"
        );
    }
    // A video that says only that its range is full says nothing the file repeats.
    let colour = output_colour_for(Some(&format_with(None, None, None, Some("pc"))));
    assert_eq!(colour.planes.range, YuvRange::Limited);
    assert_eq!(colour.tags.setparams_filter(), None);
}

#[test]
fn something_that_is_not_a_name_is_not_put_in_a_filter() {
    let colour = output_colour_for(Some(&format_with(
        Some("bt709;movie=evil"),
        Some("bt709,scale=1:1"),
        Some("a b"),
        None,
    )));
    assert_eq!(colour.planes, limited(YuvMatrix::Bt601));
    assert_eq!(colour.tags, OutputTags::default());
    let empty = output_colour_for(Some(&format_with(Some(""), Some(""), Some(""), None)));
    assert_eq!(empty.tags, OutputTags::default());
}

#[test]
fn the_matrix_of_the_finished_video_is_the_matrix_the_preview_made_the_colours_of_that_video_with() {
    // For a video that is stored the way the preview takes as planes, the engine made its colours with the matrix of `as_native_yuv420p`; converting back with the
    // same one is what gives the planes of the video back. The two rules are made apart and have to stay in step.
    for stated in [
        None,
        Some("bt470bg"),
        Some("smpte170m"),
        Some("bt709"),
        Some("bt2020nc"),
        Some("smpte240m"),
        Some("fcc"),
    ] {
        let format = format_with(stated, None, None, None);
        if let Some(native) = format.as_native_yuv420p() {
            assert_eq!(
                output_colour_for(Some(&format)).planes.matrix,
                colour_for_the_engine(native).matrix,
                "{stated:?}"
            );
        } else {
            assert!(
                matches!(stated, Some("smpte240m" | "fcc")),
                "{stated:?}: only the matrices that the preview cannot take as planes are left out"
            );
        }
    }
}
