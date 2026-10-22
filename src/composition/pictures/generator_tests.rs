// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the picture plan makes of a generator clip (phase 6, piece 6.5): **a layer** whose effects begin with what the clip makes (a shader that reads no picture) and go on with
//! the effects that are put on it; it hides nothing (what a shader makes is not known to the plan), and what is in front of a video that covers the picture hides it as anything does.

use super::tests::{frame, long_video, plan, text};
use super::*;
use crate::composition::test_projects::project_of;
use crate::project::{EffectKind, GroupKind};
use crate::test_support::put_a_hue_shift_on;

const SOLID: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"colour\", \"TYPE\": \"color\", \"DEFAULT\": [0.2, 0.4, 0.9, 1.0] } ] }*/\nvoid main() { gl_FragColor = colour; }\n";

fn a_generator() -> EffectKind {
    let file = crate::isf::read(SOLID).unwrap();
    EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "solid.fs", SOLID)).unwrap()
}

/// A project with a generator clip (from frame 10, for 30 frames) in front of a video of 60 frames; gives the generator's clip and the video's.
fn a_generator_in_front_of_a_video() -> (Project, ClipId, ClipId) {
    let mut project = project_of(vec![vec![(0, long_video(0, 60, 1))]]);
    let front = project.add_track_in_front();
    let (generator, _) = project
        .try_insert_generator_clip(front, frame(10), a_generator(), 30)
        .unwrap();
    let video = project.tracks()[1].placement_at_index(0).unwrap().id;
    (project, generator, video)
}

fn clips_of_the_layers(plan: &PicturePlan, section: usize) -> Vec<ClipId> {
    plan.sections[section].layers.iter().map(|layer| layer.clip).collect()
}

#[test]
fn a_generator_clip_is_a_layer_in_front_of_what_is_behind_it_for_as_long_as_it_lasts() {
    let (project, generator, video) = a_generator_in_front_of_a_video();

    let plan = plan(&project);

    let seen: Vec<(u64, u64, Vec<ClipId>)> = plan
        .sections
        .iter()
        .map(|section| {
            (
                section.first_timeline_frame,
                section.end_timeline_frame,
                section.layers.iter().map(|layer| layer.clip).collect(),
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            (0, 10, vec![video]),
            (10, 40, vec![video, generator]),
            (40, 60, vec![video])
        ]
    );
    let layer = &plan.sections[1].layers[1];
    assert_eq!(layer.content, LayerContent::Generator);
    assert_eq!(layer.placement, Placement::upright(CanvasArea::WHOLE_PICTURE));
    assert!(
        !layer.covers_the_picture(),
        "what a shader makes is not known to the plan"
    );
}

#[test]
fn the_effects_of_a_generator_layer_begin_with_what_it_makes_and_its_clock_is_its_clips() {
    let (mut project, generator, _) = a_generator_in_front_of_a_video();
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(generator), 40.0);

    let plan = plan(&project);

    let layer = &plan.sections[1].layers[1];
    let kinds: Vec<EffectKind> = layer.effects.iter().map(|effect| effect.kind).collect();
    assert_eq!(kinds, [a_generator(), EffectKind::HUE_SHIFT]);
    assert_eq!(layer.effects[0].clock.first_frame, 10);
    let operations = layer.operations_at(frame(25));
    match operations.as_slice() {
        [PictureOperation::Shader { key, numbers }, PictureOperation::Mix(_)] => {
            assert_eq!(key, a_generator().key());
            assert_eq!(numbers[2], 15.0, "FRAMEINDEX: 15 frames since the clip began");
            assert_eq!(
                numbers[4..],
                [0.2, 0.4, 0.9, 1.0].map(|c: f32| (c * 255.0).round() / 255.0)
            );
        }
        other => panic!("a shader and then a mix of colours were expected: {other:?}"),
    }
}

#[test]
fn what_a_generator_makes_does_not_hide_what_is_behind_it_and_a_video_that_covers_the_picture_in_front_of_it_hides_it()
{
    // In front of the video: both.
    let (project, generator, video) = a_generator_in_front_of_a_video();
    assert_eq!(clips_of_the_layers(&plan(&project), 1), [video, generator]);

    // Behind a video that covers the picture: not seen, and not in the plan.
    let mut project = project_of(vec![vec![(0, long_video(0, 60, 1))], vec![]]);
    let back = project.tracks()[1].id();
    let (hidden, _) = project
        .try_insert_generator_clip(back, frame(10), a_generator(), 30)
        .unwrap();
    let covering = project.tracks()[0].placement_at_index(0).unwrap().id;

    let plan = plan(&project);

    assert_eq!(
        clips_of_the_layers(&plan, 0),
        [covering],
        "the generator is behind a video that covers the picture"
    );
    assert!(
        !plan
            .sections
            .iter()
            .flat_map(|section| &section.layers)
            .any(|layer| layer.clip == hidden)
    );
}

#[test]
fn a_generator_in_a_clip_group_with_an_effect_is_made_into_the_picture_of_the_group_with_the_other_members() {
    let mut project = project_of(vec![vec![(0, long_video(0, 60, 1))], vec![(0, text(60))]]);
    let front = project.add_track_in_front();
    let (generator, _) = project
        .try_insert_generator_clip(front, frame(10), a_generator(), 30)
        .unwrap();
    let video = project.tracks()[1].placement_at_index(0).unwrap().id;
    let label = project.tracks()[2].placement_at_index(0).unwrap().id;
    let group = project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(generator), Member::Clip(video), Member::Clip(label)],
        )
        .unwrap();
    project.add_effect_to_the_group(group, EffectKind::BLUR).unwrap();
    project
        .set_effect_value_of_the_group(
            group,
            PropertyId::effect_value(EffectKind::BLUR, EffectInstanceId::FIRST, 0),
            &PropertyValue::Number(4.0),
        )
        .unwrap();

    let plan = plan(&project);

    let section = &plan.sections[1];
    assert_eq!(section.groups.len(), 1, "the group makes a picture of its own");
    assert_eq!(
        (section.groups[0].first_layer, section.groups[0].end_layer),
        (0, section.layers.len()),
        "and the generator is in it with the others"
    );
    assert!(section.layers.iter().any(|layer| layer.clip == generator));
}
