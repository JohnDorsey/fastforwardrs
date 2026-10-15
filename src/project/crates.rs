// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Keyframe crates: making them, and keeping each exactly as long as the keyframes in it need.
//!
//! A crate (see [`CrateClip`]) is a stretch of a track that holds racks of keyframes, and it lasts from its first keyframe (in
//! whichever of its racks) to its last, and at least one frame. The racks are counted in frames from the first frame of the crate, so
//! that the crate and what is in it move together, and so that nothing but the edits of the keyframes in it ever has to change them.
//! Those edits keep it so:
//!
//! * [`Project::make_the_crate_begin_by`] makes the crate begin earlier, before keyframes are put in it, where they would be before it
//!   (the frames of its racks are counted from where it begins, so they are counted from the earlier place from then on);
//! * [`Project::fit_the_crate`] makes it as long as the keyframes in it need, after keyframes have gone out of it or in: longer, where
//!   there is room, which is the empty frames of the track round it, as far as the end of the video; shorter if they leave it room; and
//!   not there at all if they leave it none.

use super::racks::{FrameRoom, room_of_the_drawer_of};
use super::tracks::{Clip, ClipId, CrateClip};
use super::{AnimationTarget, EditError, Project, TimelineFrameIndex, TrackId};

impl Project {
    /// Puts a keyframe crate on the track `track`, starting at `timeline_frame` and lasting `length` frames, which holds no
    /// rack yet. There must be nothing on the track at any of those frames, and it must not be inside the rectangle of a clip group
    /// (where only the clips that are in the group may be), which is the error [`EditError::BreaksTheRulesOfGroups`] says.
    ///
    /// A crate that has no rack in it is not one that keeps ([`Self::fit_the_crate`] takes it away as soon as it is looked at), so this is
    /// only for an edit that puts racks in it before it is done (see [`Self::move_racks`], which is how a crate is made by the person
    /// using the editor), and for the tests of the project, which do the same.
    pub(super) fn insert_crate(
        &mut self,
        track: TrackId,
        timeline_frame: TimelineFrameIndex,
        length: u64,
    ) -> Result<ClipId, EditError> {
        let crate_clip = CrateClip::new(length)?;
        self.keeping_the_groups_valid(|project| {
            Ok(project
                .track_mut_or_error(track)?
                .insert_clip_at(timeline_frame, Clip::Crate(crate_clip))?)
        })
    }

    /// Whether the clip or track `owner` is a keyframe crate.
    pub(super) fn is_a_crate(&self, owner: AnimationTarget) -> bool {
        matches!(owner, AnimationTarget::Clip(clip)
            if self.placement_of(clip).is_some_and(|placement| matches!(placement.clip, Clip::Crate(_))))
    }

    /// The frames that the keyframes of the crate `clip` can be at, which are those that it could be made to cover: its own, and the
    /// empty frames round it, as far as the end of the video (a crate does not make the video longer).
    pub(super) fn room_of_the_crate(&self, clip: ClipId) -> Option<FrameRoom> {
        let track = &self.tracks[self.track_position_holding(clip)?];
        let placement = track.placement_of(clip)?;
        Some(room_of_the_drawer_of(track, &placement, self.total_frame_count()))
    }

    /// Makes the keyframe crate `owner` begin at the frame `first_frame` of the timeline, if it is a crate that begins after that
    /// (nothing is done for any other owner of a drawer, or for a crate that begins at that frame or before it), so that keyframes can
    /// be put in it at frames that are before where it begins: the frames of its racks are counted from where it begins, so what is in it
    /// stays at the frames of the timeline it is at, and a rack that is put in it after this is counted from the earlier place. Nothing
    /// else on the track moves. It is not made longer at its end: [`Self::fit_the_crate`] does that once the keyframes are in it.
    /// [`EditError::NoRoomForTheKeyframes`] if there is not room for it to begin there: nothing is changed.
    pub(super) fn make_the_crate_begin_by(
        &mut self,
        owner: AnimationTarget,
        first_frame: u64,
    ) -> Result<(), EditError> {
        let AnimationTarget::Clip(clip) = owner else {
            return Ok(());
        };
        let Some(placement) = self.placement_of(clip) else {
            return Ok(());
        };
        let (start, end) = (
            placement.first_timeline_frame.value(),
            placement.end_timeline_frame.value(),
        );
        if !matches!(placement.clip, Clip::Crate(_)) || first_frame >= start {
            return Ok(());
        }
        self.set_the_extent_of_the_crate(clip, first_frame, end)?;
        // The frames of the racks are counted from where the crate begins, which is earlier now.
        let more = start - first_frame;
        if let Some(drawer) = self.drawer_mut(owner) {
            *drawer = drawer.with_each_rack_changed(|rack| rack.with_frames_mapped(|frame| frame + more));
        }
        Ok(())
    }

