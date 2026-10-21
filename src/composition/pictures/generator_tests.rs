// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the picture plan makes of a generator clip (phase 5, piece 5.4; design H8): **a layer** that is a solid colour in the place the clip says, with the effects it has; it
//! hides what is behind it only if it fills the picture with a colour that is not see-through and stays so.

use super::tests::{frame, long_video, plan};
use super::*;
use crate::composition::test_projects::{project_of, random_project_with_generators};
use crate::project::{Angle, GeneratorClip, GeneratorKind, GroupKind};
use crate::test_support::{make_a_keyframe, put_a_hue_shift_on};

const RED: TextColour = TextColour::new(255, 0, 0, 255);
const GREEN: TextColour = TextColour::new(0, 255, 0, 255);

fn a_solid_colour(duration: u64, colour: TextColour) -> Clip {
    Clip::Generator(
        GeneratorClip::new(GeneratorKind::SolidColour, duration)
            .unwrap()
            .with_colour(colour),
    )
}

fn in_front_of_a_video(front: Clip) -> (Project, ClipId, ClipId) {
    let project = project_of(vec![vec![(0, front)], vec![(0, long_video(0, 40, 1))]]);
    let (front, video) = (
        project.tracks()[0].placement_at_index(0).unwrap().id,
        project.tracks()[1].placement_at_index(0).unwrap().id,
    );
    (project, front, video)
}

/// What is done to a solid colour in front of a video so that it does not hide it.
type ChangeOfASolidColour = Box<dyn Fn(&mut Project, ClipId)>;

fn clips_of_the_layers(plan: &PicturePlan, section: usize) -> Vec<ClipId> {
    plan.sections[section].layers.iter().map(|layer| layer.clip).collect()
}

#[test]
fn a_generator_clip_is_a_layer_that_is_a_solid_colour_over_the_whole_picture_until_it_is_put_somewhere() {
    let (project, solid, video) = in_front_of_a_video(a_solid_colour(40, RED));
    let mut project = project;
    // (See-through, so that it does not hide the video.)
    project
        .set_generator_colour(solid, TextColour::new(255, 0, 0, 100))
        .unwrap();

    let plan = plan(&project);

    assert_eq!(clips_of_the_layers(&plan, 0), [video, solid], "behind first");
    let layer = &plan.sections[0].layers[1];
    assert_eq!(
        layer.content,
        LayerContent::Solid {
            colour: TextColour::new(255, 0, 0, 100)
        }
    );
    assert_eq!(layer.placement, Placement::upright(CanvasArea::WHOLE_PICTURE));
    assert_eq!(layer.solid_colour_at(frame(5)), Some(TextColour::new(255, 0, 0, 100)));
    assert_eq!(
        plan.sections[0].layers[0].solid_colour_at(frame(5)),
        None,
        "a video is not one"
    );
    assert!(layer.video().is_none() && layer.text_at(frame(5)).is_none());
}

#[test]
fn a_generator_clip_is_where_the_clip_says_and_where_its_keyframes_move_it() {
    let (mut project, solid, _) = in_front_of_a_video(a_solid_colour(40, RED));
    let somewhere = Placement::new(CanvasArea::new(0.1, 0.2, 0.3, 0.4), Angle::from_degrees(30.0));
    project.set_clip_placement(solid, somewhere).unwrap();
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(solid),
        PropertyId::Position,
        10,
        PropertyValue::Pair(0.5, 0.5),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(solid),
        PropertyId::Position,
        30,
        PropertyValue::Pair(0.9, 0.5),
    );

    let plan = plan(&project);

    let layer = &plan.sections[0].layers[1];
    assert_eq!(layer.placement, somewhere);
    assert_eq!(layer.placement_at(frame(10)).centre(), (0.5, 0.5));
    let (across, down) = layer.placement_at(frame(20)).centre();
    assert!(
        (across - 0.7).abs() < 1e-5 && (down - 0.5).abs() < 1e-5,
        "half way: {across}, {down}"
    );
    assert_eq!(layer.placement_at(frame(10)).area.width(), somewhere.area.width());
}

