// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of effects in the window: the plus button of the properties panel that puts them on, the box each has with the controls of
//! its values, the menu of that box that takes it off, and what the commands for them do (as edits that are undone, and as values that
//! are changed at the playhead like any other).

use egui_kittest::kittest::{NodeT, Queryable};

use super::keyframe_tests::{keyframes_of, window_with_a_video};
use super::picture_tests::picture_source;
use super::preview_layer_tests::select;
use super::test_window::{Window, project_with_a_video_and_a_sound};
use super::undo_tests::{redo, undo};
use crate::application::commands::Command;
use crate::application::naming::what_an_effect_is_called;
use crate::application::properties_panel::{
    ADD_EFFECT_LABEL, DELETE_EFFECT_LABEL, KeyframeButton, SWITCH_EFFECT_OFF_LABEL, SWITCHED_OFF_NOTE,
    keyframe_button_label,
};
use crate::application::selection::{SelectionChange, SelectionItem};
use crate::project::{
    AnimationTarget, ClipId, Effect, EffectInstanceId, EffectKind, PropertyId, PropertyValue, TextObject,
    TimelineFrameIndex,
};
use crate::test_support::a_kind_of_effect_the_program_does_not_have;

const HUE: PropertyId = crate::test_support::HUE_SHIFT_AS_THE_FIRST_EFFECT;
/// What the slider of the value of the hue shift is called.
const TURN: &str = "Turn";

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

fn turn(degrees: f32) -> PropertyValue {
    PropertyValue::Number(degrees)
}

fn clip(id: ClipId) -> AnimationTarget {
    AnimationTarget::Clip(id)
}

/// A window with a video of 300 frames that is selected, with nothing unsaved and nothing to undo, and its clip.
fn window_with_the_video_selected() -> (Window, ClipId) {
    let (mut window, video) = window_with_a_video();
    select(&mut window, video);
    (window, video)
}

fn has_the_effect(window: &Window, target: AnimationTarget, instance: EffectInstanceId) -> bool {
    window.application().project().has_the_effect(target, instance)
}

fn value_at(window: &Window, target: AnimationTarget, at: u64) -> Option<PropertyValue> {
    window.application().project().property_value_at(target, HUE, frame(at))
}

fn is_shown(window: &Window, label: &str) -> bool {
    window.harness.query_by_label(label).is_some()
}

fn the_plus_button(window: &Window) -> egui::Rect {
    window.rectangle_of(ADD_EFFECT_LABEL)
}

/// Opens the menu of the plus button and the smaller menu of the effects of the program (the menu has one for each place effects come from).
fn click_the_plus_button(window: &mut Window) {
    let middle = the_plus_button(window).center();
    window.click(middle);
    window.open_the_smaller_menu("Built-in");
}

/// Puts the hue shift on `target` with the menu of the plus button, as a person does. (The menu is under the boxes of the effects that are
/// on, and its item has the name of the kind of effect, which a box that has no number is called as well: it is the last of those.)
fn put_the_hue_shift_on_with_the_plus_button(window: &mut Window) {
    click_the_plus_button(window);
    let name = what_an_effect_is_called(EffectKind::HUE_SHIFT);
    let item = window
        .rectangle_of_the_nth(&name, window.how_many_are_called(&name) - 1)
        .center();
    window.click(item);
}

fn the_stopwatch_of_the_hue_shift() -> String {
    keyframe_button_label(HUE.name(), KeyframeButton::Stopwatch, false)
}

// --- the plus button ---

#[test]
fn what_is_seen_has_a_plus_button_that_adds_an_effect_and_a_sound_has_none() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (window, _) = window_with_the_video_selected();
    assert!(is_shown(&window, ADD_EFFECT_LABEL), "a video");

    let mut project = project_with_a_video_and_a_sound(100, 10);
    let picture = project
        .import_source(picture_source(), frame(0))
        .unwrap()
        .clip
        .expect("a picture makes a clip");
    let text_track = project.add_track_in_front();
    let (text, _) = project
        .insert_text(text_track, frame(10), TextObject::default(), 50)
        .unwrap();
    let mut window = Window::showing(project);
    let sound = window.clip(2, 0);
    let video = window.clip(3, 0);
    for (clip, what, has_a_plus) in [
        (picture, "a picture", true),
        (text, "a text object", true),
        (video, "a video", true),
        (sound, "a sound", false),
    ] {
        select(&mut window, clip);
        assert_eq!(is_shown(&window, ADD_EFFECT_LABEL), has_a_plus, "{what}");
    }
}

#[test]
fn a_track_has_a_plus_button_and_so_does_nothing_that_is_not_selected_or_a_keyframe_crate() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, _) = window_with_a_video();
    assert!(!is_shown(&window, ADD_EFFECT_LABEL), "nothing is selected");

    window.select_the_track_at(0);
    assert!(is_shown(&window, ADD_EFFECT_LABEL), "a track");

    let track = window.application().project().tracks()[0].id();
    let front = window.application_mut().document.project.add_track_in_front();
    let a_crate = crate::test_support::make_a_crate(
        &mut window.application_mut().document.project,
        front,
        0,
        49,
        AnimationTarget::Track(front),
        PropertyId::Gain,
    );
    assert_ne!(front, track);
    window.settle();
    select(&mut window, a_crate);
    assert!(
        !is_shown(&window, ADD_EFFECT_LABEL),
        "a keyframe crate has no picture to change"
    );
}

