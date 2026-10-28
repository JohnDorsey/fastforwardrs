// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The instances of the sound effects of a track that clips share (phase 12, piece 12.2; docs/engine-phase-12-design.md, sections 2 and 5): what the plan says of them, and which clips
//! have the effects of their track in their own chain instead.

use std::hash::{DefaultHasher, Hasher};

use super::*;
use crate::composition::test_projects::{SOUND_FILE, media_clip, project_of};
use crate::media::export::sound_graph_specification;
use crate::project::FrameRate;
use crate::project::tracks::Clip;
use crate::project::{
    Badge, ChainItemKey, EffectDomain, EffectKind, InstalledPlugin, Member, PropertyValue, SourceMediaIdentifier,
    SpedUpClipSound, TrackSharing, set_installed_plugins,
};
use crate::test_support::{an_installed_sound_plugin, put_an_effect_on, the_installed_plugins_to_myself};

fn plan(project: &Project) -> SoundPlan {
    plan_sound(project, TimelineFrameIndex::new(0), |_| true)
}

/// Two sound plugins, as installed kinds of effect: the first and the second.
fn two_kinds() -> (EffectKind, EffectKind) {
    let plugin =
        |key: &str| -> InstalledPlugin { an_installed_sound_plugin(key, "Maker", &[("gain", 1.0, Some((0.0, 4.0)))]) };
    set_installed_plugins(vec![plugin("ladspa:a:first"), plugin("ladspa:a:second")]);
    let kinds = EffectKind::installed_plugin_kinds();
    (kinds[0], kinds[1])
}

/// A track of sound clips, one after another with the gaps before them that are given, and a track of one sound clip.
fn a_track_of_clips_with_gaps(gaps: &[u64]) -> Project {
    let clips: Vec<(u64, Clip)> = gaps
        .iter()
        .map(|gap| (*gap, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))))
        .collect();
    project_of(vec![clips, vec![(0, Clip::Audio(media_clip(SOUND_FILE, 0, 60, 1)))]])
}

fn clip_at(project: &Project, track: usize, index: usize) -> crate::project::ClipId {
    project.tracks()[track].placement_at_index(index).unwrap().id
}

fn values_of(effects: &[LayerEffect]) -> Vec<PropertyValue> {
    effects.iter().map(|effect| effect.values[0].clone()).collect()
}

fn clips_of(strip: &SoundStrip) -> Vec<&SoundClip> {
    strip
        .items
        .iter()
        .filter_map(|item| match item {
            SoundItem::Clip(clip) => Some(clip),
            SoundItem::Silence { .. } => None,
        })
        .collect()
}

#[test]
fn the_effects_of_a_track_are_one_instance_that_the_clips_that_follow_one_another_share_and_are_not_in_the_chain_of_any_clip()
 {
    let _alone = the_installed_plugins_to_myself();
    let (kind, _) = two_kinds();
    let mut project = a_track_of_clips_with_gaps(&[0, 0, 0]);
    let track = project.tracks()[0].id();
    put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(2.0)],
    );

    let plan = plan(&project);

    let strip = &plan.strips[0];
    assert_eq!(strip.items.len(), 3);
    assert_eq!(strip.buses.len(), 1, "three clips in a row: one instance");
    assert_eq!((strip.buses[0].first_item, strip.buses[0].last_item), (0, 2));
    assert_eq!(values_of(&strip.buses[0].effects), [PropertyValue::Number(2.0)]);
    assert!(
        clips_of(strip).iter().all(|clip| clip.effects.is_empty()),
        "the clips do not have them in their chain"
    );
    assert!(plan.strips[1].buses.is_empty(), "the other track has no effects");
}

#[test]
fn a_gap_between_two_clips_ends_a_run_and_the_next_clip_has_an_instance_that_begins_as_it_was_made() {
    let _alone = the_installed_plugins_to_myself();
    let (kind, _) = two_kinds();
    let mut project = a_track_of_clips_with_gaps(&[0, 0, 15, 0]);
    let track = project.tracks()[0].id();
    put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(2.0)],
    );

    let plan = plan(&project);

    // The items: clip, clip, silence, clip, clip.
    let strip = &plan.strips[0];
    assert_eq!(strip.items.len(), 5);
    let ranges: Vec<(usize, usize)> = strip.buses.iter().map(|bus| (bus.first_item, bus.last_item)).collect();
    assert_eq!(ranges, [(0, 1), (3, 4)]);
}

