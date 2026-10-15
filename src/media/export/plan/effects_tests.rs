// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of how the export is made to do what the effects of clips and tracks do: the filter that changes the colours of a picture and
//! where it is in the filter graph, and the stretches that a value of an effect that has keyframes is cut into.

use super::tests::{clip_location, frame, picture_source, plan, project_with_video, video_source};
use super::*;
use crate::colour_effects::ColourMatrix;
use crate::composition::test_projects::random_project_with_effects;
use crate::composition::{Layer, PicturePlan, plan_pictures};
use crate::media::export::animated_sections::{self, MOST_STRETCHES_OF_ANIMATION};
use crate::project::{AnimationTarget, EffectInstanceId, EffectKind, PropertyId, PropertyValue, TextObject};
use crate::test_support::{DeterministicRandomNumbers, make_a_keyframe, put_a_hue_shift_on};

const HUE: PropertyId = crate::test_support::HUE_SHIFT_AS_THE_FIRST_EFFECT;
const PICTURE_SIZE: (u32, u32) = (640, 360);

fn turn(degrees: f32) -> PropertyValue {
    PropertyValue::Number(degrees)
}

/// The filter that asks ffmpeg to turn the colours by `degrees`.
fn the_filter_for_a_turn_of(degrees: f32) -> String {
    ColourMatrix::hue_rotation(degrees).ffmpeg_filter()
}

fn the_graph_of(project: &Project) -> String {
    plan(project).expect("there is something to export").filter_graph.text
}

fn how_many_colour_filters_there_are(graph: &str) -> usize {
    graph.matches("colorchannelmixer").count()
}

/// A video of 640x360 pixels that lasts `frames` frames, which fills the picture.
fn project_with_a_video_of(frames: u64) -> (Project, ClipId) {
    let mut project = Project::empty();
    project
        .import_source(video_source(1, 640, 360, frames, false), frame(0))
        .unwrap();
    let clip = clip_location(&project, 0, 0);
    (project, clip)
}

fn stretches_of(project: &Project) -> Vec<PictureSection> {
    let pictures: PicturePlan = plan_pictures(project, |_| true);
    animated_sections::with_keyframes_resolved(&pictures.sections, PICTURE_SIZE).expect("not too much to export")
}

// --- the filter ---

#[test]
fn a_project_with_no_effects_has_no_filter_that_changes_colours() {
    let project = project_with_video(320, 180, false);
    assert_eq!(how_many_colour_filters_there_are(&the_graph_of(&project)), 0);
}

#[test]
fn a_video_with_a_hue_shift_has_the_filter_that_turns_its_colours_right_after_it_is_scaled() {
    let mut project = project_with_video(320, 180, false);
    let clip = clip_location(&project, 0, 0);
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), 90.0);

    let graph = the_graph_of(&project);

    assert_eq!(how_many_colour_filters_there_are(&graph), 1, "{graph}");
    assert!(
        graph.contains(&format!(
            "scale=320:180,setsar=1,{}[finished_clip_0]",
            the_filter_for_a_turn_of(90.0)
        )),
        "{graph}"
    );
}

#[test]
fn a_turn_of_nothing_or_of_whole_turns_is_no_filter_at_all() {
    for degrees in [0.0, 360.0, -720.0] {
        let mut project = project_with_video(320, 180, false);
        let clip = clip_location(&project, 0, 0);
        put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), degrees);

        let graph = the_graph_of(&project);

        assert_eq!(
            how_many_colour_filters_there_are(&graph),
            0,
            "{degrees} degrees: {graph}"
        );
    }
}

#[test]
fn the_hue_shift_of_a_track_is_on_every_video_of_it_and_a_clip_has_the_effects_of_the_clip_and_then_the_track() {
    let mut project = project_with_video(320, 180, false);
    project.split_at(frame(100), None);
    let track = project.tracks()[0].id();
    put_a_hue_shift_on(&mut project, AnimationTarget::Track(track), 30.0);

    let graph = the_graph_of(&project);
    assert_eq!(
        how_many_colour_filters_there_are(&graph),
        2,
        "one on each of the two clips: {graph}"
    );
    assert_eq!(graph.matches(&the_filter_for_a_turn_of(30.0)).count(), 2, "{graph}");

    let first = clip_location(&project, 0, 0);
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(first), 90.0);
    let graph = the_graph_of(&project);
    let both = ColourMatrix::hue_rotation(90.0)
        .then(ColourMatrix::hue_rotation(30.0))
        .ffmpeg_filter();
    assert_eq!(graph.matches(&both).count(), 1, "the first clip has both: {graph}");
    assert_eq!(
        graph.matches(&the_filter_for_a_turn_of(30.0)).count(),
        1,
        "the other has the track's only: {graph}"
    );
}

