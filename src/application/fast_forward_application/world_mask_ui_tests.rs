// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The window side of laying the mask of a number **in space** (`docs/shader-masks-design.md`): one mask button for every mask, with a diamond round its face and a hover text that says
//! world space is available when the shader says the number can be read in space; and the window of the mask, which then has the choice between over the picture and in space, the three
//! shapes, and where each is put. A number that its shader does not say can be read in space has none of it: its button, its window and its thumbnail are as they were.

use egui::Shape;
use egui_kittest::kittest::{NodeT, Queryable};

use super::mask_ui_tests::window_with_this_shader_on_the_video;
use super::test_window::Window;
use super::undo_tests::undo;
use crate::application::commands::Command;
use crate::application::mask_settings_window::{
    HEIGHT_NAME, IN_SPACE_LABEL, OVER_THE_PICTURE_LABEL, POSITION_NAMES, SHAPE_LABEL, TURN_NAMES, WIDTH_NAME,
    WhichValue, name_of_a_shape,
};
use crate::application::properties_panel::{
    label_of_the_thumbnail, mask_button_label, mask_button_label_with_world_space,
};
use crate::project::{
    AnimationTarget, ClipId, DrivenValue, EffectInstanceId, EffectKind, MaskChannel, MaskSpace, Member, TrackId,
    WorldPlacement, WorldShape,
};

/// A shader with a number that says it works in space (*Reach*, place 0) and one that does not (*How much*, place 1).
const WORKS_IN_SPACE: &str = r#"/*{
	"WORLD_SIZE": 7.5,
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "reach", "LABEL": "Reach", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5, "WORLD_SPACE": true },
		{ "NAME": "amount", "LABEL": "How much", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 }
	]
}*/
void main() { isf_world_position = vec3(isf_FragNormCoord, 0.0); gl_FragColor = IMG_THIS_NORM_PIXEL(inputImage) * reach * amount; }
"#;

const REACH: usize = 0;
const HOW_MUCH: usize = 1;

fn the_shader() -> EffectKind {
    let file = crate::isf::read(WORKS_IN_SPACE).unwrap();
    EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "space.fs", WORKS_IN_SPACE)).unwrap()
}

fn which(clip: ClipId, slot: usize) -> WhichValue {
    WhichValue {
        owner: Member::Clip(clip),
        instance: EffectInstanceId::FIRST,
        slot,
    }
}

fn read_from(window: &mut Window, clip: ClipId, slot: usize, driven: DrivenValue) {
    window.do_this(Command::SetDrivenValue {
        owner: Member::Clip(clip),
        instance: EffectInstanceId::FIRST,
        slot,
        driven: Some(driven),
    });
}

fn driven_now(window: &Window, clip: ClipId, slot: usize) -> Option<DrivenValue> {
    window
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .driven_values()
        .of(slot)
        .copied()
}

fn the_space_now(window: &Window, clip: ClipId) -> MaskSpace {
    driven_now(window, clip, REACH)
        .expect("the number is read from a mask")
        .space
}

fn is_shown(window: &Window, label: &str) -> bool {
    window.harness.query_by_label(label).is_some()
}

/// A window with the shader on the video, the number *Reach* read from the other track as brightness, and the window of its mask open.
fn window_with_the_mask_of_reach_open() -> (Window, ClipId, TrackId) {
    let (mut window, clip, the_other) = window_with_this_shader_on_the_video(the_shader());
    read_from(
        &mut window,
        clip,
        REACH,
        DrivenValue::from_a_track(the_other, MaskChannel::Brightness, 0.5),
    );
    window.do_this(Command::ShowTheMaskSettings(which(clip, REACH)));
    window.settle();
    (window, clip, the_other)
}

