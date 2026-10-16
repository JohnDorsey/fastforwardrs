// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The format of a stream that says what the arguments say of its colours (`None`: nothing).
fn said(
    matrix: Option<&str>,
    range: Option<&str>,
    primaries: Option<&str>,
    transfer: Option<&str>,
) -> VideoStreamFormat {
    VideoStreamFormat {
        pixel_format: Some("yuv420p".to_owned()),
        colour_space: matrix.map(str::to_owned),
        colour_range: range.map(str::to_owned),
        colour_primaries: primaries.map(str::to_owned),
        colour_transfer: transfer.map(str::to_owned),
    }
}

const HD: (u32, u32) = (1920, 1080);

fn colour_of(format: &VideoStreamFormat, (width, height): (u32, u32)) -> VideoColour {
    format.colour_of_a_picture_of_size(width, height)
}

#[test]
fn a_file_that_says_everything_as_screen_video_does_is_read_as_it_says_and_nothing_is_assumed() {
    let colour = colour_of(&said(Some("bt709"), Some("tv"), Some("bt709"), Some("bt709")), HD);
    assert_eq!(
        colour,
        VideoColour {
            matrix: Some(NativeMatrix::Bt709),
            full_range: Some(false),
            primaries: StandardPrimaries::Rec709,
            curve: TransferFunction::Srgb,
            assumptions: Vec::new(),
        }
    );
}

#[test]
fn the_matrix_is_what_the_file_says_whatever_the_size_of_the_picture() {
    for (said_matrix, expected) in [
        ("bt709", NativeMatrix::Bt709),
        ("bt470bg", NativeMatrix::Bt601),
        ("smpte170m", NativeMatrix::Bt601),
        ("bt2020nc", NativeMatrix::Bt2020),
    ] {
        for size in [(320, 240), (768, 432), HD, (3840, 2160)] {
            let colour = colour_of(&said(Some(said_matrix), None, None, None), size);
            assert_eq!(colour.matrix, Some(expected), "{said_matrix} at {size:?}");
            assert!(
                !colour
                    .assumptions
                    .iter()
                    .any(|assumption| matches!(assumption, Assumption::MatrixNotSaid { .. })),
                "{said_matrix}: nothing is assumed of what is said"
            );
        }
    }
}

#[test]
fn a_file_that_does_not_say_its_matrix_gets_bt709_when_the_picture_is_high_definition_and_bt601_when_it_is_not() {
    // (BT.601 was the matrix of every size, because it is what ffmpeg does when it makes the colours itself.)
    let assumed = |size: (u32, u32)| colour_of(&said(None, None, None, None), size).matrix;
    assert_eq!(
        assumed((1024, 576)),
        Some(NativeMatrix::Bt601),
        "the largest picture of standard definition"
    );
    assert_eq!(assumed((1025, 576)), Some(NativeMatrix::Bt709), "one pixel wider");
    assert_eq!(assumed((1024, 577)), Some(NativeMatrix::Bt709), "one pixel taller");
    assert_eq!(assumed((768, 432)), Some(NativeMatrix::Bt601));
    assert_eq!(assumed((720, 480)), Some(NativeMatrix::Bt601));
    assert_eq!(assumed((720, 576)), Some(NativeMatrix::Bt601));
    assert_eq!(assumed((1280, 720)), Some(NativeMatrix::Bt709));
    assert_eq!(assumed(HD), Some(NativeMatrix::Bt709));
    assert_eq!(assumed((3840, 2160)), Some(NativeMatrix::Bt709));
    assert_eq!(
        assumed((576, 1024)),
        Some(NativeMatrix::Bt709),
        "a picture that is tall is taller than standard definition is"
    );
    assert_eq!(assumed((0, 0)), Some(NativeMatrix::Bt601), "no size is no size");
    let colour = colour_of(&said(None, None, None, None), HD);
    assert!(colour.assumptions.contains(&Assumption::MatrixNotSaid {
        assumed: NativeMatrix::Bt709,
        width: 1920,
        height: 1080
    }));
}

#[test]
fn a_matrix_that_the_program_has_not_got_is_not_one_and_is_said_so() {
    let colour = colour_of(&said(Some("smpte240m"), None, None, None), HD);
    assert_eq!(colour.matrix, None);
    assert!(colour.assumptions.contains(&Assumption::MatrixNotKnown {
        said: "smpte240m".to_owned()
    }));
}

#[test]
fn the_range_is_limited_unless_the_file_says_full_and_a_range_that_is_neither_is_not_one() {
    let range = |said_range: Option<&str>| colour_of(&said(Some("bt709"), said_range, None, None), HD);
    assert_eq!(range(None).full_range, Some(false));
    assert!(range(None).assumptions.contains(&Assumption::RangeNotSaid));
    for limited in ["tv", "mpeg"] {
        assert_eq!(range(Some(limited)).full_range, Some(false), "{limited}");
        assert!(!range(Some(limited)).assumptions.contains(&Assumption::RangeNotSaid));
    }
    for full in ["pc", "jpeg"] {
        assert_eq!(range(Some(full)).full_range, Some(true), "{full}");
    }
    assert_eq!(range(Some("strange")).full_range, None);
    assert!(range(Some("strange")).assumptions.contains(&Assumption::RangeNotKnown {
        said: "strange".to_owned()
    }));
}