#[test]
fn with_one_copy_of_the_effects_for_each_clip_the_effects_are_in_the_chain_of_each_clip_as_they_were_before() {
    let _alone = the_installed_plugins_to_myself();
    let (kind, _) = two_kinds();
    let mut project = a_track_of_clips_with_gaps(&[0, 0]);
    let track = project.tracks()[0].id();
    put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(2.0)],
    );
    project
        .set_track_sharing(track, EffectDomain::Sound, TrackSharing::PerClip)
        .unwrap();

    let plan = plan(&project);

    let strip = &plan.strips[0];
    assert!(strip.buses.is_empty());
    for clip in clips_of(strip) {
        assert_eq!(values_of(&clip.effects), [PropertyValue::Number(2.0)]);
    }
}

#[test]
fn the_sharing_of_the_picture_effects_of_a_track_changes_nothing_in_the_sound() {
    let _alone = the_installed_plugins_to_myself();
    let (kind, _) = two_kinds();
    let mut project = a_track_of_clips_with_gaps(&[0, 0]);
    let track = project.tracks()[0].id();
    put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(2.0)],
    );
    let before = plan(&project);

    project
        .set_track_sharing(track, EffectDomain::Picture, TrackSharing::PerClip)
        .unwrap();

    assert_eq!(plan(&project), before);
}

#[test]
fn the_effects_of_the_clip_and_of_its_tags_stay_in_the_chain_of_the_clip_and_those_of_the_track_that_come_after_them_are_the_instance()
 {
    let _alone = the_installed_plugins_to_myself();
    let (own_kind, track_kind) = two_kinds();
    let mut project = a_track_of_clips_with_gaps(&[0, 0]);
    let track = project.tracks()[0].id();
    for index in 0..2 {
        let clip = clip_at(&project, 0, index);
        put_an_effect_on(
            &mut project,
            AnimationTarget::Clip(clip),
            own_kind,
            &[PropertyValue::Number(3.0)],
        );
    }
    put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        track_kind,
        &[PropertyValue::Number(2.0)],
    );

    let plan = plan(&project);

    let strip = &plan.strips[0];
    assert_eq!(strip.buses.len(), 1);
    assert_eq!(values_of(&strip.buses[0].effects), [PropertyValue::Number(2.0)]);
    for clip in clips_of(strip) {
        assert_eq!(values_of(&clip.effects), [PropertyValue::Number(3.0)], "the clip's own");
    }
}

#[test]
fn a_clip_that_has_the_badge_of_its_track_in_the_middle_of_its_chain_has_its_own_copy_there_and_does_not_share() {
    let _alone = the_installed_plugins_to_myself();
    let (own_kind, track_kind) = two_kinds();
    let mut project = a_track_of_clips_with_gaps(&[0, 0, 0]);
    let track = project.tracks()[0].id();
    let middle = clip_at(&project, 0, 1);
    let own = put_an_effect_on(
        &mut project,
        AnimationTarget::Clip(middle),
        own_kind,
        &[PropertyValue::Number(3.0)],
    );
    put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        track_kind,
        &[PropertyValue::Number(2.0)],
    );
    // The badge of the track goes before the clip's own effect: the track's effects are done first, and then the clip's.
    project
        .move_chain_item(
            Member::Clip(middle),
            ChainItemKey::Badge(Badge::Track),
            Some(ChainItemKey::Effect(own)),
        )
        .unwrap();

    let plan = plan(&project);

    let strip = &plan.strips[0];
    let clips = clips_of(strip);
    assert_eq!(
        values_of(&clips[1].effects),
        [PropertyValue::Number(2.0), PropertyValue::Number(3.0)],
        "the clip with the badge in the middle has the track's effects where it is, and then its own"
    );
    assert!(clips[0].effects.is_empty() && clips[2].effects.is_empty());
    let ranges: Vec<(usize, usize)> = strip.buses.iter().map(|bus| (bus.first_item, bus.last_item)).collect();
    assert_eq!(
        ranges,
        [(0, 0), (2, 2)],
        "the clips on either side do not share it with it: they are two runs"
    );
}