/// Whether a diamond (a closed line of four points) is drawn within `reach` of `middle`.
fn a_diamond_is_drawn_round(window: &Window, middle: egui::Pos2, reach: f32) -> bool {
    window.shapes_drawn().into_iter().any(|shape| match shape {
        Shape::Path(path) if path.closed && path.points.len() == 4 => {
            let centre = path
                .points
                .iter()
                .fold(egui::Vec2::ZERO, |sum, point| sum + point.to_vec2())
                / 4.0;
            (centre.to_pos2() - middle).length() < reach
        }
        _ => false,
    })
}

// --- the button

#[test]
fn one_button_reads_a_number_from_a_mask_and_it_says_in_its_name_that_the_number_can_be_read_in_space_when_it_can() {
    let (window, ..) = window_with_this_shader_on_the_video(the_shader());

    assert!(is_shown(&window, &mask_button_label_with_world_space("reach")));
    assert!(
        !is_shown(&window, &mask_button_label("reach")),
        "not the plain one as well: it is one button"
    );
    // The other number of the shader says nothing of space: its button is the one it always was.
    assert!(is_shown(&window, &mask_button_label("how much")));
    assert!(!is_shown(&window, &mask_button_label_with_world_space("how much")));
}

#[test]
fn the_button_of_a_number_that_can_be_read_in_space_has_a_diamond_round_its_face_and_the_other_has_not() {
    let (mut window, ..) = window_with_this_shader_on_the_video(the_shader());
    let with_space = window
        .rectangle_of(&mask_button_label_with_world_space("reach"))
        .center();
    let without = window.rectangle_of(&mask_button_label("how much")).center();

    // (The buttons are drawn while the pointer is over the row.)
    window.move_pointer_to(with_space);
    window.settle();
    let with_a_diamond = a_diamond_is_drawn_round(&window, with_space, 3.0);
    window.move_pointer_to(without);
    window.settle();
    let plain_has_one = a_diamond_is_drawn_round(&window, without, 3.0);

    assert!(
        with_a_diamond,
        "no diamond round the button of the number that can be read in space"
    );
    assert!(!plain_has_one, "a diamond round the plain button");
}

#[test]
fn the_hover_text_of_the_button_says_that_world_space_options_are_available_and_the_plain_one_does_not() {
    let (mut window, ..) = window_with_this_shader_on_the_video(the_shader());
    let with_space = window
        .rectangle_of(&mask_button_label_with_world_space("reach"))
        .center();
    window.move_pointer_to(with_space);
    window.settle();
    window.harness.run_steps(30);

    assert!(
        window
            .harness
            .query_by_label_contains("World space options are available")
            .is_some(),
        "the hover text of the button of a number that can be read in space"
    );

    let without = window.rectangle_of(&mask_button_label("how much")).center();
    window.move_pointer_to(without);
    window.settle();
    window.harness.run_steps(30);

    assert!(window.harness.query_by_label_contains("World space").is_none());
    assert!(
        window.harness.query_by_label_contains("pixel by pixel").is_some(),
        "its own hover text"
    );
}

#[test]
fn the_button_opens_the_window_of_the_mask_whatever_the_number_can_be() {
    let (mut window, clip, _) = window_with_this_shader_on_the_video(the_shader());

    let button = window
        .rectangle_of(&mask_button_label_with_world_space("reach"))
        .center();
    window.click(button);

    assert_eq!(
        window.application().interface.mask_settings_window_open_for,
        Some(which(clip, REACH))
    );
}

// --- the window

#[test]
fn the_window_of_a_number_that_can_be_read_in_space_has_the_choice_and_that_of_one_that_cannot_has_not() {
    let (mut window, clip, the_other) = window_with_this_shader_on_the_video(the_shader());
    read_from(
        &mut window,
        clip,
        HOW_MUCH,
        DrivenValue::from_a_track(the_other, MaskChannel::Red, 0.5),
    );
    window.do_this(Command::ShowTheMaskSettings(which(clip, HOW_MUCH)));
    window.settle();
    for label in [
        OVER_THE_PICTURE_LABEL,
        IN_SPACE_LABEL,
        SHAPE_LABEL,
        POSITION_NAMES[0],
        TURN_NAMES[0],
        WIDTH_NAME,
    ] {
        assert!(
            !is_shown(&window, label),
            "{label} is in the window of a number that cannot be read in space"
        );
    }

    window.do_this(Command::ShowTheMaskSettings(which(clip, REACH)));
    window.settle();
    read_from(
        &mut window,
        clip,
        REACH,
        DrivenValue::from_a_track(the_other, MaskChannel::Red, 0.5),
    );
    window.settle();

    assert!(is_shown(&window, OVER_THE_PICTURE_LABEL));
    assert!(is_shown(&window, IN_SPACE_LABEL));
    assert!(
        !is_shown(&window, SHAPE_LABEL),
        "no shape while the mask is over the picture"
    );
}

