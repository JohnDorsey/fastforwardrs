// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the keyframe buttons in the properties panel (the arrow to the keyframe before, the stopwatch, and the arrow to the
//! keyframe after, which float to the right of a control when the pointer is over it), of what the controls show of what has
//! keyframes, and of the controls for where a clip is on the picture.

use egui::{Color32, PointerButton, Shape, vec2};
use egui_kittest::kittest::{NodeT, Queryable};

use super::keyframe_tests::{keyframes_of, window_with_a_video};
use super::picture_tests::picture_source;
use super::preview_layer_tests::select;
use super::test_window::{Window, project_of, project_with_a_video_and_a_sound, sound_source};
use crate::application::commands::Command;
use crate::application::properties_panel::{KeyframeButton, keyframe_button_label};
use crate::application::theme;
use crate::project::{
    AnimationTarget, CanvasArea, ClipId, EffectKind, Gain, Placement, PropertyId, PropertyValue, TextObject,
    TimelineFrameIndex,
};

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

/// A window with a video of 300 frames, which is selected so that its properties show, and its clip.
fn window_with_the_video_selected() -> (Window, ClipId) {
    let (mut window, clip) = window_with_a_video();
    select(&mut window, clip);
    (window, clip)
}

fn label_of(property: PropertyId, button: KeyframeButton, has_one_here: bool) -> String {
    keyframe_button_label(property, button, has_one_here)
}

/// The colour that the button with `label` is drawn in (the orange or the grey of the buttons), if it is drawn.
fn colour_of_the_button(window: &Window, label: &str) -> Option<Color32> {
    let button = window.rectangle_of(label);
    window.shapes_drawn().into_iter().find_map(|shape| {
        let colour = match shape {
            Shape::Path(path) if path.fill != Color32::TRANSPARENT => path.fill,
            Shape::Circle(circle) if circle.stroke.width > 0.0 => circle.stroke.color,
            Shape::LineSegment { stroke, .. } => stroke.color,
            _ => return None,
        };
        let is_a_button_colour = colour == theme::KEYFRAME_BUTTON_ACTIVE || colour == theme::KEYFRAME_BUTTON_INACTIVE;
        (is_a_button_colour && button.contains_rect(shape.visual_bounding_rect())).then_some(colour)
    })
}

/// Puts the pointer on the control whose label is `label`.
fn put_the_pointer_on(window: &mut Window, label: &str) {
    let middle = window.rectangle_of(label).center();
    window.move_pointer_to(middle);
}

fn click_the_button(window: &mut Window, label: &str) {
    let middle = window.rectangle_of(label).center();
    window.click(middle);
}

fn make_keyframes_of_the_gain(window: &mut Window, clip: ClipId, keyframes: &[(u64, f32)]) {
    for &(at, decibels) in keyframes {
        window.move_current_frame_to(at);
        window.do_this(Command::AddKeyframe {
            target: AnimationTarget::Clip(clip),
            property: PropertyId::Gain,
            frame: frame(at),
        });
        window.do_this(Command::SetClipGain {
            clip,
            gain: Gain::from_decibels(decibels),
        });
    }
}

fn the_stopwatch_of(property: PropertyId, has_one_here: bool) -> String {
    label_of(property, KeyframeButton::Stopwatch, has_one_here)
}

// --- the buttons ---

#[test]
fn the_buttons_are_not_drawn_until_the_pointer_is_over_the_control_and_then_all_three_are_grey_if_there_are_no_keyframes()
 {
    let (mut window, _) = window_with_the_video_selected();
    let before = label_of(PropertyId::Gain, KeyframeButton::Before, false);
    let stopwatch = the_stopwatch_of(PropertyId::Gain, false);
    let after = label_of(PropertyId::Gain, KeyframeButton::After, false);
    for button in [&before, &stopwatch, &after] {
        assert_eq!(colour_of_the_button(&window, button), None, "{button} is not shown yet");
    }

    put_the_pointer_on(&mut window, "Gain");

    for button in [&before, &stopwatch, &after] {
        assert_eq!(
            colour_of_the_button(&window, button),
            Some(theme::KEYFRAME_BUTTON_INACTIVE),
            "{button} is grey: there is no keyframe"
        );
    }

    // Moving the pointer off the row takes them away again.
    let somewhere_else = window.rectangle_of("Speed").center();
    window.move_pointer_to(somewhere_else);
    assert_eq!(colour_of_the_button(&window, &stopwatch), None);
}

