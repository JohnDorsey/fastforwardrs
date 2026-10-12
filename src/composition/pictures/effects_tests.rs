//! Tests of what the picture plan makes of the effects of clips and of tracks: each layer carries those of its clip and then those of
//! its track, with the keyframes of their values, and says what they are at every frame it is seen at.

use super::tests::{frame, long_video, picture, text};
use super::*;
use crate::colour_effects::ColourMatrix;
use crate::composition::test_projects::{project_of, random_project_with_effects};
use crate::project::{AnimationTarget, EffectKind, PropertyValue};
use crate::test_support::{self, DeterministicRandomNumbers, put_a_hue_shift_on};

const HUE: PropertyId = PropertyId::HueShift;

fn turn(degrees: f32) -> PropertyValue {
    PropertyValue::Number(degrees)
}

fn plan(project: &Project) -> PicturePlan {
    super::tests::plan(project)
}

/// The first clip of the front-most track.
fn the_first_clip(project: &Project) -> ClipId {
    project.tracks()[0].placement_at_index(0).expect("a clip").id
}

fn the_front_track(project: &Project) -> TrackId {
    project.tracks()[0].id()
}

fn make_a_keyframe(project: &mut Project, target: AnimationTarget, timeline_frame: u64, degrees: f32) {
    test_support::make_a_keyframe(project, target, HUE, timeline_frame, turn(degrees));
}

fn frames_of_the_keyframes_of(effect: &LayerEffect) -> Vec<u64> {
    effect
        .animation
        .keyframes_of(HUE)
        .map(|keyframes| keyframes.iter().map(|keyframe| keyframe.frame).collect())
        .unwrap_or_default()
}

/// A video of 100 frames that starts at frame 20 of the timeline, with a hue shift that is at 0 degrees at its first frame and at 120
/// at its frame 60 (frame 80 of the timeline).
fn a_video_whose_colours_turn() -> (Project, ClipId) {
    let mut project = project_of(vec![vec![(20, long_video(0, 100, 1))]]);
    let clip = the_first_clip(&project);
    project
        .add_effect(AnimationTarget::Clip(clip), EffectKind::HueShift)
        .unwrap();
    make_a_keyframe(&mut project, AnimationTarget::Clip(clip), 20, 0.0);
    make_a_keyframe(&mut project, AnimationTarget::Clip(clip), 80, 120.0);
    (project, clip)
}

// --- what a layer carries ---

#[test]
fn a_layer_whose_clip_and_track_have_no_effects_has_none_and_changes_no_colour() {
    let project = project_of(vec![vec![(0, long_video(0, 100, 1))]]);
    let plan = plan(&project);
    let layer = &plan.sections[0].layers[0];

    assert!(layer.effects.is_empty());
    assert!(!layer.has_keyframes());
    assert_eq!(layer.colour_matrix(), ColourMatrix::NOTHING_CHANGES);
    assert_eq!(layer.colour_matrix_at(frame(50)), ColourMatrix::NOTHING_CHANGES);
}

#[test]
fn a_layer_has_the_effects_of_its_clip_with_the_values_they_have_when_they_are_not_animated() {
    let mut project = project_of(vec![vec![(0, long_video(0, 100, 1))]]);
    let clip = the_first_clip(&project);
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), 90.0);

    let plan = plan(&project);
    let layer = &plan.sections[0].layers[0];

    assert_eq!(
        layer.effects,
        vec![LayerEffect {
            kind: EffectKind::HueShift,
            values: vec![turn(90.0)],
            animation: Animation::default(),
        }]
    );
    assert!(!layer.has_keyframes());
    assert_eq!(layer.colour_matrix(), ColourMatrix::hue_rotation(90.0));
    assert_eq!(layer.colour_matrix_at(frame(70)), ColourMatrix::hue_rotation(90.0));
}

