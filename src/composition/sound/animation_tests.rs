// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of what the sound plan makes of clips and tracks that have keyframes for how loud they are and whether they are
//! silent.

use super::*;
use crate::composition::test_projects::{
    LONG_VIDEO, SOUND_FILE, media_clip, project_of, random_project_with_sound_keyframes,
};
use crate::project::tracks::Clip;
use crate::project::{AnimationTarget, PropertyValue};
use crate::test_support::{DeterministicRandomNumbers, make_a_keyframe};

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

fn long_video(first: u64, end: u64, factor: u32) -> Clip {
    Clip::Video(media_clip(LONG_VIDEO, first, end, factor))
}

fn sound_file(first: u64, end: u64, factor: u32) -> Clip {
    Clip::Audio(media_clip(SOUND_FILE, first, end, factor))
}

fn plan(project: &Project) -> SoundPlan {
    plan_sound(project, frame(0), |_| true)
}

fn decibels(decibels: f32) -> PropertyValue {
    PropertyValue::Number(decibels)
}

/// A project of one track with a sound clip of 90 frames starting at frame 0, and the clip.
fn project_with_a_sound_clip() -> (Project, ClipId) {
    let project = project_of(vec![vec![(0, sound_file(0, 90, 1))]]);
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    (project, clip)
}

/// The pieces of the one strip in short: how long each lasts, and for a clip its source range.
fn pieces(strip: &SoundStrip) -> Vec<(u64, Option<(u64, u64)>)> {
    strip
        .items
        .iter()
        .map(|item| match item {
            SoundItem::Silence { frame_count } => (*frame_count, None),
            SoundItem::Clip(clip) => (
                clip.timeline_frame_count,
                Some((clip.first_source_frame, clip.end_source_frame)),
            ),
        })
        .collect()
}

fn the_clip(strip: &SoundStrip, index: usize) -> &SoundClip {
    match &strip.items[index] {
        SoundItem::Clip(clip) => clip,
        SoundItem::Silence { .. } => panic!("item {index} is silence"),
    }
}

fn factor(decibels: f64) -> f64 {
    10f64.powf(decibels / 20.0)
}

// --- how loud ---

#[test]
fn a_clip_whose_gain_has_keyframes_is_as_loud_as_they_say_changing_evenly_between_them() {
    let (mut project, clip) = project_with_a_sound_clip();
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Gain,
        0,
        decibels(0.0),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Gain,
        60,
        decibels(-12.0),
    );

    let plan = plan(&project);

    assert_eq!(plan.strips[0].items.len(), 1);
    assert_eq!(
        the_clip(&plan.strips[0], 0).loudness,
        Loudness::Changing(vec![
            DecibelsAt {
                frame: 0,
                decibels: 0.0
            },
            DecibelsAt {
                frame: 60,
                decibels: -12.0
            },
            DecibelsAt {
                frame: 90,
                decibels: -12.0
            },
        ]),
        "it is as loud at the end as at the last keyframe"
    );
    let loudness = &the_clip(&plan.strips[0], 0).loudness;
    assert!((loudness.linear_factor_at(0) - 1.0).abs() < 1e-12);
    assert!(
        (loudness.linear_factor_at(30) - factor(-6.0)).abs() < 1e-9,
        "half way down"
    );
    assert!((loudness.linear_factor_at(60) - factor(-12.0)).abs() < 1e-9);
    assert!((loudness.linear_factor_at(89) - factor(-12.0)).abs() < 1e-9);
}

