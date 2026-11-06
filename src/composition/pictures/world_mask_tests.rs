// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the picture plan makes of a number of a shader whose mask is laid **in space** (`docs/shader-masks-design.md`): the shader that reads it is made for the shape, and the numbers of the
//! call carry the place of the shape **after the numbers of all the inputs and of the graphs**, one place after another in the order of the numbers of the effect.

use super::mask_tests::{first_clip_of, the_shader_operation};
use super::tests::{long_video, plan};
use super::*;
use crate::composition::test_projects::project_of;
use crate::project::{
    DrivenValue, Graph, MaskChannel, MaskMap, MaskSpace, Member, NUMBERS_OF_A_GRAPH, NUMBERS_OF_A_WORLD_PLACEMENT,
    WorldPlacement, WorldShape,
};

/// A shader whose two numbers say `WORLD_SPACE`: `amount` (0 to 1, 0.25 to begin with) and `strength` (2 to 6, 3 to begin with).
const WORKS_IN_SPACE: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"amount\", \"TYPE\": \"float\", \"DEFAULT\": 0.25, \"WORLD_SPACE\": true }, { \"NAME\": \"strength\", \"TYPE\": \"float\", \"MIN\": 2.0, \"MAX\": 6.0, \"DEFAULT\": 3.0, \"WORLD_SPACE\": true } ] }*/\nvoid main() { isf_world_position = vec3(isf_FragNormCoord, 0.0); gl_FragColor = IMG_THIS_PIXEL(inputImage) * amount * strength; }\n";

fn the_shader() -> EffectKind {
    let file = crate::isf::read(WORKS_IN_SPACE).unwrap();
    EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "space.fs", WORKS_IN_SPACE)).unwrap()
}

/// Puts the shader on the clip `clip` if it is not there, with its number in place `slot` read from `source` as `value` says.
fn read_in(project: &mut Project, clip: ClipId, slot: usize, value: DrivenValue) {
    let instance = match project
        .chain_of_the_member(Member::Clip(clip))
        .unwrap()
        .iter()
        .find(|effect| effect.kind() == the_shader())
    {
        Some(effect) => effect.instance(),
        None => project.add_effect(AnimationTarget::Clip(clip), the_shader()).unwrap(),
    };
    project
        .set_driven_value(Member::Clip(clip), instance, slot, Some(value))
        .unwrap();
}

fn laid_as(source: TrackId, placement: WorldPlacement) -> DrivenValue {
    DrivenValue::from_a_track(source, MaskChannel::Red, 0.5).with_the_space(MaskSpace::World(placement))
}

/// A project of a hidden track in front (the mask) and a track behind with a clip (the host), as the ids.
fn a_mask_track_and_a_host_clip() -> (Project, TrackId, ClipId) {
    let mut project = project_of(vec![vec![(0, long_video(0, 100, 1))], vec![(0, long_video(0, 100, 1))]]);
    let (mask_track, host_clip) = (project.tracks()[0].id(), first_clip_of(&project, 1));
    project.set_track_hidden(mask_track, true).unwrap();
    (project, mask_track, host_clip)
}

#[test]
fn a_number_whose_mask_is_laid_in_space_is_a_shader_made_for_the_shape_that_carries_the_place_after_the_numbers() {
    let (mut project, mask_track, host_clip) = a_mask_track_and_a_host_clip();
    let placement = WorldPlacement {
        position: [1.0, 2.0, 3.0],
        yaw_pitch_roll_degrees: [90.0, 0.0, 0.0],
        width: 5.0,
        height: 6.0,
        ..WorldPlacement::default()
    };
    let mut keys = Vec::new();
    for shape in WorldShape::ALL {
        read_in(
            &mut project,
            host_clip,
            0,
            laid_as(mask_track, placement.with_the_shape(shape)),
        );

        let (key, numbers, masks) = the_shader_operation(&plan(&project).sections[0], 10);

        assert!(key.contains("in-space-"), "{key}");
        assert!(key.contains(shape.word()), "{key}");
        assert_eq!(masks, [mask_track]);
        assert_eq!(numbers.len(), 4 + 2 + NUMBERS_OF_A_WORLD_PLACEMENT, "{shape:?}");
        assert_eq!(
            numbers[4..6],
            [0.5, 3.0],
            "0.5 where the mask has nothing, and the slider of the other"
        );
        assert_eq!(
            numbers[6..],
            placement.numbers(),
            "{shape:?}: the place comes after the numbers of the inputs"
        );
        keys.push(key);
    }
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), 3, "each shape is a shader of its own");
}