#[test]
fn a_colour_with_keyframes_is_the_colour_they_say_at_each_frame_and_the_layer_keeps_the_colour_the_clip_says() {
    let (mut project, solid, _) = in_front_of_a_video(a_solid_colour(40, RED));
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(solid),
        PropertyId::Colour,
        0,
        PropertyValue::Colour(RED),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(solid),
        PropertyId::Colour,
        20,
        PropertyValue::Colour(GREEN),
    );

    let plan = plan(&project);

    let layer = &plan.sections[0].layers.last().unwrap();
    assert_eq!(layer.content, LayerContent::Solid { colour: RED });
    assert_eq!(layer.solid_colour_at(frame(0)), Some(RED));
    assert_eq!(
        layer.solid_colour_at(frame(10)),
        Some(TextColour::new(128, 128, 0, 255))
    );
    assert_eq!(layer.solid_colour_at(frame(20)), Some(GREEN));
    assert_eq!(layer.solid_colour_at(frame(39)), Some(GREEN));
}

#[test]
fn a_solid_colour_that_fills_the_picture_and_is_not_see_through_hides_what_is_behind_it() {
    let (project, solid, _) = in_front_of_a_video(a_solid_colour(40, RED));

    let plan = plan(&project);

    assert_eq!(clips_of_the_layers(&plan, 0), [solid], "the video cannot be seen");
    assert!(plan.sections[0].layers[0].covers_the_picture());
}

#[test]
fn a_solid_colour_that_is_see_through_moved_turned_or_animated_in_colour_does_not_hide_what_is_behind_it() {
    let somewhere = Placement::upright(CanvasArea::new(0.0, 0.0, 0.5, 0.5));
    let turned = Placement::new(CanvasArea::WHOLE_PICTURE, Angle::from_degrees(3.0));
    let cases: [(&str, ChangeOfASolidColour); 5] = [
        (
            "see-through",
            Box::new(|project, solid| {
                project
                    .set_generator_colour(solid, TextColour::new(255, 0, 0, 254))
                    .unwrap();
            }),
        ),
        (
            "smaller than the picture",
            Box::new(move |project, solid| {
                project.set_clip_placement(solid, somewhere).unwrap();
            }),
        ),
        (
            "turned",
            Box::new(move |project, solid| {
                project.set_clip_placement(solid, turned).unwrap();
            }),
        ),
        (
            "moved by keyframes",
            Box::new(|project, solid| {
                make_a_keyframe(
                    project,
                    AnimationTarget::Clip(solid),
                    PropertyId::Position,
                    10,
                    PropertyValue::Pair(0.5, 0.5),
                );
            }),
        ),
        (
            "coloured by keyframes",
            Box::new(|project, solid| {
                make_a_keyframe(
                    project,
                    AnimationTarget::Clip(solid),
                    PropertyId::Colour,
                    10,
                    PropertyValue::Colour(GREEN),
                );
            }),
        ),
    ];
    for (name, make_it_so) in cases {
        let (mut project, solid, video) = in_front_of_a_video(a_solid_colour(40, RED));
        make_it_so(&mut project, solid);

        let plan = plan(&project);

        assert_eq!(clips_of_the_layers(&plan, 0), [video, solid], "{name}");
        assert!(!plan.sections[0].layers[1].covers_the_picture(), "{name}");
    }
}

#[test]
fn what_a_solid_colour_that_has_effects_does_to_what_is_behind_it_is_by_its_effects() {
    // An effect that changes colours and nothing else keeps it hiding what is behind it; one that can make it clear (an opacity, a crop, a blur) does not.
    let (mut project, solid, video) = in_front_of_a_video(a_solid_colour(40, RED));
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(solid), 40.0);
    assert_eq!(clips_of_the_layers(&plan(&project), 0), [solid]);
    let layer_effects = &plan(&project).sections[0].layers[0].effects.len();
    assert_eq!(*layer_effects, 1, "its hue shift is its own effect");

    project
        .add_effect(AnimationTarget::Clip(solid), EffectKind::OPACITY)
        .unwrap();
    assert_eq!(
        clips_of_the_layers(&plan(&project), 0),
        [video, solid],
        "an opacity may let what is behind it show"
    );
}