#[test]
fn the_primaries_are_the_set_the_file_names_and_rec709s_where_it_does_not_or_names_one_that_is_not_known() {
    let primaries = |said_primaries: Option<&str>| colour_of(&said(Some("bt709"), None, said_primaries, None), HD);
    for (name, expected) in [
        ("bt709", StandardPrimaries::Rec709),
        ("smpte170m", StandardPrimaries::Bt601Ntsc),
        ("bt470bg", StandardPrimaries::Bt601Pal),
        ("bt2020", StandardPrimaries::Rec2020),
        ("smpte432", StandardPrimaries::DisplayP3),
    ] {
        assert_eq!(primaries(Some(name)).primaries, expected, "{name}");
        assert!(
            !primaries(Some(name)).assumptions.iter().any(|assumption| matches!(
                assumption,
                Assumption::PrimariesNotSaid | Assumption::PrimariesNotKnown { .. }
            )),
            "{name}"
        );
    }
    assert_eq!(primaries(None).primaries, StandardPrimaries::Rec709);
    assert!(primaries(None).assumptions.contains(&Assumption::PrimariesNotSaid));
    assert_eq!(primaries(Some("film")).primaries, StandardPrimaries::Rec709);
    assert!(
        primaries(Some("film"))
            .assumptions
            .contains(&Assumption::PrimariesNotKnown {
                said: "film".to_owned()
            })
    );
}

#[test]
fn a_transfer_of_standard_dynamic_range_is_read_as_the_curve_of_a_screen_and_not_as_the_camera_curve() {
    for name in [
        "bt709",
        "smpte170m",
        "bt470bg",
        "smpte240m",
        "bt2020-10",
        "bt2020-12",
        "iec61966-2-1",
    ] {
        let colour = colour_of(&said(Some("bt709"), None, None, Some(name)), HD);
        assert_eq!(colour.curve, TransferFunction::Srgb, "{name}");
        assert!(
            !colour.assumptions.iter().any(|assumption| matches!(
                assumption,
                Assumption::TransferNotSaid
                    | Assumption::TransferNotKnown { .. }
                    | Assumption::TransferIsForHighDynamicRange { .. }
            )),
            "{name}: nothing is assumed of what is said"
        );
        assert_ne!(
            colour.curve,
            TransferFunction::Rec709,
            "{name}: never the camera curve, which would lift every shadow"
        );
    }
    assert_eq!(
        colour_of(&said(Some("bt709"), None, None, Some("linear")), HD).curve,
        TransferFunction::Linear
    );
    assert_eq!(
        colour_of(&said(Some("bt709"), None, None, Some("gamma22")), HD).curve,
        TransferFunction::Gamma22
    );
}

#[test]
fn a_transfer_that_is_not_said_or_not_known_is_the_curve_of_a_screen_and_one_for_high_dynamic_range_is_said_not_to_be_converted()
 {
    let transfer = |said_transfer: Option<&str>| colour_of(&said(Some("bt709"), None, None, said_transfer), HD);
    assert_eq!(transfer(None).curve, TransferFunction::Srgb);
    assert!(transfer(None).assumptions.contains(&Assumption::TransferNotSaid));
    for high in ["smpte2084", "arib-std-b67"] {
        assert_eq!(transfer(Some(high)).curve, TransferFunction::Srgb, "{high}");
        assert!(
            transfer(Some(high))
                .assumptions
                .contains(&Assumption::TransferIsForHighDynamicRange { said: high.to_owned() }),
            "{high}"
        );
    }
    assert_eq!(transfer(Some("log")).curve, TransferFunction::Srgb);
    assert!(
        transfer(Some("log"))
            .assumptions
            .contains(&Assumption::TransferNotKnown { said: "log".to_owned() })
    );
}

#[test]
fn every_assumption_has_words_that_say_what_was_assumed_and_why() {
    let colour = colour_of(&said(None, None, None, None), HD);
    assert_eq!(
        colour.assumptions.len(),
        4,
        "a file that says nothing has four things assumed of it: {:?}",
        colour.assumptions
    );
    let words: Vec<String> = colour.assumptions.iter().map(Assumption::in_words).collect();
    assert!(
        words[0].contains("BT.709") && words[0].contains("1920 by 1080"),
        "{}",
        words[0]
    );
    assert!(words[1].contains("limited"), "{}", words[1]);
    assert!(words[2].contains("Rec.709"), "{}", words[2]);
    assert!(words[3].contains("sRGB"), "{}", words[3]);
    // And the others say what the file said.
    assert!(
        Assumption::MatrixNotKnown {
            said: "ycgco".to_owned()
        }
        .in_words()
        .contains("ycgco")
    );
    assert!(
        Assumption::TransferIsForHighDynamicRange {
            said: "smpte2084".to_owned()
        }
        .in_words()
        .contains("smpte2084")
    );
    assert!(
        Assumption::PrimariesNotKnown {
            said: "film".to_owned()
        }
        .in_words()
        .contains("film")
    );
    assert!(
        Assumption::RangeNotKnown {
            said: "strange".to_owned()
        }
        .in_words()
        .contains("strange")
    );
    assert!(
        Assumption::TransferNotKnown { said: "log".to_owned() }
            .in_words()
            .contains("log")
    );
}

#[test]
fn the_matrices_have_their_names_in_the_words_of_the_dialog() {
    assert_eq!(matrix_name(NativeMatrix::Bt601), "BT.601");
    assert_eq!(matrix_name(NativeMatrix::Bt709), "BT.709");
    assert_eq!(matrix_name(NativeMatrix::Bt2020), "BT.2020");
}