#[test]
fn the_instances_are_in_the_specification_that_the_sound_is_made_from() {
    let _alone = the_installed_plugins_to_myself();
    let (kind, _) = two_kinds();
    let mut project = a_track_of_clips_with_gaps(&[0, 0]);
    let track = project.tracks()[0].id();
    put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(2.0)],
    );
    let plan = plan(&project);
    let sources: Vec<SourceMediaIdentifier> = crate::media::export::sources_of_sound(&plan).into_iter().collect();

    let specification = sound_graph_specification(&plan, &sources, SpedUpClipSound::MaintainPitch);

    let bus = &specification.strips[0].buses;
    assert_eq!(bus.len(), 1);
    assert_eq!((bus[0].first_item, bus[0].last_item), (0, 1));
    assert_eq!(values_of(&bus[0].effects), [PropertyValue::Number(2.0)]);
}

#[test]
fn a_change_of_how_much_of_the_track_one_copy_of_its_sound_effects_covers_is_a_change_of_what_is_heard() {
    let _alone = the_installed_plugins_to_myself();
    let (kind, _) = two_kinds();
    let mut project = a_track_of_clips_with_gaps(&[0, 0]);
    let track = project.tracks()[0].id();
    put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(2.0)],
    );
    let hash = |project: &Project| {
        let mut hasher = DefaultHasher::new();
        hash_the_sound_effects_into(project, &mut hasher);
        hasher.finish()
    };
    let as_it_is = hash(&project);

    project
        .set_track_sharing(track, EffectDomain::Sound, TrackSharing::PerClip)
        .unwrap();

    assert_ne!(hash(&project), as_it_is);
    project
        .set_track_sharing(track, EffectDomain::Picture, TrackSharing::PerTrackIndivisible)
        .unwrap();
    let with_the_sound_alone = hash(&project);
    project
        .set_track_sharing(track, EffectDomain::Picture, TrackSharing::PerRun)
        .unwrap();
    assert_eq!(
        hash(&project),
        with_the_sound_alone,
        "the sharing of the picture effects is not heard"
    );
}

// --- tails ---

fn a_plugin_effect(tail_length: crate::project::TailLength, pre_roll_samples: usize) -> LayerEffect {
    let mut plugin = an_installed_sound_plugin("ladspa:a:tail", "Maker", &[("gain", 1.0, Some((0.0, 4.0)))]);
    plugin.pre_roll_samples = pre_roll_samples;
    set_installed_plugins(vec![plugin]);
    LayerEffect {
        clock: Default::default(),
        instance: crate::project::EffectInstanceId::FIRST,
        kind: EffectKind::installed_plugin_kinds()[0],
        values: vec![PropertyValue::Number(1.0)],
        animation: Default::default(),
        tables: Vec::new(),
        state: None,
        tail_length,
        shared_by: None,
    }
}

#[test]
fn the_tail_of_an_effect_is_what_its_person_said_and_where_they_said_nothing_what_its_plugin_says_and_an_effect_that_is_not_a_plugin_has_none()
 {
    use crate::project::TailLength;
    let _alone = the_installed_plugins_to_myself();

    assert_eq!(tail_samples_of(&a_plugin_effect(TailLength::Automatic, 24_000)), 24_000);
    assert_eq!(tail_samples_of(&a_plugin_effect(TailLength::None, 24_000)), 0);
    assert_eq!(
        tail_samples_of(&a_plugin_effect(TailLength::Milliseconds(250), 24_000)),
        12_000
    );
    assert_eq!(
        tail_samples_of(&a_plugin_effect(TailLength::Milliseconds(2_500), 0)),
        120_000
    );
    let mut not_a_plugin = a_plugin_effect(TailLength::Milliseconds(250), 0);
    not_a_plugin.kind = EffectKind::HUE_SHIFT;
    assert_eq!(tail_samples_of(&not_a_plugin), 0, "the sound goes through it unchanged");
    assert_eq!(
        tail_samples_of_a_chain(&[
            a_plugin_effect(TailLength::Milliseconds(100), 0),
            a_plugin_effect(TailLength::Milliseconds(200), 0)
        ]),
        14_400,
        "a chain rings on for the tails of its effects added up"
    );
    assert_eq!(tail_samples_of_a_chain(&[]), 0);
}