#[test]
fn the_menu_of_the_plus_button_has_each_effect_that_the_clip_can_have_and_choosing_one_puts_it_on() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_the_video_selected();
    click_the_plus_button(&mut window);
    for kind in EffectKind::BUILT_IN {
        assert!(
            is_shown(&window, &what_an_effect_is_called(kind)),
            "the menu has {kind}: the effects of the table are all offered"
        );
    }
    assert!(
        !has_the_effect(&window, clip(video), EffectInstanceId::FIRST),
        "opening the menu changes nothing"
    );

    let item = window
        .rectangle_of(&what_an_effect_is_called(EffectKind::HUE_SHIFT))
        .center();
    window.click(item);

    assert!(has_the_effect(&window, clip(video), EffectInstanceId::FIRST));
    assert_eq!(value_at(&window, clip(video), 0), Some(turn(0.0)), "as it begins");
    assert!(window.application().has_unsaved_changes());
}

#[test]
fn the_effect_that_is_put_on_has_a_box_with_its_name_and_the_controls_of_its_values_under_the_box_of_the_clip() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_the_video_selected();
    assert!(!is_shown(&window, TURN), "no effect, no controls");
    let box_of_the_clip = window.rectangle_of("Video clip");

    put_the_hue_shift_on_with_the_plus_button(&mut window);

    assert!(has_the_effect(&window, clip(video), EffectInstanceId::FIRST));
    assert!(
        is_shown(&window, &what_an_effect_is_called(EffectKind::HUE_SHIFT)),
        "the box has its name"
    );
    assert!(is_shown(&window, TURN), "and the control of its value");
    assert!(
        window
            .rectangle_of(&what_an_effect_is_called(EffectKind::HUE_SHIFT))
            .top()
            > box_of_the_clip.top(),
        "under the box of the clip"
    );
    let slider = window
        .harness
        .get_by_label(TURN)
        .accesskit_node()
        .numeric_value()
        .unwrap();
    assert_eq!(slider, 0.0);
}

#[test]
fn the_plus_button_is_not_dimmed_by_an_effect_that_is_on_because_there_may_be_several_of_one_kind() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_the_video_selected();
    assert!(
        !window
            .harness
            .get_by_label(ADD_EFFECT_LABEL)
            .accesskit_node()
            .is_disabled()
    );

    put_the_hue_shift_on_with_the_plus_button(&mut window);

    assert!(
        !window
            .harness
            .get_by_label(ADD_EFFECT_LABEL)
            .accesskit_node()
            .is_disabled(),
        "another of the same kind can be put on"
    );
    put_the_hue_shift_on_with_the_plus_button(&mut window);
    assert_eq!(window.application().project().effects_of(clip(video)).unwrap().len(), 2);
}

#[test]
fn an_effect_is_put_on_a_track_from_the_panel_of_the_track_and_is_the_track_s_and_not_the_clip_s() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_a_video();
    window.select_the_track_at(0);
    let track = window.application().project().tracks()[0].id();

    put_the_hue_shift_on_with_the_plus_button(&mut window);

    assert!(has_the_effect(
        &window,
        AnimationTarget::Track(track),
        EffectInstanceId::FIRST
    ));
    assert!(!has_the_effect(&window, clip(video), EffectInstanceId::FIRST));
    window.set_slider(TURN, 45.0);
    assert_eq!(value_at(&window, AnimationTarget::Track(track), 10), Some(turn(45.0)));
}

// --- the value ---

#[test]
fn the_slider_of_a_value_that_is_not_animated_changes_it_for_the_whole_clip_and_is_one_step_of_undo() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_the_video_selected();
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HUE_SHIFT,
    });
    window.move_current_frame_to(120);

    window.set_slider(TURN, 90.0);

    for at in [0, 120, 299] {
        assert_eq!(value_at(&window, clip(video), at), Some(turn(90.0)), "frame {at}");
    }
    assert_eq!(
        keyframes_of(&window, video, HUE),
        Vec::<u64>::new(),
        "it is not animated"
    );
    undo(&mut window);
    assert_eq!(value_at(&window, clip(video), 0), Some(turn(0.0)));
    redo(&mut window);
    assert_eq!(value_at(&window, clip(video), 0), Some(turn(90.0)));
}

#[test]
fn the_slider_can_be_given_more_than_half_a_turn_either_way_and_keeps_what_it_is_given() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_the_video_selected();
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HUE_SHIFT,
    });

    window.set_slider(TURN, 450.0);
    assert_eq!(value_at(&window, clip(video), 0), Some(turn(450.0)));
    window.set_slider(TURN, -270.0);
    assert_eq!(value_at(&window, clip(video), 0), Some(turn(-270.0)));
}

