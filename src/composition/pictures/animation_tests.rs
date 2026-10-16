// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of what the picture plan makes of clips that have keyframes: each layer carries them, and says where it is
//! (and what a text object says) at every frame it is seen at.

use std::borrow::Cow;

use super::tests::{frame, long_video, long_video_placed, over_the_whole_picture, plan, text};
use super::*;
use crate::composition::test_projects::{project_of, random_project_with_places_and_keyframes};
use crate::project::tracks::Angle;
use crate::project::{AnimationTarget, PropertyValue, TextColour, with_placement_value};
use crate::test_support::{self, DeterministicRandomNumbers};

/// Puts a keyframe at `timeline_frame` that makes `property` of `clip` have `value` there. (The property is animated
/// from then on, if it was not.)
fn make_a_keyframe(
    project: &mut Project,
    clip: ClipId,
    property: PropertyId,
    timeline_frame: u64,
    value: PropertyValue,
) {
    test_support::make_a_keyframe(project, AnimationTarget::Clip(clip), property, timeline_frame, value);
}

/// The first clip of the front-most track, which is the only track in most of these projects.
fn the_first_clip(project: &Project) -> ClipId {
    project.tracks()[0].placement_at_index(0).expect("a clip").id
}

fn assert_centre_is(placement: Placement, expected: (f32, f32), when: &str) {
    let centre = placement.centre();
    assert!(
        (centre.0 - expected.0).abs() < 1e-4 && (centre.1 - expected.1).abs() < 1e-4,
        "{when}: the middle is {centre:?}, not {expected:?}"
    );
}

/// A video of 100 frames that starts at frame 20 of the timeline and fills the picture, whose middle is at the
/// middle of the picture at its first frame (frame 20) and at the bottom right corner at frame 80.
fn a_video_that_moves_across() -> (Project, ClipId) {
    let mut project = project_of(vec![vec![(20, long_video(0, 100, 1))]]);
    let clip = the_first_clip(&project);
    make_a_keyframe(
        &mut project,
        clip,
        PropertyId::Position,
        20,
        PropertyValue::Pair(0.25, 0.25),
    );
    make_a_keyframe(
        &mut project,
        clip,
        PropertyId::Position,
        80,
        PropertyValue::Pair(0.75, 0.75),
    );
    (project, clip)
}

// --- where a layer is ---

#[test]
fn a_layer_whose_clip_has_no_keyframes_is_where_the_clip_says_at_every_frame() {
    let somewhere = Placement::new(CanvasArea::new(0.1, 0.2, 0.3, 0.4), Angle::from_degrees(25.0));
    let project = project_of(vec![vec![(0, long_video_placed(0, 50, somewhere))]]);
    let plan = plan(&project);
    let layer = &plan.sections[0].layers[0];
    assert!(layer.animation.is_empty());
    assert!(!layer.is_moved_by_keyframes());
    for timeline_frame in [0, 1, 25, 49] {
        assert_eq!(
            layer.placement_at(frame(timeline_frame)),
            somewhere,
            "frame {timeline_frame}"
        );
    }
}

#[test]
fn the_keyframes_of_a_layer_are_at_the_frames_of_the_timeline_that_they_are_at() {
    let (project, _) = a_video_that_moves_across();
    let plan = plan(&project);
    let layer = &plan.section_at(frame(50)).unwrap().layers[0];
    let frames: Vec<u64> = layer
        .animation
        .keyframes_of(PropertyId::Position)
        .unwrap()
        .iter()
        .map(|keyframe| keyframe.frame)
        .collect();
    assert_eq!(
        frames,
        vec![20, 80],
        "the clip starts at frame 20 and has them at its first frame and 60 frames on"
    );
    assert!(layer.is_moved_by_keyframes());

    assert_centre_is(layer.placement_at(frame(20)), (0.25, 0.25), "the clip's first frame");
    assert_centre_is(
        layer.placement_at(frame(50)),
        (0.5, 0.5),
        "half way between the keyframes",
    );
    assert_centre_is(layer.placement_at(frame(80)), (0.75, 0.75), "the second keyframe");
    assert_centre_is(
        layer.placement_at(frame(119)),
        (0.75, 0.75),
        "after it: it stays where it got to",
    );
}