#[test]
fn the_buttons_are_still_there_when_the_pointer_goes_from_the_control_to_them() {
    let (mut window, _) = window_with_the_video_selected();
    put_the_pointer_on(&mut window, "Gain");
    let stopwatch = the_stopwatch_of(PropertyId::Gain, false);
    let on_the_stopwatch = window.rectangle_of(&stopwatch).center();

    window.move_pointer_to(on_the_stopwatch);

    assert!(
        colour_of_the_button(&window, &stopwatch).is_some(),
        "the stopwatch has not gone from under the pointer"
    );
}

#[test]
fn the_stopwatch_makes_a_keyframe_at_the_playhead_and_turns_orange_and_clicking_it_again_takes_the_keyframe_away() {
    let (mut window, clip) = window_with_the_video_selected();
    window.move_current_frame_to(40);
    put_the_pointer_on(&mut window, "Gain");

    click_the_button(&mut window, &the_stopwatch_of(PropertyId::Gain, false));

    assert_eq!(keyframes_of(&window, clip, PropertyId::Gain), vec![40]);
    let stopwatch = the_stopwatch_of(PropertyId::Gain, true);
    assert_eq!(
        colour_of_the_button(&window, &stopwatch),
        Some(theme::KEYFRAME_BUTTON_ACTIVE),
        "it is orange: there is a keyframe here now"
    );

    click_the_button(&mut window, &stopwatch);

    assert_eq!(keyframes_of(&window, clip, PropertyId::Gain), Vec::<u64>::new());
    assert_eq!(
        colour_of_the_button(&window, &the_stopwatch_of(PropertyId::Gain, false)),
        Some(theme::KEYFRAME_BUTTON_INACTIVE)
    );
}

#[test]
fn the_stopwatch_is_grey_at_a_frame_that_has_no_keyframe_though_there_are_others() {
    let (mut window, clip) = window_with_the_video_selected();
    make_keyframes_of_the_gain(&mut window, clip, &[(20, -3.0), (80, -9.0)]);

    window.move_current_frame_to(50);
    put_the_pointer_on(&mut window, "Gain");
    assert_eq!(
        colour_of_the_button(&window, &the_stopwatch_of(PropertyId::Gain, false)),
        Some(theme::KEYFRAME_BUTTON_INACTIVE)
    );
    window.move_current_frame_to(80);
    put_the_pointer_on(&mut window, "Gain");
    assert_eq!(
        colour_of_the_button(&window, &the_stopwatch_of(PropertyId::Gain, true)),
        Some(theme::KEYFRAME_BUTTON_ACTIVE)
    );
}

#[test]
fn the_arrows_are_orange_where_there_is_a_keyframe_that_way_however_far_and_they_go_to_it() {
    let (mut window, clip) = window_with_the_video_selected();
    make_keyframes_of_the_gain(&mut window, clip, &[(20, -3.0), (250, -9.0)]);
    let before = label_of(PropertyId::Gain, KeyframeButton::Before, false);
    let after = label_of(PropertyId::Gain, KeyframeButton::After, false);

    window.move_current_frame_to(100);
    put_the_pointer_on(&mut window, "Gain");
    assert_eq!(
        colour_of_the_button(&window, &before),
        Some(theme::KEYFRAME_BUTTON_ACTIVE)
    );
    assert_eq!(
        colour_of_the_button(&window, &after),
        Some(theme::KEYFRAME_BUTTON_ACTIVE),
        "(150 frames away)"
    );

    click_the_button(&mut window, &after);
    assert_eq!(window.current_frame(), 250, "the playhead went to the keyframe after");
    put_the_pointer_on(&mut window, "Gain");
    assert_eq!(
        colour_of_the_button(&window, &label_of(PropertyId::Gain, KeyframeButton::After, true)),
        Some(theme::KEYFRAME_BUTTON_INACTIVE),
        "there is none after the last"
    );
    assert_eq!(
        colour_of_the_button(&window, &before),
        Some(theme::KEYFRAME_BUTTON_ACTIVE)
    );

    click_the_button(&mut window, &before);
    assert_eq!(window.current_frame(), 20, "and to the one before, which is the first");
    put_the_pointer_on(&mut window, "Gain");
    assert_eq!(
        colour_of_the_button(&window, &before),
        Some(theme::KEYFRAME_BUTTON_INACTIVE),
        "none before the first"
    );
}