#[test]
fn the_stopwatch_of_the_value_starts_its_keyframes_and_the_slider_then_changes_the_keyframe_at_the_playhead() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_the_video_selected();
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HUE_SHIFT,
    });
    window.set_slider(TURN, 30.0);
    window.move_current_frame_to(40);
    let middle_of_the_row = window.rectangle_of(TURN).center();
    window.move_pointer_to(middle_of_the_row);
    let stopwatch = window.rectangle_of(&the_stopwatch_of_the_hue_shift()).center();

    window.click(stopwatch);

    assert_eq!(
        keyframes_of(&window, video, HUE),
        vec![40],
        "a keyframe where the playhead is"
    );
    assert_eq!(
        value_at(&window, clip(video), 40),
        Some(turn(30.0)),
        "with what the value was"
    );
    window.move_current_frame_to(100);
    window.set_slider(TURN, 210.0);
    assert_eq!(
        keyframes_of(&window, video, HUE),
        vec![40, 100],
        "a second keyframe where the playhead is now"
    );
    assert_eq!(
        value_at(&window, clip(video), 70),
        Some(turn(120.0)),
        "half way between the two"
    );
    assert_eq!(
        value_at(&window, clip(video), 250),
        Some(turn(210.0)),
        "and after the last, what it says"
    );
}

#[test]
fn the_slider_of_an_animated_value_shows_what_it_is_at_the_playhead_and_is_dimmed_beyond_the_clip() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    // The video lasts 100 frames and a sound in front of it 300, so that the playhead can be past the video.
    let mut window = Window::showing(project_with_a_video_and_a_sound(100, 10));
    let video = window.clip(1, 0);
    select(&mut window, video);
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HUE_SHIFT,
    });
    for (at, degrees) in [(10, 0.0), (90, 80.0)] {
        window.move_current_frame_to(at);
        window.do_this(Command::AddKeyframe {
            target: clip(video),
            property: HUE,
            frame: frame(at),
        });
        window.do_this(Command::SetEffectValue {
            target: clip(video),
            property: HUE,
            value: turn(degrees),
        });
    }

    window.move_current_frame_to(50);
    assert_eq!(
        window.harness.get_by_label(TURN).accesskit_node().numeric_value(),
        Some(40.0),
        "half way"
    );
    assert!(!window.harness.get_by_label(TURN).accesskit_node().is_disabled());

    window.move_current_frame_to(200);
    assert!(
        window.harness.get_by_label(TURN).accesskit_node().is_disabled(),
        "beyond the clip it cannot be changed: there is no frame of it to put a keyframe at"
    );
    window.do_this(Command::SetEffectValue {
        target: clip(video),
        property: HUE,
        value: turn(5.0),
    });
    assert_eq!(keyframes_of(&window, video, HUE), vec![10, 90], "no keyframe was made");
}

// --- taking it off ---

#[test]
fn the_menu_of_the_box_deletes_the_effect_with_its_keyframes_and_undo_brings_them_all_back() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_the_video_selected();
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HUE_SHIFT,
    });
    for (at, degrees) in [(20, 10.0), (200, 100.0)] {
        window.move_current_frame_to(at);
        window.do_this(Command::AddKeyframe {
            target: clip(video),
            property: HUE,
            frame: frame(at),
        });
        window.do_this(Command::SetEffectValue {
            target: clip(video),
            property: HUE,
            value: turn(degrees),
        });
    }
    window.application_mut().note_everything_is_saved();
    let heading = window
        .rectangle_of(&what_an_effect_is_called(EffectKind::HUE_SHIFT))
        .center();

    window.right_click(heading);
    // (The words themselves, and not the constant that holds them: they are what a person reads.)
    assert_eq!(DELETE_EFFECT_LABEL, "Delete effect");
    assert!(is_shown(&window, "Delete effect"), "the menu of the box");
    assert!(
        !is_shown(&window, "Take this effect off"),
        "it is not called what it used to be called"
    );
    assert!(
        has_the_effect(&window, clip(video), EffectInstanceId::FIRST),
        "opening it deletes nothing"
    );
    let item = window.rectangle_of("Delete effect").center();
    window.click(item);

    assert!(!has_the_effect(&window, clip(video), EffectInstanceId::FIRST));
    assert_eq!(keyframes_of(&window, video, HUE), Vec::<u64>::new());
    assert!(!is_shown(&window, TURN), "the box is gone");
    assert!(window.application().has_unsaved_changes());

    undo(&mut window);
    assert!(has_the_effect(&window, clip(video), EffectInstanceId::FIRST));
    assert_eq!(keyframes_of(&window, video, HUE), vec![20, 200], "with its keyframes");
    assert_eq!(value_at(&window, clip(video), 110), Some(turn(55.0)));
    assert!(is_shown(&window, TURN), "and its box");
}