#[test]
fn the_gain_of_the_clip_is_the_loudness_of_the_clip_and_that_of_its_track_is_the_loudness_of_the_strip_and_together_they_are_what_both_say()
 {
    let (mut project, clip) = project_with_a_sound_clip();
    let track = project.tracks()[0].id();
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Gain,
        0,
        decibels(0.0),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Gain,
        40,
        decibels(-8.0),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Track(track),
        PropertyId::Gain,
        0,
        decibels(0.0),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Track(track),
        PropertyId::Gain,
        80,
        decibels(8.0),
    );

    let plan = plan(&project);

    assert_eq!(
        the_clip(&plan.strips[0], 0).loudness,
        Loudness::Changing(vec![
            DecibelsAt {
                frame: 0,
                decibels: 0.0
            },
            DecibelsAt {
                frame: 40,
                decibels: -8.0
            },
            DecibelsAt {
                frame: 90,
                decibels: -8.0
            },
        ]),
        "the clip's own: down 8 decibels by frame 40 and there for the rest of the clip"
    );
    assert_eq!(
        plan.strips[0].loudness,
        Loudness::Changing(vec![
            DecibelsAt {
                frame: 0,
                decibels: 0.0
            },
            DecibelsAt {
                frame: 80,
                decibels: 8.0
            },
            DecibelsAt {
                frame: 90,
                decibels: 8.0
            },
        ]),
        "the track's own, counted from the start of the plan: up 8 decibels by frame 80, and there to the end of the strip"
    );
    // Together they are what the two say: -4 decibels at frame 20 (-4 and +4... the clip is at -4, the track at +2), and so on.
    let together = |frame: u64| {
        the_clip(&plan.strips[0], 0).loudness.linear_factor_at(frame) * plan.strips[0].loudness.linear_factor_at(frame)
    };
    let factor = |decibels: f64| 10f64.powf(decibels / 20.0);
    assert!((together(20) - factor(-4.0 + 2.0)).abs() < 1e-9);
    assert!((together(40) - factor(-8.0 + 4.0)).abs() < 1e-9);
    assert!((together(80) - factor(-8.0 + 8.0)).abs() < 1e-9);
}

#[test]
fn a_gain_that_is_the_same_wherever_it_is_looked_at_is_a_constant_loudness() {
    // One keyframe says the gain is that everywhere.
    let (mut project, clip) = project_with_a_sound_clip();
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Gain,
        30,
        decibels(-6.0),
    );
    let plan = plan(&project);
    match the_clip(&plan.strips[0], 0).loudness {
        Loudness::Constant(factor_of_the_clip) => assert!((factor_of_the_clip - factor(-6.0)).abs() < 1e-9),
        ref other => panic!("{other:?}"),
    }
}

#[test]
fn a_clip_that_has_no_keyframes_for_its_gain_keeps_exactly_the_factor_it_always_had() {
    let mut project = project_of(vec![vec![(0, long_video(0, 90, 1))]]);
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    project.set_clip_gain(clip, Gain::from_decibels(6.0)).unwrap();
    // (Keyframes for what is not heard change nothing.)
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Position,
        10,
        PropertyValue::Pair(0.5, 0.5),
    );
    let plan = plan(&project);
    assert_eq!(
        the_clip(&plan.strips[0], 0).loudness,
        Loudness::Constant(Gain::from_decibels(6.0).combined_linear_factor(Gain::UNCHANGED))
    );
}

#[test]
fn a_plan_that_starts_in_the_middle_of_a_clip_counts_the_loudness_from_there() {
    let (mut project, clip) = project_with_a_sound_clip();
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Gain,
        0,
        decibels(0.0),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Gain,
        60,
        decibels(-12.0),
    );

    let plan = plan_sound(&project, frame(30), |_| true);

    assert_eq!(
        the_clip(&plan.strips[0], 0).loudness,
        Loudness::Changing(vec![
            DecibelsAt {
                frame: 0,
                decibels: -6.0
            },
            DecibelsAt {
                frame: 30,
                decibels: -12.0
            },
            DecibelsAt {
                frame: 60,
                decibels: -12.0
            },
        ]),
        "it starts at -6, half way down, and gets to -12 30 frames later"
    );
}

// --- silent ---

