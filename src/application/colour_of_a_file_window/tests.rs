// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use super::*;
use crate::project::{FrameRate, SourceImageProperties, SourceMediaLink, SourceVideoProperties, TimelineFrameIndex};

const VIDEO: SourceMediaIdentifier = SourceMediaIdentifier::new(1);
const PICTURE: SourceMediaIdentifier = SourceMediaIdentifier::new(2);

/// What the harness holds: a project, which file's window is open, what is known of the formats of the videos, and the commands that were asked for.
struct Shown {
    project: Project,
    open_for: Option<SourceMediaIdentifier>,
    formats: HashMap<SourceMediaIdentifier, VideoStreamFormat>,
    commands: Vec<Command>,
}

impl Shown {
    fn of(open_for: Option<SourceMediaIdentifier>, format: Option<VideoStreamFormat>) -> Self {
        let mut project = Project::empty();
        project
            .import_source(
                SourceMedia::video(
                    VIDEO,
                    SourceMediaLink::from_absolute_path("/videos/holiday.mp4".into()),
                    SourceVideoProperties {
                        frame_rate: FrameRate::new(30, 1).unwrap(),
                        total_frame_count: 90,
                        display_width_in_pixels: 1920,
                        display_height_in_pixels: 1080,
                        has_audio: false,
                    },
                ),
                TimelineFrameIndex::new(0),
            )
            .unwrap();
        project
            .import_source(
                SourceMedia::image(
                    PICTURE,
                    SourceMediaLink::from_absolute_path("/pictures/cat.png".into()),
                    SourceImageProperties {
                        display_width_in_pixels: 100,
                        display_height_in_pixels: 50,
                    },
                ),
                TimelineFrameIndex::new(90),
            )
            .unwrap();
        Self {
            project,
            open_for,
            formats: format.map(|format| (VIDEO, format)).into_iter().collect(),
            commands: Vec::new(),
        }
    }
}

fn format_that_says(colour_space: Option<&str>) -> VideoStreamFormat {
    VideoStreamFormat {
        pixel_format: Some("yuv420p".to_owned()),
        colour_space: colour_space.map(str::to_owned),
        ..VideoStreamFormat::default()
    }
}

/// A window of the harness: it applies the commands that the window asks for to the project, as the application does, so that the window can be used again.
fn harness_of(shown: Shown) -> Harness<'static, Shown> {
    let mut harness = Harness::builder().with_size(vec2(900.0, 600.0)).build_ui_state(
        |ui, shown: &mut Shown| {
            let formats = &shown.formats;
            show(
                ui.ctx(),
                &mut shown.open_for,
                &shown.project,
                |source| formats.get(&source),
                &mut shown.commands,
            );
            for command in shown.commands.drain(..) {
                match command {
                    Command::SetTheColoursOfAFile { source, colour } => {
                        assert!(shown.project.set_source_colour(source, colour));
                    }
                    other => panic!("the window asked for something else: {other:?}"),
                }
            }
        },
        shown,
    );
    harness.run();
    harness
}

#[test]
fn nothing_is_shown_while_no_file_is_asked_about() {
    let harness = harness_of(Shown::of(None, Some(format_that_says(None))));

    assert!(harness.query_by_label(MATRIX_LABEL).is_none());
    assert!(harness.query_by_label_contains("Colour space of").is_none());
}

#[test]
fn the_window_of_a_video_has_the_four_lines_and_says_what_automatic_is_for_this_file() {
    let harness = harness_of(Shown::of(Some(VIDEO), Some(format_that_says(None))));

    harness.get_by_label("Colour space of holiday.mp4");
    for label in [MATRIX_LABEL, RANGE_LABEL, PRIMARIES_LABEL, CURVE_LABEL] {
        harness.get_by_label(label);
    }
    // A file that says nothing, 1920 by 1080: BT.709 (the size says so), the limited range, Rec.709's primaries, the curve of a screen.
    harness.get_by_value("Automatic (BT.709)");
    harness.get_by_value("Automatic (Limited (16 to 235))");
    harness.get_by_value("Automatic (Rec.709 / sRGB)");
    harness.get_by_value("Automatic (sRGB)");
}

#[test]
fn what_automatic_is_follows_what_the_file_says() {
    let mut says_bt601 = format_that_says(Some("smpte170m"));
    says_bt601.colour_range = Some("pc".to_owned());
    says_bt601.colour_transfer = Some("smpte2084".to_owned());
    let harness = harness_of(Shown::of(Some(VIDEO), Some(says_bt601)));

    harness.get_by_value("Automatic (BT.601)");
    harness.get_by_value("Automatic (Full (0 to 255))");
    harness.get_by_value("Automatic (sRGB)");
}