#[test]
fn a_text_object_with_a_hue_shift_has_the_filter_on_its_own_picture_before_it_is_laid_over_the_video() {
    let mut project = project_with_video(320, 180, false);
    let front = project.add_track_in_front();
    let (text, _) = project
        .insert_text(front, frame(30), TextObject::default(), 60)
        .unwrap();
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(text), -30.0);

    let graph = the_graph_of(&project);

    assert_eq!(
        how_many_colour_filters_there_are(&graph),
        1,
        "the video has none: {graph}"
    );
    assert!(
        graph.contains(&format!(
            "[still_picture_1_copy_0]{}[clip_layer_1_1]",
            the_filter_for_a_turn_of(-30.0)
        )),
        "{graph}"
    );
}

#[test]
fn a_picture_with_a_hue_shift_is_scaled_to_its_place_and_then_has_its_colours_turned() {
    let mut project = project_with_video(320, 180, false);
    let front = project.add_track_in_front();
    let outcome = project.import_source(picture_source(2, 400, 200), frame(30)).unwrap();
    assert_eq!(outcome.track, front, "the empty track in front is the one it goes on");
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(outcome.clip.unwrap()), 120.0);

    let graph = the_graph_of(&project);

    assert_eq!(how_many_colour_filters_there_are(&graph), 1, "{graph}");
    assert!(
        graph.contains(&format!(",setsar=1,{}[clip_layer_", the_filter_for_a_turn_of(120.0))),
        "{graph}"
    );
}

#[test]
fn a_hue_shift_on_the_front_track_does_not_change_the_video_on_the_track_behind_it() {
    let mut project = project_with_video(320, 180, false);
    let front = project.add_track_in_front();
    let (text, _) = project
        .insert_text(front, frame(30), TextObject::default(), 60)
        .unwrap();
    put_a_hue_shift_on(&mut project, AnimationTarget::Track(front), 45.0);
    assert!(project.has_the_effect(AnimationTarget::Track(front), EffectInstanceId::FIRST));
    assert_eq!(
        project.tracks()[0].placement_of(text).unwrap().first_timeline_frame,
        frame(30)
    );

    let graph = the_graph_of(&project);

    assert_eq!(how_many_colour_filters_there_are(&graph), 1, "only the text: {graph}");
}

// --- values with keyframes ---

#[test]
fn a_hue_shift_with_keyframes_is_a_stretch_for_each_whole_degree_it_comes_to_and_is_exported_in_whole_degrees() {
    let (mut project, clip) = project_with_a_video_of(300);
    project
        .add_effect(AnimationTarget::Clip(clip), EffectKind::HueShift)
        .unwrap();
    make_a_keyframe(&mut project, AnimationTarget::Clip(clip), HUE, 0, turn(0.0));
    make_a_keyframe(&mut project, AnimationTarget::Clip(clip), HUE, 299, turn(30.0));

    let stretches = stretches_of(&project);

    assert_eq!(stretches.len(), 31, "0 up to 30 degrees in steps of 1");
    assert_eq!(stretches.first().unwrap().first_timeline_frame, 0);
    assert_eq!(stretches.last().unwrap().end_timeline_frame, 300);
    for pair in stretches.windows(2) {
        assert_eq!(pair[0].end_timeline_frame, pair[1].first_timeline_frame);
    }
    for (number, stretch) in stretches.iter().enumerate() {
        let layer = &stretch.layers[0];
        assert_eq!(layer.effects[0].values, vec![turn(number as f32)], "stretch {number}");
        assert!(!layer.has_keyframes(), "each is as it is at its first frame");
    }

    let graph = the_graph_of(&project);
    assert_eq!(
        how_many_colour_filters_there_are(&graph),
        30,
        "one for each of the stretches that turns the colours at all: {graph}"
    );
    for degrees in 1..=30 {
        assert!(
            graph.contains(&the_filter_for_a_turn_of(degrees as f32)),
            "{degrees} degrees: {graph}"
        );
    }
}