    /// Makes the keyframe crate `owner` last exactly as long as the keyframes in its racks need: from the first of them to the
    /// last, and takes it away if it has none. (Nothing is done for any other owner of a drawer.) It is made longer only where the
    /// keyframes that are beyond it have room (see [`Self::make_the_crate_begin_by`], which is what makes it begin earlier before they are
    /// put there): [`EditError::NoRoomForTheKeyframes`] if there is not, and nothing is changed.
    pub(super) fn fit_the_crate(&mut self, owner: AnimationTarget) -> Result<(), EditError> {
        let AnimationTarget::Clip(clip) = owner else {
            return Ok(());
        };
        let Some(placement) = self.placement_of(clip) else {
            return Ok(());
        };
        if !matches!(placement.clip, Clip::Crate(_)) {
            return Ok(());
        }
        let start = placement.first_timeline_frame.value();
        let frames_of_keyframes = placement
            .drawer
            .racks()
            .iter()
            .filter_map(|rack| rack.first_frame().zip(rack.last_frame()));
        let first_and_last = frames_of_keyframes.fold(None, |found: Option<(u64, u64)>, (first, last)| match found {
            None => Some((first, last)),
            Some((earliest, latest)) => Some((earliest.min(first), latest.max(last))),
        });
        let Some((first, last)) = first_and_last else {
            // (A crate that is taken away moves nothing, so what the groups hold cannot be broken by it, and it is not looked at: the
            // crate is taken out of the groups it was in, which is all there is to do for them.)
            return self.delete_clip_without_looking_at_the_groups(clip);
        };
        if first == 0 && last + 1 == placement.timeline_frame_count() {
            return Ok(());
        }
        let (new_start, new_end) = (start + first, start + last + 1);
        let room = self.room_of_the_crate(clip).ok_or(EditError::NoRoomForTheKeyframes)?;
        if !room.contains(new_start) || room.end_frame.is_some_and(|room_end| new_end > room_end) {
            return Err(EditError::NoRoomForTheKeyframes);
        }
        self.set_the_extent_of_the_crate(clip, new_start, new_end)?;
        if first > 0 {
            // The frames of the racks are counted from where the crate begins, which is later now.
            if let Some(drawer) = self.drawer_mut(owner) {
                *drawer = drawer.with_each_rack_changed(|rack| rack.with_frames_mapped(|frame| frame - first));
            }
        }
        Ok(())
    }

    /// [`Self::fit_the_crate`] for each of `owners` (the ones that are not crates are left alone, and one that is in the list more than once
    /// is fitted once, so that a list may be made of every drawer that an edit touched, repeats and all).
    pub(super) fn fit_the_crates(&mut self, owners: &[AnimationTarget]) -> Result<(), EditError> {
        for (place, owner) in owners.iter().enumerate() {
            if !owners[..place].contains(owner) {
                self.fit_the_crate(*owner)?;
            }
        }
        Ok(())
    }

    /// Makes every crate of the project last as long as its keyframes need and takes away those that have none (for a project that
    /// is loaded from a file, or has been counted again at another frame rate).
    pub fn fit_every_crate(&mut self) -> Result<(), EditError> {
        let crates: Vec<AnimationTarget> = self
            .tracks
            .iter()
            .flat_map(|track| track.placements())
            .filter(|placement| matches!(placement.clip, Clip::Crate(_)))
            .map(|placement| AnimationTarget::Clip(placement.id))
            .collect();
        self.fit_the_crates(&crates)
    }

    /// Makes the keyframe crate `clip` cover the frames from `first_frame` up to (not including) `end_frame`.
    fn set_the_extent_of_the_crate(&mut self, clip: ClipId, first_frame: u64, end_frame: u64) -> Result<(), EditError> {
        let position = self
            .track_position_holding(clip)
            .ok_or(super::ClipEditError::UnknownClip { clip })?;
        self.tracks[position]
            .set_the_extent_of_a_crate(clip, first_frame, end_frame)
            .map_err(|_| EditError::NoRoomForTheKeyframes)
    }

    /// Makes `edit`, and puts the project back as it was if it fails: for an edit that changes several things, some of which are
    /// found out to be wrong only after others are changed.
    pub(super) fn atomically<T>(
        &mut self,
        edit: impl FnOnce(&mut Self) -> Result<T, EditError>,
    ) -> Result<T, EditError> {
        let before = self.clone();
        let outcome = edit(self);
        if outcome.is_err() {
            *self = before;
        }
        outcome
    }
}