#[test]
fn what_the_effects_of_the_clip_do_comes_first_and_then_what_those_of_its_track_do() {
    let mut project = project_of(vec![vec![(0, long_video(0, 100, 1))]]);
    let (clip, track) = (the_first_clip(&project), the_front_track(&project));
    put_a_hue_shift_on(&mut project, AnimationTarget::Track(track), 30.0);
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), 90.0);

    let plan = plan(&project);
    let layer = &plan.sections[0].layers[0];

    let values: Vec<_> = layer.effects.iter().map(|effect| effect.values.clone()).collect();
    assert_eq!(
        values,
        vec![vec![turn(90.0)], vec![turn(30.0)]],
        "the clip's, then the track's"
    );
    assert_eq!(
        layer.colour_matrix(),
        ColourMatrix::hue_rotation(90.0).then(ColourMatrix::hue_rotation(30.0))
    );
    // (Both together are a turn of 120 degrees.)
    let mut together = [200, 50, 10, 255];
    layer.colour_matrix().apply_to_rgba_pixels(&mut together);
    let mut turned_at_once = [200, 50, 10, 255];
    ColourMatrix::hue_rotation(120.0).apply_to_rgba_pixels(&mut turned_at_once);
    for (channel, (one, other)) in together.iter().zip(turned_at_once).enumerate() {
        assert!(
            one.abs_diff(other) <= 1,
            "channel {channel}: {together:?} and {turned_at_once:?}"
        );
    }
}

#[test]
fn the_layer_of_a_picture_and_of_a_text_object_carry_the_effects_of_their_clips_too() {
    let mut project = project_of(vec![vec![(0, text(40))], vec![(0, picture(40))]]);
    let text_clip = the_first_clip(&project);
    let picture_clip = project.tracks()[1].placement_at_index(0).unwrap().id;
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(text_clip), 15.0);
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(picture_clip), -45.0);

    let plan = plan(&project);

    let layers = &plan.sections[0].layers;
    assert_eq!(layers.len(), 2);
    for layer in layers {
        let wanted = if layer.clip == text_clip { 15.0 } else { -45.0 };
        assert_eq!(layer.effects.len(), 1, "{}", layer.clip);
        assert_eq!(layer.effects[0].values, vec![turn(wanted)], "{}", layer.clip);
    }
}

#[test]
fn the_keyframes_of_an_effect_are_in_the_effect_and_not_among_those_of_where_the_layer_is() {
    let (project, _) = a_video_whose_colours_turn();
    let plan = plan(&project);
    let layer = &plan.section_at(frame(50)).unwrap().layers[0];

    assert!(layer.has_keyframes());
    assert!(layer.effects[0].is_animated());
    assert_eq!(frames_of_the_keyframes_of(&layer.effects[0]), vec![20, 80]);
    assert!(layer.animation.is_empty(), "{:?}", layer.animation);
    assert!(
        !layer.is_moved_by_keyframes(),
        "turning the colours does not move the picture"
    );
    assert_eq!(layer.placement_at(frame(50)), layer.placement);
}

#[test]
fn what_an_effect_does_at_a_frame_is_what_its_keyframes_say_there() {
    let (project, _) = a_video_whose_colours_turn();
    let plan = plan(&project);
    let layer = &plan.section_at(frame(50)).unwrap().layers[0];

    assert_eq!(layer.effects[0].values_at(20), vec![turn(0.0)], "the first keyframe");
    assert_eq!(layer.effects[0].values_at(50), vec![turn(60.0)], "half way");
    assert_eq!(layer.effects[0].values_at(80), vec![turn(120.0)], "the second keyframe");
    assert_eq!(
        layer.effects[0].values_at(119),
        vec![turn(120.0)],
        "after it, what it says"
    );
    assert_eq!(layer.colour_matrix_at(frame(50)), ColourMatrix::hue_rotation(60.0));
    assert_eq!(layer.colour_matrix_at(frame(20)), ColourMatrix::NOTHING_CHANGES);
}