#[test]
fn a_number_of_samples_reaches_a_whole_number_of_frames_rounded_up_and_the_plan_counts_samples_as_the_engine_does() {
    let thirty = FrameRate::new(30, 1).unwrap();
    assert_eq!(frames_that_reach(thirty, 0), 0);
    assert_eq!(frames_that_reach(thirty, 1), 1);
    assert_eq!(frames_that_reach(thirty, 1600), 1);
    assert_eq!(frames_that_reach(thirty, 1601), 2);
    assert_eq!(frames_that_reach(FrameRate::new(24, 1).unwrap(), 48_000), 24);
    assert_eq!(frames_that_reach(FrameRate::new(30_000, 1001).unwrap(), 48_000), 30);
    assert_eq!(SOUND_SAMPLES_A_SECOND, u64::from(crate::sound_engine::SAMPLE_RATE));
}

/// The clips of `gaps` (the number before each is the frames of silence before it) with the effect on the track given this tail: the runs that the plan makes, as the first and the last item of each.
fn runs_with_a_tail_of(tail: crate::project::TailLength, gaps: &[u64]) -> Vec<(usize, usize)> {
    let mut project = a_track_of_clips_with_gaps(gaps);
    let (kind, _) = two_kinds();
    let track = project.tracks()[0].id();
    let instance = put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(2.0)],
    );
    project
        .set_effect_tail_length(Member::Track(track), instance, tail)
        .unwrap();
    plan(&project).strips[0]
        .buses
        .iter()
        .map(|bus| (bus.first_item, bus.last_item))
        .collect()
}

#[test]
fn a_gap_that_is_no_longer_than_the_tail_of_the_effects_of_the_track_is_inside_a_run_and_a_longer_one_ends_it() {
    use crate::project::TailLength;
    let _alone = the_installed_plugins_to_myself();

    // A tail of a second is 30 frames at 30 frames a second.
    assert_eq!(
        runs_with_a_tail_of(TailLength::Milliseconds(1_000), &[0, 15]),
        [(0, 2)],
        "15 frames of silence between: the effects are still ringing, one instance hears both clips and the silence"
    );
    assert_eq!(
        runs_with_a_tail_of(TailLength::Milliseconds(1_000), &[0, 30]),
        [(0, 2)],
        "as long as the tail: still one"
    );
    assert_eq!(
        runs_with_a_tail_of(TailLength::Milliseconds(1_000), &[0, 31]),
        [(0, 0), (2, 2)],
        "one frame longer than the tail: the memory is gone, and the next clip has the effects as they were made"
    );
    assert_eq!(
        runs_with_a_tail_of(TailLength::None, &[0, 1]),
        [(0, 0), (2, 2)],
        "no tail: a silence of any length ends a run"
    );
    assert_eq!(
        runs_with_a_tail_of(TailLength::None, &[0, 0]),
        [(0, 1)],
        "and clips that touch are one"
    );
    // (Each gap is its own silence: three gaps of ten frames between four clips are three silences, each within the tail, and one run.)
    assert_eq!(
        runs_with_a_tail_of(TailLength::Milliseconds(1_000), &[0, 10, 10, 10]),
        [(0, 6)]
    );
}

// --- lead-in ---

/// A track of three clips of 30 frames, one after the other (0 to 90), with the track's effect given this tail, planned from `from`.
fn planned_from(from: u64, tail: crate::project::TailLength) -> SoundPlan {
    let mut project = a_track_of_clips_with_gaps(&[0, 0, 0]);
    let (kind, _) = two_kinds();
    let track = project.tracks()[0].id();
    let instance = put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(2.0)],
    );
    project
        .set_effect_tail_length(Member::Track(track), instance, tail)
        .unwrap();
    plan_sound(&project, TimelineFrameIndex::new(from), |_| true)
}