#[test]
fn a_clip_muted_by_keyframes_is_cut_into_the_stretches_that_are_heard_and_the_one_that_is_silent() {
    let (mut project, clip) = project_with_a_sound_clip();
    let target = AnimationTarget::Clip(clip);
    make_a_keyframe(&mut project, target, PropertyId::Muted, 0, PropertyValue::Flag(false));
    make_a_keyframe(&mut project, target, PropertyId::Muted, 30, PropertyValue::Flag(true));
    make_a_keyframe(&mut project, target, PropertyId::Muted, 60, PropertyValue::Flag(false));

    let plan = plan(&project);

    assert_eq!(
        pieces(&plan.strips[0]),
        vec![(30, Some((0, 30))), (30, None), (30, Some((60, 90)))],
        "the sound after the silence is the sound that is at that time of the clip"
    );
}

#[test]
fn silence_at_the_end_of_a_clip_that_is_muted_there_is_dropped_like_other_silence_at_the_end() {
    let (mut project, clip) = project_with_a_sound_clip();
    let target = AnimationTarget::Clip(clip);
    make_a_keyframe(&mut project, target, PropertyId::Muted, 0, PropertyValue::Flag(false));
    make_a_keyframe(&mut project, target, PropertyId::Muted, 60, PropertyValue::Flag(true));

    let plan = plan(&project);

    assert_eq!(pieces(&plan.strips[0]), vec![(60, Some((0, 60)))]);
    assert_eq!(plan.total_frame_count, 90, "the timeline is as long as it was");
}

#[test]
fn a_clip_that_is_muted_by_keyframes_all_the_time_makes_no_strip() {
    let (mut project, clip) = project_with_a_sound_clip();
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Muted,
        10,
        PropertyValue::Flag(true),
    );
    assert!(plan(&project).strips.is_empty());
}

#[test]
fn a_track_muted_by_keyframes_is_muted_after_its_effects_for_as_long_as_it_says_and_its_clips_are_all_there() {
    let mut project = project_of(vec![vec![(0, sound_file(0, 40, 1)), (0, sound_file(40, 100, 1))]]);
    let track = AnimationTarget::Track(project.tracks()[0].id());
    make_a_keyframe(&mut project, track, PropertyId::Muted, 0, PropertyValue::Flag(false));
    make_a_keyframe(&mut project, track, PropertyId::Muted, 30, PropertyValue::Flag(true));
    make_a_keyframe(&mut project, track, PropertyId::Muted, 50, PropertyValue::Flag(false));

    let plan = plan(&project);

    assert_eq!(
        pieces(&plan.strips[0]),
        vec![(40, Some((0, 40))), (60, Some((40, 100)))],
        "the clips are as they are: the track is muted after its effects, and what its effects remember goes on"
    );
    assert_eq!(
        plan.strips[0].muted,
        vec![(30, 50)],
        "the mute is a stretch of the strip, from frame 30 up to frame 50"
    );
}

#[test]
fn a_clip_is_silent_while_it_is_muted_and_a_track_is_muted_after_its_effects_while_it_is() {
    let (mut project, clip) = project_with_a_sound_clip();
    let track = AnimationTarget::Track(project.tracks()[0].id());
    let clip = AnimationTarget::Clip(clip);
    make_a_keyframe(&mut project, clip, PropertyId::Muted, 0, PropertyValue::Flag(false));
    make_a_keyframe(&mut project, clip, PropertyId::Muted, 20, PropertyValue::Flag(true));
    make_a_keyframe(&mut project, clip, PropertyId::Muted, 40, PropertyValue::Flag(false));
    make_a_keyframe(&mut project, track, PropertyId::Muted, 0, PropertyValue::Flag(false));
    make_a_keyframe(&mut project, track, PropertyId::Muted, 30, PropertyValue::Flag(true));
    make_a_keyframe(&mut project, track, PropertyId::Muted, 60, PropertyValue::Flag(false));

    let plan = plan(&project);

    assert_eq!(
        pieces(&plan.strips[0]),
        vec![(20, Some((0, 20))), (20, None), (50, Some((40, 90)))],
        "silent from 20 to 40, where the clip is muted"
    );
    assert_eq!(
        plan.strips[0].muted,
        vec![(30, 60)],
        "and the track is muted from 30 to 60, after its effects: so the two are silent from 20 to 60"
    );
}