#[test]
fn an_arrow_with_nowhere_to_go_does_nothing_when_it_is_clicked() {
    let (mut window, _) = window_with_the_video_selected();
    window.move_current_frame_to(30);
    put_the_pointer_on(&mut window, "Gain");

    click_the_button(&mut window, &label_of(PropertyId::Gain, KeyframeButton::After, false));

    assert_eq!(window.current_frame(), 30);
    assert!(!window.application().has_unsaved_changes());
}

// --- room ---

/// Every control in the panel (not the buttons of the keyframes): its role and where it is.
fn controls_in_the_panel(window: &Window) -> Vec<(egui::accesskit::Role, egui::Rect)> {
    use egui::accesskit::Role;
    let roles = [
        Role::Slider,
        Role::SpinButton,
        Role::CheckBox,
        Role::TextInput,
        Role::MultilineTextInput,
        Role::ComboBox,
        Role::Button,
    ];
    let panel = window.the_properties_panel();
    roles
        .into_iter()
        .flat_map(|role| window.harness.query_all_by_role(role))
        .filter(|node| {
            let label = node.accesskit_node().label().unwrap_or_default();
            !(label.starts_with("Previous keyframe")
                || label.starts_with("Next keyframe")
                || label.starts_with("Make a keyframe")
                || label.starts_with("Take away the keyframe"))
        })
        .map(|node| (node.accesskit_node().role(), node.rect()))
        .filter(|(_, rectangle)| panel.contains_rect(*rectangle))
        .collect()
}

#[test]
fn no_control_reaches_into_the_room_the_buttons_of_the_keyframes_have_beside_it() {
    let mut project = super::test_window::project_of(300);
    let front = project.add_track_in_front();
    let (text, _) = project
        .insert_text(front, frame(20), TextObject::default(), 100)
        .unwrap();
    let mut window = Window::showing(project);
    let video = window.clip(1, 0);
    for what_is_selected in [Some(video), Some(text), None] {
        match what_is_selected {
            Some(clip) => select(&mut window, clip),
            None => window.select_the_track_at(0),
        }
        // (The fonts take a moment to be found, and are in a box that is as wide as there is room for.)
        window.settle();
        let buttons: Vec<egui::Rect> = window
            .harness
            .query_all_by_role(egui::accesskit::Role::Button)
            .filter(|node| {
                let label = node.accesskit_node().label().unwrap_or_default();
                label.starts_with("Previous keyframe")
                    || label.starts_with("Next keyframe")
                    || label.starts_with("Make a keyframe")
                    || label.starts_with("Take away the keyframe")
            })
            .map(|node| node.rect())
            .collect();
        assert!(buttons.len() >= 6, "there are buttons: {}", buttons.len());
        for (role, control) in controls_in_the_panel(&window) {
            for button in &buttons {
                assert!(
                    !control.intersects(*button),
                    "{role:?} at {control:?} is under a button of the keyframes at {button:?}"
                );
            }
        }
    }
}

// --- what the controls show ---

fn gain_slider_value(window: &Window) -> f64 {
    window
        .harness
        .get_by_label("Gain")
        .accesskit_node()
        .numeric_value()
        .expect("the gain is a slider with a value")
}