#[test]
fn deleting_an_effect_leaves_what_was_selected_that_is_still_there_and_forgets_the_keyframes_that_went() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_the_video_selected();
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HUE_SHIFT,
    });
    window.move_current_frame_to(20);
    window.do_this(Command::AddKeyframe {
        target: clip(video),
        property: HUE,
        frame: frame(20),
    });
    let keyframe = window
        .application()
        .project()
        .located_racks()
        .find(|located| located.rack.property() == HUE)
        .expect("a rack of the hue shift")
        .rack
        .keyframes()[0]
        .id();
    window.do_this(Command::SelectItems {
        items: vec![SelectionItem::Keyframe(keyframe), SelectionItem::Clip(video)],
        change: SelectionChange::Replace,
    });
    assert!(
        window
            .application()
            .selected_items()
            .contains(&SelectionItem::Keyframe(keyframe))
    );

    // (At once: the window forgets what is gone from the selection in every frame too, so one frame later this would be so
    // whether or not taking the effect off did it.)
    window.do_this_and_draw_nothing(Command::RemoveEffect {
        target: clip(video),
        instance: EffectInstanceId::FIRST,
    });

    let selected = window.application().selected_items();
    assert!(
        !selected.contains(&SelectionItem::Keyframe(keyframe)),
        "the keyframe is gone: {selected:?}"
    );
    assert!(
        selected.contains(&SelectionItem::Clip(video)),
        "the clip is not: {selected:?}"
    );
}

// --- the commands ---

#[test]
fn putting_an_effect_on_and_changing_its_value_and_deleting_it_are_edits_that_are_undone_one_at_a_time() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_a_video();
    assert!(!window.application().can_undo());

    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HUE_SHIFT,
    });
    window.do_this(Command::SetEffectValue {
        target: clip(video),
        property: HUE,
        value: turn(60.0),
    });
    window.do_this(Command::RemoveEffect {
        target: clip(video),
        instance: EffectInstanceId::FIRST,
    });

    undo(&mut window);
    assert_eq!(
        value_at(&window, clip(video), 0),
        Some(turn(60.0)),
        "taken off is taken back"
    );
    undo(&mut window);
    assert_eq!(
        value_at(&window, clip(video), 0),
        Some(turn(0.0)),
        "the value is taken back"
    );
    undo(&mut window);
    assert!(
        !has_the_effect(&window, clip(video), EffectInstanceId::FIRST),
        "and putting it on"
    );
    assert!(!window.application().can_undo());
}

#[test]
fn commands_that_cannot_be_done_leave_the_project_as_it_is_and_are_not_edits_to_undo() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_a_video();
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HUE_SHIFT,
    });
    window.application_mut().note_everything_is_saved();

    // Taking off one that is not on (of the clip, which has another of its kind, and of the track, which has none), and a value of an effect
    // that the track does not have.
    window.do_this(Command::RemoveEffect {
        target: clip(video),
        instance: EffectInstanceId::new(1),
    });
    let track = window.application().project().tracks()[0].id();
    window.do_this(Command::RemoveEffect {
        target: AnimationTarget::Track(track),
        instance: EffectInstanceId::FIRST,
    });
    window.do_this(Command::SetEffectValue {
        target: AnimationTarget::Track(track),
        property: HUE,
        value: turn(1.0),
    });

    assert_eq!(value_at(&window, clip(video), 0), Some(turn(0.0)));
    assert!(!has_the_effect(
        &window,
        AnimationTarget::Track(track),
        EffectInstanceId::FIRST
    ));
    assert!(!window.application().has_unsaved_changes(), "nothing is unsaved");
    undo(&mut window);
    assert!(
        !has_the_effect(&window, clip(video), EffectInstanceId::FIRST),
        "the one step to undo is putting the effect on: the others made no step"
    );
    assert!(!window.application().can_undo());
}

#[test]
fn a_project_with_effects_is_saved_and_opened_again_with_them() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_the_video_selected();
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HUE_SHIFT,
    });
    window.set_slider(TURN, 75.0);
    let project_before = window.application().project().clone();

    let text = crate::project_file::project_to_json_text(&project_before, std::path::Path::new("/projects")).unwrap();
    let opened = crate::project_file::project_from_json_text(&text).unwrap();

    let target = clip(opened.tracks()[0].entries()[0].id());
    assert_eq!(opened.property_value_at(target, HUE, frame(0)), Some(turn(75.0)));
    assert!(opened.has_the_effect(target, EffectInstanceId::FIRST));
}

#[test]
fn changing_the_value_of_an_effect_again_and_again_is_one_edit_and_the_value_of_the_effect_of_another_thing_is_another()
{
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_a_video();
    let track = AnimationTarget::Track(window.application().project().tracks()[0].id());
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HUE_SHIFT,
    });
    window.do_this(Command::AddEffect {
        target: track,
        kind: EffectKind::HUE_SHIFT,
    });
    let set = |window: &mut Window, target, degrees| {
        window.do_this(Command::SetEffectValue {
            target,
            property: HUE,
            value: turn(degrees),
        });
    };

    for degrees in [10.0, 20.0, 30.0] {
        set(&mut window, clip(video), degrees);
    }
    set(&mut window, track, 40.0);

    undo(&mut window);
    assert_eq!(
        value_at(&window, track, 0),
        Some(turn(0.0)),
        "the track's value is taken back"
    );
    assert_eq!(
        value_at(&window, clip(video), 0),
        Some(turn(30.0)),
        "and the clip's is not"
    );
    undo(&mut window);
    assert_eq!(
        value_at(&window, clip(video), 0),
        Some(turn(0.0)),
        "the three changes of the clip's value were one edit"
    );
    assert!(
        has_the_effect(&window, track, EffectInstanceId::FIRST),
        "that was not what was taken back"
    );
}

