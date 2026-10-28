// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What is heard at every frame: for each track that has any sound, a *strip* that says, from the
//! start (of the timeline, or of the place the sound is wanted from) to the end of its last
//! sound, which clips are heard when. The strips are heard together, mixed.
//!
//! A clip whose mute switch, or whose track's, is turned by keyframes is cut into the stretches over which it is
//! heard and those over which it is silent. A clip whose gain, or whose track's, has keyframes is heard at a
//! loudness that changes with them ([`Loudness::Changing`]), and one that has none at the same loudness throughout.

use std::collections::BTreeSet;

use std::collections::HashMap;

use super::pictures::{LayerEffect, layer_effect_of};
use crate::project::tracks::{ClipId, ClipPlacement, Gain, MediaClip, Track, TrackId};
use crate::project::{
    AnimationTarget, AppliedEffect, EffectDomain, FrameRate, Member, Project, PropertyId, ResolvedAnimations,
    SourceMediaIdentifier, SpeedFactor, TimelineFrameIndex, TrackSharing,
};

/// How loud a stretch of sound is at a frame of it: the loudness of the clip and of its track together.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecibelsAt {
    /// How many frames after the first frame of the stretch.
    pub frame: u64,
    pub decibels: f32,
}

/// How loud a stretch of sound is.
#[derive(Clone, Debug, PartialEq)]
pub enum Loudness {
    /// The number the sound's samples are multiplied by, the same at every frame.
    Constant(f64),
    /// Louder at some frames than at others: how loud it is, in decibels, at each of some frames, between which it
    /// changes evenly (in decibels). Before the first and after the last it is as loud as at that one. There are at
    /// least two, the first at frame 0 and the last at the frame after the stretch's last, and not all are the same
    /// (that is [`Self::Constant`]).
    Changing(Vec<DecibelsAt>),
}

impl Loudness {
    /// The number the samples are multiplied by at `frame` frames after the start of the stretch.
    pub fn linear_factor_at(&self, frame: u64) -> f64 {
        match self {
            Self::Constant(factor) => *factor,
            Self::Changing(points) => {
                let after = points.partition_point(|point| point.frame <= frame);
                let decibels = match (after.checked_sub(1).map(|index| points[index]), points.get(after)) {
                    (Some(before), Some(next)) => {
                        let fraction = (frame - before.frame) as f64 / (next.frame - before.frame) as f64;
                        f64::from(before.decibels) + (f64::from(next.decibels) - f64::from(before.decibels)) * fraction
                    }
                    (Some(last), None) => f64::from(last.decibels),
                    (None, Some(first)) => f64::from(first.decibels),
                    (None, None) => 0.0,
                };
                Gain::linear_factor_of_decibels(decibels)
            }
        }
    }
}

/// A stretch of the sound of a file, heard during part of a track.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundClip {
    pub clip: ClipId,
    pub source: SourceMediaIdentifier,
    /// The frame of the timeline at which this stretch of the clip is first heard. (The frames of the keyframes of its effects are frames of the timeline.)
    pub first_timeline_frame: u64,
    /// How many frames of the timeline the clip was heard for before this stretch of it begins: the stretches of it that came before, or the part that a seek skipped. **It is
    /// as much of the clip's own sound as there is to be given to the effects that remember** (see `pre_roll_frames_of`): the file has more before the clip's first frame, which
    /// the clip was cut to leave out, and that is not heard.
    pub frames_of_the_clip_before: u64,
    /// The source frame whose sound is heard first. (The sound has no frames: a frame stands for
    /// the stretch of sound as long as a frame of the video.)
    pub first_source_frame: u64,
    /// The frame just after the last source frame heard (the end is exclusive).
    pub end_source_frame: u64,
    /// How much faster than normal it is played: the clip's speed times its track's.
    pub speed: SpeedFactor,
    /// How many frames of the timeline it is heard for. (If the file is too short for that, the
    /// rest is silence.)
    pub timeline_frame_count: u64,
    /// How loud it is: the clip's volume and its track's.
    pub loudness: Loudness,
    /// What is done to the sound of the clip before it is made as loud as `loudness` says, one after another: the sound effects that are applied to the clip, in the order
    /// they are applied in (see `Project::effects_applied_to_the_clip`), with their values and the keyframes of them. An effect that is not installed is here too: it is the
    /// one that makes the sound pass through unchanged, and it is kept in the plan so that the sound knows it was there.
    pub effects: Vec<LayerEffect>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SoundItem {
    /// Nothing is heard for this many frames.
    Silence {
        frame_count: u64,
    },
    Clip(SoundClip),
}