#[test]
fn the_gain_slider_shows_what_the_gain_is_at_the_playhead() {
    let (mut window, clip) = window_with_the_video_selected();
    make_keyframes_of_the_gain(&mut window, clip, &[(0, 0.0), (100, -20.0)]);

    for (at, expected) in [(0, 0.0), (25, -5.0), (50, -10.0), (100, -20.0), (200, -20.0)] {
        window.move_current_frame_to(at);
        assert!(
            (gain_slider_value(&window) - expected).abs() < 0.06,
            "frame {at}: {} and not {expected}",
            gain_slider_value(&window)
        );
    }
}

/// Regression test: the slider rounds what it shows to a tenth of a decibel, and gave the rounded gain back as a change of the gain
/// at once when the gain that keyframes make at the playhead was not a whole number of tenths, which made a keyframe there, by
/// itself, wherever the playhead was moved to.
#[test]
fn moving_the_playhead_along_an_animated_gain_makes_no_keyframes_by_itself() {
    let (mut window, clip) = window_with_the_video_selected();
    make_keyframes_of_the_gain(&mut window, clip, &[(20, -3.0), (250, -9.0)]);
    window.application_mut().note_everything_is_saved();

    for at in [100, 101, 133, 177, 249] {
        window.move_current_frame_to(at);
    }

    assert_eq!(keyframes_of(&window, clip, PropertyId::Gain), vec![20, 250]);
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn changing_the_slider_of_an_animated_gain_makes_a_keyframe_at_the_playhead_and_not_a_new_gain_for_the_clip() {
    let (mut window, clip) = window_with_the_video_selected();
    make_keyframes_of_the_gain(&mut window, clip, &[(0, 0.0), (100, -20.0)]);
    window.move_current_frame_to(50);

    window.set_slider("Gain", -3.0);

    assert_eq!(keyframes_of(&window, clip, PropertyId::Gain), vec![0, 50, 100]);
    assert_eq!(
        window
            .application()
            .project()
            .property_value_at(AnimationTarget::Clip(clip), PropertyId::Gain, frame(50)),
        Some(PropertyValue::Number(-3.0))
    );
}

#[test]
fn the_mute_box_shows_whether_the_clip_is_silent_at_the_playhead() {
    let (mut window, clip) = window_with_the_video_selected();
    let target = AnimationTarget::Clip(clip);
    window.move_current_frame_to(0);
    window.do_this(Command::AddKeyframe {
        target,
        property: PropertyId::Muted,
        frame: frame(0),
    });
    window.move_current_frame_to(100);
    window.do_this(Command::SetClipMuted { clip, muted: true });

    window.move_current_frame_to(50);
    assert!(!window.is_checked("Mute"), "not yet");
    window.move_current_frame_to(150);
    assert!(window.is_checked("Mute"), "from frame 100 on");
}

#[test]
fn the_controls_of_a_property_that_is_animated_are_dimmed_where_the_playhead_is_not_on_the_clip_and_the_stopwatch_cannot_make_a_keyframe_there()
 {
    // The video lasts 100 frames and a sound in front of it 300, so that the playhead can be past the video.
    let mut window = Window::showing(project_with_a_video_and_a_sound(100, 10));
    let video = window.clip(1, 0);
    select(&mut window, video);
    make_keyframes_of_the_gain(&mut window, video, &[(10, -3.0)]);

    window.move_current_frame_to(50);
    assert!(
        !window.harness.get_by_label("Gain").accesskit_node().is_disabled(),
        "on the clip it can be changed"
    );
    window.move_current_frame_to(200);
    assert!(
        window.harness.get_by_label("Gain").accesskit_node().is_disabled(),
        "beyond the clip it cannot"
    );
    assert!(
        !window.harness.get_by_label("Speed").accesskit_node().is_disabled(),
        "and what is not animated can"
    );
    put_the_pointer_on(&mut window, "Gain");
    let stopwatch = the_stopwatch_of(PropertyId::Gain, false);
    assert!(window.harness.get_by_label(&stopwatch).accesskit_node().is_disabled());
    assert_eq!(
        colour_of_the_button(&window, &stopwatch),
        Some(theme::KEYFRAME_BUTTON_INACTIVE)
    );
    click_the_button(&mut window, &stopwatch);
    assert_eq!(
        keyframes_of(&window, video, PropertyId::Gain),
        vec![10],
        "no keyframe was made"
    );
}

#[test]
fn a_property_that_is_not_animated_can_be_changed_wherever_the_playhead_is_even_beyond_the_clip() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(100, 10));
    let video = window.clip(1, 0);
    select(&mut window, video);
    window.move_current_frame_to(200);

    assert!(!window.harness.get_by_label("Gain").accesskit_node().is_disabled());
    window.set_slider("Gain", -6.0);
    assert_eq!(
        window
            .application()
            .project()
            .placement_of(video)
            .unwrap()
            .clip
            .as_media()
            .unwrap()
            .gain(),
        Gain::from_decibels(-6.0)
    );
}