#[test]
fn changing_an_animated_value_at_another_frame_is_another_edit_however_soon_it_comes() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_a_video();
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HUE_SHIFT,
    });
    window.do_this(Command::AddKeyframe {
        target: clip(video),
        property: HUE,
        frame: frame(0),
    });
    let set_at = |window: &mut Window, at: u64, degrees: f32| {
        window.move_current_frame_to(at);
        window.do_this(Command::SetEffectValue {
            target: clip(video),
            property: HUE,
            value: turn(degrees),
        });
    };

    set_at(&mut window, 10, 30.0);
    set_at(&mut window, 20, 60.0);
    assert_eq!(keyframes_of(&window, video, HUE), vec![0, 10, 20]);

    undo(&mut window);

    assert_eq!(
        keyframes_of(&window, video, HUE),
        vec![0, 10],
        "only the keyframe at the frame that was changed last is taken back"
    );
    assert_eq!(value_at(&window, clip(video), 10), Some(turn(30.0)));
}

// --- several effects of one kind ---

/// The value of the hue shift that is named `number` in its chain.
fn hue_of(number: u32) -> PropertyId {
    PropertyId::effect_value(EffectKind::HUE_SHIFT, EffectInstanceId::new(number), 0)
}

/// A window with the video selected and two hue shifts on it, which turn the colours by 10 degrees and by 20. Nothing is unsaved and there is
/// nothing to undo.
fn window_with_two_hue_shifts() -> (Window, ClipId) {
    let (mut window, video) = window_with_a_video();
    for _ in 0..2 {
        window.do_this(Command::AddEffect {
            target: clip(video),
            kind: EffectKind::HUE_SHIFT,
        });
    }
    // (The value of the second last of all: two changes of one value in a row are one edit, so the change that the tests make to the second
    // value is another edit than these.)
    for (property, degrees) in [(hue_of(1), 20.0), (hue_of(0), 10.0)] {
        window.do_this(Command::SetEffectValue {
            target: clip(video),
            property,
            value: turn(degrees),
        });
    }
    select(&mut window, video);
    window.application_mut().note_everything_is_saved();
    (window, video)
}

fn value_of_the_hue_shift_at(window: &Window, video: ClipId, number: u32, at: u64) -> Option<PropertyValue> {
    window
        .application()
        .project()
        .property_value_at(clip(video), hue_of(number), frame(at))
}

#[test]
fn the_plus_button_puts_a_second_hue_shift_on_and_the_boxes_say_which_of_the_hue_shifts_each_is_once_there_are_two() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_the_video_selected();

    put_the_hue_shift_on_with_the_plus_button(&mut window);
    assert!(is_shown(&window, "Hue shift"), "alone, it is called by its name");
    assert!(!is_shown(&window, "Hue shift 1"), "and has no number");

    put_the_hue_shift_on_with_the_plus_button(&mut window);

    assert!(is_shown(&window, "Hue shift 1"));
    assert!(is_shown(&window, "Hue shift 2"));
    assert!(
        !is_shown(&window, "Hue shift"),
        "neither is called by the name alone: the menu is closed"
    );
    assert!(
        window.rectangle_of("Hue shift 1").top() < window.rectangle_of("Hue shift 2").top(),
        "in the order they are applied in, the first at the top"
    );
    assert_eq!(window.how_many_are_called(TURN), 2, "each has the control of its value");
    assert_eq!(window.application().project().effects_of(clip(video)).unwrap().len(), 2);
}

#[test]
fn each_of_two_hue_shifts_has_a_slider_of_its_own_that_changes_its_own_value_and_is_one_step_of_undo() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_two_hue_shifts();
    assert_eq!(window.how_many_are_called(TURN), 2);
    let shown = |window: &Window, number: usize| {
        window
            .harness
            .query_all_by_label(TURN)
            .map(|node| (node.rect().top(), node.accesskit_node().numeric_value()))
            .nth(number)
            .map(|(_, value)| value)
    };
    assert_eq!(shown(&window, 0), Some(Some(10.0)), "the first shows its own");
    assert_eq!(shown(&window, 1), Some(Some(20.0)), "and the second its own");

    window.set_the_nth_slider(TURN, 1, 75.0);

    assert_eq!(
        value_of_the_hue_shift_at(&window, video, 0, 5),
        Some(turn(10.0)),
        "the first is not changed"
    );
    assert_eq!(value_of_the_hue_shift_at(&window, video, 1, 5), Some(turn(75.0)));
    assert_eq!(
        window.application().project().effects_of(clip(video)).unwrap().len(),
        2,
        "and nothing is put on or taken off by it"
    );
    undo(&mut window);
    assert_eq!(value_of_the_hue_shift_at(&window, video, 1, 5), Some(turn(20.0)));
    assert_eq!(value_of_the_hue_shift_at(&window, video, 0, 5), Some(turn(10.0)));
}