#[test]
fn a_layer_in_a_later_section_of_the_clip_is_still_counted_from_the_start_of_the_clip() {
    // Text over frames 60 to 90 cuts the video into three sections.
    let (mut project, _) = a_video_that_moves_across();
    let text_track = project.add_track_in_front();
    project
        .insert_text(text_track, frame(60), TextObject::default(), 30)
        .expect("there is room");
    let plan = plan(&project);
    let sections_of_the_video: Vec<_> = plan
        .sections
        .iter()
        .filter(|section| section.layers.iter().any(|layer| layer.video().is_some()))
        .collect();
    assert_eq!(sections_of_the_video.len(), 3, "{sections_of_the_video:?}");

    let last = sections_of_the_video[2];
    let video_layer = last.layers.iter().find(|layer| layer.video().is_some()).unwrap();
    assert!(
        last.first_timeline_frame > 80,
        "the last section is after the second keyframe"
    );
    assert_centre_is(
        video_layer.placement_at(frame(100)),
        (0.75, 0.75),
        "frame 100 is 80 frames in: after the keyframes",
    );

    let middle = sections_of_the_video[1];
    let video_layer = middle.layers.iter().find(|layer| layer.video().is_some()).unwrap();
    // Frame 65 is 45 frames in: three quarters of the way between the keyframes, which are 60 frames apart.
    assert_centre_is(video_layer.placement_at(frame(65)), (0.625, 0.625), "frame 65");
}

#[test]
fn the_place_the_clip_says_is_not_changed_by_its_keyframes() {
    let (project, _) = a_video_that_moves_across();
    let plan = plan(&project);
    let layer = &plan.section_at(frame(50)).unwrap().layers[0];
    assert_eq!(layer.placement, over_the_whole_picture(), "that is what the clip says");
}

#[test]
fn each_property_of_where_a_layer_is_is_changed_by_its_own_keyframes_and_the_rest_is_as_the_clip_says() {
    let mut project = project_of(vec![vec![(
        0,
        long_video_placed(0, 100, Placement::upright(CanvasArea::new(0.0, 0.0, 0.5, 0.5))),
    )]]);
    let clip = the_first_clip(&project);
    make_a_keyframe(&mut project, clip, PropertyId::Rotation, 0, PropertyValue::Number(0.0));
    make_a_keyframe(
        &mut project,
        clip,
        PropertyId::Rotation,
        50,
        PropertyValue::Number(90.0),
    );
    let plan = plan(&project);
    let layer = &plan.sections[0].layers[0];

    let at_25 = layer.placement_at(frame(25));
    assert!((at_25.rotation.degrees() - 45.0).abs() < 1e-3, "{at_25:?}");
    assert_eq!(
        at_25.area,
        CanvasArea::new(0.0, 0.0, 0.5, 0.5),
        "the area is as the clip says"
    );
}

// --- what a layer carries ---

#[test]
fn a_layer_carries_only_the_keyframes_of_what_is_seen() {
    let mut project = project_of(vec![vec![(0, long_video(0, 100, 1))]]);
    let clip = the_first_clip(&project);
    make_a_keyframe(&mut project, clip, PropertyId::Gain, 0, PropertyValue::Number(-6.0));
    make_a_keyframe(&mut project, clip, PropertyId::Muted, 10, PropertyValue::Flag(true));
    make_a_keyframe(&mut project, clip, PropertyId::Size, 20, PropertyValue::Pair(0.5, 0.5));
    let plan = plan(&project);
    let layer = &plan.sections[0].layers[0];
    let carried: Vec<PropertyId> = layer.animation.properties().map(|(property, _)| property).collect();
    assert_eq!(
        carried,
        vec![PropertyId::Size],
        "how loud it is and whether it is silent are not seen"
    );
}

#[test]
fn only_keyframes_for_where_a_layer_is_make_it_move_and_the_look_of_a_text_object_does_not() {
    let mut project = project_of(vec![vec![(0, text(40))]]);
    let clip = the_first_clip(&project);
    make_a_keyframe(
        &mut project,
        clip,
        PropertyId::Colour,
        10,
        PropertyValue::Colour(TextColour::new(255, 0, 0, 255)),
    );
    let plan = plan(&project);
    let layer = &plan.sections[0].layers[0];
    assert!(!layer.animation.is_empty());
    assert!(!layer.is_moved_by_keyframes());
    assert_eq!(layer.placement_at(frame(30)), layer.placement);
}

// --- what hides what ---