#[test]
fn nothing_about_keyframes_can_be_done_while_a_message_is_up() {
    let (mut window, _) = window_with_the_video_selected();
    window.application_mut().dialog = Some(super::Dialog::Message {
        title: "Note".into(),
        text: "Read me".into(),
    });
    window.settle();
    let stopwatch = the_stopwatch_of(PropertyId::Gain, false);
    assert!(window.harness.get_by_label(&stopwatch).accesskit_node().is_disabled());
}

// --- a track ---

#[test]
fn a_track_has_the_buttons_for_how_loud_it_is_and_whether_it_is_silent() {
    let (mut window, _) = window_with_a_video();
    window.select_the_track_at(0);
    let track = window.application().project().tracks()[0].id();
    window.move_current_frame_to(120);
    put_the_pointer_on(&mut window, "Gain");

    click_the_button(&mut window, &the_stopwatch_of(PropertyId::Gain, false));

    assert!(window.application().project().has_keyframe_at(
        AnimationTarget::Track(track),
        PropertyId::Gain,
        frame(120)
    ));
    put_the_pointer_on(&mut window, "Mute");
    assert!(
        window
            .harness
            .query_by_label(&the_stopwatch_of(PropertyId::Muted, false))
            .is_some()
    );
}

/// What keeps the panel honest: the table of properties says which of them a kind of clip, and a track, has, while the panel
/// has a row (and so the three buttons beside its control) for each by hand. A property that is added to the table and not to
/// the panel, or the other way round, is found here and not by somebody who looks for its buttons.
#[test]
fn the_panel_has_the_buttons_of_just_the_properties_that_the_table_says_what_is_selected_has() {
    let mut project = project_of(300);
    let video = project.tracks()[0].entries()[0].id();
    let sound = project
        .import_source(sound_source(10), frame(0))
        .unwrap()
        .clip
        .expect("a sound makes a clip");
    let picture = project
        .import_source(picture_source(), frame(0))
        .unwrap()
        .clip
        .expect("a picture makes a clip");
    let in_front = project.add_track_in_front();
    let (text, _) = project
        .insert_text(in_front, frame(20), TextObject::default(), 100)
        .unwrap();
    let mut window = Window::showing(project);
    let has_the_buttons_of = |window: &Window, property: PropertyId| {
        window
            .harness
            .query_by_label(&the_stopwatch_of(property, false))
            .is_some()
    };
    // (A value of an effect has its buttons only when what is selected has the effect.)
    let has_what_the_property_is_of = |window: &Window, target: AnimationTarget, property: PropertyId| {
        property
            .effect()
            .is_none_or(|effect| window.application().project().has_the_effect(target, effect))
    };

    // Before an effect is put on anything, and then with every effect there is put on all that can have it.
    for effects_are_on in [false, true] {
        for clip in [video, sound, picture, text] {
            let target = AnimationTarget::Clip(clip);
            let kind = window.application().project().placement_of(clip).unwrap().clip.kind();
            if effects_are_on {
                for effect in EffectKind::ALL
                    .into_iter()
                    .filter(|effect| effect.applies_to_a_clip_of_kind(kind))
                {
                    window.do_this(Command::AddEffect { target, kind: effect });
                }
            }
            select(&mut window, clip);
            for property in PropertyId::ALL {
                assert_eq!(
                    has_the_buttons_of(&window, property),
                    property.applies_to_a_clip_of_kind(kind) && has_what_the_property_is_of(&window, target, property),
                    "a {kind:?} clip and its {property:?} (effects on: {effects_are_on}): the panel and the table of properties disagree"
                );
            }
        }

        let track = window.application().project().tracks()[0].id();
        if effects_are_on {
            for effect in EffectKind::ALL.into_iter().filter(|effect| effect.applies_to_a_track()) {
                window.do_this(Command::AddEffect {
                    target: AnimationTarget::Track(track),
                    kind: effect,
                });
            }
        }
        window.select_the_track_at(0);
        for property in PropertyId::ALL {
            assert_eq!(
                has_the_buttons_of(&window, property),
                property.applies_to_a_track()
                    && has_what_the_property_is_of(&window, AnimationTarget::Track(track), property),
                "a track and its {property:?} (effects on: {effects_are_on}): the panel and the table of properties disagree"
            );
        }
    }
}