#[test]
fn the_stopwatch_of_the_second_hue_shift_starts_the_keyframes_of_the_second_and_not_those_of_the_first() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_two_hue_shifts();
    window.move_current_frame_to(40);
    let stopwatch_of_the_second = keyframe_button_label("hue shift 2", KeyframeButton::Stopwatch, false);
    let middle_of_the_row = window.rectangle_of_the_nth(TURN, 1).center();
    window.move_pointer_to(middle_of_the_row);
    assert_eq!(
        window.how_many_are_called(&stopwatch_of_the_second),
        1,
        "the buttons of the second are called by which it is"
    );
    assert_eq!(
        window.how_many_are_called(&keyframe_button_label("hue shift", KeyframeButton::Stopwatch, false)),
        0,
        "and none is called by the name alone, which would not say which"
    );

    let stopwatch = window.rectangle_of(&stopwatch_of_the_second).center();
    window.click(stopwatch);

    assert_eq!(keyframes_of(&window, video, hue_of(1)), vec![40]);
    assert_eq!(keyframes_of(&window, video, hue_of(0)), Vec::<u64>::new());
    assert_eq!(
        value_of_the_hue_shift_at(&window, video, 1, 40),
        Some(turn(20.0)),
        "with what it was"
    );
}

#[test]
fn deleting_one_of_two_hue_shifts_takes_it_with_its_keyframes_and_the_other_stays_as_it_was_and_is_called_by_its_name_alone_again()
 {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_two_hue_shifts();
    for (number, at, degrees) in [(0, 10, 15.0), (1, 30, 25.0)] {
        window.move_current_frame_to(at);
        window.do_this(Command::AddKeyframe {
            target: clip(video),
            property: hue_of(number),
            frame: frame(at),
        });
        window.do_this(Command::SetEffectValue {
            target: clip(video),
            property: hue_of(number),
            value: turn(degrees),
        });
    }
    window.application_mut().note_everything_is_saved();

    let heading = window.rectangle_of("Hue shift 1").center();
    window.right_click(heading);
    let item = window.rectangle_of("Delete effect").center();
    window.click(item);

    let left = window.application().project().effects_of(clip(video)).unwrap();
    assert_eq!(left.len(), 1);
    assert!(
        left.has_the_instance(EffectInstanceId::new(1)),
        "the second is the one that is left"
    );
    assert_eq!(
        keyframes_of(&window, video, hue_of(0)),
        Vec::<u64>::new(),
        "the first took its keyframes"
    );
    assert_eq!(
        keyframes_of(&window, video, hue_of(1)),
        vec![30],
        "and the second has its own"
    );
    assert_eq!(value_of_the_hue_shift_at(&window, video, 1, 0), Some(turn(25.0)));
    assert!(
        is_shown(&window, "Hue shift"),
        "the one that is left has no number any more"
    );
    assert!(!is_shown(&window, "Hue shift 2"));
    assert_eq!(window.how_many_are_called(TURN), 1);
    assert!(window.application().has_unsaved_changes());

    undo(&mut window);
    assert_eq!(window.application().project().effects_of(clip(video)).unwrap().len(), 2);
    assert_eq!(
        keyframes_of(&window, video, hue_of(0)),
        vec![10],
        "the keyframes of the first are back"
    );
    assert!(is_shown(&window, "Hue shift 1") && is_shown(&window, "Hue shift 2"));
}

#[test]
fn deleting_the_second_of_two_hue_shifts_takes_the_second_and_leaves_the_first_with_its_values() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_two_hue_shifts();

    let heading = window.rectangle_of("Hue shift 2").center();
    window.right_click(heading);
    let item = window.rectangle_of("Delete effect").center();
    window.click(item);

    let left = window.application().project().effects_of(clip(video)).unwrap();
    assert_eq!(left.len(), 1);
    assert!(
        left.has_the_instance(EffectInstanceId::FIRST),
        "the first is the one that is left"
    );
    assert!(!left.has_the_instance(EffectInstanceId::new(1)));
    assert_eq!(
        value_of_the_hue_shift_at(&window, video, 0, 5),
        Some(turn(10.0)),
        "with its own value"
    );
    assert_eq!(
        value_of_the_hue_shift_at(&window, video, 1, 5),
        None,
        "the second is gone"
    );
    assert!(is_shown(&window, "Hue shift"), "and it is alone, so it has no number");
}

#[test]
fn putting_a_second_hue_shift_on_is_an_edit_of_its_own_that_is_undone_and_done_again() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_a_video();
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HUE_SHIFT,
    });
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HUE_SHIFT,
    });
    let on_it = |window: &Window| window.application().project().effects_of(clip(video)).unwrap().len();
    assert_eq!(on_it(&window), 2);

    undo(&mut window);
    assert_eq!(on_it(&window), 1, "one step takes the second back");
    assert!(has_the_effect(&window, clip(video), EffectInstanceId::FIRST));
    undo(&mut window);
    assert_eq!(on_it(&window), 0);
    redo(&mut window);
    redo(&mut window);
    assert_eq!(on_it(&window), 2);
}

// --- an effect that the program does not have ---

/// What the box of an effect that the program does not have says, written out: that it does nothing, and that its values are kept.
const NOT_INSTALLED: &str = "Not installed: it does nothing here, and its values are kept.";