#[test]
fn a_video_that_keyframes_move_about_does_not_hide_what_is_behind_it_though_it_fills_the_picture_where_it_says_it_is() {
    for moved in [false, true] {
        let mut project = project_of(vec![
            vec![(0, long_video_placed(0, 60, over_the_whole_picture()))],
            vec![(0, long_video(0, 60, 1))],
        ]);
        if moved {
            let front = the_first_clip(&project);
            make_a_keyframe(
                &mut project,
                front,
                PropertyId::Position,
                30,
                PropertyValue::Pair(0.9, 0.9),
            );
        }
        let plan = plan(&project);
        let layers = &plan.sections[0].layers;
        if moved {
            assert_eq!(
                layers.len(),
                2,
                "the video behind may show, where the one in front has moved off it"
            );
            assert!(!layers[1].covers_the_picture());
        } else {
            assert_eq!(
                layers.len(),
                1,
                "without keyframes the video in front hides the one behind"
            );
            assert!(layers[0].covers_the_picture());
        }
    }
}

#[test]
fn a_video_that_has_keyframes_for_something_that_does_not_move_it_still_hides_what_is_behind_it() {
    let mut project = project_of(vec![
        vec![(0, long_video_placed(0, 60, over_the_whole_picture()))],
        vec![(0, long_video(0, 60, 1))],
    ]);
    let front = the_first_clip(&project);
    make_a_keyframe(&mut project, front, PropertyId::Gain, 30, PropertyValue::Number(-6.0));
    assert_eq!(plan(&project).sections[0].layers.len(), 1);
}

// --- text ---

#[test]
fn a_text_layer_is_the_text_object_as_its_keyframes_change_it_at_each_frame() {
    let mut project = project_of(vec![vec![(10, text(40))]]);
    let clip = the_first_clip(&project);
    let red = TextColour::new(255, 0, 0, 255);
    make_a_keyframe(
        &mut project,
        clip,
        PropertyId::Colour,
        10,
        PropertyValue::Colour(TextColour::new(0, 0, 0, 255)),
    );
    make_a_keyframe(&mut project, clip, PropertyId::Colour, 30, PropertyValue::Colour(red));
    make_a_keyframe(
        &mut project,
        clip,
        PropertyId::Text,
        10,
        PropertyValue::Words("Before".to_owned()),
    );
    make_a_keyframe(
        &mut project,
        clip,
        PropertyId::Text,
        30,
        PropertyValue::Words("Later".to_owned()),
    );
    let plan = plan(&project);
    let layer = &plan.section_at(frame(20)).unwrap().layers[0];

    let early = layer.text_at(frame(10)).unwrap();
    assert_eq!(early.colour, TextColour::new(0, 0, 0, 255));
    assert_eq!(early.text, "Before");
    let middle = layer.text_at(frame(20)).unwrap();
    assert_eq!(
        middle.colour,
        TextColour::new(128, 0, 0, 255),
        "half way from black to red"
    );
    assert_eq!(
        middle.text, "Before",
        "words do not change gradually: they stay until the next keyframe"
    );
    let late = layer.text_at(frame(35)).unwrap();
    assert_eq!(late.colour, red);
    assert_eq!(late.text, "Later");
}

#[test]
fn a_text_object_that_is_not_animated_is_lent_as_it_is_and_a_video_is_not_a_text_object() {
    let project = project_of(vec![vec![(0, text(40))], vec![(0, long_video(0, 40, 1))]]);
    let plan = plan(&project);
    let layers = &plan.sections[0].layers;
    // Back to front: the video, then the text.
    assert!(layers[0].text_at(frame(5)).is_none());
    assert!(matches!(layers[1].text_at(frame(5)), Some(Cow::Borrowed(_))));
}

#[test]
fn the_place_of_a_text_object_is_where_the_object_at_that_frame_says_it_is() {
    let mut project = project_of(vec![vec![(0, text(40))]]);
    let clip = the_first_clip(&project);
    make_a_keyframe(
        &mut project,
        clip,
        PropertyId::Position,
        0,
        PropertyValue::Pair(0.2, 0.2),
    );
    make_a_keyframe(
        &mut project,
        clip,
        PropertyId::Position,
        30,
        PropertyValue::Pair(0.8, 0.8),
    );
    make_a_keyframe(
        &mut project,
        clip,
        PropertyId::Rotation,
        30,
        PropertyValue::Number(60.0),
    );
    let plan = plan(&project);
    let layer = &plan.sections[0].layers[0];
    for timeline_frame in [0, 7, 15, 30, 39] {
        let object = layer.text_at(frame(timeline_frame)).unwrap();
        assert_eq!(
            object.placement(),
            layer.placement_at(frame(timeline_frame)),
            "frame {timeline_frame}: the object and the layer agree on where it is"
        );
    }
}

// --- sections ---