// --- text ---

#[test]
fn a_text_object_has_the_buttons_for_what_it_says_how_it_looks_and_where_it_is() {
    let mut project = super::test_window::project_of(300);
    let front = project.add_track_in_front();
    let (text, _) = project
        .insert_text(front, frame(20), TextObject::default(), 100)
        .unwrap();
    let mut window = Window::showing(project);
    select(&mut window, text);
    for property in [
        PropertyId::Text,
        PropertyId::FontFamily,
        PropertyId::FontSize,
        PropertyId::Colour,
        PropertyId::Position,
        PropertyId::Size,
        PropertyId::Rotation,
    ] {
        assert!(
            window
                .harness
                .query_by_label(&the_stopwatch_of(property, false))
                .is_some(),
            "{property:?} has a stopwatch"
        );
    }
    assert!(
        window
            .harness
            .query_by_label(&the_stopwatch_of(PropertyId::Gain, false))
            .is_none(),
        "a text object has no sound"
    );

    window.move_current_frame_to(60);
    click_the_button(&mut window, &the_stopwatch_of(PropertyId::Colour, false));
    assert_eq!(
        keyframes_of(&window, text, PropertyId::Colour),
        vec![40],
        "frame 60 is 40 frames into the text"
    );
}

// --- where a clip is on the picture ---

fn drag_value(window: &Window, label: &str) -> f64 {
    window
        .harness
        .get_by_label(label)
        .accesskit_node()
        .numeric_value()
        .unwrap_or_else(|| panic!("{label} has a number"))
}

#[test]
fn a_video_has_rows_for_its_position_its_size_and_how_far_it_is_turned_that_show_where_it_is() {
    let (mut window, clip) = window_with_the_video_selected();
    window.do_this(Command::SetClipPlacement {
        clip,
        placement: Placement::new(
            CanvasArea::new(0.1, 0.2, 0.4, 0.3),
            crate::project::Angle::from_degrees(30.0),
        ),
    });

    assert!(
        (drag_value(&window, "x") - 30.0).abs() < 0.01,
        "the middle is at 30% across: {}",
        drag_value(&window, "x")
    );
    assert!((drag_value(&window, "y") - 35.0).abs() < 0.01);
    assert!((drag_value(&window, "w") - 40.0).abs() < 0.01);
    assert!((drag_value(&window, "h") - 30.0).abs() < 0.01);
    assert!((drag_value(&window, "Rotation") - 30.0).abs() < 0.01);
}

#[test]
fn the_rows_change_where_the_video_is_and_one_row_leaves_the_other_things_alone() {
    let (mut window, clip) = window_with_the_video_selected();
    window.do_this(Command::SetClipPlacement {
        clip,
        placement: Placement::upright(CanvasArea::new(0.0, 0.0, 0.5, 0.5)),
    });

    window.set_slider("x", 60.0);

    let placed = window.application().project().placement_at(clip, frame(0)).unwrap();
    assert!((placed.centre().0 - 0.6).abs() < 1e-4, "{placed:?}");
    assert!(
        (placed.centre().1 - 0.25).abs() < 1e-4,
        "the middle down is as it was: {placed:?}"
    );
    assert!((placed.area.width() - 0.5).abs() < 1e-4, "and so is how big it is");

    window.set_slider("Rotation", 90.0);
    let turned = window.application().project().placement_at(clip, frame(0)).unwrap();
    assert!((turned.rotation.degrees() - 90.0).abs() < 1e-3);
    assert!((turned.centre().0 - 0.6).abs() < 1e-4, "it is still where it was put");

    window.set_slider("w", 20.0);
    let narrower = window.application().project().placement_at(clip, frame(0)).unwrap();
    assert!((narrower.area.width() - 0.2).abs() < 1e-4);
    assert!(
        (narrower.centre().0 - 0.6).abs() < 1e-4,
        "it is made narrower round its middle"
    );
}