impl SoundItem {
    pub fn timeline_frame_count(&self) -> u64 {
        match self {
            Self::Silence { frame_count } => *frame_count,
            Self::Clip(clip) => clip.timeline_frame_count,
        }
    }
}

/// What one track sounds like, from the start of the plan on: its items follow one another, with
/// no gaps, until the end of the last sound.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundStrip {
    pub track: TrackId,
    pub items: Vec<SoundItem>,
    /// The instances of the effects of the track that clips share (phase 12, piece 12.2): one for each **run** of clips, in the order of `items`. A clip that is in none has
    /// all of its effects in its own chain.
    pub buses: Vec<SoundBus>,
}

/// One instance of the sound effects of a track, which hears the clips of a run one after another: they are put together (each as its own effects and its volume make it) and
/// given to the effects, which have one state from the first of them to the last.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundBus {
    /// The first and the last item of the strip that it hears (the items between are heard too).
    pub first_item: usize,
    pub last_item: usize,
    /// The effects of the track, one after another, with their values and the keyframes of them (the frames of which are frames of the timeline).
    pub effects: Vec<LayerEffect>,
}

impl SoundStrip {
    /// How many frames the strip lasts.
    pub fn timeline_frame_count(&self) -> u64 {
        self.items.iter().map(SoundItem::timeline_frame_count).sum()
    }
}

/// The sound at every frame from some frame on.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundPlan {
    pub frame_rate: FrameRate,
    /// How many frames the plan lasts: from where it starts to the end of the timeline. The strips
    /// are no longer than this.
    pub total_frame_count: u64,
    /// The strips of the tracks that have any sound, the one in front first.
    pub strips: Vec<SoundStrip>,
}

/// Works out the sound of the project from `from` to the end, as it is when all the sound is
/// on (the setting for what sped-up clips sound like is for whatever turns the plan into sound).
/// The sound of a file that is not `usable` is left out.
pub fn plan_sound(
    project: &Project,
    from: TimelineFrameIndex,
    is_usable: impl Fn(SourceMediaIdentifier) -> bool,
) -> SoundPlan {
    let from = from.value();
    let total_frame_count = project.total_frame_count().saturating_sub(from);
    let animations = project.resolved_animations();
    let effects = project.effects_applied_to_every_clip();
    let strips = project
        .tracks()
        .iter()
        .filter_map(|track| strip_of(project, &animations, &effects, track, from, &is_usable))
        .collect();
    SoundPlan {
        frame_rate: project.frame_rate(),
        total_frame_count,
        strips,
    }
}

/// Feeds everything that is heard of the sound effects of the clips of `project` into `hasher`, so that a number made from it is different whenever it is: for each clip that
/// has sound effects, which they are (the kind and version, which plugin of which library stands in for it now, if one does, so that a plugin that is found by the scan is a
/// change), their values, the keyframes of them and the state that their plugin saved. A project with no sound effects feeds nothing.
pub fn hash_the_sound_effects_into(project: &Project, hasher: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    let applied = project.effects_applied_to_every_clip();
    let animations = (!applied.is_empty()).then(|| project.resolved_animations());
    let Some(animations) = animations else {
        return;
    };
    for track in project.tracks() {
        for entry in track.entries() {
            let Some(effects) = applied.get(&entry.id()) else {
                continue;
            };
            for effect in effects
                .iter()
                .filter(|applied| applied.effect.kind().domain() == EffectDomain::Sound)
            {
                let layer = layer_effect_of(project, &animations, effect);
                entry.id().hash(hasher);
                layer.instance.number().hash(hasher);
                // (How much of the track one copy of its effects covers is heard too.)
                track.sharing().sound.hash(hasher);
                layer.kind.key().hash(hasher);
                layer.kind.version().hash(hasher);
                layer
                    .kind
                    .installed_plugin()
                    .map(|plugin| (&plugin.library, &plugin.address))
                    .hash(hasher);
                for value in &layer.values {
                    value.hash_into(hasher);
                }
                layer.animation.hash_properties_into(hasher, |_| true);
                // (What its plugin saved is heard too: a window changed it. A number that was made when it was saved: the bytes are not read at every frame.)
                effect.effect.state_fingerprint().hash(hasher);
            }
        }
    }
}

