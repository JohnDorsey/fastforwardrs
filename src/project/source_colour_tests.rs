// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::colour_space::{StandardPrimaries, TransferFunction, VideoMatrix, VideoRange};
use crate::project::{SourceMedia, SourceMediaLink, SourceVideoProperties, TimelineFrameIndex};

fn video_source(number: u32) -> SourceMedia {
    SourceMedia::video(
        SourceMediaIdentifier::new(number),
        SourceMediaLink::from_absolute_path(format!("/videos/{number}.mp4").into()),
        SourceVideoProperties {
            frame_rate: FrameRate::new(30, 1).unwrap(),
            total_frame_count: 90,
            display_width_in_pixels: 320,
            display_height_in_pixels: 180,
            has_audio: false,
        },
    )
}

fn said_by_hand() -> InputColourOverride {
    InputColourOverride {
        matrix: Some(VideoMatrix::Bt2020),
        range: Some(VideoRange::Full),
        primaries: Some(StandardPrimaries::DisplayP3),
        curve: Some(TransferFunction::Gamma22),
    }
}

use crate::project::FrameRate;

#[test]
fn a_source_says_nothing_of_its_colours_until_it_is_told_to() {
    assert!(InputColourOverride::default().is_automatic());
    assert_eq!(InputColourOverride::default(), InputColourOverride::AUTOMATIC);
    assert!(video_source(1).colour.is_automatic());
    for part in 0..4 {
        let mut said = InputColourOverride::AUTOMATIC;
        match part {
            0 => said.matrix = Some(VideoMatrix::Bt601),
            1 => said.range = Some(VideoRange::Full),
            2 => said.primaries = Some(StandardPrimaries::Rec709),
            _ => said.curve = Some(TransferFunction::Linear),
        }
        assert!(!said.is_automatic(), "one part said: {said:?}");
    }
}

#[test]
fn what_is_said_of_a_source_is_kept_with_that_source_and_no_other() {
    let mut project = Project::empty();
    project
        .import_source(video_source(1), TimelineFrameIndex::new(0))
        .unwrap();
    project
        .import_source(video_source(2), TimelineFrameIndex::new(90))
        .unwrap();

    assert!(project.set_source_colour(SourceMediaIdentifier::new(2), said_by_hand()));

    assert_eq!(
        project.source_colour(SourceMediaIdentifier::new(1)),
        Some(InputColourOverride::AUTOMATIC)
    );
    assert_eq!(
        project.source_colour(SourceMediaIdentifier::new(2)),
        Some(said_by_hand())
    );
    assert!(project.set_source_colour(SourceMediaIdentifier::new(2), InputColourOverride::AUTOMATIC));
    assert_eq!(
        project.source_colour(SourceMediaIdentifier::new(2)),
        Some(InputColourOverride::AUTOMATIC)
    );
}

#[test]
fn a_source_that_is_not_there_has_no_colours_and_cannot_be_told_any() {
    let mut project = Project::empty();
    assert_eq!(project.source_colour(SourceMediaIdentifier::new(7)), None);
    assert!(!project.set_source_colour(SourceMediaIdentifier::new(7), said_by_hand()));
}

#[test]
fn what_is_said_of_a_source_stays_said_when_the_file_is_found_again_somewhere_else() {
    // (The colours are the user's word about the file, not what was found out about it.)
    let mut project = Project::empty();
    project
        .import_source(video_source(1), TimelineFrameIndex::new(0))
        .unwrap();
    project.set_source_colour(SourceMediaIdentifier::new(1), said_by_hand());
    let moved = video_source(1);

    project
        .relink_source(SourceMediaIdentifier::new(1), moved.link.clone(), moved.properties)
        .unwrap();

    assert_eq!(
        project.source_colour(SourceMediaIdentifier::new(1)),
        Some(said_by_hand())
    );
}

#[test]
fn a_sound_has_no_colours_to_say_anything_of() {
    use crate::project::{SourceAudioProperties, SourceMediaKind};
    let mut project = Project::empty();
    project
        .import_source(
            SourceMedia::audio(
                SourceMediaIdentifier::new(5),
                SourceMediaLink::from_absolute_path("/sounds/song.wav".into()),
                SourceAudioProperties {
                    duration_in_microseconds: 1_000_000,
                },
            ),
            TimelineFrameIndex::new(0),
        )
        .unwrap();
    assert_eq!(
        project.source(SourceMediaIdentifier::new(5)).unwrap().kind(),
        SourceMediaKind::Audio
    );

    assert!(!project.set_source_colour(SourceMediaIdentifier::new(5), said_by_hand()));

    assert_eq!(
        project.source_colour(SourceMediaIdentifier::new(5)),
        Some(InputColourOverride::AUTOMATIC)
    );
}
