// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Importing a Standard MIDI File into the project (phase 13, piece 13.6): the file becomes a resource and **each track of it that has notes becomes a MIDI clip of all of it on a track of its
//! own**, from the frame it is imported at; a file that is not music or has no notes changes nothing.

use std::path::PathBuf;
use std::sync::Arc;

use super::*;
use crate::project::tests::{frame, frame_rate};
use crate::project::{ClipId, ResourceKind, SourceMediaLink, TrackId};
use crate::test_support::{a_midi_file_with_notes, a_midi_file_with_tracks};

fn link() -> SourceMediaLink {
    SourceMediaLink::from_absolute_path(PathBuf::from("/music/tune.mid"))
}

fn at_30() -> Project {
    Project::empty().with_frame_rate(Some(frame_rate(30)))
}

/// The MIDI clip with the clip id `clip`, and the place of its track in the file.
fn the_midi_clip(project: &Project, clip: ClipId) -> MidiClip {
    project
        .placement_of(clip)
        .and_then(|placement| placement.clip.as_midi().cloned())
        .expect("a MIDI clip")
}

fn the_track_of(project: &Project, clip: ClipId) -> TrackId {
    project
        .tracks()
        .iter()
        .find(|track| track.placement_of(clip).is_some())
        .expect("a track has it")
        .id()
}

#[test]
fn each_track_of_the_file_that_has_notes_becomes_a_clip_of_all_of_it_on_a_track_of_its_own_and_the_track_of_the_tempo_alone_does_not()
 {
    let mut project = at_30();
    // (Three tracks of the file after the conductor's: the first two have notes, the third has none. The notes end at 2.5 s: 75 frames.)
    let file = a_midi_file_with_tracks(&[&[(0, 500, 60)], &[(1000, 2500, 64)], &[]]);

    let import = project.import_a_midi_file("tune.mid", file, link(), frame(12)).unwrap();

    assert_eq!(import.resource, "tune.mid");
    assert_eq!(
        import.clips.len(),
        2,
        "the conductor's track and the empty one are left out"
    );
    let clips: Vec<MidiClip> = import.clips.iter().map(|clip| the_midi_clip(&project, *clip)).collect();
    // (The places in the file: the conductor's track is 0, so these are 1 and 2.)
    assert_eq!(clips.iter().map(MidiClip::midi_track).collect::<Vec<_>>(), vec![1, 2]);
    for (clip_id, clip) in import.clips.iter().zip(&clips) {
        assert_eq!(clip.resource(), "tune.mid");
        assert_eq!(clip.first_source_frame().value(), 0);
        assert_eq!(
            clip.end_source_frame().value(),
            75,
            "all of it: 2.5 s at 30 frames a second"
        );
        let placement = project.placement_of(*clip_id).unwrap();
        assert_eq!(
            placement.first_timeline_frame.value(),
            12,
            "at the frame it was imported at"
        );
    }
    let tracks: Vec<TrackId> = import.clips.iter().map(|clip| the_track_of(&project, *clip)).collect();
    assert_ne!(tracks[0], tracks[1], "each on a track of its own");
    let resource = project.resources().get("tune.mid").unwrap();
    assert_eq!(resource.kind(), ResourceKind::Midi);
}

