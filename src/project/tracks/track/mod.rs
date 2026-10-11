//! A track: a row of clips on the timeline, with a title and a volume, a speed and a mute switch
//! of its own.
//!
//! The clips on a track are in order, one after another, and may have empty time between them (or
//! before the first). Which clip is at which frame of the timeline is worked out here, in whole
//! frames, and nowhere else, so that the pictures and the sound, the live preview and the export,
//! all agree.
//!
//! **What moves when a clip changes its length.** A clip that comes from a file (a video or a
//! sound) gets longer or shorter when it is sped up or slowed down, and when the speed of the
//! track changes; and it can be deleted. The clips *touching* its end move along so that no gap
//! opens up or closes; a gap, though, stops the movement, and the clips after it stay where they
//! are (and are pushed on only if the clip before them grows into them). Text objects never move
//! because of another clip's change: they stay exactly where they were put (and are pushed on only
//! the same way). So a video made of clips that touch moves up as one, and captions stay in time.

mod editing;
mod layout;
#[cfg(test)]
mod tests;

pub use editing::{ClipEditError, InsertError, SplitError, SplitOutcome};

use super::clip::Clip;
use super::gain::Gain;
use super::ids::{ClipId, TrackId};
use crate::project::{SourceFrameIndex, SpeedFactor, TimelineFrameIndex};

/// A clip on a track, and where it is in relation to the clip before it.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackEntry {
    id: ClipId,
    clip: Clip,
    /// How many frames of empty timeline come before the clip: after the end of the clip before
    /// it, or after the start of the timeline for the first clip.
    gap_before_frames: u64,
}

impl TrackEntry {
    pub fn id(&self) -> ClipId {
        self.id
    }

    pub fn clip(&self) -> &Clip {
        &self.clip
    }

    pub fn gap_before_frames(&self) -> u64 {
        self.gap_before_frames
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    id: TrackId,
    title: String,
    gain: Gain,
    speed: SpeedFactor,
    muted: bool,
    entries: Vec<TrackEntry>,
    /// The number the next new clip of this track is given (see [`ClipId`]).
    next_clip_number: u32,
    /// `first_timeline_frames[i]` is the timeline frame at which `entries[i]` starts. It is
    /// strictly increasing, because every clip lasts at least one frame. Worked out from the gaps
    /// and the lengths after every edit, so that finding what is at a frame is fast however many
    /// clips there are.
    first_timeline_frames: Vec<u64>,
    /// The frame just after the last clip's last frame: how long the track is.
    total_frame_count: u64,
}

/// Where a clip sits on the timeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipPlacement<'track> {
    /// The clip's place in the order of the track's clips.
    pub index: usize,
    pub id: ClipId,
    pub clip: &'track Clip,
    pub first_timeline_frame: TimelineFrameIndex,
    /// The frame just after the clip's last frame (the end is exclusive).
    pub end_timeline_frame: TimelineFrameIndex,
    /// How much faster than normal the clip plays: its own speed times its track's. Normal speed
    /// for a text clip, which has none.
    pub effective_speed: SpeedFactor,
}

impl ClipPlacement<'_> {
    /// How many frames of the timeline the clip occupies.
    pub fn timeline_frame_count(&self) -> u64 {
        self.end_timeline_frame.value() - self.first_timeline_frame.value()
    }

    pub fn contains(&self, timeline_frame: TimelineFrameIndex) -> bool {
        self.first_timeline_frame <= timeline_frame && timeline_frame < self.end_timeline_frame
    }

    /// How many frames into the clip `timeline_frame` is (0 for its first frame), or `None` if the
    /// frame is not in the clip.
    pub fn offset_of(&self, timeline_frame: TimelineFrameIndex) -> Option<u64> {
        self.contains(timeline_frame)
            .then(|| timeline_frame.value() - self.first_timeline_frame.value())
    }

    /// The frame of the source file that is shown at `timeline_frame`: for a video or a sound clip
    /// that is in the clip. (`None` for a text clip, or a frame the clip is not at.)
    pub fn source_frame_at(&self, timeline_frame: TimelineFrameIndex) -> Option<SourceFrameIndex> {
        let offset = self.offset_of(timeline_frame)?;
        self.clip
            .as_media()?
            .source_frame_at_timeline_offset(self.effective_speed, offset)
    }
}

/// What is at one frame of a track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Room {
    /// A clip is there.
    Occupied(ClipId),
    /// Nothing is there: empty timeline from `from_frame` up to (not including) `until_frame`, or
    /// for ever if the frame is past the last clip.
    Free { from_frame: u64, until_frame: Option<u64> },
}

