// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A number of a shader whose mask is laid **in space** (`docs/shader-masks-design.md`): which numbers can be (those their shader says can), what the project refuses, and how the room among the
//! numbers of a call is shared between graphs and places in space.

use super::driven_value_tests::{a_graph, driven_of, from, three_tracks};
use super::*;

/// A shader with a number `reach` that says `WORLD_SPACE` (place 0), a number `amount` that does not (place 1), another that does, `spread` (place 2), and a colour.
const WORKS_IN_SPACE: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"reach\", \"TYPE\": \"float\", \"MIN\": 1.0, \"MAX\": 9.0, \"DEFAULT\": 4.0, \"WORLD_SPACE\": true }, { \"NAME\": \"amount\", \"TYPE\": \"float\", \"DEFAULT\": 0.25 }, { \"NAME\": \"spread\", \"TYPE\": \"float\", \"DEFAULT\": 0.5, \"WORLD_SPACE\": true }, { \"NAME\": \"tint\", \"TYPE\": \"color\", \"DEFAULT\": [1.0, 0.0, 0.0, 1.0] } ] }*/\nvoid main() { isf_world_position = vec3(isf_FragNormCoord, 0.0); gl_FragColor = vec4(vec3(reach * amount * spread), 1.0); }\n";

const REACH: usize = 0;
const AMOUNT: usize = 1;
const SPREAD: usize = 2;
const THE_COLOUR: usize = 3;

fn works_in_space() -> EffectKind {
    let file = crate::isf::read(WORKS_IN_SPACE).unwrap();
    EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "space.fs", WORKS_IN_SPACE)).unwrap()
}

/// A shader with `count` numbers that have a range and say `WORLD_SPACE`, called `p0`, `p1`, ..., that sets the point of space.
fn a_shader_with_this_many_numbers_that_work_in_space(count: usize) -> EffectKind {
    let floats: Vec<String> = (0..count)
        .map(|number| {
            format!("{{ \"NAME\": \"p{number}\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 1.0, \"DEFAULT\": 0.5, \"WORLD_SPACE\": true }}")
        })
        .collect();
    let text = format!(
        "/*{{ \"INPUTS\": [ {{ \"NAME\": \"inputImage\", \"TYPE\": \"image\" }}, {} ] }}*/\nvoid main() {{ isf_world_position = vec3(0.0); gl_FragColor = IMG_THIS_PIXEL(inputImage) * p0; }}\n",
        floats.join(", ")
    );
    let file = crate::isf::read(&text).unwrap();
    EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "many_in_space.fs", &text)).unwrap()
}

fn in_space(track: TrackId, shape: WorldShape) -> Option<DrivenValue> {
    Some(
        DrivenValue::from_a_track(track, MaskChannel::Brightness, 0.5)
            .with_the_space(MaskSpace::World(WorldPlacement::default().with_the_shape(shape))),
    )
}

fn effect_of(project: &Project, owner: Member, instance: EffectInstanceId) -> Effect {
    project
        .chain_of_the_member(owner)
        .unwrap()
        .iter()
        .find(|effect| effect.instance() == instance)
        .unwrap()
        .clone()
}

#[test]
fn a_number_that_its_shader_says_works_in_space_can_have_its_mask_laid_in_space_on_each_shape() {
    let (mut project, [_, middle, _], [clip, ..]) = three_tracks();
    let owner = Member::Clip(clip);
    let instance = project
        .add_effect(AnimationTarget::Clip(clip), works_in_space())
        .unwrap();
    let slider_before = effect_of(&project, owner, instance).values().to_vec();

    for shape in WorldShape::ALL {
        project
            .set_driven_value(owner, instance, REACH, in_space(middle, shape))
            .unwrap();

        let found = driven_of(&project, owner, instance, REACH).unwrap();
        assert_eq!(found.space.for_the_shader(), crate::isf::Space::World(shape));
        assert_eq!(found.source, MaskSource::Track(middle));
    }
    assert_eq!(
        effect_of(&project, owner, instance).values(),
        slider_before,
        "the slider is kept"
    );
    // The other number that says so can be too, with a placement of its own.
    let moved = WorldPlacement {
        position: [1.0, 2.0, 3.0],
        ..WorldPlacement::default()
    };
    project
        .set_driven_value(
            owner,
            instance,
            SPREAD,
            Some(DrivenValue::from_a_track(middle, MaskChannel::Red, 0.1).with_the_space(MaskSpace::World(moved))),
        )
        .unwrap();
    assert_eq!(
        driven_of(&project, owner, instance, SPREAD).unwrap().space,
        MaskSpace::World(moved)
    );
    assert_eq!(
        driven_of(&project, owner, instance, REACH).unwrap().space,
        MaskSpace::World(WorldPlacement::default().with_the_shape(WorldShape::Cylinder)),
        "and the first is as it was"
    );
}

