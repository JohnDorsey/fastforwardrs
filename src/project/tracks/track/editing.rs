//! Changing a track: cutting, changing speeds and volumes, deleting, adding and changing text.

use thiserror::Error;

use super::{Room, Track, TrackEntry};
use crate::project::tracks::clip::Clip;
use crate::project::tracks::gain::Gain;
use crate::project::tracks::ids::ClipId;
use crate::project::tracks::text_object::{TextClip, TextObject};
use crate::project::{SpeedFactor, TimelineFrameIndex};

/// The result of cutting a clip in two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitOutcome {
    /// The first half, which is the clip that was cut: it keeps the clip's name.
    pub first_half: ClipId,
    pub second_half: ClipId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum SplitError {
    #[error("there is no clip at {timeline_frame} to cut")]
    NoClipThere { timeline_frame: TimelineFrameIndex },
    #[error("the clip at {timeline_frame} already starts there")]
    AlreadyAtClipBoundary { timeline_frame: TimelineFrameIndex },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ClipEditError {
    #[error("the track has no {clip}")]
    UnknownClip { clip: ClipId },
    #[error("{clip} is not a video or a sound, so this does not apply to it")]
    NotAMediaClip { clip: ClipId },
    #[error("{clip} is not a text object, so this does not apply to it")]
    NotATextClip { clip: ClipId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum InsertError {
    #[error("{timeline_frame} is taken by another clip")]
    Occupied { timeline_frame: TimelineFrameIndex },
    #[error("the clip would run into the next one: there is room for {frames_of_room} frames, and it needs {needed}")]
    NotEnoughRoom { frames_of_room: u64, needed: u64 },
}

impl Track {
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    pub fn set_gain(&mut self, gain: Gain) {
        self.gain = gain;
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    /// Makes everything on the track play `speed` times faster (on top of the clips' own speeds).
    /// The clips that come from files get shorter or longer, and the ones touching them follow.
    pub fn set_speed(&mut self, speed: SpeedFactor) {
        if speed == self.speed {
            return;
        }
        let anchors = self.anchors();
        self.speed = speed;
        self.reposition(&anchors);
    }

    /// Cuts the clip at `timeline_frame` in two, so that the clip's frame at that position becomes
    /// the first frame of the second half. Both halves keep everything else about the clip, and
    /// together last as long as the clip did, so that nothing else moves.
    pub fn split_at(&mut self, timeline_frame: TimelineFrameIndex) -> Result<SplitOutcome, SplitError> {
        let placement = self
            .placement_covering(timeline_frame)
            .ok_or(SplitError::NoClipThere { timeline_frame })?;
        let index = placement.index;
        let frames_in = timeline_frame.value() - placement.first_timeline_frame.value();
        let (first_half, second_half) = placement
            .clip
            .split_after(frames_in, self.speed)
            .ok_or(SplitError::AlreadyAtClipBoundary { timeline_frame })?;

        let second_id = self.take_next_clip_id();
        let first_id = self.entries[index].id;
        self.entries[index].clip = first_half;
        self.entries.insert(
            index + 1,
            TrackEntry {
                id: second_id,
                clip: second_half,
                gap_before_frames: 0,
            },
        );
        self.rebuild_derived_values();
        Ok(SplitOutcome {
            first_half: first_id,
            second_half: second_id,
        })
    }

    /// Plays the clip `clip` at `speed`. The clips touching it move along to close up or make room.
    pub fn set_clip_speed(&mut self, clip: ClipId, speed: SpeedFactor) -> Result<(), ClipEditError> {
        self.change_media_clip(clip, |media| media.with_speed(speed))
    }

    /// Makes the clip `clip` louder or quieter than it was.
    pub fn set_clip_gain(&mut self, clip: ClipId, gain: Gain) -> Result<(), ClipEditError> {
        self.change_media_clip(clip, |media| media.with_gain(gain))
    }

    /// Mutes the clip `clip`, or lets it be heard again.
    pub fn set_clip_muted(&mut self, clip: ClipId, muted: bool) -> Result<(), ClipEditError> {
        self.change_media_clip(clip, |media| media.with_muted(muted))
    }

    fn change_media_clip(
        &mut self,
        clip: ClipId,
        change: impl FnOnce(&crate::project::tracks::media_clip::MediaClip) -> crate::project::tracks::media_clip::MediaClip,
    ) -> Result<(), ClipEditError> {
        let index = self.index_of(clip).ok_or(ClipEditError::UnknownClip { clip })?;
        let anchors = self.anchors();
        let changed = match &self.entries[index].clip {
            Clip::Video(media) => Clip::Video(change(media)),
            Clip::Audio(media) => Clip::Audio(change(media)),
            Clip::Text(_) => return Err(ClipEditError::NotAMediaClip { clip }),
        };
        self.entries[index].clip = changed;
        self.reposition(&anchors);
        Ok(())
    }

    /// Takes the clip `clip` off the track. If it was a video or a sound, the clips touching its
    /// end move up to where it began, so that no hole is left; if it was text, nothing moves.
    pub fn delete_clip(&mut self, clip: ClipId) -> Result<(), ClipEditError> {
        let index = self.index_of(clip).ok_or(ClipEditError::UnknownClip { clip })?;
        let mut anchors = self.anchors();
        let removed_anchor = anchors.remove(index);
        let removed = self.entries.remove(index);
        if let Some(next_anchor) = anchors.get_mut(index) {
            if removed.clip.is_media() && next_anchor.moves_along {
                // It was touching the clip that has gone, so it closes up to where that began.
                *next_anchor = super::layout::Anchor {
                    start: removed_anchor.start,
                    moves_along: false,
                };
            } else if !removed.clip.is_media() {
                // A text object that goes leaves what came after it where it was.
                next_anchor.moves_along = false;
            }
        }
        self.reposition(&anchors);
        Ok(())
    }

    /// Changes what the text clip `clip` says and how it looks. (Its length is not touched: see
    /// [`Self::set_text_duration`].)
    pub fn set_text_object(&mut self, clip: ClipId, object: TextObject) -> Result<(), ClipEditError> {
        let index = self.index_of(clip).ok_or(ClipEditError::UnknownClip { clip })?;
        let Clip::Text(text) = &mut self.entries[index].clip else {
            return Err(ClipEditError::NotATextClip { clip });
        };
        text.object = object;
        Ok(())
    }

    /// Makes the text clip `clip` last `duration_in_frames` frames, or as long as there is room
    /// for before the next clip if that is less (and at least one frame). Nothing else moves.
    /// Returns how many frames it lasts now.
    pub fn set_text_duration(&mut self, clip: ClipId, duration_in_frames: u64) -> Result<u64, ClipEditError> {
        let index = self.index_of(clip).ok_or(ClipEditError::UnknownClip { clip })?;
        let start = self.first_timeline_frames[index];
        let longest = self
            .first_timeline_frames
            .get(index + 1)
            .map_or(u64::MAX, |next_start| next_start - start);
        let Clip::Text(text) = &mut self.entries[index].clip else {
            return Err(ClipEditError::NotATextClip { clip });
        };
        let new_duration = duration_in_frames.clamp(1, longest);
        if let Some(changed) = text.with_duration_in_frames(new_duration) {
            let old_end = start + text.duration_in_frames();
            *text = changed;
            if let Some(next) = self.entries.get_mut(index + 1) {
                // The next clip stays where it is, so the gap before it takes up the difference.
                next.gap_before_frames = next.gap_before_frames + old_end - (start + new_duration);
            }
            self.rebuild_derived_values();
        }
        Ok(new_duration)
    }

    /// How many frames long a new text clip can be if it starts at `timeline_frame`, when it is
    /// wanted to last `wanted_frames`: that, or less if it would run into the next clip. `None` if
    /// another clip is at that frame.
    pub fn longest_new_text_clip_at(&self, timeline_frame: TimelineFrameIndex, wanted_frames: u64) -> Option<u64> {
        match self.room_at(timeline_frame) {
            Room::Occupied(_) => None,
            Room::Free { until_frame, .. } => Some(
                until_frame
                    .map_or(wanted_frames, |until| {
                        (until - timeline_frame.value()).min(wanted_frames)
                    })
                    .max(1),
            ),
        }
    }

    /// Puts `clip` on the track so that it starts at `timeline_frame`. There must be nothing at
    /// any of the frames it occupies. Nothing else moves.
    pub fn insert_clip_at(&mut self, timeline_frame: TimelineFrameIndex, clip: Clip) -> Result<ClipId, InsertError> {
        let needed = clip.timeline_frame_count(self.speed);
        let Room::Free {
            from_frame,
            until_frame,
        } = self.room_at(timeline_frame)
        else {
            return Err(InsertError::Occupied { timeline_frame });
        };
        if let Some(until_frame) = until_frame {
            let frames_of_room = until_frame - timeline_frame.value();
            if needed > frames_of_room {
                return Err(InsertError::NotEnoughRoom { frames_of_room, needed });
            }
        }

        let index = self
            .first_timeline_frames
            .partition_point(|&first| first <= timeline_frame.value());
        let id = self.take_next_clip_id();
        let new_start = timeline_frame.value();
        let new_end = new_start + needed;
        if let Some(next) = self.entries.get_mut(index) {
            // The next clip stays where it is, so the gap before it takes up the difference.
            next.gap_before_frames -= new_end - from_frame;
        }
        self.entries.insert(
            index,
            TrackEntry {
                id,
                clip,
                gap_before_frames: new_start - from_frame,
            },
        );
        self.rebuild_derived_values();
        Ok(id)
    }

    /// Adds a text clip at `timeline_frame` that lasts `wanted_frames` frames, or less if it would
    /// run into the next clip. Returns the clip's name and how many frames it lasts, or `None` if
    /// another clip is at that frame.
    pub fn insert_text_at(
        &mut self,
        timeline_frame: TimelineFrameIndex,
        object: TextObject,
        wanted_frames: u64,
    ) -> Option<(ClipId, u64)> {
        let duration = self.longest_new_text_clip_at(timeline_frame, wanted_frames)?;
        let clip = Clip::Text(TextClip::new(object, duration).ok()?);
        let id = self.insert_clip_at(timeline_frame, clip).ok()?;
        Some((id, duration))
    }
}