/// A window with the video selected, which has an effect that a project file could bring and the program does not have: three values, a number
/// (`amount`, 0.5), a pair (`point`) and a flag (`on`). Nothing is unsaved and there is nothing to undo.
fn window_with_an_effect_the_program_does_not_have() -> (Window, ClipId, EffectKind) {
    let (mut window, video) = window_with_a_video();
    let kind = a_kind_of_effect_the_program_does_not_have(
        "isf:window_made_up",
        &[
            ("amount", turn(0.5)),
            ("point", PropertyValue::Pair(0.5, 0.25)),
            ("on", PropertyValue::Flag(true)),
        ],
    );
    window
        .application_mut()
        .document
        .project
        .add_effect(clip(video), kind)
        .unwrap();
    select(&mut window, video);
    window.application_mut().note_everything_is_saved();
    (window, video, kind)
}

#[test]
fn an_effect_the_program_does_not_have_has_a_box_with_its_name_a_note_that_it_does_nothing_and_a_control_for_each_value()
 {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (window, _, _) = window_with_an_effect_the_program_does_not_have();

    assert!(
        is_shown(&window, "Window made up"),
        "the box has the name the file gave the effect"
    );
    assert!(is_shown(&window, NOT_INSTALLED));
    assert_eq!(
        window.harness.get_by_label("Amount").accesskit_node().numeric_value(),
        Some(0.5),
        "a number has a slider, with the label the effect gives it"
    );
    // (Rewritten on purpose in phase 6: a pair has two boxes to drag in now, which a shader's point needs, and not a line that says what it is.)
    assert_eq!(
        window.harness.query_all_by_label("Point").count(),
        2,
        "a pair has a box for across and one for down, with the label the effect gives it"
    );
    assert!(!is_shown(&window, "Point: 0.5, 0.25"));
    // (Rewritten on purpose in phase 4: a flag has a switch now, made from what the effect says of the value, and not a line that says what it is.)
    assert_eq!(
        window.harness.get_by_label("On").accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True),
        "and a flag is a switch that is on"
    );
    assert!(!is_shown(&window, TURN), "none of what the program's own effect has");
}

#[test]
fn the_effects_of_the_program_do_not_say_that_they_are_not_installed() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, _) = window_with_the_video_selected();
    put_the_hue_shift_on_with_the_plus_button(&mut window);

    assert!(is_shown(&window, "Hue shift"));
    assert!(!is_shown(&window, NOT_INSTALLED));
}

#[test]
fn the_menu_of_the_plus_button_offers_the_effects_of_the_program_and_not_one_that_a_file_brought() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, _, _) = window_with_an_effect_the_program_does_not_have();
    assert!(
        !window
            .harness
            .get_by_label(ADD_EFFECT_LABEL)
            .accesskit_node()
            .is_disabled()
    );

    click_the_plus_button(&mut window);

    assert_eq!(
        window.how_many_are_called("Hue shift"),
        1,
        "the effect of the program is offered"
    );
    assert_eq!(
        window.how_many_are_called("Window made up"),
        1,
        "and the one that is not is there as the box that it is, and not as a thing that can be put on"
    );
}

#[test]
fn the_number_of_an_effect_the_program_does_not_have_is_changed_with_its_slider_and_animated_as_any_and_is_one_step_of_undo()
 {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video, kind) = window_with_an_effect_the_program_does_not_have();
    let amount = Effect::new(kind).property(0).unwrap();
    let value = |window: &Window| {
        window
            .application()
            .project()
            .property_value_at(clip(video), amount, frame(10))
    };
    window.move_current_frame_to(10);

    window.set_slider("Amount", 0.8);

    assert_eq!(value(&window), Some(turn(0.8)));
    assert!(window.application().has_unsaved_changes());
    undo(&mut window);
    assert_eq!(value(&window), Some(turn(0.5)), "one step takes it back");

    let middle_of_the_row = window.rectangle_of("Amount").center();
    window.move_pointer_to(middle_of_the_row);
    let stopwatch = window
        .rectangle_of(&keyframe_button_label("amount", KeyframeButton::Stopwatch, false))
        .center();
    window.click(stopwatch);
    assert_eq!(
        keyframes_of(&window, video, amount),
        vec![10],
        "it has the stopwatch of a number, which makes a keyframe of it"
    );
}

#[test]
fn an_effect_the_program_does_not_have_is_deleted_with_its_keyframes_and_undo_brings_it_all_back() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video, kind) = window_with_an_effect_the_program_does_not_have();
    let amount = Effect::new(kind).property(0).unwrap();
    window.move_current_frame_to(10);
    window.do_this(Command::AddKeyframe {
        target: clip(video),
        property: amount,
        frame: frame(10),
    });
    window.application_mut().note_everything_is_saved();

    let heading = window.rectangle_of("Window made up").center();
    window.right_click(heading);
    let item = window.rectangle_of("Delete effect").center();
    window.click(item);

    assert!(!has_the_effect(&window, clip(video), EffectInstanceId::FIRST));
    assert_eq!(keyframes_of(&window, video, amount), Vec::<u64>::new());
    assert!(!is_shown(&window, "Window made up"));
    undo(&mut window);
    assert!(has_the_effect(&window, clip(video), EffectInstanceId::FIRST));
    assert_eq!(keyframes_of(&window, video, amount), vec![10]);
    assert!(is_shown(&window, "Window made up") && is_shown(&window, NOT_INSTALLED));
}