impl Track {
    /// An empty track that plays at normal speed and normal loudness.
    pub fn new(id: TrackId, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            gain: Gain::UNCHANGED,
            speed: SpeedFactor::NORMAL,
            muted: false,
            entries: Vec::new(),
            next_clip_number: 0,
            first_timeline_frames: Vec::new(),
            total_frame_count: 0,
        }
    }

    /// A track holding `clips`, each with the number of empty frames before it.
    #[must_use]
    pub fn with_clips(mut self, clips: impl IntoIterator<Item = (u64, Clip)>) -> Self {
        for (gap_before_frames, clip) in clips {
            let id = self.take_next_clip_id();
            self.entries.push(TrackEntry {
                id,
                clip,
                gap_before_frames,
            });
        }
        self.rebuild_derived_values();
        self
    }

    #[must_use]
    pub fn with_gain(mut self, gain: Gain) -> Self {
        self.gain = gain;
        self
    }

    /// The same track playing at `speed`. (Use [`Self::set_speed`] to change a track that is in a project.)
    #[must_use]
    pub fn with_speed(mut self, speed: SpeedFactor) -> Self {
        self.speed = speed;
        self.rebuild_derived_values();
        self
    }

    #[must_use]
    pub fn with_muted(mut self, muted: bool) -> Self {
        self.muted = muted;
        self
    }

    pub fn id(&self) -> TrackId {
        self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn gain(&self) -> Gain {
        self.gain
    }

    /// How much faster than normal everything on the track plays, on top of each clip's own speed.
    pub fn speed(&self) -> SpeedFactor {
        self.speed
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }

    pub fn entries(&self) -> &[TrackEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many frames the track lasts: up to the end of its last clip.
    pub fn total_frame_count(&self) -> u64 {
        self.total_frame_count
    }

    pub fn placement_at_index(&self, index: usize) -> Option<ClipPlacement<'_>> {
        let entry = self.entries.get(index)?;
        let first = self.first_timeline_frames[index];
        Some(ClipPlacement {
            index,
            id: entry.id,
            clip: &entry.clip,
            first_timeline_frame: TimelineFrameIndex::new(first),
            end_timeline_frame: TimelineFrameIndex::new(first + entry.clip.timeline_frame_count(self.speed)),
            effective_speed: entry.clip.effective_speed(self.speed),
        })
    }

    /// Every clip with its place on the timeline, in order.
    pub fn placements(&self) -> impl Iterator<Item = ClipPlacement<'_>> {
        (0..self.entries.len()).filter_map(|index| self.placement_at_index(index))
    }

    pub fn placement_of(&self, clip: ClipId) -> Option<ClipPlacement<'_>> {
        let index = self.index_of(clip)?;
        self.placement_at_index(index)
    }

    /// The clip that is at `timeline_frame`, or `None` if nothing is (the frame is in a gap, or
    /// past the end of the track).
    pub fn placement_covering(&self, timeline_frame: TimelineFrameIndex) -> Option<ClipPlacement<'_>> {
        let index = self.index_of_clip_starting_at_or_before(timeline_frame)?;
        self.placement_at_index(index)
            .filter(|placement| placement.contains(timeline_frame))
    }

    /// What is at `timeline_frame`: a clip, or how much room there is for one.
    pub fn room_at(&self, timeline_frame: TimelineFrameIndex) -> Room {
        let frame = timeline_frame.value();
        if let Some(placement) = self.placement_covering(timeline_frame) {
            return Room::Occupied(placement.id);
        }
        let clips_starting_at_or_before = self.first_timeline_frames.partition_point(|&first| first <= frame);
        let from_frame = clips_starting_at_or_before
            .checked_sub(1)
            .and_then(|index| self.placement_at_index(index))
            .map_or(0, |previous| previous.end_timeline_frame.value());
        let until_frame = self.first_timeline_frames.get(clips_starting_at_or_before).copied();
        Room::Free {
            from_frame,
            until_frame,
        }
    }

    /// The place in the order of the clip called `clip`.
    pub fn index_of(&self, clip: ClipId) -> Option<usize> {
        self.entries.iter().position(|entry| entry.id == clip)
    }

    fn index_of_clip_starting_at_or_before(&self, timeline_frame: TimelineFrameIndex) -> Option<usize> {
        self.first_timeline_frames
            .partition_point(|&first| first <= timeline_frame.value())
            .checked_sub(1)
    }

    fn take_next_clip_id(&mut self) -> ClipId {
        let id = ClipId::new(self.next_clip_number);
        self.next_clip_number += 1;
        id
    }
}