#[test]
fn the_choices_of_space_are_not_there_to_use_until_a_source_is_chosen() {
    let (mut window, clip, _) = window_with_this_shader_on_the_video(the_shader());
    window.do_this(Command::ShowTheMaskSettings(which(clip, REACH)));
    window.settle();

    let button = window.rectangle_of(IN_SPACE_LABEL).center();
    window.click(button);

    assert_eq!(driven_now(&window, clip, REACH), None, "nothing is read from anywhere");
    assert!(
        window
            .harness
            .get_by_label(IN_SPACE_LABEL)
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn choosing_in_space_lays_the_mask_on_a_plane_at_the_middle_and_choosing_over_the_picture_takes_it_back() {
    let (mut window, clip, _) = window_with_the_mask_of_reach_open();
    assert_eq!(the_space_now(&window, clip), MaskSpace::Screen);

    let in_space = window.rectangle_of(IN_SPACE_LABEL).center();
    window.click(in_space);

    // (As wide and tall as the shape of the shader, which says it is 7.5 units across.)
    assert_eq!(
        the_space_now(&window, clip),
        MaskSpace::World(WorldPlacement {
            width: 7.5,
            height: 7.5,
            ..WorldPlacement::default()
        })
    );
    for label in [SHAPE_LABEL, POSITION_NAMES[2], TURN_NAMES[2], WIDTH_NAME, HEIGHT_NAME] {
        assert!(is_shown(&window, label), "{label}");
    }
    let over = window.rectangle_of(OVER_THE_PICTURE_LABEL).center();
    window.click(over);

    assert_eq!(the_space_now(&window, clip), MaskSpace::Screen);
    assert!(!is_shown(&window, SHAPE_LABEL));
}

#[test]
fn choosing_the_other_shapes_keeps_where_the_shape_is_and_shows_only_the_sizes_that_the_shape_uses() {
    let (mut window, clip, _) = window_with_the_mask_of_reach_open();
    let in_space = window.rectangle_of(IN_SPACE_LABEL).center();
    window.click(in_space);
    window.set_the_nth_slider(POSITION_NAMES[0], 0, 1.5);

    for (shape, uses_width, uses_height) in [
        (WorldShape::Sphere, false, false),
        (WorldShape::Cylinder, false, true),
        (WorldShape::Plane, true, true),
    ] {
        let button = window.rectangle_of(name_of_a_shape(shape)).center();
        window.click(button);

        let MaskSpace::World(placement) = the_space_now(&window, clip) else {
            panic!("the mask is in space");
        };
        assert_eq!(placement.shape, shape);
        assert_eq!(placement.position[0], 1.5, "{shape:?}: where it is is kept");
        assert_eq!(is_shown(&window, WIDTH_NAME), uses_width, "{shape:?}: the width");
        assert_eq!(is_shown(&window, HEIGHT_NAME), uses_height, "{shape:?}: the height");
    }
}

#[test]
fn the_numbers_of_the_position_the_turn_and_the_size_are_each_a_box_that_changes_only_its_own_number() {
    let (mut window, clip, _) = window_with_the_mask_of_reach_open();
    let in_space = window.rectangle_of(IN_SPACE_LABEL).center();
    window.click(in_space);

    window.set_the_nth_slider(POSITION_NAMES[0], 0, 1.5);
    window.set_the_nth_slider(POSITION_NAMES[1], 0, -2.0);
    window.set_the_nth_slider(POSITION_NAMES[2], 0, 0.25);
    window.set_the_nth_slider(TURN_NAMES[0], 0, 30.0);
    window.set_the_nth_slider(TURN_NAMES[1], 0, -10.0);
    window.set_the_nth_slider(TURN_NAMES[2], 0, 45.0);
    window.set_the_nth_slider(WIDTH_NAME, 0, 4.5);
    window.set_the_nth_slider(HEIGHT_NAME, 0, 2.5);

    assert_eq!(
        the_space_now(&window, clip),
        MaskSpace::World(WorldPlacement {
            shape: WorldShape::Plane,
            position: [1.5, -2.0, 0.25],
            yaw_pitch_roll_degrees: [30.0, -10.0, 45.0],
            width: 4.5,
            height: 2.5,
        })
    );
    // And the rest of what the mask is is as it was.
    let driven = driven_now(&window, clip, REACH).unwrap();
    assert_eq!(driven.channel, MaskChannel::Brightness);
    assert_eq!(driven.value_where_nothing, 0.5);
}

#[test]
fn a_size_that_is_not_one_cannot_be_typed_in() {
    let (mut window, clip, _) = window_with_the_mask_of_reach_open();
    let in_space = window.rectangle_of(IN_SPACE_LABEL).center();
    window.click(in_space);

    window.set_the_nth_slider(WIDTH_NAME, 0, -3.0);
    window.set_the_nth_slider(HEIGHT_NAME, 0, 0.0);

    let MaskSpace::World(placement) = the_space_now(&window, clip) else {
        panic!("the mask is in space");
    };
    assert!(placement.is_well_formed(), "{placement:?}");
    assert!(placement.width > 0.0 && placement.height > 0.0);
}

#[test]
fn laying_the_mask_in_space_is_joined_with_the_other_choices_about_the_number_into_one_step_that_undoing_takes_back() {
    let (mut window, clip, the_other) = window_with_this_shader_on_the_video(the_shader());
    let before = window.application().project().clone();
    read_from(
        &mut window,
        clip,
        REACH,
        DrivenValue::from_a_track(the_other, MaskChannel::Brightness, 0.5),
    );
    window.do_this(Command::ShowTheMaskSettings(which(clip, REACH)));
    window.settle();
    let in_space = window.rectangle_of(IN_SPACE_LABEL).center();

    window.click(in_space);
    assert_ne!(the_space_now(&window, clip), MaskSpace::Screen);
    undo(&mut window);

    assert_eq!(window.application().project(), &before);
}

#[test]
fn the_thumbnail_of_a_number_whose_mask_is_in_space_is_still_the_way_in_and_says_the_shape() {
    let (mut window, clip, the_other) = window_with_this_shader_on_the_video(the_shader());
    let sphere = DrivenValue::from_a_track(the_other, MaskChannel::Green, 0.5).with_the_space(MaskSpace::World(
        WorldPlacement::default().with_the_shape(WorldShape::Sphere),
    ));
    read_from(&mut window, clip, REACH, sphere);
    window.settle();

    assert!(is_shown(&window, &label_of_the_thumbnail("reach")));
    assert!(
        !is_shown(&window, &mask_button_label_with_world_space("reach")),
        "the thumbnail takes the place of the button while the number is read from a mask"
    );
    let thumbnail = window.rectangle_of(&label_of_the_thumbnail("reach")).center();
    window.click(thumbnail);
    assert_eq!(
        window.application().interface.mask_settings_window_open_for,
        Some(which(clip, REACH))
    );
    assert_eq!(
        crate::application::properties_panel::words_about_the_space(MaskSpace::World(
            WorldPlacement::default().with_the_shape(WorldShape::Sphere)
        ))
        .as_deref(),
        Some("sphere")
    );
    assert_eq!(
        crate::application::properties_panel::words_about_the_space(MaskSpace::Screen),
        None
    );
}