#[test]
fn a_number_that_its_shader_does_not_say_works_in_space_is_refused_and_can_still_be_read_over_the_picture() {
    let (mut project, [_, middle, _], [clip, ..]) = three_tracks();
    let owner = Member::Clip(clip);
    let instance = project
        .add_effect(AnimationTarget::Clip(clip), works_in_space())
        .unwrap();
    let before = effect_of(&project, owner, instance);

    let refused = project.set_driven_value(owner, instance, AMOUNT, in_space(middle, WorldShape::Plane));

    assert!(
        matches!(refused, Err(EditError::ValueCannotBeDriven { reason }) if reason.contains("in space")),
        "{refused:?}"
    );
    assert_eq!(effect_of(&project, owner, instance), before, "nothing changed");
    assert!(!before.can_the_value_be_laid_in_space(AMOUNT));
    assert!(before.can_the_value_be_laid_in_space(REACH));
    assert!(before.can_the_value_be_laid_in_space(SPREAD));
    assert!(!before.can_the_value_be_laid_in_space(THE_COLOUR));
    assert!(!before.can_the_value_be_laid_in_space(99));
    assert_eq!(
        project.set_driven_value(owner, instance, AMOUNT, from(middle, MaskChannel::Red, 0.5)),
        Ok(())
    );
}

#[test]
fn a_shader_that_says_nothing_about_space_offers_none() {
    let (mut project, [_, middle, _], [clip, ..]) = three_tracks();
    let owner = Member::Clip(clip);
    let plain = {
        let text = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"amount\", \"TYPE\": \"float\", \"DEFAULT\": 0.25 } ] }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(inputImage) * amount; }\n";
        let file = crate::isf::read(text).unwrap();
        EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "plain.fs", text)).unwrap()
    };
    let instance = project.add_effect(AnimationTarget::Clip(clip), plain).unwrap();

    let refused = project.set_driven_value(owner, instance, 0, in_space(middle, WorldShape::Sphere));

    assert!(
        matches!(refused, Err(EditError::ValueCannotBeDriven { .. })),
        "{refused:?}"
    );
    assert!(!effect_of(&project, owner, instance).can_the_value_be_laid_in_space(0));
}

#[test]
fn an_effect_that_is_not_a_shader_offers_no_space() {
    let (mut project, [_, middle, _], [clip, ..]) = three_tracks();
    let owner = Member::Clip(clip);
    let instance = project
        .add_effect(AnimationTarget::Clip(clip), EffectKind::HUE_SHIFT)
        .unwrap();

    let effect = effect_of(&project, owner, instance);
    let refused = project.set_driven_value(owner, instance, 0, in_space(middle, WorldShape::Plane));

    assert!(!effect.can_the_value_be_laid_in_space(0));
    assert!(
        matches!(refused, Err(EditError::ValueCannotBeDriven { .. })),
        "{refused:?}"
    );
}

#[test]
fn a_placement_that_is_not_one_cannot_be_kept() {
    let (mut project, [_, middle, _], [clip, ..]) = three_tracks();
    let owner = Member::Clip(clip);
    let instance = project
        .add_effect(AnimationTarget::Clip(clip), works_in_space())
        .unwrap();
    let nowhere = WorldPlacement {
        position: [f32::NAN, 0.0, 0.0],
        ..WorldPlacement::default()
    };
    let flat = WorldPlacement {
        width: 0.0,
        ..WorldPlacement::default()
    };

    for placement in [nowhere, flat] {
        let refused = project.set_driven_value(
            owner,
            instance,
            REACH,
            Some(DrivenValue::from_a_track(middle, MaskChannel::Red, 0.5).with_the_space(MaskSpace::World(placement))),
        );
        assert!(
            matches!(refused, Err(EditError::ValueCannotBeDriven { .. })),
            "{placement:?}: {refused:?}"
        );
        assert_eq!(driven_of(&project, owner, instance, REACH), None);
    }
}