#[test]
fn a_mask_over_the_picture_and_a_mask_laid_in_space_are_two_shaders_and_only_the_second_has_a_place() {
    let (mut project, mask_track, host_clip) = a_mask_track_and_a_host_clip();
    read_in(
        &mut project,
        host_clip,
        0,
        DrivenValue::from_a_track(mask_track, MaskChannel::Red, 0.5),
    );
    let (over_the_picture, plain_numbers, _) = the_shader_operation(&plan(&project).sections[0], 10);

    read_in(
        &mut project,
        host_clip,
        0,
        laid_as(mask_track, WorldPlacement::default()),
    );
    let (in_space, space_numbers, _) = the_shader_operation(&plan(&project).sections[0], 10);

    assert_eq!(plain_numbers.len(), 4 + 2);
    assert_eq!(space_numbers.len(), 4 + 2 + NUMBERS_OF_A_WORLD_PLACEMENT);
    assert_ne!(over_the_picture, in_space);
    assert!(!over_the_picture.contains("in-space"), "{over_the_picture}");
}

#[test]
fn the_places_of_two_numbers_come_after_the_points_of_the_graphs_in_the_order_of_the_numbers_of_the_effect() {
    let (mut project, mask_track, host_clip) = a_mask_track_and_a_host_clip();
    let first = WorldPlacement {
        position: [1.0, 0.0, 0.0],
        ..WorldPlacement::default()
    };
    let second = WorldPlacement {
        position: [0.0, 2.0, 0.0],
        shape: WorldShape::Sphere,
        ..WorldPlacement::default()
    };
    let down = Graph::from_points(&[(0.0, 1.0), (0.4, 0.5), (1.0, 0.0)]).unwrap();
    // (The second number first: the order of the numbers is what counts. And the first number has a graph as well.)
    read_in(&mut project, host_clip, 1, laid_as(mask_track, second));
    read_in(
        &mut project,
        host_clip,
        0,
        laid_as(mask_track, first).with_the_map(MaskMap::Graph(down)),
    );

    let (_, numbers, _) = the_shader_operation(&plan(&project).sections[0], 10);

    assert_eq!(
        numbers.len(),
        4 + 2 + NUMBERS_OF_A_GRAPH + 2 * NUMBERS_OF_A_WORLD_PLACEMENT
    );
    assert_eq!(
        numbers[6..6 + NUMBERS_OF_A_GRAPH],
        down.numbers(),
        "the points of the graph first"
    );
    let from = 6 + NUMBERS_OF_A_GRAPH;
    assert_eq!(numbers[from..from + NUMBERS_OF_A_WORLD_PLACEMENT], first.numbers());
    assert_eq!(numbers[from + NUMBERS_OF_A_WORLD_PLACEMENT..], second.numbers());
}

#[test]
fn a_placement_is_read_at_every_frame_as_it_is_and_the_shader_is_the_same_whatever_the_place() {
    let (mut project, mask_track, host_clip) = a_mask_track_and_a_host_clip();
    read_in(
        &mut project,
        host_clip,
        0,
        laid_as(mask_track, WorldPlacement::default()),
    );
    let (key, ..) = the_shader_operation(&plan(&project).sections[0], 10);
    let moved = WorldPlacement {
        position: [4.0, 4.0, 4.0],
        yaw_pitch_roll_degrees: [10.0, 20.0, 30.0],
        width: 1.0,
        height: 2.0,
        ..WorldPlacement::default()
    };

    read_in(&mut project, host_clip, 0, laid_as(mask_track, moved));
    let (moved_key, numbers, _) = the_shader_operation(&plan(&project).sections[0], 10);
    let (_, later_numbers, _) = the_shader_operation(&plan(&project).sections[0], 80);

    assert_eq!(
        moved_key, key,
        "moving or turning a shape compiles nothing: the place is numbers"
    );
    assert_eq!(numbers[6..], moved.numbers());
    assert_eq!(later_numbers[6..], moved.numbers());
}