#[test]
fn the_window_of_a_picture_file_has_only_the_primaries_and_the_transfer_function() {
    let harness = harness_of(Shown::of(Some(PICTURE), None));

    harness.get_by_label("Colour space of cat.png");
    harness.get_by_label(PRIMARIES_LABEL);
    harness.get_by_label(CURVE_LABEL);
    assert!(harness.query_by_label(MATRIX_LABEL).is_none());
    assert!(harness.query_by_label(RANGE_LABEL).is_none());
    harness.get_by_value("Automatic (Rec.709 / sRGB)");
    harness.get_by_value("Automatic (sRGB)");
}

#[test]
fn choosing_a_matrix_asks_for_it_and_only_for_it_and_the_window_then_shows_it_and_says_so_in_words() {
    let mut harness = harness_of(Shown::of(Some(VIDEO), Some(format_that_says(None))));

    harness.get_by_value("Automatic (BT.709)").click();
    harness.run();
    harness.get_by_label("BT.601").click();
    harness.run();

    assert_eq!(
        harness.state().project.source_colour(VIDEO),
        Some(InputColourOverride {
            matrix: Some(VideoMatrix::Bt601),
            ..InputColourOverride::AUTOMATIC
        }),
        "the matrix, and nothing else, is said"
    );
    harness.get_by_value("BT.601");
    harness.get_by_label_contains("You chose BT.601 as the matrix");
}

#[test]
fn choosing_automatic_again_takes_the_word_back() {
    let mut shown = Shown::of(Some(VIDEO), Some(format_that_says(None)));
    shown.project.set_source_colour(
        VIDEO,
        InputColourOverride {
            matrix: Some(VideoMatrix::Bt2020),
            curve: Some(TransferFunction::Gamma24),
            ..InputColourOverride::AUTOMATIC
        },
    );
    let mut harness = harness_of(shown);

    harness.get_by_value("BT.2020").click();
    harness.run();
    harness.get_by_label("Automatic (BT.709)").click();
    harness.run();

    assert_eq!(
        harness.state().project.source_colour(VIDEO),
        Some(InputColourOverride {
            curve: Some(TransferFunction::Gamma24),
            ..InputColourOverride::AUTOMATIC
        }),
        "the matrix is automatic again and the curve stays as it was said"
    );
}

#[test]
fn every_choice_of_every_line_can_be_made() {
    for (line, current, option, check) in [
        (
            PRIMARIES_LABEL,
            "Automatic (Rec.709 / sRGB)",
            "Display P3",
            Box::new(|said: InputColourOverride| said.primaries == Some(StandardPrimaries::DisplayP3))
                as Box<dyn Fn(InputColourOverride) -> bool>,
        ),
        (
            CURVE_LABEL,
            "Automatic (sRGB)",
            "Gamma 2.2",
            Box::new(|said| said.curve == Some(TransferFunction::Gamma22)),
        ),
        (
            RANGE_LABEL,
            "Automatic (Limited (16 to 235))",
            "Full (0 to 255)",
            Box::new(|said| said.range == Some(VideoRange::Full)),
        ),
    ] {
        let mut harness = harness_of(Shown::of(Some(VIDEO), Some(format_that_says(None))));
        harness.get_by_value(current).click();
        harness.run();
        harness.get_by_label(option).click();
        harness.run();
        let said = harness.state().project.source_colour(VIDEO).unwrap();
        assert!(check(said), "{line}: {said:?}");
    }
}

#[test]
fn a_video_whose_format_is_not_known_yet_is_said_to_be_being_examined() {
    let harness = harness_of(Shown::of(Some(VIDEO), None));

    harness.get_by_label_contains("is being examined");
    harness.get_by_value("Automatic (BT.709)");
}

#[test]
fn what_was_assumed_is_said_in_words_and_a_format_that_ffmpeg_turns_into_colours_is_said_to_have_its_own_matrix() {
    let harness = harness_of(Shown::of(Some(VIDEO), Some(format_that_says(None))));
    harness.get_by_label_contains("The file does not say which matrix its colours use: BT.709 is assumed");

    let mut other_way = format_that_says(Some("bt709"));
    other_way.pixel_format = Some("yuv444p".to_owned());
    let harness = harness_of(Shown::of(Some(VIDEO), Some(other_way)));
    harness.get_by_label_contains("yuv444p");
}

#[test]
fn a_file_that_is_not_in_the_project_any_more_closes_its_window() {
    let mut shown = Shown::of(Some(SourceMediaIdentifier::new(77)), None);
    shown.formats.clear();
    let harness = harness_of(shown);

    assert_eq!(harness.state().open_for, None);
}
