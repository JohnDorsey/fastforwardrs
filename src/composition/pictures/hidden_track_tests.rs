// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the picture plan makes of a track whose eye is closed: **none of its clips is in the picture** (no layer, no adjustment, nothing that splits a section where the clip begins or ends), and
//! the project is as long as it was and sounds as it did. The picture is the same as if the track were not there.

use super::tests::{frame, long_video, plan, text};
use super::*;
use crate::composition::test_projects::project_of;
use crate::composition::{plan_pictures, plan_sound};
use crate::project::{AdjustmentClip, GroupKind};
use crate::test_support::put_a_hue_shift_on;

fn an_adjustment_clip(duration: u64) -> Clip {
    Clip::Adjustment(AdjustmentClip::new(duration).unwrap())
}

/// Where each section is and which clips its layers are, the one at the back first.
fn sections_and_their_clips(plan: &PicturePlan) -> Vec<(u64, u64, Vec<ClipId>)> {
    plan.sections
        .iter()
        .map(|section| {
            (
                section.first_timeline_frame,
                section.end_timeline_frame,
                section.layers.iter().map(|layer| layer.clip).collect(),
            )
        })
        .collect()
}

#[test]
fn the_clips_of_a_hidden_track_are_not_in_the_plan_and_do_not_split_its_sections_and_the_project_is_as_long_as_it_was()
{
    // A text in front, there from frame 30 to frame 60, and a video behind it for 100 frames. (In front, because the plan leaves out what a picture that covers everything hides.)
    let mut project = project_of(vec![vec![(30, text(30))], vec![(0, long_video(0, 100, 1))]]);
    let video = project.tracks()[1].placement_at_index(0).unwrap().id;
    let behind = project.tracks()[0].id();
    let seen = plan(&project);
    assert_eq!(
        sections_and_their_clips(&seen).len(),
        3,
        "seen, the text splits the picture in three"
    );

    project.set_track_hidden(behind, true).unwrap();
    let plan = plan(&project);

    assert_eq!(sections_and_their_clips(&plan), [(0, 100, vec![video])]);
    assert_eq!(plan.total_frame_count, seen.total_frame_count, "as long as it was");
    // And it is back when the eye is opened.
    project.set_track_hidden(behind, false).unwrap();
    assert_eq!(
        sections_and_their_clips(&super::tests::plan(&project)),
        sections_and_their_clips(&seen)
    );
}

#[test]
fn a_project_whose_tracks_are_all_hidden_is_as_long_as_it_was_and_every_frame_of_it_is_black() {
    let mut project = project_of(vec![vec![(0, long_video(0, 100, 1))], vec![(0, text(60))]]);
    for track in project.tracks().iter().map(|track| track.id()).collect::<Vec<_>>() {
        project.set_track_hidden(track, true).unwrap();
    }

    let plan = plan(&project);

    assert_eq!(plan.total_frame_count, 100);
    assert!(
        plan.sections
            .iter()
            .all(|section| section.layers.is_empty() && section.groups.is_empty())
    );
    assert_eq!(
        plan.sections.last().map(|section| section.end_timeline_frame),
        Some(100),
        "the sections go to the end"
    );
    assert!(plan.section_at(frame(50)).is_some(), "a black frame is still a frame");
}

#[test]
fn an_adjustment_clip_on_a_hidden_track_does_nothing_to_what_is_behind_it() {
    let mut project = project_of(vec![vec![(0, an_adjustment_clip(40))], vec![(0, text(40))]]);
    let adjustment = project.tracks()[0].placement_at_index(0).unwrap().id;
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(adjustment), 40.0);
    assert_eq!(
        super::tests::plan(&project).sections[0].groups.len(),
        1,
        "it does something when seen"
    );

    project.set_track_hidden(project.tracks()[0].id(), true).unwrap();

    let plan = plan(&project);
    assert!(
        plan.sections[0].groups.is_empty(),
        "no group is made over what is behind it"
    );
    assert_eq!(plan.sections[0].layers.len(), 1, "the text is there, as it was");
}

#[test]
fn a_track_group_of_hidden_tracks_makes_no_picture_and_one_of_a_hidden_and_a_seen_track_has_only_the_seen_one() {
    let mut project = project_of(vec![vec![(0, text(40))], vec![(0, text(40))], vec![(0, text(40))]]);
    let tracks: Vec<TrackId> = project.tracks().iter().map(|track| track.id()).collect();
    let clips: Vec<ClipId> = (0..3)
        .map(|track| project.tracks()[track].placement_at_index(0).unwrap().id)
        .collect();
    let group = project
        .create_group(
            GroupKind::TrackGroup,
            &[Member::Track(tracks[0]), Member::Track(tracks[1])],
        )
        .unwrap();
    // (A hue shift that turns the colours by some degrees: one that does nothing makes no picture of its own.)
    project.add_effect_to_the_group(group, EffectKind::HUE_SHIFT).unwrap();
    project
        .set_effect_value_of_the_group(
            group,
            crate::test_support::HUE_SHIFT_AS_THE_FIRST_EFFECT,
            &PropertyValue::Number(60.0),
        )
        .unwrap();
    assert_eq!(
        super::tests::plan(&project).sections[0].groups.len(),
        1,
        "it makes a picture when seen"
    );

    project.set_track_hidden(tracks[0], true).unwrap();
    let one_of_two = super::tests::plan(&project);
    let layers: Vec<ClipId> = one_of_two.sections[0].layers.iter().map(|layer| layer.clip).collect();
    assert_eq!(
        layers,
        [clips[2], clips[1]],
        "the hidden one is not there, the group's other member is"
    );
    assert_eq!(
        one_of_two.sections[0].groups.len(),
        1,
        "the group still makes a picture of what is left of it"
    );
    let span = &one_of_two.sections[0].groups[0];
    assert_eq!(
        span.end_layer - span.first_layer,
        1,
        "it covers the one layer that is left"
    );

    project.set_track_hidden(tracks[1], true).unwrap();
    let none_of_two = super::tests::plan(&project);
    let layers: Vec<ClipId> = none_of_two.sections[0].layers.iter().map(|layer| layer.clip).collect();
    assert_eq!(
        layers,
        [clips[2]],
        "only the track that is not in the group, and is seen"
    );
    assert!(
        none_of_two.sections[0].groups.is_empty(),
        "a group of nothing makes no picture"
    );
}

#[test]
fn the_sound_of_a_hidden_track_is_the_sound_it_was_and_only_the_picture_is_left_out() {
    let mut project = project_of(vec![vec![(0, long_video(0, 100, 1))]]);
    let seen = plan_sound(&project, frame(0), |_| true);
    let track = project.tracks()[0].id();

    project.set_track_hidden(track, true).unwrap();
    let hidden = plan_sound(&project, frame(0), |_| true);

    assert_eq!(format!("{hidden:?}"), format!("{seen:?}"));
    // (And the picture, with the same project, has nothing in it: it is not that nothing was planned at all.)
    assert!(
        plan_pictures(&project, |_| true)
            .sections
            .iter()
            .all(|section| section.layers.is_empty())
    );
}