#[test]
fn an_effect_the_program_does_not_have_is_saved_with_the_project_and_opened_again_as_it_was() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (window, video, _) = window_with_an_effect_the_program_does_not_have();
    let project = window.application().project().clone();

    let text = crate::project_file::project_to_json_text(&project, std::path::Path::new("/projects")).unwrap();
    let opened = crate::project_file::project_from_json_text(&text).unwrap();

    let before = project.effects_of(clip(video)).unwrap().iter().next().unwrap();
    let opened_video = opened.tracks()[0].entries()[0].id();
    let after = opened.effects_of(clip(opened_video)).unwrap().iter().next().unwrap();
    assert_eq!(after, before, "the same effect, of the same kind, with the same values");
    assert!(!after.kind().is_installed());
    assert!(text.contains("isf:window_made_up"), "{text}");
}

#[test]
fn a_value_that_is_not_a_number_has_no_stopwatch_and_a_number_that_has_a_unit_shows_it() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, _, _) = window_with_an_effect_the_program_does_not_have();
    let middle_of_the_row = window.rectangle_of("Amount").center();
    window.move_pointer_to(middle_of_the_row);
    assert_eq!(
        window.how_many_are_called(&keyframe_button_label("amount", KeyframeButton::Stopwatch, false)),
        1,
        "a number has the stopwatch"
    );
    // (Rewritten on purpose in phase 6: a pair has boxes to drag in now, and so has the stopwatch, as the position of a clip has; a flag has none.)
    let middle_of_the_pair = window.rectangle_of_the_nth("Point", 0).center();
    window.move_pointer_to(middle_of_the_pair);
    assert_eq!(
        window.how_many_are_called(&keyframe_button_label("point", KeyframeButton::Stopwatch, false)),
        1,
        "a pair has the stopwatch"
    );
    let middle_of_the_flag = window.rectangle_of("On").center();
    window.move_pointer_to(middle_of_the_flag);
    assert_eq!(
        window.how_many_are_called(&keyframe_button_label("on", KeyframeButton::Stopwatch, false)),
        0,
        "a flag has none, as it holds until the next keyframe"
    );

    let (window, _) = window_with_the_video_selected();
    let mut window = window;
    put_the_hue_shift_on_with_the_plus_button(&mut window);
    // (The number that the slider shows is drawn at the end of its row, with the unit after it: the turn of the clip is another row, with its own.)
    let row_of_the_slider = window.rectangle_of(TURN);
    let units_in_the_row: Vec<String> = window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            egui::Shape::Text(text) if (row_of_the_slider.top()..=row_of_the_slider.bottom()).contains(&text.pos.y) => {
                Some(text.galley.text().to_owned())
            }
            _ => None,
        })
        .collect();
    assert!(
        units_in_the_row.iter().any(|text| text == "°"),
        "the hue shift is in degrees, and says so: {units_in_the_row:?}"
    );
}

// --- switching an effect off and on ---

#[test]
fn the_menu_of_the_box_of_an_effect_switches_it_off_and_on_each_a_step_of_the_history_and_the_box_says_when_it_is_off()
{
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, video) = window_with_the_video_selected();
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HUE_SHIFT,
    });
    window.do_this(Command::SetEffectValue {
        target: clip(video),
        property: HUE,
        value: turn(30.0),
    });
    window.application_mut().note_everything_is_saved();
    let enabled = |window: &Window| {
        window
            .application()
            .project()
            .effects_of(clip(video))
            .unwrap()
            .instance(EffectInstanceId::FIRST)
            .unwrap()
            .is_enabled()
    };
    let heading = || what_an_effect_is_called(EffectKind::HUE_SHIFT);

    // (The words themselves: they are what a person reads.)
    assert_eq!(SWITCH_EFFECT_OFF_LABEL, "Switch off");
    window.right_click(window.rectangle_of(&heading()).center());
    assert!(is_shown(&window, "Switch off"));
    assert!(!is_shown(&window, "Switch on"), "it is on");
    assert!(enabled(&window), "opening a menu changes nothing");
    let item = window.rectangle_of("Switch off").center();
    window.click(item);

    assert!(!enabled(&window));
    assert!(window.application().has_unsaved_changes());
    assert_eq!(
        value_at(&window, clip(video), 0),
        Some(turn(30.0)),
        "its values are kept"
    );
    assert!(is_shown(&window, TURN), "and its box is there");
    assert!(is_shown(&window, SWITCHED_OFF_NOTE), "and says that it is off");

    window.right_click(window.rectangle_of(&heading()).center());
    assert!(is_shown(&window, "Switch on"));
    assert!(!is_shown(&window, "Switch off"));
    let item = window.rectangle_of("Switch on").center();
    window.click(item);
    assert!(enabled(&window));
    assert!(!is_shown(&window, SWITCHED_OFF_NOTE));

    undo(&mut window);
    assert!(!enabled(&window), "switching it on was a step");
    undo(&mut window);
    assert!(enabled(&window), "and so was switching it off");
    redo(&mut window);
    assert!(!enabled(&window));
}