fn strip_of(
    project: &Project,
    animations: &ResolvedAnimations,
    effects: &HashMap<ClipId, Vec<AppliedEffect<'_>>>,
    track: &Track,
    from: u64,
    is_usable: &impl Fn(SourceMediaIdentifier) -> bool,
) -> Option<SoundStrip> {
    let mut items: Vec<SoundItem> = Vec::new();
    // What each item shares with the other clips of the track: the effects of the track, for a clip that has them at the end of its chain (see `effects_of_a_clip`).
    let mut shared_effects: Vec<Option<Vec<LayerEffect>>> = Vec::new();
    let mut next_frame = from;
    for placement in track.placements() {
        let Some(media) = placement.clip.as_media() else {
            continue;
        };
        let end = placement.end_timeline_frame.value();
        if end <= from {
            continue;
        }
        let start = placement.first_timeline_frame.value().max(from);
        push_silence(&mut items, start - next_frame);
        shared_effects.resize(items.len(), None);

        let has_sound = project
            .source(media.source())
            .is_some_and(|source| source.properties.has_sound())
            && is_usable(media.source());
        if has_sound {
            for stretch in stretches_heard_or_not(animations, track, &placement, media, start, end) {
                if stretch.is_heard {
                    let (own, shared) = effects_of_a_clip(project, animations, effects, track, placement.id);
                    shared_effects.resize(items.len(), None);
                    items.push(SoundItem::Clip(sound_clip_over(
                        animations,
                        own,
                        track,
                        &placement,
                        media,
                        stretch.first_frame,
                        stretch.end_frame,
                    )));
                    shared_effects.push(shared);
                } else {
                    push_silence(&mut items, stretch.end_frame - stretch.first_frame);
                    shared_effects.resize(items.len(), None);
                }
            }
        } else {
            push_silence(&mut items, end - start);
            shared_effects.resize(items.len(), None);
        }
        next_frame = end;
    }
    // Silence at the end is not part of the sound.
    while matches!(items.last(), Some(SoundItem::Silence { .. })) {
        items.pop();
    }
    shared_effects.truncate(items.len());
    shared_effects.resize(items.len(), None);
    let buses = runs_of_clips_that_share_the_effects_of_the_track(&shared_effects);
    (!items.is_empty()).then_some(SoundStrip {
        track: track.id(),
        items,
        buses,
    })
}

/// The runs of clips that share the effects of their track: consecutive items that are clips and have the effects of the track at the end of their chain, with no silence
/// between them (a silence ends the run: where the next clip begins after one, the effects begin again as they were made). Each is one instance, with the effects that all of
/// its clips have.
fn runs_of_clips_that_share_the_effects_of_the_track(shared: &[Option<Vec<LayerEffect>>]) -> Vec<SoundBus> {
    let mut buses: Vec<SoundBus> = Vec::new();
    for (place, effects) in shared.iter().enumerate() {
        let Some(effects) = effects else {
            continue;
        };
        match buses.last_mut() {
            Some(last) if last.last_item + 1 == place => last.last_item = place,
            _ => buses.push(SoundBus {
                first_item: place,
                last_item: place,
                effects: effects.clone(),
            }),
        }
    }
    buses
}