#[test]
fn a_place_in_space_takes_nine_numbers_of_the_call_and_shares_the_room_with_the_graphs() {
    let (mut project, [_, middle, _], [clip, ..]) = three_tracks();
    let owner = Member::Clip(clip);
    // (The shader's own four and 40 numbers are 44; one place in space is 53, a graph more is 69: a call has 64.)
    let instance = project
        .add_effect(
            AnimationTarget::Clip(clip),
            a_shader_with_this_many_numbers_that_work_in_space(40),
        )
        .unwrap();
    let in_space_with_a_graph = in_space(middle, WorldShape::Plane).unwrap().with_the_map(a_graph());

    assert_eq!(
        project.set_driven_value(owner, instance, 3, in_space(middle, WorldShape::Plane)),
        Ok(())
    );
    let with_a_graph_too = project.set_driven_value(owner, instance, 3, Some(in_space_with_a_graph));
    assert!(
        matches!(with_a_graph_too, Err(EditError::ValueCannotBeDriven { reason }) if reason.contains("no room")),
        "{with_a_graph_too:?}"
    );
    let another_graph = project.set_driven_value(
        owner,
        instance,
        4,
        Some(DrivenValue::from_a_track(middle, MaskChannel::Red, 0.5).with_the_map(a_graph())),
    );
    assert!(
        matches!(another_graph, Err(EditError::ValueCannotBeDriven { reason }) if reason.contains("no room")),
        "{another_graph:?}"
    );
    // Four more places in space fit (44 + 9 * 2 = 62), and a third does not.
    assert_eq!(
        project.set_driven_value(owner, instance, 4, in_space(middle, WorldShape::Sphere)),
        Ok(())
    );
    let third = project.set_driven_value(owner, instance, 5, in_space(middle, WorldShape::Cylinder));
    assert!(
        matches!(third, Err(EditError::ValueCannotBeDriven { reason }) if reason.contains("no room")),
        "{third:?}"
    );
    // A mask over the picture takes no room, and a place in space let go of makes room again.
    assert_eq!(
        project.set_driven_value(owner, instance, 5, from(middle, MaskChannel::Red, 0.5)),
        Ok(())
    );
    assert_eq!(project.set_driven_value(owner, instance, 4, None), Ok(()));
    assert_eq!(
        project.set_driven_value(owner, instance, 6, in_space(middle, WorldShape::Cylinder)),
        Ok(())
    );
    assert!(effect_of(&project, owner, instance).has_room_for_the_numbers_of_its_masks());
}

#[test]
fn a_shader_with_no_room_for_a_place_in_space_still_reads_its_masks_over_the_picture() {
    let (mut project, [_, middle, _], [clip, ..]) = three_tracks();
    let owner = Member::Clip(clip);
    // The four of the shader and 58 numbers are 62: nine more do not fit.
    let instance = project
        .add_effect(
            AnimationTarget::Clip(clip),
            a_shader_with_this_many_numbers_that_work_in_space(58),
        )
        .unwrap();

    let refused = project.set_driven_value(owner, instance, 0, in_space(middle, WorldShape::Plane));

    assert!(
        matches!(refused, Err(EditError::ValueCannotBeDriven { reason }) if reason.contains("no room")),
        "{refused:?}"
    );
    assert_eq!(
        project.set_driven_value(owner, instance, 0, from(middle, MaskChannel::Red, 0.5)),
        Ok(())
    );
}

#[test]
fn the_effect_says_why_a_value_cannot_be_laid_in_space() {
    let (mut project, _, [clip, ..]) = three_tracks();
    let owner = Member::Clip(clip);
    let instance = project
        .add_effect(AnimationTarget::Clip(clip), works_in_space())
        .unwrap();
    let effect = effect_of(&project, owner, instance);

    assert_eq!(effect.why_the_value_cannot_be_in_space(REACH), None);
    assert_eq!(effect.why_the_value_cannot_be_in_space(SPREAD), None);
    assert!(
        effect
            .why_the_value_cannot_be_in_space(AMOUNT)
            .unwrap()
            .contains("in space")
    );
    assert!(effect.why_the_value_cannot_be_in_space(THE_COLOUR).is_some());
}

#[test]
fn a_mask_laid_in_space_begins_as_big_as_the_shape_the_shader_says_it_makes_and_the_default_size_if_it_says_nothing() {
    let (mut project, _, [clip, ..]) = three_tracks();
    let owner = Member::Clip(clip);
    let plain = project
        .add_effect(AnimationTarget::Clip(clip), works_in_space())
        .unwrap();
    let sized_text = WORKS_IN_SPACE.replacen("/*{ ", "/*{ \"WORLD_SIZE\": 23.0, ", 1);
    let sized_kind = {
        let file = crate::isf::read(&sized_text).unwrap();
        EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "sized.fs", &sized_text)).unwrap()
    };
    let sized = project.add_effect(AnimationTarget::Clip(clip), sized_kind).unwrap();

    let default = effect_of(&project, owner, plain).placement_in_space_to_begin_with();
    let big = effect_of(&project, owner, sized).placement_in_space_to_begin_with();

    assert_eq!(default, WorldPlacement::default());
    assert_eq!(
        big,
        WorldPlacement {
            width: 23.0,
            height: 23.0,
            ..WorldPlacement::default()
        }
    );
}