#[test]
fn a_stretch_begins_at_the_first_frame_whose_hue_shift_comes_to_a_whole_degree_more() {
    let (mut project, clip) = project_with_a_video_of(300);
    project
        .add_effect(AnimationTarget::Clip(clip), EffectKind::HueShift)
        .unwrap();
    make_a_keyframe(&mut project, AnimationTarget::Clip(clip), HUE, 0, turn(0.0));
    make_a_keyframe(&mut project, AnimationTarget::Clip(clip), HUE, 299, turn(0.9));

    let stretches = stretches_of(&project);

    // 0.9 * frame / 299 is a half degree or more from frame 167 on (frame 166 is 0.4997).
    assert_eq!(stretches.len(), 2, "{stretches:?}");
    assert_eq!(
        (stretches[0].first_timeline_frame, stretches[0].end_timeline_frame),
        (0, 167)
    );
    assert_eq!(
        (stretches[1].first_timeline_frame, stretches[1].end_timeline_frame),
        (167, 300)
    );
    assert_eq!(stretches[0].layers[0].effects[0].values, vec![turn(0.0)]);
    assert_eq!(stretches[1].layers[0].effects[0].values, vec![turn(1.0)]);
}

#[test]
fn a_hue_shift_that_changes_by_less_than_a_whole_degree_is_one_stretch_and_changes_nothing() {
    let (mut project, clip) = project_with_a_video_of(300);
    project
        .add_effect(AnimationTarget::Clip(clip), EffectKind::HueShift)
        .unwrap();
    make_a_keyframe(&mut project, AnimationTarget::Clip(clip), HUE, 0, turn(0.0));
    make_a_keyframe(&mut project, AnimationTarget::Clip(clip), HUE, 299, turn(0.4));

    assert_eq!(stretches_of(&project).len(), 1);
    assert_eq!(how_many_colour_filters_there_are(&the_graph_of(&project)), 0);
}

#[test]
fn a_hue_shift_that_goes_round_more_than_once_between_keyframes_is_a_stretch_for_each_degree_all_the_way() {
    let (mut project, clip) = project_with_a_video_of(400);
    project
        .add_effect(AnimationTarget::Clip(clip), EffectKind::HueShift)
        .unwrap();
    make_a_keyframe(&mut project, AnimationTarget::Clip(clip), HUE, 0, turn(0.0));
    make_a_keyframe(&mut project, AnimationTarget::Clip(clip), HUE, 399, turn(720.0));

    let stretches = stretches_of(&project);

    // (A frame is 1.8 degrees: no frame comes to the same degree as the one before, so every frame is a stretch.)
    assert_eq!(stretches.len(), 400);
    let exported_at = |frame: usize| stretches[frame].layers[0].effects[0].values.clone();
    assert_eq!(exported_at(0), vec![turn(0.0)]);
    assert_eq!(exported_at(199), vec![turn(359.0)], "720 * 199 / 399 is 359.1");
    assert_eq!(
        exported_at(200),
        vec![turn(361.0)],
        "past the whole turn: it is not taken round"
    );
    assert_eq!(exported_at(399), vec![turn(720.0)]);
    assert!(stretches[0].layers[0].colour_matrix().changes_nothing());
    assert!(
        stretches[399].layers[0].colour_matrix().changes_nothing(),
        "two whole turns"
    );
}

#[test]
fn the_stretches_of_a_hue_shift_count_with_the_rest_and_one_stretch_too_many_is_refused() {
    let most = MOST_STRETCHES_OF_ANIMATION as u64;
    let project_turned_a_degree_every_frame = |frames: u64| {
        let (mut project, clip) = project_with_a_video_of(frames);
        project
            .add_effect(AnimationTarget::Clip(clip), EffectKind::HueShift)
            .unwrap();
        make_a_keyframe(&mut project, AnimationTarget::Clip(clip), HUE, 0, turn(0.0));
        make_a_keyframe(
            &mut project,
            AnimationTarget::Clip(clip),
            HUE,
            frames - 1,
            turn((frames - 1) as f32),
        );
        project
    };

    assert_eq!(
        stretches_of(&project_turned_a_degree_every_frame(most)).len(),
        MOST_STRETCHES_OF_ANIMATION
    );

    let pictures = plan_pictures(&project_turned_a_degree_every_frame(most + 1), |_| true);
    assert_eq!(
        animated_sections::with_keyframes_resolved(&pictures.sections, PICTURE_SIZE),
        Err(ExportPlanError::TooMuchAnimation {
            most: MOST_STRETCHES_OF_ANIMATION
        })
    );
}