#[test]
fn a_sound_has_no_place_on_the_picture_to_show() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(100, 10));
    let sound = window.clip(0, 0);
    select(&mut window, sound);
    assert!(window.harness.query_by_label("Position").is_none());
    assert!(window.harness.query_by_label("Rotation").is_none());
}

#[test]
fn the_rows_for_a_place_that_is_animated_show_where_it_is_at_the_playhead_and_change_it_there() {
    let (mut window, clip) = window_with_the_video_selected();
    let target = AnimationTarget::Clip(clip);
    window.do_this(Command::SetClipPlacement {
        clip,
        placement: Placement::upright(CanvasArea::new(0.0, 0.0, 0.5, 0.5)),
    });
    window.do_this(Command::AddKeyframe {
        target,
        property: PropertyId::Position,
        frame: frame(0),
    });
    window.move_current_frame_to(100);
    window.set_slider("x", 75.0);
    assert_eq!(keyframes_of(&window, clip, PropertyId::Position), vec![0, 100]);

    window.move_current_frame_to(50);
    assert!(
        (drag_value(&window, "x") - 50.0).abs() < 0.01,
        "half way from 25% to 75%: {}",
        drag_value(&window, "x")
    );
    window.set_slider("y", 80.0);

    assert_eq!(keyframes_of(&window, clip, PropertyId::Position), vec![0, 50, 100]);
}

// --- turning more than half a turn ---

#[test]
fn the_field_for_how_far_something_is_turned_takes_more_than_half_a_turn_either_way_and_shows_what_it_was_given() {
    let (mut window, clip) = window_with_the_video_selected();
    for degrees in [200.0, 720.0, -540.0, 3600.0] {
        window.set_slider("Rotation", degrees);

        let placed = window.application().project().placement_at(clip, frame(0)).unwrap();
        assert_eq!(placed.rotation.degrees(), degrees as f32);
        assert_eq!(
            drag_value(&window, "Rotation"),
            degrees,
            "the field shows the turn, and not the same angle brought back to between -180 and 180"
        );
    }
}

#[test]
fn two_whole_turns_typed_for_a_later_keyframe_make_the_clip_go_round_twice_between_them() {
    let (mut window, clip) = window_with_the_video_selected();
    let target = AnimationTarget::Clip(clip);
    window.do_this(Command::AddKeyframe {
        target,
        property: PropertyId::Rotation,
        frame: frame(0),
    });
    window.move_current_frame_to(100);

    window.set_slider("Rotation", 720.0);

    assert_eq!(keyframes_of(&window, clip, PropertyId::Rotation), vec![0, 100]);
    for (at, expected) in [(0, 0.0), (25, 180.0), (50, 360.0), (75, 540.0), (100, 720.0)] {
        window.move_current_frame_to(at);
        assert_eq!(drag_value(&window, "Rotation"), expected, "at frame {at}");
    }
}

#[test]
fn dragging_the_field_for_how_far_something_is_turned_goes_on_past_a_whole_turn() {
    let (mut window, clip) = window_with_the_video_selected();
    let field = window.rectangle_of("Rotation").center();

    window.drag(field, field + vec2(500.0, 0.0), PointerButton::Primary);

    let turned = window
        .application()
        .project()
        .placement_at(clip, frame(0))
        .unwrap()
        .rotation
        .degrees();
    assert!(turned > 360.0, "dragged 500 points, it is {turned} degrees");
}
