//! What is heard at every frame: for each track that has any sound, a *strip* that says, from the
//! start (of the timeline, or of the place the sound is wanted from) to the end of its last
//! sound, which clips are heard when. The strips are heard together, mixed.

use crate::project::tracks::{ClipLocation, TrackId};
use crate::project::{FrameRate, Project, SourceMediaIdentifier, SpeedFactor, TimelineFrameIndex};

/// A stretch of the sound of a file, heard during part of a track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundClip {
    pub clip: ClipLocation,
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
    /// The number the sound's samples are multiplied by: the clip's volume and its track's.
    pub loudness_factor: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
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
    let strips = project
        .tracks()
        .iter()
        .filter_map(|track| strip_of(project, track, from, &is_usable))
        .collect();
    SoundPlan {
        frame_rate: project.frame_rate(),
        total_frame_count,
        strips,
    }
}

fn strip_of(
    project: &Project,
    track: &crate::project::tracks::Track,
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

        let frame_count = end - start;
        let has_sound = project
            .source(media.source())
            .is_some_and(|source| source.properties.has_sound())
            && is_usable(media.source());
        if has_sound && !track.is_muted() && !media.is_muted() {
            let frames_cut_off_the_start = start - placement.first_timeline_frame.value();
            items.push(SoundItem::Clip(SoundClip {
                clip: ClipLocation {
                    track: track.id(),
                    clip: placement.id,
                },
                source: media.source(),
                first_source_frame: media.first_source_frame().value()
                    + frames_cut_off_the_start * u64::from(placement.effective_speed.value()),
                end_source_frame: media.end_source_frame().value(),
                speed: placement.effective_speed,
                timeline_frame_count: frame_count,
                loudness_factor: media.gain().combined_linear_factor(track.gain()),
            }));
        } else {
            push_silence(&mut items, frame_count);
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
mod tests;