#[test]
fn a_plan_from_a_frame_begins_before_it_by_as_much_as_the_effects_ring_on_for_and_what_is_made_of_that_is_not_heard() {
    use crate::project::TailLength;
    let _alone = the_installed_plugins_to_myself();

    // A tail of a second is 30 frames: the plan from frame 60 begins at frame 30, with the clip that ended there, and the first of the 30 frames that it has more is not heard.
    let plan = planned_from(60, TailLength::Milliseconds(1_000));

    assert_eq!(plan.lead_in_frames, 30);
    assert_eq!(
        plan.total_frame_count, 60,
        "from frame 30 to the end of the timeline (frame 90)"
    );
    let strip = &plan.strips[0];
    let first_frames: Vec<u64> = clips_of(strip).iter().map(|clip| clip.first_timeline_frame).collect();
    assert_eq!(
        first_frames,
        [30, 60],
        "the clip that is heard from frame 60, and the one before it, which the effects are still ringing from"
    );
}

#[test]
fn the_lead_in_is_no_more_than_there_is_before_the_frame_and_is_nothing_where_nothing_rings_and_from_the_start() {
    use crate::project::TailLength;
    let _alone = the_installed_plugins_to_myself();

    assert_eq!(
        planned_from(10, TailLength::Milliseconds(1_000)).lead_in_frames,
        10,
        "there is only so much before it"
    );
    assert_eq!(
        planned_from(0, TailLength::Milliseconds(1_000)).lead_in_frames,
        0,
        "from the start there is no before"
    );
    assert_eq!(
        planned_from(60, TailLength::None).lead_in_frames,
        0,
        "no tail: nothing rings"
    );
    let from_the_middle = planned_from(60, TailLength::None);
    assert_eq!(from_the_middle.total_frame_count, 30);
    assert_eq!(
        planned_from(80, TailLength::Milliseconds(60_000)).lead_in_frames,
        80,
        "a minute is more than there is before it: no more than is before it"
    );
    let mut project = a_track_of_clips_with_gaps(&[0, 0, 0]);
    // (A project with no sound effects at all has none, wherever it is planned from.)
    project
        .set_track_sharing(project.tracks()[0].id(), EffectDomain::Sound, TrackSharing::PerClip)
        .unwrap();
    assert_eq!(
        plan_sound(&project, TimelineFrameIndex::new(60), |_| true).lead_in_frames,
        0
    );
}

#[test]
fn the_lead_in_is_no_more_than_ten_seconds_and_the_tails_of_what_a_clip_goes_through_are_added() {
    use crate::project::TailLength;
    let _alone = the_installed_plugins_to_myself();
    let (own_kind, track_kind) = two_kinds();
    let mut project = a_track_of_clips_with_gaps(&[0]);
    let track = project.tracks()[0].id();
    let clip = clip_at(&project, 0, 0);
    let on_the_clip = put_an_effect_on(
        &mut project,
        AnimationTarget::Clip(clip),
        own_kind,
        &[PropertyValue::Number(3.0)],
    );
    let on_the_track = put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        track_kind,
        &[PropertyValue::Number(2.0)],
    );
    project
        .set_effect_tail_length(Member::Clip(clip), on_the_clip, TailLength::Milliseconds(500))
        .unwrap();
    project
        .set_effect_tail_length(Member::Track(track), on_the_track, TailLength::Milliseconds(500))
        .unwrap();

    // 500 ms and 500 ms: the sound of the clip goes through both, so the lead-in is 30 frames.
    assert_eq!(
        plan_sound(&project, TimelineFrameIndex::new(1_000), |_| true).lead_in_frames,
        30
    );

    project
        .set_effect_tail_length(Member::Track(track), on_the_track, TailLength::Milliseconds(600_000))
        .unwrap();
    assert_eq!(
        plan_sound(&project, TimelineFrameIndex::new(100_000), |_| true).lead_in_frames,
        300,
        "ten seconds at 30 frames a second, however long they ring"
    );
}
