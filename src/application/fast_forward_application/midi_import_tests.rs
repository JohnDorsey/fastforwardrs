// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Importing a MIDI file (phase 13, piece 13.6; docs/engine-phase-13-midi-design.md, question 4): the same two items of the File menu that import a video, a sound or a picture, choosing a file whose
//! name ends in `.mid` or `.midi`. Each track of it that has notes becomes a MIDI clip on a track of its own at the current frame, as one step of undo; **no ffmpeg is needed**; the file is kept as
//! the item says (linked, or included in the project file); and one that is not music is refused and changes nothing.

use std::fs;
use std::path::PathBuf;

use super::included_files_tests::{Fixture, dialog_showing};
use super::undo_tests::{redo, undo};
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::project::{ResourceKind, TimelineFrameIndex};
use crate::test_support::a_midi_file_with_tracks;

/// A MIDI file of two tracks with notes (and the tempo track before them, which has none) in the folder of the fixture.
fn a_midi_file_in(fixture: &Fixture, name: &str) -> PathBuf {
    let path = fixture.path("media").join(name);
    fs::write(&path, &*a_midi_file_with_tracks(&[&[(0, 500, 60)], &[(0, 1500, 64)]])).unwrap();
    path
}

fn the_midi_clips(fixture: &Fixture) -> Vec<(u64, u16)> {
    fixture
        .window
        .application()
        .project()
        .tracks()
        .iter()
        .flat_map(|track| track.placements())
        .filter_map(|placement| {
            placement
                .clip
                .as_midi()
                .map(|midi| (placement.first_timeline_frame.value(), midi.midi_track()))
        })
        .collect()
}

#[test]
fn both_items_of_the_menu_offer_midi_files_among_the_kinds_of_file_they_ask_for() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    fixture.chooser.will_answer(None);

    fixture.window.do_this(Command::LinkAndImportFile);

    let asked = fixture.chooser.questions_asked();
    let (name, extensions) = &asked[0].kinds[0];
    assert_eq!(name, "Video, sound, picture and MIDI files");
    assert!(extensions.iter().any(|extension| extension == "mid"));
    assert!(extensions.iter().any(|extension| extension == "midi"));
    assert!(
        asked[0]
            .kinds
            .iter()
            .any(|(name, extensions)| name == "MIDI files" && extensions == &["mid", "midi"]),
        "{:?}",
        asked[0].kinds
    );
}

#[test]
fn a_midi_file_that_is_linked_and_imported_becomes_a_clip_for_each_track_with_notes_at_the_current_frame_in_one_step_of_undo()
 {
    let mut fixture = Fixture::new();
    let path = a_midi_file_in(&fixture, "tune.mid");
    fixture.chooser.will_answer(Some(path));
    fixture.window.do_this(Command::SeekTo(TimelineFrameIndex::new(40)));

    fixture.window.do_this(Command::LinkAndImportFile);

    // (The conductor's track is the first of the file, so the tracks with notes are the second and the third.)
    let mut clips = the_midi_clips(&fixture);
    clips.sort_by_key(|(_, track)| *track);
    assert_eq!(clips, vec![(40, 1), (40, 2)]);
    let project = fixture.window.application().project();
    let resource = project.resources().get("tune.mid").expect("the file is a resource");
    assert_eq!(resource.kind(), ResourceKind::Midi);
    assert!(!resource.is_included(), "linked: it stays the file it is");
    undo(&mut fixture.window);
    assert_eq!(the_midi_clips(&fixture), Vec::new(), "one step takes all of it away");
    assert!(
        fixture
            .window
            .application()
            .project()
            .resources()
            .get("tune.mid")
            .is_none()
    );
    redo(&mut fixture.window);
    assert_eq!(the_midi_clips(&fixture).len(), 2);
}

#[test]
fn a_midi_file_that_is_embedded_and_imported_is_included_in_the_project_file() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    let path = a_midi_file_in(&fixture, "tune.midi");
    fixture.chooser.will_answer(Some(path));

    fixture.window.do_this(Command::EmbedAndImportFile);

    assert_eq!(
        the_midi_clips(&fixture).len(),
        2,
        "a file that ends in .midi is one too"
    );
    let resource = fixture
        .window
        .application()
        .project()
        .resources()
        .get("tune.midi")
        .expect("the file is a resource")
        .clone();
    assert!(resource.is_included(), "embedded: it is in the project file");
}

#[test]
fn a_file_with_the_ending_of_a_midi_file_that_is_not_music_is_refused_and_nothing_changes() {
    let mut fixture = Fixture::new();
    let path = fixture.path("media").join("words.mid");
    fs::write(&path, b"these are words").unwrap();
    fixture.chooser.will_answer(Some(path));
    let before = fixture.window.application().project().clone();

    fixture.window.do_this(Command::LinkAndImportFile);

    assert!(
        matches!(dialog_showing(&fixture.window), Some(Dialog::Message { title, .. }) if title == "This is not a MIDI file that can be used"),
        "{:?}",
        dialog_showing(&fixture.window)
    );
    assert_eq!(*fixture.window.application().project(), before);
}

