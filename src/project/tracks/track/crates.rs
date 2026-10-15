// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Giving a keyframe crate another stretch of its track: a crate lasts exactly as long as the keyframes in it need, so
//! its first and last frames change as they do (see [`crate::project::CrateClip`]).

use super::Track;
use super::editing::ClipEditError;
use crate::project::tracks::clip::Clip;
use crate::project::tracks::crate_clip::CrateClip;
use crate::project::tracks::ids::ClipId;

impl Track {
    /// The empty frames round the clip `clip` and the frames of the clip itself, which are the frames it could cover if it were
    /// made longer at either end: from the end of the clip before it (or 0) up to (not including) the start of the clip after
    /// it, or for ever (`None`) if there is none. `None` if there is no such clip.
    pub fn stretch_around(&self, clip: ClipId) -> Option<(u64, Option<u64>)> {
        let index = self.index_of(clip)?;
        let from_frame = index
            .checked_sub(1)
            .and_then(|before| self.placement_at_index(before))
            .map_or(0, |before| before.end_timeline_frame.value());
        let until_frame = self.first_timeline_frames.get(index + 1).copied();
        Some((from_frame, until_frame))
    }

    /// Makes the keyframe crate `clip` cover the frames from `first_frame` up to (not including) `end_frame`, which are not
    /// none, and are no further than the stretch round the crate (see [`Self::stretch_around`]). Nothing else on the track
    /// moves. The racks in the crate's drawer are not changed, though their frames are counted from where the crate begins: the
    /// project makes them so (see `Project::fit_the_crate`).
    pub(in crate::project) fn set_the_extent_of_a_crate(
        &mut self,
        clip: ClipId,
        first_frame: u64,
        end_frame: u64,
    ) -> Result<(), ClipEditError> {
        let index = self.index_of(clip).ok_or(ClipEditError::UnknownClip { clip })?;
        if !matches!(self.entries[index].clip, Clip::Crate(_)) {
            return Err(ClipEditError::NotAKeyframeCrate { clip });
        }
        let (from_frame, until_frame) = self.stretch_around(clip).ok_or(ClipEditError::UnknownClip { clip })?;
        let crate_clip = end_frame
            .checked_sub(first_frame)
            .and_then(|duration| CrateClip::new(duration).ok())
            .ok_or(ClipEditError::LastsAsLongAsItsKeyframes { clip })?;
        if first_frame < from_frame || until_frame.is_some_and(|until| end_frame > until) {
            return Err(ClipEditError::WouldOverlap { clip });
        }
        self.entries[index].clip = Clip::Crate(crate_clip);
        self.entries[index].gap_before_frames = first_frame - from_frame;
        if let (Some(next), Some(until)) = (self.entries.get_mut(index + 1), until_frame) {
            // The next clip stays where it is, so the gap before it takes up the difference.
            next.gap_before_frames = until - end_frame;
        }
        self.rebuild_derived_values();
        Ok(())
    }
}
