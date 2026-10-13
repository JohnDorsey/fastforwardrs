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
    ADD_EFFECT_LABEL, DELETE_EFFECT_LABEL, KeyframeButton, keyframe_button_label,
};
use crate::application::selection::{SelectionChange, SelectionItem};
use crate::project::{AnimationTarget, ClipId, EffectKind, PropertyId, PropertyValue, TextObject, TimelineFrameIndex};

const HUE: PropertyId = PropertyId::HueShift;
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

fn has_the_effect(window: &Window, target: AnimationTarget, kind: EffectKind) -> bool {
    window.application().project().has_the_effect(target, kind)
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

fn click_the_plus_button(window: &mut Window) {
    let middle = the_plus_button(window).center();
    window.click(middle);
}

/// Puts the hue shift on `target` with the menu of the plus button, as a person does.
fn put_the_hue_shift_on_with_the_plus_button(window: &mut Window) {
    click_the_plus_button(window);
    let item = window
        .rectangle_of(&what_an_effect_is_called(EffectKind::HueShift))
        .center();
    window.click(item);
}

fn the_stopwatch_of_the_hue_shift() -> String {
    keyframe_button_label(HUE, KeyframeButton::Stopwatch, false)
}

// --- the plus button ---

#[test]
fn what_is_seen_has_a_plus_button_that_adds_an_effect_and_a_sound_has_none() {
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
    let (mut window, _) = window_with_a_video();
    assert!(!is_shown(&window, ADD_EFFECT_LABEL), "nothing is selected");

    window.select_the_track_at(0);
    assert!(is_shown(&window, ADD_EFFECT_LABEL), "a track");

    let track = window.application().project().tracks()[0].id();
    let front = window.application_mut().project.add_track_in_front();
    let a_crate = window
        .application_mut()
        .project
        .insert_crate(front, frame(0), 50)
        .unwrap();
    assert_ne!(front, track);
    window.settle();
    select(&mut window, a_crate);
    assert!(
        !is_shown(&window, ADD_EFFECT_LABEL),
        "a keyframe crate has no picture to change"
    );
}

#[test]
fn the_menu_of_the_plus_button_has_each_effect_that_the_clip_can_have_and_has_not_got_and_choosing_one_puts_it_on() {
    let (mut window, video) = window_with_the_video_selected();
    click_the_plus_button(&mut window);
    for kind in EffectKind::ALL {
        assert!(
            is_shown(&window, &what_an_effect_is_called(kind)),
            "the menu has {kind}: the effects of the table are all offered"
        );
    }
    assert!(
        !has_the_effect(&window, clip(video), EffectKind::HueShift),
        "opening the menu changes nothing"
    );

    let item = window
        .rectangle_of(&what_an_effect_is_called(EffectKind::HueShift))
        .center();
    window.click(item);

    assert!(has_the_effect(&window, clip(video), EffectKind::HueShift));
    assert_eq!(value_at(&window, clip(video), 0), Some(turn(0.0)), "as it begins");
    assert!(window.application().has_unsaved_changes());
}

#[test]
fn the_effect_that_is_put_on_has_a_box_with_its_name_and_the_controls_of_its_values_under_the_box_of_the_clip() {
    let (mut window, video) = window_with_the_video_selected();
    assert!(!is_shown(&window, TURN), "no effect, no controls");
    let box_of_the_clip = window.rectangle_of("Video clip");

    put_the_hue_shift_on_with_the_plus_button(&mut window);

    assert!(has_the_effect(&window, clip(video), EffectKind::HueShift));
    assert!(
        is_shown(&window, &what_an_effect_is_called(EffectKind::HueShift)),
        "the box has its name"
    );
    assert!(is_shown(&window, TURN), "and the control of its value");
    assert!(
        window
            .rectangle_of(&what_an_effect_is_called(EffectKind::HueShift))
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
fn the_plus_button_is_dimmed_when_the_clip_has_every_effect_it_can_have() {
    let (mut window, _) = window_with_the_video_selected();
    assert!(
        !window
            .harness
            .get_by_label(ADD_EFFECT_LABEL)
            .accesskit_node()
            .is_disabled()
    );

    put_the_hue_shift_on_with_the_plus_button(&mut window);

    assert!(
        window
            .harness
            .get_by_label(ADD_EFFECT_LABEL)
            .accesskit_node()
            .is_disabled(),
        "there is nothing left to put on"
    );
}

#[test]
fn an_effect_is_put_on_a_track_from_the_panel_of_the_track_and_is_the_track_s_and_not_the_clip_s() {
    let (mut window, video) = window_with_a_video();
    window.select_the_track_at(0);
    let track = window.application().project().tracks()[0].id();

    put_the_hue_shift_on_with_the_plus_button(&mut window);

    assert!(has_the_effect(
        &window,
        AnimationTarget::Track(track),
        EffectKind::HueShift
    ));
    assert!(!has_the_effect(&window, clip(video), EffectKind::HueShift));
    window.set_slider(TURN, 45.0);
    assert_eq!(value_at(&window, AnimationTarget::Track(track), 10), Some(turn(45.0)));
}

// --- the value ---

#[test]
fn the_slider_of_a_value_that_is_not_animated_changes_it_for_the_whole_clip_and_is_one_step_of_undo() {
    let (mut window, video) = window_with_the_video_selected();
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HueShift,
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
    let (mut window, video) = window_with_the_video_selected();
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HueShift,
    });

    window.set_slider(TURN, 450.0);
    assert_eq!(value_at(&window, clip(video), 0), Some(turn(450.0)));
    window.set_slider(TURN, -270.0);
    assert_eq!(value_at(&window, clip(video), 0), Some(turn(-270.0)));
}

#[test]
fn the_stopwatch_of_the_value_starts_its_keyframes_and_the_slider_then_changes_the_keyframe_at_the_playhead() {
    let (mut window, video) = window_with_the_video_selected();
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HueShift,
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
    // The video lasts 100 frames and a sound in front of it 300, so that the playhead can be past the video.
    let mut window = Window::showing(project_with_a_video_and_a_sound(100, 10));
    let video = window.clip(1, 0);
    select(&mut window, video);
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HueShift,
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
    let (mut window, video) = window_with_the_video_selected();
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HueShift,
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
        .rectangle_of(&what_an_effect_is_called(EffectKind::HueShift))
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
        has_the_effect(&window, clip(video), EffectKind::HueShift),
        "opening it deletes nothing"
    );
    let item = window.rectangle_of("Delete effect").center();
    window.click(item);

    assert!(!has_the_effect(&window, clip(video), EffectKind::HueShift));
    assert_eq!(keyframes_of(&window, video, HUE), Vec::<u64>::new());
    assert!(!is_shown(&window, TURN), "the box is gone");
    assert!(window.application().has_unsaved_changes());

    undo(&mut window);
    assert!(has_the_effect(&window, clip(video), EffectKind::HueShift));
    assert_eq!(keyframes_of(&window, video, HUE), vec![20, 200], "with its keyframes");
    assert_eq!(value_at(&window, clip(video), 110), Some(turn(55.0)));
    assert!(is_shown(&window, TURN), "and its box");
}