#[test]
fn the_hue_shift_of_a_track_that_has_keyframes_cuts_the_sections_of_what_is_on_it() {
    let (mut project, _) = project_with_a_video_of(300);
    let track = project.tracks()[0].id();
    project
        .add_effect(AnimationTarget::Track(track), EffectKind::HueShift)
        .unwrap();
    make_a_keyframe(&mut project, AnimationTarget::Track(track), HUE, 0, turn(0.0));
    make_a_keyframe(&mut project, AnimationTarget::Track(track), HUE, 299, turn(10.0));

    let stretches = stretches_of(&project);

    assert_eq!(stretches.len(), 11, "0 up to 10 degrees");
    assert_eq!(stretches[10].layers[0].effects[0].values, vec![turn(10.0)]);
}

#[test]
fn a_hue_shift_that_is_not_animated_does_not_cut_the_sections() {
    let (mut project, clip) = project_with_a_video_of(300);
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), 77.5);
    let pictures = plan_pictures(&project, |_| true);

    let resolved = animated_sections::with_keyframes_resolved(&pictures.sections, PICTURE_SIZE).unwrap();

    assert_eq!(resolved, pictures.sections, "as they were");
    assert_eq!(
        resolved[0].layers[0].effects[0].values,
        vec![turn(77.5)],
        "its value is not rounded to a degree when it does not change: the export has it as it is"
    );
}

// --- checked against the project ---

/// What the effects of `target` have as their values at `timeline_frame`, as they are exported, by looking at the project.
fn exported_effects_at(
    project: &Project,
    target: AnimationTarget,
    timeline_frame: u64,
) -> Vec<(EffectKind, Vec<PropertyValue>)> {
    project
        .effects_of(target)
        .expect("the clip or the track is there")
        .iter()
        .map(|effect| {
            let values: Vec<PropertyValue> = effect
                .properties()
                .map(|property| {
                    project
                        .property_value_at(target, property, frame(timeline_frame))
                        .expect("it has the effect")
                })
                .collect();
            (effect.kind(), effect.kind().as_exported(&values))
        })
        .collect()
}

/// How the effects of a layer change colours at `timeline_frame`, as they are exported, by looking at the project.
fn exported_colour_at(project: &Project, layer: &Layer, timeline_frame: u64) -> ColourMatrix {
    [AnimationTarget::Clip(layer.clip), AnimationTarget::Track(layer.track)]
        .into_iter()
        .flat_map(|target| exported_effects_at(project, target, timeline_frame))
        .filter_map(|(kind, values)| kind.colour_matrix(&values))
        .fold(ColourMatrix::NOTHING_CHANGES, ColourMatrix::then)
}

#[test]
fn every_stretch_is_exported_as_the_project_says_at_every_one_of_its_frames_and_as_one_thing() {
    let (mut turning_stretches, mut long_stretches, mut projects_refused) = (0, 0, 0);
    for seed in 1..=60u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed + 31_000);
        let project = random_project_with_effects(&mut random);
        let pictures = plan_pictures(&project, |_| true);
        let Ok(stretches) = animated_sections::with_keyframes_resolved(&pictures.sections, pictures.picture_size)
        else {
            projects_refused += 1;
            continue;
        };
        for stretch in &stretches {
            for layer in &stretch.layers {
                let at_the_start = exported_colour_at(&project, layer, stretch.first_timeline_frame);
                assert_eq!(
                    layer.colour_matrix(),
                    at_the_start,
                    "seed {seed}, stretch {}..{}, clip {}",
                    stretch.first_timeline_frame,
                    stretch.end_timeline_frame,
                    layer.clip
                );
                for timeline_frame in stretch.first_timeline_frame..stretch.end_timeline_frame {
                    assert_eq!(
                        exported_colour_at(&project, layer, timeline_frame),
                        at_the_start,
                        "seed {seed}, frame {timeline_frame} of the stretch {}..{}, clip {}",
                        stretch.first_timeline_frame,
                        stretch.end_timeline_frame,
                        layer.clip
                    );
                }
                turning_stretches += usize::from(!at_the_start.changes_nothing());
                long_stretches += usize::from(!at_the_start.changes_nothing() && stretch.timeline_frame_count() > 1);
            }
        }
    }
    // (So that the projects made at random cannot stop having effects that change colours in them without this noticing.)
    assert!(
        turning_stretches > 500,
        "only {turning_stretches} layers that turn colours were checked"
    );
    assert!(
        long_stretches > 100,
        "only {long_stretches} of them were stretches of more than a frame"
    );
    assert!(
        projects_refused < 20,
        "{projects_refused} of the projects had too much animation to export"
    );
}
