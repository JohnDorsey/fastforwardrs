//! What is heard at every frame: for each track that has any sound, a *strip* that says, from the
//! start (of the timeline, or of the place the sound is wanted from) to the end of its last
//! sound, which clips are heard when. The strips are heard together, mixed.
//!
//! A clip whose mute switch, or whose track's, is turned by keyframes is cut into the stretches over which it is
//! heard and those over which it is silent. A clip whose gain, or whose track's, has keyframes is heard at a
//! loudness that changes with them ([`Loudness::Changing`]), and one that has none at the same loudness throughout.

use std::collections::BTreeSet;

use crate::project::tracks::{ClipId, ClipPlacement, Gain, MediaClip, Track, TrackId};
use crate::project::{
    AnimationTarget, FrameRate, Project, PropertyId, ResolvedAnimations, SourceMediaIdentifier, SpeedFactor,
    TimelineFrameIndex,
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
    let strips = project
        .tracks()
        .iter()
        .filter_map(|track| strip_of(project, &animations, track, from, &is_usable))
        .collect();
    SoundPlan {
        frame_rate: project.frame_rate(),
        total_frame_count,
        strips,
    }
}

fn strip_of(
    project: &Project,
    animations: &ResolvedAnimations,
    track: &Track,
    from: u64,
    is_usable: &impl Fn(SourceMediaIdentifier) -> bool,
) -> Option<SoundStrip> {
    let mut items: Vec<SoundItem> = Vec::new();
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

        let has_sound = project
            .source(media.source())
            .is_some_and(|source| source.properties.has_sound())
            && is_usable(media.source());
        if has_sound {
            for stretch in stretches_heard_or_not(animations, track, &placement, media, start, end) {
                if stretch.is_heard {
                    items.push(SoundItem::Clip(sound_clip_over(
                        animations,
                        track,
                        &placement,
                        media,
                        stretch.first_frame,
                        stretch.end_frame,
                    )));
                } else {
                    push_silence(&mut items, stretch.end_frame - stretch.first_frame);
                }
            }
        } else {
            push_silence(&mut items, end - start);
        }
        next_frame = end;
    }
    // Silence at the end is not part of the sound.
    while matches!(items.last(), Some(SoundItem::Silence { .. })) {
        items.pop();
    }
    (!items.is_empty()).then_some(SoundStrip {
        track: track.id(),
        items,
    })
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
        first_source_frame,
        end_source_frame: media
            .end_source_frame()
            .value()
            .min(first_source_frame + (end_frame - first_frame) * speed),
        speed: placement.effective_speed,
        timeline_frame_count: end_frame - first_frame,
        loudness: loudness_over(animations, track, placement, media, first_frame, end_frame),
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
mod tests;