#[test]
fn deleting_an_effect_leaves_what_was_selected_that_is_still_there_and_forgets_the_keyframes_that_went() {
    let (mut window, video) = window_with_the_video_selected();
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HueShift,
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
        kind: EffectKind::HueShift,
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
    let (mut window, video) = window_with_a_video();
    assert!(!window.application().can_undo());

    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HueShift,
    });
    window.do_this(Command::SetEffectValue {
        target: clip(video),
        property: HUE,
        value: turn(60.0),
    });
    window.do_this(Command::RemoveEffect {
        target: clip(video),
        kind: EffectKind::HueShift,
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
        !has_the_effect(&window, clip(video), EffectKind::HueShift),
        "and putting it on"
    );
    assert!(!window.application().can_undo());
}

#[test]
fn commands_that_cannot_be_done_leave_the_project_as_it_is_and_are_not_edits_to_undo() {
    let (mut window, video) = window_with_a_video();
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HueShift,
    });
    window.application_mut().note_everything_is_saved();

    // It has it already, and taking off one that is not on, and a value of an effect that the track does not have.
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HueShift,
    });
    let track = window.application().project().tracks()[0].id();
    window.do_this(Command::RemoveEffect {
        target: AnimationTarget::Track(track),
        kind: EffectKind::HueShift,
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
        EffectKind::HueShift
    ));
    assert!(!window.application().has_unsaved_changes(), "nothing is unsaved");
    undo(&mut window);
    assert!(
        !has_the_effect(&window, clip(video), EffectKind::HueShift),
        "the one step to undo is putting the effect on: the others made no step"
    );
    assert!(!window.application().can_undo());
}

#[test]
fn a_project_with_effects_is_saved_and_opened_again_with_them() {
    let (mut window, video) = window_with_the_video_selected();
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HueShift,
    });
    window.set_slider(TURN, 75.0);
    let project_before = window.application().project().clone();

    let text = crate::project_file::project_to_json_text(&project_before, std::path::Path::new("/projects")).unwrap();
    let opened = crate::project_file::project_from_json_text(&text).unwrap();

    let target = clip(opened.tracks()[0].entries()[0].id());
    assert_eq!(opened.property_value_at(target, HUE, frame(0)), Some(turn(75.0)));
    assert!(opened.has_the_effect(target, EffectKind::HueShift));
}

#[test]
fn changing_the_value_of_an_effect_again_and_again_is_one_edit_and_the_value_of_the_effect_of_another_thing_is_another()
{
    let (mut window, video) = window_with_a_video();
    let track = AnimationTarget::Track(window.application().project().tracks()[0].id());
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HueShift,
    });
    window.do_this(Command::AddEffect {
        target: track,
        kind: EffectKind::HueShift,
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
        has_the_effect(&window, track, EffectKind::HueShift),
        "that was not what was taken back"
    );
}

#[test]
fn changing_an_animated_value_at_another_frame_is_another_edit_however_soon_it_comes() {
    let (mut window, video) = window_with_a_video();
    window.do_this(Command::AddEffect {
        target: clip(video),
        kind: EffectKind::HueShift,
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