#[test]
fn a_plan_that_starts_in_a_stretch_that_is_silent_starts_with_the_silence_that_is_left() {
    let (mut project, clip) = project_with_a_sound_clip();
    let target = AnimationTarget::Clip(clip);
    make_a_keyframe(&mut project, target, PropertyId::Muted, 0, PropertyValue::Flag(false));
    make_a_keyframe(&mut project, target, PropertyId::Muted, 30, PropertyValue::Flag(true));
    make_a_keyframe(&mut project, target, PropertyId::Muted, 60, PropertyValue::Flag(false));

    let plan = plan_sound(&project, frame(40), |_| true);

    assert_eq!(pieces(&plan.strips[0]), vec![(20, None), (30, Some((60, 90)))]);
}

#[test]
fn the_pieces_of_a_sped_up_clip_run_on_from_each_other_in_the_file() {
    let mut project = project_of(vec![vec![(0, sound_file(0, 180, 2))]]);
    let clip = AnimationTarget::Clip(project.tracks()[0].placement_at_index(0).unwrap().id);
    make_a_keyframe(&mut project, clip, PropertyId::Muted, 0, PropertyValue::Flag(false));
    make_a_keyframe(&mut project, clip, PropertyId::Muted, 30, PropertyValue::Flag(true));
    make_a_keyframe(&mut project, clip, PropertyId::Muted, 60, PropertyValue::Flag(false));

    let plan = plan(&project);

    // 90 frames of the timeline at twice the speed: 30 of them are heard, 30 not, and 30 are.
    assert_eq!(
        pieces(&plan.strips[0]),
        vec![(30, Some((0, 60))), (30, None), (30, Some((120, 180)))]
    );
}

// --- checked against the project ---

/// What track `track` plays at `timeline_frame`, worked out the plain way with what the project says: the clip, its source
/// frame and how loud it is (as the number the samples are multiplied by).
fn plainly_heard(
    project: &Project,
    track: &crate::project::tracks::Track,
    timeline_frame: u64,
) -> Option<(ClipId, u64, f64)> {
    let placement = track.placement_covering(frame(timeline_frame))?;
    let media = placement.clip.as_media()?;
    if !project.source(media.source())?.properties.has_sound() {
        return None;
    }
    let clip = AnimationTarget::Clip(placement.id);
    let track_target = AnimationTarget::Track(track.id());
    let flag = |target| match project.property_value_at(target, PropertyId::Muted, frame(timeline_frame)) {
        Some(PropertyValue::Flag(muted)) => muted,
        other => panic!("{other:?}"),
    };
    let decibels_of = |target| match project.property_value_at(target, PropertyId::Gain, frame(timeline_frame)) {
        Some(PropertyValue::Number(decibels)) => f64::from(decibels),
        other => panic!("{other:?}"),
    };
    (!flag(clip) && !flag(track_target)).then(|| {
        (
            placement.id,
            placement.source_frame_at(frame(timeline_frame)).unwrap().value(),
            factor(decibels_of(clip) + decibels_of(track_target)),
        )
    })
}