#[test]
fn the_keyframes_of_the_effect_of_a_track_are_at_the_frames_of_the_timeline_and_apart_from_those_of_the_clip() {
    let (mut project, clip) = a_video_whose_colours_turn();
    let track = the_front_track(&project);
    project
        .add_effect(AnimationTarget::Track(track), EffectKind::HueShift)
        .unwrap();
    make_a_keyframe(&mut project, AnimationTarget::Track(track), 30, 10.0);
    make_a_keyframe(&mut project, AnimationTarget::Track(track), 90, 70.0);

    let plan = plan(&project);
    let layer = &plan.section_at(frame(50)).unwrap().layers[0];

    assert_eq!(layer.clip, clip);
    assert_eq!(layer.effects.len(), 2);
    assert_eq!(
        frames_of_the_keyframes_of(&layer.effects[0]),
        vec![20, 80],
        "the clip's"
    );
    assert_eq!(
        frames_of_the_keyframes_of(&layer.effects[1]),
        vec![30, 90],
        "the track's"
    );
    assert_eq!(layer.effects[0].values_at(60), vec![turn(80.0)]);
    assert_eq!(layer.effects[1].values_at(60), vec![turn(40.0)]);
}

#[test]
fn a_clip_cut_into_sections_by_what_is_over_it_has_the_same_effects_in_each_of_them() {
    let (mut project, clip) = a_video_whose_colours_turn();
    let text_track = project.add_track_in_front();
    project
        .insert_text(text_track, frame(60), TextObject::default(), 30)
        .expect("there is room");

    let plan = plan(&project);

    let sections_of_the_video: Vec<&PictureSection> = plan
        .sections
        .iter()
        .filter(|section| section.layers.iter().any(|layer| layer.clip == clip))
        .collect();
    assert_eq!(sections_of_the_video.len(), 3, "before the text, with it, and after it");
    let effects_of_the_clip = |section: &PictureSection| {
        section
            .layers
            .iter()
            .find(|layer| layer.clip == clip)
            .map(|layer| layer.effects.clone())
            .unwrap()
    };
    let first = effects_of_the_clip(sections_of_the_video[0]);
    assert_eq!(first.len(), 1);
    assert_eq!(effects_of_the_clip(sections_of_the_video[1]), first);
    assert_eq!(effects_of_the_clip(sections_of_the_video[2]), first);
}

// --- a layer at one frame ---

#[test]
fn a_layer_as_it_is_at_a_frame_has_its_effects_as_they_are_there_and_no_keyframes() {
    let (project, _) = a_video_whose_colours_turn();
    let plan = plan(&project);
    let layer = &plan.section_at(frame(50)).unwrap().layers[0];

    let at_50 = layer.as_it_is_at(frame(50));

    assert_eq!(at_50.effects.len(), 1);
    assert_eq!(at_50.effects[0].kind, EffectKind::HueShift);
    assert_eq!(at_50.effects[0].values, vec![turn(60.0)]);
    assert!(!at_50.effects[0].is_animated());
    assert!(!at_50.has_keyframes());
    assert!(layer.has_keyframes(), "the layer it was made from still has them");
    assert_eq!(at_50.colour_matrix(), layer.colour_matrix_at(frame(50)));
    for timeline_frame in [20, 70, 119] {
        assert_eq!(
            at_50.colour_matrix_at(frame(timeline_frame)),
            at_50.colour_matrix(),
            "the same at frame {timeline_frame}"
        );
    }
}

// --- checked against the project ---

/// What the effects of the layer `layer` are at `timeline_frame` by looking at the project, one frame at a time: those of the clip
/// and then those of its track, each with the value that the project says each of its properties has there.
fn effects_by_looking_at_the_project(
    project: &Project,
    layer: &Layer,
    timeline_frame: u64,
) -> Vec<(EffectKind, Vec<PropertyValue>)> {
    [AnimationTarget::Clip(layer.clip), AnimationTarget::Track(layer.track)]
        .into_iter()
        .flat_map(|target| {
            project
                .effects_of(target)
                .expect("the clip and its track are there")
                .iter()
                .map(move |effect| {
                    let values = effect
                        .kind()
                        .properties()
                        .iter()
                        .map(|property| {
                            project
                                .property_value_at(target, *property, frame(timeline_frame))
                                .expect("the thing has the effect")
                        })
                        .collect();
                    (effect.kind(), values)
                })
        })
        .collect()
}

