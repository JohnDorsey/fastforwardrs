// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The matrix the planes of the finished video are made with, and what the file says: the rules of `docs/engine-phase-3-design.md`, decisions C9 and C14, one by one.
//! (Rewritten on purpose in phase 3: until then these rules followed the *first video* of the project, and a first video that said nothing left the file saying nothing.)

use super::*;
use crate::colour_space::StandardPrimaries;
use crate::media::{NativeMatrix, NativeYuv420p, VideoStreamFormat};
use crate::playback::colour_for_the_engine;
use crate::project::OutputColourSpace;

fn limited(matrix: YuvMatrix) -> YuvColour {
    YuvColour {
        matrix,
        range: YuvRange::Limited,
    }
}

#[test]
fn the_default_output_space_is_made_with_the_matrix_of_bt709_in_the_limited_range_and_the_file_says_all_of_it() {
    let colour = output_colour_for(OutputColourSpace::Rec709AsPreviewed);
    assert_eq!(colour.planes, limited(YuvMatrix::Bt709));
    assert_eq!(
        colour.tags,
        OutputTags {
            matrix: Some("bt709".to_owned()),
            primaries: Some("bt709".to_owned()),
            transfer: Some("bt709".to_owned()),
        }
    );
    assert_eq!(
        colour.tags.setparams_filter().as_deref(),
        Some("setparams=colorspace=bt709:range=tv:color_primaries=bt709:color_trc=bt709")
    );
}

#[test]
fn bt1886_is_tagged_as_the_default_is_and_srgb_says_that_it_is_srgb() {
    assert_eq!(
        output_colour_for(OutputColourSpace::Rec709Bt1886),
        output_colour_for(OutputColourSpace::Rec709AsPreviewed),
        "the two are Rec.709 video; the numbers are what differs, and they are made by the engine"
    );
    let srgb = output_colour_for(OutputColourSpace::Srgb);
    assert_eq!(
        srgb.planes,
        limited(YuvMatrix::Bt709),
        "the matrix is BT.709's for sRGB as well"
    );
    assert_eq!(srgb.tags.transfer.as_deref(), Some("iec61966-2-1"));
    assert_eq!(srgb.tags.matrix.as_deref(), Some("bt709"));
    assert_eq!(srgb.tags.primaries.as_deref(), Some("bt709"));
    assert_eq!(
        srgb.tags.setparams_filter().as_deref(),
        Some("setparams=colorspace=bt709:range=tv:color_primaries=bt709:color_trc=iec61966-2-1")
    );
}

#[test]
fn whatever_the_output_space_the_file_always_says_its_matrix_its_primaries_its_transfer_and_its_range() {
    // (A first video that said nothing used to make a file that said nothing: that is what this rule is about.)
    for space in OutputColourSpace::ALL {
        let colour = output_colour_for(space);
        assert!(colour.tags.matrix.is_some(), "{space:?}: matrix");
        assert!(colour.tags.primaries.is_some(), "{space:?}: primaries");
        assert!(colour.tags.transfer.is_some(), "{space:?}: transfer");
        assert!(!colour.tags.is_empty(), "{space:?}");
        let filter = colour
            .tags
            .setparams_filter()
            .expect("a file that says its colours has a filter");
        assert!(filter.contains(":range=tv"), "{space:?}: {filter}");
        assert_eq!(
            colour.planes.range,
            YuvRange::Limited,
            "{space:?}: the finished video is of the limited range"
        );
    }
}

#[test]
fn what_the_file_says_is_read_back_by_the_function_that_reads_a_file_as_what_the_planes_were_made_with() {
    // The preview reads a file by `colour_of_a_picture_of_size` and the export writes one by `output_colour_for`: the two are made apart and have to stay in step. A file
    // that this export made has to be read as BT.709, limited, with the primaries of its output space, and with nothing assumed of it but what a transfer tag means.
    for space in OutputColourSpace::ALL {
        let colour = output_colour_for(space);
        let file = VideoStreamFormat {
            pixel_format: Some("yuv420p".to_owned()),
            colour_space: colour.tags.matrix.clone(),
            colour_range: Some("tv".to_owned()),
            colour_primaries: colour.tags.primaries.clone(),
            colour_transfer: colour.tags.transfer.clone(),
        };
        for size in [(320, 180), (1920, 1080)] {
            let read = file.colour_of_a_picture_of_size(size.0, size.1);
            assert_eq!(read.matrix, Some(NativeMatrix::Bt709), "{space:?} at {size:?}");
            assert_eq!(read.full_range, Some(false), "{space:?} at {size:?}");
            assert_eq!(read.primaries, space.primaries(), "{space:?} at {size:?}");
            assert_eq!(
                read.assumptions,
                Vec::new(),
                "{space:?} at {size:?}: the file leaves nothing to be assumed"
            );
            assert_eq!(
                colour_for_the_engine(NativeYuv420p {
                    matrix: read.matrix.unwrap(),
                    full_range: false,
                }),
                colour.planes,
                "{space:?}: the planes are made with the matrix that reads them back"
            );
        }
    }
}

#[test]
fn the_primaries_that_the_file_says_are_the_primaries_of_the_output_space() {
    // (Every output space is Rec.709's primaries today; the tag has to follow when another is added, and this says so by failing.)
    for space in OutputColourSpace::ALL {
        assert_eq!(space.primaries(), StandardPrimaries::Rec709, "{space:?}");
        assert_eq!(
            output_colour_for(space).tags.primaries.as_deref(),
            Some("bt709"),
            "{space:?}"
        );
    }
}

#[test]
fn every_name_in_the_tags_is_the_kind_of_text_that_can_be_put_in_a_filter_as_it_is() {
    for space in OutputColourSpace::ALL {
        let tags = output_colour_for(space).tags;
        for name in [&tags.matrix, &tags.primaries, &tags.transfer].into_iter().flatten() {
            assert!(
                !name.is_empty()
                    && name.chars().all(|character| character.is_ascii_lowercase()
                        || character.is_ascii_digit()
                        || character == '-'),
                "{space:?}: {name:?}"
            );
        }
    }
}

#[test]
fn tags_that_say_any_one_thing_are_not_empty_and_make_a_filter_and_tags_that_say_nothing_make_none() {
    assert!(OutputTags::default().is_empty());
    assert_eq!(OutputTags::default().setparams_filter(), None);
    for (tags, filter) in [
        (
            OutputTags {
                matrix: Some("bt709".to_owned()),
                ..OutputTags::default()
            },
            "setparams=colorspace=bt709:range=tv",
        ),
        (
            OutputTags {
                primaries: Some("bt709".to_owned()),
                ..OutputTags::default()
            },
            "setparams=range=tv:color_primaries=bt709",
        ),
        (
            OutputTags {
                transfer: Some("bt709".to_owned()),
                ..OutputTags::default()
            },
            "setparams=range=tv:color_trc=bt709",
        ),
    ] {
        assert!(!tags.is_empty(), "{tags:?}");
        assert_eq!(tags.setparams_filter().as_deref(), Some(filter));
    }
}