#[test]
fn a_solid_colour_in_a_clip_group_hides_what_is_behind_it_in_the_group_and_the_group_hides_what_is_behind_it_if_its_effects_keep_the_opacity()
 {
    let build = |kind: EffectKind| {
        let mut project = project_of(vec![
            vec![(0, a_solid_colour(40, RED))],
            vec![(0, long_video(0, 40, 1))],
            vec![(0, long_video(0, 40, 1))],
        ]);
        let solid = project.tracks()[0].placement_at_index(0).unwrap().id;
        let inside = project.tracks()[1].placement_at_index(0).unwrap().id;
        let outside = project.tracks()[2].placement_at_index(0).unwrap().id;
        let group = project
            .create_group(GroupKind::ClipGroup, &[Member::Clip(solid), Member::Clip(inside)])
            .unwrap();
        // (An effect that is on with the value it begins as does nothing, and a group that has none that does something is not a picture of its own: the first value of
        // each of these two is the one that makes it do something.)
        let instance = project.add_effect_to_the_group(group, kind).unwrap();
        let first_value = PropertyId::effect_value(kind, instance, 0);
        let value = PropertyValue::Number(if kind == EffectKind::BLUR { 5.0 } else { 40.0 });
        project
            .set_effect_value_of_the_group(group, first_value, &value)
            .unwrap();
        (project, solid, inside, outside)
    };

    // A change of colours keeps what is opaque opaque: the group covers the picture, so the video outside it (behind it) is not seen. The one inside it is behind the solid
    // colour, which covers, so it is not seen either.
    let (project, solid, _, _) = build(EffectKind::HUE_SHIFT);
    let planned = plan(&project);
    assert_eq!(clips_of_the_layers(&planned, 0), [solid]);
    assert_eq!(planned.sections[0].groups.len(), 1);

    // A blur does not: the group has soft edges, so what is behind it is seen. What is behind the solid colour inside the group is not.
    let (project, solid, _, outside) = build(EffectKind::BLUR);
    let planned = plan(&project);
    assert_eq!(clips_of_the_layers(&planned, 0), [outside, solid]);
    assert_eq!(
        (
            planned.sections[0].groups[0].first_layer,
            planned.sections[0].groups[0].end_layer
        ),
        (1, 2),
        "the group is the solid colour alone, which is all that is seen of it"
    );
}

#[test]
fn projects_made_at_random_have_solid_colours_that_hide_what_is_behind_them_and_that_do_not_and_in_places() {
    let (mut solids, mut hiding, mut seen_over_something, mut somewhere_else) = (0, 0, 0, 0);
    for seed in 1..=300u64 {
        let mut random = crate::test_support::DeterministicRandomNumbers::with_seed(seed + 31_000);
        let plan = plan(&random_project_with_generators(&mut random));
        for section in &plan.sections {
            for (position, layer) in section.layers.iter().enumerate() {
                if layer.solid_colour_at(frame(section.first_timeline_frame)).is_none() {
                    continue;
                }
                solids += 1;
                hiding += usize::from(layer.covers_the_picture());
                seen_over_something += usize::from(position > 0);
                somewhere_else += usize::from(layer.placement.area != CanvasArea::WHOLE_PICTURE);
            }
        }
    }
    // (So that the projects made at random cannot stop having solid colours of each sort in them without this noticing: measured, 321, 125, 53 and 152.)
    assert!(
        solids > 250 && hiding > 80 && seen_over_something > 30 && somewhere_else > 100,
        "{solids} solid colours in the sections, {hiding} hide what is behind them, {seen_over_something} are over something, {somewhere_else} are not the whole picture"
    );
}