/// What the strip plays at `offset` frames from its start: the clip, its source frame and its loudness factor.
fn heard_at(strip: &SoundStrip, offset: u64) -> Option<(ClipId, u64, f64)> {
    // (The mute and the volume of the track are done to the sound of the whole track, after its effects: they are in the strip, and not in its items.)
    if strip.muted.iter().any(|&(first, end)| first <= offset && offset < end) {
        return None;
    }
    let mut item_start = 0;
    for item in &strip.items {
        let length = item.timeline_frame_count();
        if offset < item_start + length {
            return match item {
                SoundItem::Silence { .. } => None,
                SoundItem::Clip(clip) => Some((
                    clip.clip,
                    clip.first_source_frame + (offset - item_start) * u64::from(clip.speed.value()),
                    clip.loudness.linear_factor_at(offset - item_start) * strip.loudness.linear_factor_at(offset),
                )),
            };
        }
        item_start += length;
    }
    None
}

#[test]
fn the_plan_has_at_every_frame_exactly_the_sound_and_loudness_the_project_says_when_keyframes_change_them() {
    let (mut changing_checked, mut clips_muted_by_keyframes, mut tracks_muted_by_keyframes) = (0, 0, 0);
    for seed in 1..=300u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed + 11_000);
        let project = random_project_with_sound_keyframes(&mut random);
        let from = random.next_below(project.total_frame_count() + 20);
        let plan = plan_sound(&project, frame(from), |_| true);
        let describe = || format!("seed {seed}, from {from}: tracks {:?}", project.tracks());

        for strip in &plan.strips {
            assert!(
                matches!(strip.items.last(), Some(SoundItem::Clip(_))),
                "a strip ends with sound, not silence: {}",
                describe()
            );
            changing_checked += strip
                .items
                .iter()
                .filter(|item| {
                    matches!(
                        item,
                        SoundItem::Clip(SoundClip {
                            loudness: Loudness::Changing(_),
                            ..
                        })
                    )
                })
                .count();
            // (A track's own volume is in its strip, not in its clips: those count too.)
            changing_checked += usize::from(matches!(strip.loudness, Loudness::Changing(_)));
        }
        for track in project.tracks() {
            let strip = plan.strips.iter().find(|strip| strip.track == track.id());
            for offset in 0..plan.total_frame_count {
                let expected = plainly_heard(&project, track, from + offset);
                let heard = strip.and_then(|strip| heard_at(strip, offset));
                match (heard, expected) {
                    (None, None) => {}
                    (
                        Some((clip, source_frame, factor_heard)),
                        Some((expected_clip, expected_source_frame, expected_factor)),
                    ) => {
                        assert_eq!(
                            (clip, source_frame),
                            (expected_clip, expected_source_frame),
                            "{} at frame {}: {}",
                            track.title(),
                            from + offset,
                            describe()
                        );
                        assert!(
                            (factor_heard - expected_factor).abs() <= expected_factor * 1e-6,
                            "{} at frame {}: loud {factor_heard}, not {expected_factor}: {}",
                            track.title(),
                            from + offset,
                            describe()
                        );
                    }
                    (heard, expected) => panic!(
                        "{} at frame {}: {heard:?}, not {expected:?}: {}",
                        track.title(),
                        from + offset,
                        describe()
                    ),
                }
            }
        }
        tracks_muted_by_keyframes += project
            .tracks()
            .iter()
            .filter(|track| project.is_animated(AnimationTarget::Track(track.id()), PropertyId::Muted))
            .count();
        clips_muted_by_keyframes += project
            .tracks()
            .iter()
            .flat_map(|track| track.placements())
            .filter(|placement| project.is_animated(AnimationTarget::Clip(placement.id), PropertyId::Muted))
            .count();
    }
    // (So that the projects made at random cannot stop having keyframes that matter in them without this noticing.)
    assert!(
        changing_checked > 100,
        "only {changing_checked} clips with a changing loudness were checked"
    );
    assert!(
        clips_muted_by_keyframes > 100,
        "only {clips_muted_by_keyframes} clips with keyframes for being silent"
    );
    assert!(
        tracks_muted_by_keyframes > 30,
        "only {tracks_muted_by_keyframes} tracks with keyframes for being silent"
    );
}