#[test]
fn every_layer_has_the_effects_and_values_the_project_says_at_every_frame_it_is_seen_at() {
    let (mut with_effects, mut animated, mut with_both_of_clip_and_track) = (0, 0, 0);
    for seed in 1..=150u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed + 21_000);
        let project = random_project_with_effects(&mut random);
        let plan = plan(&project);
        for timeline_frame in 0..plan.total_frame_count {
            let section = plan.section_at(frame(timeline_frame)).unwrap();
            for layer in &section.layers {
                let expected = effects_by_looking_at_the_project(&project, layer, timeline_frame);
                let carried: Vec<(EffectKind, Vec<PropertyValue>)> = layer
                    .effects
                    .iter()
                    .map(|effect| (effect.kind, effect.values_at(timeline_frame)))
                    .collect();
                assert_eq!(
                    carried, expected,
                    "seed {seed}, frame {timeline_frame}, clip {}",
                    layer.clip
                );

                let as_it_is = layer.as_it_is_at(frame(timeline_frame));
                assert_eq!(
                    as_it_is.colour_matrix(),
                    layer.colour_matrix_at(frame(timeline_frame)),
                    "seed {seed}, frame {timeline_frame}, clip {}",
                    layer.clip
                );
                with_effects += usize::from(!layer.effects.is_empty());
                animated += usize::from(layer.effects.iter().any(LayerEffect::is_animated));
                with_both_of_clip_and_track += usize::from(layer.effects.len() >= 2);
            }
        }
    }
    // (So that the projects made at random cannot stop having effects in them without this noticing.)
    assert!(
        with_effects > 2000,
        "only {with_effects} frames of layers with effects were checked"
    );
    assert!(
        animated > 500,
        "only {animated} frames of layers with animated effects were checked"
    );
    assert!(
        with_both_of_clip_and_track > 300,
        "only {with_both_of_clip_and_track} frames of layers with effects on the clip and on its track were checked"
    );
}

/// What a section of the plan shows, in short: where it is, and the clips of its layers with the source frames they show (if
/// they are videos).
type WhatASectionShows = (u64, u64, Vec<(ClipId, Option<(u64, u64)>)>);

#[test]
fn effects_do_not_change_where_the_sections_of_the_plan_begin_and_end_or_what_they_show() {
    let mut projects_with_effects_checked = 0;
    for seed in 1..=100u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed + 22_000);
        let project = random_project_with_effects(&mut random);
        let mut without = project.clone();
        let mut had_one = false;
        let targets: Vec<AnimationTarget> = project
            .tracks()
            .iter()
            .flat_map(|track| {
                std::iter::once(AnimationTarget::Track(track.id()))
                    .chain(track.placements().map(|placement| AnimationTarget::Clip(placement.id)))
            })
            .collect();
        for target in targets {
            for kind in EffectKind::ALL {
                if without.has_the_effect(target, kind) {
                    without.remove_effect(target, kind).unwrap();
                    had_one = true;
                }
            }
        }

        let shown = |project: &Project| -> Vec<WhatASectionShows> {
            plan(project)
                .sections
                .iter()
                .map(|section| {
                    (
                        section.first_timeline_frame,
                        section.end_timeline_frame,
                        section
                            .layers
                            .iter()
                            .map(|layer| {
                                (
                                    layer.clip,
                                    layer
                                        .video()
                                        .map(|video| (video.first_source_frame, video.end_source_frame)),
                                )
                            })
                            .collect(),
                    )
                })
                .collect()
        };

        assert_eq!(shown(&project), shown(&without), "seed {seed}");
        projects_with_effects_checked += usize::from(had_one);
    }
    assert!(
        projects_with_effects_checked > 80,
        "only {projects_with_effects_checked} projects had effects"
    );
}