/// The sound effects that are applied to the clip `clip` (a clip of `track`), with their values and keyframes, in the order they are applied in, as two lists: **the clip's own**
/// (its own effects and those of its tags), and **the effects of its track, if the track's effects at the end of the chain are to be shared with the clips next to it** (`None`
/// when they are not: they are then part of the clip's own). They are shared when the track's sharing of its sound effects is not one copy for each clip, and the effects of the
/// track come after all the others in the clip's chain (a clip whose badge of its track is moved to the middle of its chain has its own copy of them, where the badge is).
/// (Its picture effects are not here.)
fn effects_of_a_clip(
    project: &Project,
    animations: &ResolvedAnimations,
    effects: &HashMap<ClipId, Vec<AppliedEffect<'_>>>,
    track: &Track,
    clip: ClipId,
) -> (Vec<LayerEffect>, Option<Vec<LayerEffect>>) {
    let applied: Vec<&AppliedEffect<'_>> = effects
        .get(&clip)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter(|applied| applied.effect.kind().domain() == EffectDomain::Sound)
        .collect();
    let is_the_tracks = |applied: &&AppliedEffect<'_>| applied.owner == Member::Track(track.id());
    let those_of_the_track = applied.iter().filter(|applied| is_the_tracks(applied)).count();
    let at_the_end = applied[applied.len() - those_of_the_track..].iter().all(is_the_tracks);
    let layer_of = |applied: &&AppliedEffect<'_>| layer_effect_of(project, animations, applied);
    if those_of_the_track == 0 || !at_the_end || track.sharing().sound == TrackSharing::PerClip {
        return (applied.iter().map(layer_of).collect(), None);
    }
    let (own, track_effects) = applied.split_at(applied.len() - those_of_the_track);
    (
        own.iter().map(layer_of).collect(),
        Some(track_effects.iter().map(layer_of).collect()),
    )
}

/// A stretch of a clip, from `first_frame` up to `end_frame` of the timeline, over which it is heard, or is not
/// because it or its track is muted.
struct Stretch {
    first_frame: u64,
    end_frame: u64,
    is_heard: bool,
}

/// The stretches, one after another, that the frames `start` up to `end` of the clip `placement` (a clip of `track`) are
/// cut into by turning the mute switch of the clip or of the track on and off with keyframes. A clip that is not muted by
/// keyframes is one stretch.
fn stretches_heard_or_not(
    animations: &ResolvedAnimations,
    track: &Track,
    placement: &ClipPlacement<'_>,
    media: &MediaClip,
    start: u64,
    end: u64,
) -> Vec<Stretch> {
    let clip_animation = animations.of(AnimationTarget::Clip(placement.id));
    let track_animation = animations.of(AnimationTarget::Track(track.id()));
    let is_heard_at = |frame: u64| {
        !clip_animation.is_muted_at(media.is_muted(), frame) && !track_animation.is_muted_at(track.is_muted(), frame)
    };
    // The switches only turn at a keyframe, and what is between two is what the earlier one says.
    let frames_it_may_turn_at: BTreeSet<u64> = clip_animation
        .keyframes_of(PropertyId::Muted)
        .into_iter()
        .flat_map(|keyframes| keyframes.iter().map(|keyframe| keyframe.frame))
        .chain(
            track_animation
                .keyframes_of(PropertyId::Muted)
                .into_iter()
                .flat_map(|keyframes| keyframes.iter().map(|keyframe| keyframe.frame)),
        )
        .filter(|frame| start < *frame && *frame < end)
        .collect();

    let mut stretches: Vec<Stretch> = Vec::new();
    for first_frame in std::iter::once(start).chain(frames_it_may_turn_at) {
        let is_heard = is_heard_at(first_frame);
        match stretches.last_mut() {
            Some(last) if last.is_heard == is_heard => last.end_frame = end,
            _ => {
                if let Some(last) = stretches.last_mut() {
                    last.end_frame = first_frame;
                }
                stretches.push(Stretch {
                    first_frame,
                    end_frame: end,
                    is_heard,
                });
            }
        }
    }
    stretches
}