#[test]
fn a_boundary_that_changes_nothing_that_is_seen_does_not_split_the_sections_of_an_animated_clip() {
    // From the front: text that moves, a video over the whole picture, and a video behind that, which is hidden
    // by it, and whose clip starts and ends in the middle of the others.
    let mut project = project_of(vec![
        vec![(0, text(100))],
        vec![(0, long_video_placed(0, 100, over_the_whole_picture()))],
        vec![(40, long_video(0, 20, 1))],
    ]);
    let moving_text = the_first_clip(&project);
    make_a_keyframe(
        &mut project,
        moving_text,
        PropertyId::Position,
        0,
        PropertyValue::Pair(0.2, 0.2),
    );
    make_a_keyframe(
        &mut project,
        moving_text,
        PropertyId::Position,
        99,
        PropertyValue::Pair(0.8, 0.8),
    );

    let plan = plan(&project);

    assert_eq!(plan.sections.len(), 1, "{plan:?}");
    assert_eq!(plan.sections[0].layers.len(), 2);
    let text_layer = &plan.sections[0].layers[1];
    assert_centre_is(
        text_layer.placement_at(frame(99)),
        (0.8, 0.8),
        "the last frame of the one section",
    );
}

#[test]
fn the_layers_of_an_animated_clip_cut_in_two_follow_the_same_curve_as_before_the_cut() {
    let (mut project, _) = a_video_that_moves_across();
    let curve_before: Vec<Placement> = {
        let plan = plan(&project);
        (20..120)
            .map(|timeline_frame| {
                plan.section_at(frame(timeline_frame)).unwrap().layers[0].placement_at(frame(timeline_frame))
            })
            .collect()
    };
    project.split_at(frame(55), None);
    let plan = plan(&project);
    assert_eq!(
        plan.sections.len(),
        3,
        "black, then one section for each half of the clip: {plan:?}"
    );
    for (index, timeline_frame) in (20..120).enumerate() {
        let layer = &plan.section_at(frame(timeline_frame)).unwrap().layers[0];
        let after_the_cut = layer.placement_at(frame(timeline_frame));
        let before_the_cut = curve_before[index];
        assert_centre_is(
            after_the_cut,
            before_the_cut.centre(),
            &format!("frame {timeline_frame}"),
        );
    }
}

// --- checked against the project ---

#[test]
fn every_layer_is_where_and_what_the_project_says_at_every_frame_it_is_seen_at() {
    let (mut moved_layers_checked, mut animated_text_checked) = (0, 0);
    for seed in 1..=200u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed + 9000);
        let project = random_project_with_places_and_keyframes(&mut random);
        let plan = plan(&project);
        for timeline_frame in 0..plan.total_frame_count {
            let section = plan.section_at(frame(timeline_frame)).unwrap();
            for layer in &section.layers {
                let target = AnimationTarget::Clip(layer.clip);
                let placement = project.placement_of(layer.clip).unwrap();
                let says = project.placement_on_the_picture(placement.clip).unwrap();
                let expected_place = PROPERTIES_OF_WHERE_A_LAYER_IS
                    .into_iter()
                    .filter(|property| project.is_animated(target, *property))
                    .fold(says, |place, property| {
                        let value = project
                            .property_value_at(target, property, frame(timeline_frame))
                            .unwrap();
                        with_placement_value(place, property, &value)
                    });
                assert_eq!(
                    layer.placement_at(frame(timeline_frame)),
                    expected_place,
                    "seed {seed}, frame {timeline_frame}, clip {}",
                    layer.clip
                );
                moved_layers_checked += usize::from(layer.is_moved_by_keyframes());
                if let Some(object) = layer.text_at(frame(timeline_frame)) {
                    animated_text_checked += usize::from(matches!(object, Cow::Owned(_)));
                    let expected_object = crate::test_support::properties_of_every_kind()
                        .into_iter()
                        .filter(|property| project.is_animated(target, *property))
                        .fold(placement.clip.as_text().unwrap().object.clone(), |object, property| {
                            let value = project
                                .property_value_at(target, property, frame(timeline_frame))
                                .unwrap();
                            crate::project::with_text_value(&object, property, &value)
                        });
                    assert_eq!(
                        *object, expected_object,
                        "seed {seed}, frame {timeline_frame}, text clip {}",
                        layer.clip
                    );
                }
            }
        }
    }
    // (So that the projects made at random cannot stop having keyframes in them without this noticing.)
    assert!(
        moved_layers_checked > 1000,
        "only {moved_layers_checked} frames of moved layers were checked"
    );
    assert!(
        animated_text_checked > 100,
        "only {animated_text_checked} frames of animated text were checked"
    );
}
