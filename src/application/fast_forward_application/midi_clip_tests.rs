// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The MIDI clip in the window (phase 13, piece 13.3): it is on the timeline, can be selected, shows its properties (its file, its speed, its volume and its mute switch) in the panel, and
//! what is changed there is one step that undo takes back.

use std::path::PathBuf;

use egui_kittest::kittest::Queryable;

use super::test_window::{Window, project_of};
use super::undo_tests::undo;
use crate::application::commands::Command;
use crate::project::{
    ClipId, Gain, MidiClip, Project, ResourceKind, SourceFrameIndex, SourceMediaLink, SpeedFactor, TimelineFrameIndex,
};
use crate::test_support::a_midi_file;

/// A project of 90 frames of video with a track in front of it that has a MIDI clip of 60 frames from frame 0, and the file it plays.
fn project_with_music() -> (Project, ClipId) {
    let mut project = project_of(90);
    project
        .add_resource(
            "tune.mid",
            ResourceKind::Midi,
            a_midi_file(),
            SourceMediaLink::from_absolute_path(PathBuf::from("/music/tune.mid")),
        )
        .unwrap();
    let front = project.add_track_in_front();
    let clip = project
        .try_insert_midi_clip(
            front,
            TimelineFrameIndex::new(0),
            MidiClip::new(
                "tune.mid",
                0,
                SourceFrameIndex::new(0),
                SourceFrameIndex::new(60),
                SpeedFactor::NORMAL,
            )
            .unwrap(),
        )
        .unwrap();
    (project, clip)
}

fn the_midi_clip(window: &Window, clip: ClipId) -> MidiClip {
    window
        .application()
        .project()
        .placement_of(clip)
        .unwrap()
        .clip
        .as_midi()
        .expect("it is a MIDI clip")
        .clone()
}

/// A window that shows the project with the MIDI clip, which is selected by a click on it (as a person selects it), with nothing unsaved.
fn window_with_the_midi_clip_selected() -> (Window, ClipId) {
    let (project, clip) = project_with_music();
    let position = project
        .tracks()
        .iter()
        .position(|track| track.placements().any(|placement| placement.id == clip))
        .unwrap();
    let mut window = Window::showing(project);
    window.application_mut().note_everything_is_saved();
    window.settle();
    let middle = window.middle_of_the_strip_at(position, 20);
    window.click(middle);
    window.settle();
    (window, clip)
}

#[test]
fn a_midi_clip_is_selected_by_a_click_and_its_box_is_in_the_panel_with_its_file_and_what_can_be_changed() {
    let (window, clip) = window_with_the_midi_clip_selected();

    assert_eq!(window.application().selected_clip(), Some(clip));
    assert!(
        window.harness.query_by_label("MIDI clip").is_some(),
        "the box of its properties"
    );
    assert!(
        window.harness.query_by_label("tune.mid").is_some(),
        "the name of its file"
    );
    assert!(window.harness.query_by_label("Mute").is_some(), "its mute switch");
    assert!(
        !window.application().has_unsaved_changes(),
        "selecting a clip changes nothing"
    );
}

#[test]
fn the_speed_of_a_midi_clip_is_changed_by_the_command_and_it_lasts_as_long_as_that_says_and_undo_takes_it_back() {
    let (mut window, clip) = window_with_the_midi_clip_selected();

    window.do_this(Command::SetClipSpeed {
        clip,
        speed_factor: SpeedFactor::new(2).unwrap(),
    });

    assert_eq!(the_midi_clip(&window, clip).speed(), SpeedFactor::new(2).unwrap());
    let placement = window.application().project().placement_of(clip).unwrap();
    assert_eq!(
        placement.timeline_frame_count(),
        30,
        "sixty frames of music at twice the speed"
    );
    assert!(window.application().has_unsaved_changes());

    undo(&mut window);

    assert_eq!(the_midi_clip(&window, clip).speed(), SpeedFactor::NORMAL);
    assert_eq!(
        window
            .application()
            .project()
            .placement_of(clip)
            .unwrap()
            .timeline_frame_count(),
        60
    );
}

#[test]
fn giving_a_midi_clip_the_speed_it_has_is_no_edit_and_makes_no_step_to_undo() {
    let (mut window, clip) = window_with_the_midi_clip_selected();
    window.do_this(Command::SetClipSpeed {
        clip,
        speed_factor: SpeedFactor::new(3).unwrap(),
    });
    window.application_mut().note_everything_is_saved();

    window.do_this(Command::SetClipSpeed {
        clip,
        speed_factor: SpeedFactor::new(3).unwrap(),
    });

    assert!(!window.application().has_unsaved_changes(), "nothing was changed");
    undo(&mut window);
    assert_eq!(
        the_midi_clip(&window, clip).speed(),
        SpeedFactor::NORMAL,
        "the one step that was made is the one taken back"
    );
}

#[test]
fn the_volume_and_the_mute_switch_of_a_midi_clip_are_changed_by_the_commands_and_undone_one_by_one() {
    let (mut window, clip) = window_with_the_midi_clip_selected();

    window.do_this(Command::SetClipGain {
        clip,
        gain: Gain::from_decibels(-9.0),
    });
    window.do_this(Command::SetClipMuted { clip, muted: true });

    let midi = the_midi_clip(&window, clip);
    assert_eq!(midi.gain(), Gain::from_decibels(-9.0));
    assert!(midi.is_muted());
    undo(&mut window);
    assert!(!the_midi_clip(&window, clip).is_muted());
    assert_eq!(the_midi_clip(&window, clip).gain(), Gain::from_decibels(-9.0));
    undo(&mut window);
    assert_eq!(the_midi_clip(&window, clip).gain(), Gain::UNCHANGED);
}

#[test]
fn the_mute_switch_in_the_panel_mutes_a_midi_clip() {
    let (mut window, clip) = window_with_the_midi_clip_selected();

    window.harness.get_by_label("Mute").click();
    window.settle();

    assert!(the_midi_clip(&window, clip).is_muted());
    assert!(window.application().has_unsaved_changes());
}