#[test]
fn the_last_clip_goes_on_the_front_most_track_that_has_nothing_on_it_and_the_others_on_new_tracks_in_front_so_that_they_lie_in_the_order_of_the_file()
 {
    let mut project = at_30();
    let empty = project.tracks()[0].id();
    let tracks_before = project.tracks().len();
    let file = a_midi_file_with_tracks(&[&[(0, 500, 60)], &[(0, 500, 62)], &[(0, 500, 64)]]);

    let import = project.import_a_midi_file("tune.mid", file, link(), frame(0)).unwrap();

    assert_eq!(the_track_of(&project, import.clips[2]), empty);
    assert_eq!(project.tracks().len(), tracks_before + 2);
    let place_of = |clip: ClipId| {
        let track = the_track_of(&project, clip);
        project
            .tracks()
            .iter()
            .position(|candidate| candidate.id() == track)
            .unwrap()
    };
    assert!(
        place_of(import.clips[0]) < place_of(import.clips[1]) && place_of(import.clips[1]) < place_of(import.clips[2]),
        "from the top: the tracks of the file in their order"
    );
    let notes: Vec<u8> = import
        .clips
        .iter()
        .map(|clip| u8::try_from(the_midi_clip(&project, *clip).midi_track()).unwrap())
        .collect();
    assert_eq!(
        notes,
        vec![1, 2, 3],
        "the clips are in the order of the tracks of the file"
    );
}

#[test]
fn a_clip_that_would_not_fit_on_the_empty_track_goes_on_a_new_one_and_nothing_is_overwritten() {
    let mut project = at_30();
    let first = project
        .import_a_midi_file("one.mid", a_midi_file_with_notes(&[(0, 2000, 60)]), link(), frame(0))
        .unwrap();
    let before = project.clone();

    // (The same frames again: the first import's track is not empty, so the second goes on another.)
    let second = project
        .import_a_midi_file("two.mid", a_midi_file_with_notes(&[(0, 2000, 62)]), link(), frame(0))
        .unwrap();

    assert_ne!(
        the_track_of(&project, first.clips[0]),
        the_track_of(&project, second.clips[0])
    );
    assert!(project.placement_of(first.clips[0]).is_some());
    assert_eq!(before.tracks().len() + 1, project.tracks().len());
}

#[test]
fn a_file_that_is_not_music_or_has_no_notes_is_refused_and_the_project_is_as_it_was() {
    let mut project = at_30();
    let before = project.clone();

    let not_music = project.import_a_midi_file("words.mid", Arc::from(&b"these are words"[..]), link(), frame(0));
    let no_notes = project.import_a_midi_file("silence.mid", a_midi_file_with_tracks(&[&[], &[]]), link(), frame(0));

    assert!(matches!(not_music, Err(MidiImportError::NotMusic(_))), "{not_music:?}");
    assert_eq!(no_notes, Err(MidiImportError::NoNotes));
    assert_eq!(project, before);
}

#[test]
fn the_length_of_a_clip_is_counted_in_the_frames_of_the_project_and_rounded_up() {
    // (2.5 s: at 24 frames a second 60; at 29.97 a second 74.925, which is 75; a note of 10 ms is one frame.)
    for ((numerator, denominator), seconds_in_ms, frames) in
        [((24, 1), 2500, 60), ((30_000, 1001), 2500, 75), ((30, 1), 10, 1)]
    {
        let mut project =
            Project::empty().with_frame_rate(Some(crate::project::FrameRate::new(numerator, denominator).unwrap()));

        let import = project
            .import_a_midi_file(
                "tune.mid",
                a_midi_file_with_notes(&[(0, seconds_in_ms, 60)]),
                link(),
                frame(0),
            )
            .unwrap();

        assert_eq!(
            the_midi_clip(&project, import.clips[0]).end_source_frame().value(),
            frames,
            "{numerator}/{denominator}"
        );
    }
}

#[test]
fn the_resource_is_named_by_the_wanted_name_and_a_file_with_the_same_bytes_is_one_resource() {
    let mut project = at_30();
    let bytes = a_midi_file_with_notes(&[(0, 500, 60)]);

    let first = project
        .import_a_midi_file("tune.mid", Arc::clone(&bytes), link(), frame(0))
        .unwrap();
    let second = project
        .import_a_midi_file("again.mid", bytes, link(), frame(100))
        .unwrap();

    assert_eq!(first.resource, "tune.mid");
    assert_eq!(
        second.resource, "tune.mid",
        "the same bytes are the resource that is already there"
    );
    assert_eq!(project.resources().len(), 1);
}