/// The sound of the clip `placement` (a clip of `track`) that is heard from `first_frame` up to `end_frame` of the timeline.
fn sound_clip_over(
    animations: &ResolvedAnimations,
    effects: Vec<LayerEffect>,
    track: &Track,
    placement: &ClipPlacement<'_>,
    media: &MediaClip,
    first_frame: u64,
    end_frame: u64,
) -> SoundClip {
    let speed = u64::from(placement.effective_speed.value());
    let frames_cut_off_the_start = first_frame - placement.first_timeline_frame.value();
    let first_source_frame = media.first_source_frame().value() + frames_cut_off_the_start * speed;
    SoundClip {
        clip: placement.id,
        source: media.source(),
        first_timeline_frame: first_frame,
        frames_of_the_clip_before: frames_cut_off_the_start,
        first_source_frame,
        end_source_frame: media
            .end_source_frame()
            .value()
            .min(first_source_frame + (end_frame - first_frame) * speed),
        speed: placement.effective_speed,
        timeline_frame_count: end_frame - first_frame,
        loudness: loudness_over(animations, track, placement, media, first_frame, end_frame),
        effects,
    }
}

/// How loud the clip `placement` (a clip of `track`) is, together with its track, from `first_frame` up to `end_frame` of
/// the timeline.
fn loudness_over(
    animations: &ResolvedAnimations,
    track: &Track,
    placement: &ClipPlacement<'_>,
    media: &MediaClip,
    first_frame: u64,
    end_frame: u64,
) -> Loudness {
    let clip_animation = animations.of(AnimationTarget::Clip(placement.id));
    let track_animation = animations.of(AnimationTarget::Track(track.id()));
    let clip_gain_keyframes = clip_animation.keyframes_of(PropertyId::Gain);
    let track_gain_keyframes = track_animation.keyframes_of(PropertyId::Gain);
    if clip_gain_keyframes.is_none() && track_gain_keyframes.is_none() {
        return Loudness::Constant(media.gain().combined_linear_factor(track.gain()));
    }
    let decibels_at = |frame: u64| {
        clip_animation.gain_at(media.gain(), frame).decibels() + track_animation.gain_at(track.gain(), frame).decibels()
    };
    // Both change evenly between their keyframes, so together they do between all the keyframes of both.
    let frames_it_may_bend_at: BTreeSet<u64> = clip_gain_keyframes
        .into_iter()
        .flat_map(|keyframes| keyframes.iter().map(|keyframe| keyframe.frame))
        .chain(
            track_gain_keyframes
                .into_iter()
                .flat_map(|keyframes| keyframes.iter().map(|keyframe| keyframe.frame)),
        )
        .filter(|frame| first_frame < *frame && *frame < end_frame)
        .chain([first_frame, end_frame])
        .collect();
    let points: Vec<DecibelsAt> = frames_it_may_bend_at
        .into_iter()
        .map(|frame| DecibelsAt {
            frame: frame - first_frame,
            decibels: decibels_at(frame),
        })
        .collect();
    if points.iter().all(|point| point.decibels == points[0].decibels) {
        Loudness::Constant(Gain::linear_factor_of_decibels(f64::from(points[0].decibels)))
    } else {
        Loudness::Changing(points)
    }
}

/// Adds `frame_count` frames of silence, joined to the silence before if there is any.
fn push_silence(items: &mut Vec<SoundItem>, frame_count: u64) {
    if frame_count == 0 {
        return;
    }
    if let Some(SoundItem::Silence { frame_count: before }) = items.last_mut() {
        *before += frame_count;
    } else {
        items.push(SoundItem::Silence { frame_count });
    }
}

#[cfg(test)]
mod animation_tests;
#[cfg(test)]
mod buses_tests;
#[cfg(test)]
mod effects_tests;
#[cfg(test)]
mod tails_as_they_are_tests;
#[cfg(test)]
mod tests;