#[test]
fn a_midi_file_is_put_on_the_timeline_whether_or_not_ffmpeg_is_there() {
    let mut fixture = Fixture::new();
    let path = a_midi_file_in(&fixture, "tune.mid");
    fixture.chooser.will_answer(Some(path));

    fixture.window.do_this(Command::LinkAndImportFile);

    // (No background work was started for it: nothing is opened by ffprobe.)
    assert!(
        fixture.window.application().sources.opening_purposes_started.is_empty(),
        "{:?}",
        fixture.window.application().sources.opening_purposes_started
    );
    assert_eq!(the_midi_clips(&fixture).len(), 2);
}

#[test]
fn the_notes_of_an_imported_midi_clip_are_drawn_inside_it_and_its_label_says_it_has_no_synthesizer() {
    use egui::Shape;
    let mut fixture = Fixture::new();
    let path = a_midi_file_in(&fixture, "tune.mid");
    let notes_before = fixture
        .window
        .shapes_drawn()
        .into_iter()
        .map(vertices_of_the_colour_of_notes)
        .sum::<usize>();
    fixture.chooser.will_answer(Some(path));

    fixture.window.do_this(Command::LinkAndImportFile);

    let note_vertices: usize = fixture
        .window
        .shapes_drawn()
        .into_iter()
        .map(vertices_of_the_colour_of_notes)
        .sum();
    // (Four corners for each bar: one note in each of the two clips.)
    assert_eq!(notes_before, 0);
    assert_eq!(note_vertices, 8, "a bar for each of the two notes");
    let words_drawn: Vec<String> = fixture
        .window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect();
    assert!(
        words_drawn
            .iter()
            .any(|words| words.contains("tune.mid") && words.contains("no synthesizer")),
        "{words_drawn:?}"
    );
}

fn vertices_of_the_colour_of_notes(shape: &egui::Shape) -> usize {
    match shape {
        egui::Shape::Mesh(mesh) => mesh
            .vertices
            .iter()
            .filter(|vertex| vertex.color == crate::application::theme::MIDI_CLIP_NOTE)
            .count(),
        _ => 0,
    }
}

#[test]
fn a_midi_file_whose_name_ends_in_capital_letters_is_a_midi_file_too() {
    let mut fixture = Fixture::new();
    let path = a_midi_file_in(&fixture, "TUNE.MID");
    fixture.chooser.will_answer(Some(path));

    fixture.window.do_this(Command::LinkAndImportFile);

    assert_eq!(the_midi_clips(&fixture).len(), 2);
}

#[test]
fn embedding_a_midi_file_where_the_project_file_cannot_hold_it_links_it_and_says_so() {
    let mut fixture = Fixture::new();
    let path = a_midi_file_in(&fixture, "tune.mid");

    // (Not through the menu, which first asks for a project file that can: the programmatic way puts the file on the timeline and asks as for any file that is included.)
    fixture.window.application_mut().embed_and_import_file_from(path);
    fixture.window.settle();

    assert_eq!(the_midi_clips(&fixture).len(), 2);
    let resource = fixture
        .window
        .application()
        .project()
        .resources()
        .get("tune.mid")
        .unwrap()
        .clone();
    assert!(
        !resource.is_included(),
        "linked: there is no project file that can hold it"
    );
    assert!(
        matches!(dialog_showing(&fixture.window), Some(Dialog::Message { title, .. }) if title == "The MIDI file is linked"),
        "{:?}",
        dialog_showing(&fixture.window)
    );
}

/// The tops of the bars of notes that were drawn (each bar is a rectangle of four vertices of the colour of notes), one for each bar.
fn tops_of_the_bars_of_notes(window: &super::test_window::Window) -> Vec<f32> {
    let mut vertices: Vec<f32> = Vec::new();
    for shape in window.shapes_drawn() {
        if let egui::Shape::Mesh(mesh) = shape {
            vertices.extend(
                mesh.vertices
                    .iter()
                    .filter(|vertex| vertex.color == crate::application::theme::MIDI_CLIP_NOTE)
                    .map(|vertex| vertex.pos.y),
            );
        }
    }
    vertices
        .chunks(4)
        .map(|corners| corners.iter().copied().fold(f32::INFINITY, f32::min))
        .collect()
}

#[test]
fn the_bars_are_placed_by_the_keys_of_their_track_and_a_clip_that_is_cut_draws_only_the_notes_that_begin_in_it() {
    let mut fixture = Fixture::new();
    let path = fixture.path("media").join("two_keys.mid");
    // (The first track has two keys, 60 and 72, and the second one, 64.)
    fs::write(
        &path,
        &*a_midi_file_with_tracks(&[&[(0, 500, 60), (500, 1000, 72)], &[(0, 1500, 64)]]),
    )
    .unwrap();
    fixture.chooser.will_answer(Some(path));

    fixture.window.do_this(Command::LinkAndImportFile);

    let tops = tops_of_the_bars_of_notes(&fixture.window);
    assert_eq!(tops.len(), 3, "three notes");
    let mut distinct = tops.clone();
    distinct.sort_by(f32::total_cmp);
    distinct.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    assert_eq!(
        distinct.len(),
        3,
        "the two keys of the first track are two heights, and the second track's key is a third: {tops:?}"
    );

    // (Cut at frame 20, 0.67 s: the left pieces have the notes that begin in them (the first track's two, the second's one), the right pieces have none that begin in them, whatever the notes before
    // them are.)
    fixture.window.do_this(Command::SplitAt {
        frame: TimelineFrameIndex::new(20),
        only: None,
    });
    assert_eq!(
        tops_of_the_bars_of_notes(&fixture.window).len(),
        3,
        "a note that began before a piece is not drawn in it"
    );
}
