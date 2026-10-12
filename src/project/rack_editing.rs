//! Editing racks and the keyframes in them: moving keyframes in time, taking them away, moving a rack to another drawer or
//! onto a track, and making keyframe crates.
//!
//! A keyframe never leaves its rack, and a rack never changes what it animates: what moves is the keyframes in time, and the
//! rack from one drawer to another. A rack keeps its name (see [`RackId`]) wherever it goes, and so do its keyframes.

use std::collections::{BTreeMap, HashSet};

use super::animating::AnimationTarget;
use super::racks::DrawerExtent;
use super::tracks::{Clip, ClipEditError, ClipId, CrateClip, KeyframeId, Rack, RackId, TrackId};
use super::{EditError, Project, PropertyId, PropertyValue, TimelineFrameIndex};

/// A keyframe to move, and the frame to move it to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyframeMove {
    pub keyframe: KeyframeId,
    /// Counted from the start of the drawer the keyframe's rack is in.
    pub to_frame: u64,
}

/// Where [`Project::move_rack`] is to put a rack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RackDestination {
    /// In the drawer of a clip (a keyframe crate is one) or of a track, under the racks that are in it.
    Drawer(AnimationTarget),
    /// In a new keyframe crate on `track` that starts at frame `start` of the timeline and lasts `length` frames.
    NewCrate { track: TrackId, start: u64, length: u64 },
}

/// A move of a rack that was found to be possible: everything that is needed to make it.
struct RackMove {
    rack: RackId,
    from: AnimationTarget,
    to: RackDestination,
    /// The rack as it is once it has gone: its keyframes at the frames of the drawer it goes to.
    moved: Rack,
}

impl Project {
    /// Where the drawer of `owner` is on the timeline, if there is such a clip or track.
    pub fn extent_of_the_drawer_of(&self, owner: AnimationTarget) -> Option<DrawerExtent> {
        match owner {
            AnimationTarget::Clip(clip) => {
                let placement = self.placement_of(clip)?;
                Some(DrawerExtent {
                    start: placement.first_timeline_frame.value(),
                    length: Some(placement.timeline_frame_count()),
                })
            }
            AnimationTarget::Track(track) => {
                self.track(track)?;
                Some(DrawerExtent { start: 0, length: None })
            }
        }
    }

    /// Moves keyframes, each to the frame given for it (counted from the start of the drawer its rack is in). Either all of
    /// them move or, if one cannot (it is not there, or would be at a frame its drawer does not have, or two keyframes of a
    /// rack would be at the same frame), none does.
    pub fn move_keyframes(&mut self, moves: &[KeyframeMove]) -> Result<(), EditError> {
        let mut by_rack: BTreeMap<RackId, Vec<(KeyframeId, u64)>> = BTreeMap::new();
        for requested in moves {
            by_rack
                .entry(requested.keyframe.rack())
                .or_default()
                .push((requested.keyframe, requested.to_frame));
        }
        // The racks as they will be are all made before any is put in, so that one that cannot be made leaves the rest alone.
        let mut moved_racks: Vec<(AnimationTarget, Rack)> = Vec::with_capacity(by_rack.len());
        for (rack_id, moves_in_the_rack) in by_rack {
            let located = self
                .located_rack(rack_id)
                .ok_or(EditError::UnknownRack { rack: rack_id })?;
            let has_a_frame_for_each = located
                .length
                .is_none_or(|length| moves_in_the_rack.iter().all(|(_, to_frame)| *to_frame < length));
            if !has_a_frame_for_each {
                return Err(EditError::NoRoomForTheKeyframes);
            }
            let mut rack = located.rack.clone();
            rack.move_keyframes(&moves_in_the_rack)?;
            moved_racks.push((located.owner, rack));
        }
        for (owner, rack) in moved_racks {
            if let Some(slot) = self.drawer_mut(owner).and_then(|drawer| drawer.rack_mut(rack.id())) {
                *slot = rack;
            }
        }
        Ok(())
    }

    /// How far later (earlier, if negative) than where they are the keyframes `moving` can be moved all together, in frames,
    /// when `wanted` is how far they are wanted: `wanted` itself, or as much of it as they can. Each keeps to the frames its
    /// drawer has, and stops short of a keyframe of its rack that is not moving (it does not pass it nor come to its
    /// frame); keyframes that move together are never in one another's way. (The names of keyframes that are not there
    /// are left out.)
    pub fn allowed_shift_of_keyframes(&self, moving: &[KeyframeId], wanted: i64) -> i64 {
        let moving_set: HashSet<KeyframeId> = moving.iter().copied().collect();
        let racks: HashSet<RackId> = moving.iter().map(|keyframe| keyframe.rack()).collect();
        let mut furthest_earlier = i64::MIN;
        let mut furthest_later = i64::MAX;
        for located in self
            .located_racks()
            .filter(|located| racks.contains(&located.rack.id()))
        {
            for keyframe in located.rack.keyframes() {
                if !moving_set.contains(&located.rack.id_of(keyframe)) {
                    continue;
                }
                let at = keyframe.frame as i64;
                furthest_earlier = furthest_earlier.max(-at);
                if let Some(length) = located.length {
                    furthest_later = furthest_later.min(length as i64 - 1 - at);
                }
                for other in located.rack.keyframes() {
                    if moving_set.contains(&located.rack.id_of(other)) {
                        continue;
                    }
                    let there = other.frame as i64;
                    if there > at {
                        furthest_later = furthest_later.min(there - at - 1);
                    } else {
                        furthest_earlier = furthest_earlier.max(there - at + 1);
                    }
                }
            }
        }
        // (A keyframe is where it is, so no limit is on the wrong side of nothing: the range holds 0.)
        wanted.clamp(furthest_earlier.min(0), furthest_later.max(0))
    }

    /// The moves that put each of the keyframes `moving` `shift` frames later (earlier, if negative) than it is. (The names of
    /// keyframes that are not there are left out. Use [`Self::allowed_shift_of_keyframes`] to find a shift that they
    /// can all be moved by.)
    pub fn moves_of_keyframes_by(&self, moving: &[KeyframeId], shift: i64) -> Vec<KeyframeMove> {
        moving
            .iter()
            .filter_map(|id| {
                let located = self.located_rack(id.rack())?;
                let keyframe = located.rack.keyframe(*id)?;
                Some(KeyframeMove {
                    keyframe: *id,
                    to_frame: keyframe.frame.checked_add_signed(shift)?,
                })
            })
            .collect()
    }

    /// The frame of the timeline that the keyframe `keyframe` is at, if it is there.
    pub fn timeline_frame_of_keyframe(&self, keyframe: KeyframeId) -> Option<u64> {
        let located = self.located_rack(keyframe.rack())?;
        let point = located.rack.keyframe(keyframe)?;
        Some(located.timeline_frame_of(point.frame))
    }

    /// Takes away the keyframes (those that are there: a name of one that is not is left alone). A rack that has none left is
    /// taken away with them, and so is a keyframe crate that has no rack left in it. A property that has no keyframe at all
    /// any more, in any rack, is not animated any more, and stays what its last keyframe in time said.
    pub fn remove_keyframes(&mut self, keyframes: &[KeyframeId]) -> Result<(), EditError> {
        // What each property that loses a keyframe ends as, by the keyframes it has now: if it loses them all, it stays that.
        let mut what_they_end_as: Vec<(AnimationTarget, PropertyId, PropertyValue)> = Vec::new();
        let mut drawers_that_lost_a_rack: Vec<AnimationTarget> = Vec::new();
        for id in keyframes {
            let Some(located) = self.located_rack(id.rack()) else {
                continue;
            };
            if located.rack.keyframe(*id).is_none() {
                continue;
            }
            let (owner, target, property) = (located.owner, located.rack.target(), located.rack.property());
            let is_new_to_the_list = !what_they_end_as
                .iter()
                .any(|(listed_target, listed_property, _)| *listed_target == target && *listed_property == property);
            if is_new_to_the_list && let Some(last) = self.keyframes_of(target, property).iter().last() {
                what_they_end_as.push((target, property, last.value.clone()));
            }
            if let Some(drawer) = self.drawer_mut(owner) {
                if let Some(rack) = drawer.rack_mut(id.rack()) {
                    rack.remove(*id);
                }
                drawer.remove_the_racks_with_no_keyframes();
            }
            if !drawers_that_lost_a_rack.contains(&owner) {
                drawers_that_lost_a_rack.push(owner);
            }
        }
        for (target, property, value) in what_they_end_as {
            if !self.is_animated(target, property) {
                self.set_static_value(target, property, &value)?;
            }
        }
        for owner in drawers_that_lost_a_rack {
            self.take_away_the_crate_if_it_has_no_rack(owner);
        }
        Ok(())
    }

    /// Whether [`Self::move_rack`] would move the rack, and if not, why not.
    pub fn check_the_move_of_a_rack(
        &self,
        rack: RackId,
        to: RackDestination,
        shift_in_frames: i64,
    ) -> Result<(), EditError> {
        self.plan_the_move_of_a_rack(rack, to, shift_in_frames).map(|_| ())
    }

    /// Moves the rack `rack` to another drawer, or to a new keyframe crate, shifting all of its keyframes `shift_in_frames`
    /// frames later (earlier if that is less than 0) than the frames of the timeline they are at now. (A shift of 0 leaves
    /// them where they are in time, whatever drawer they are in.) It goes under the racks that are in the drawer it goes to,
    /// except if that is the drawer it is in, where it keeps its place.
    ///
    /// A rack always lasts as long as the drawer it is in, so it can only go where all of its keyframes (the first and the
    /// last, and so those between) are at frames the drawer has: nothing is changed, and the error says so
    /// ([`EditError::NoRoomForTheKeyframes`]), if they are not. Nor can it go to a drawer that already has a rack of the
    /// same property of the same thing. A keyframe crate that is left with no rack by a rack that goes is taken away.
    pub fn move_rack(&mut self, rack: RackId, to: RackDestination, shift_in_frames: i64) -> Result<(), EditError> {
        let plan = self.plan_the_move_of_a_rack(rack, to, shift_in_frames)?;
        let RackMove { rack, from, to, moved } = plan;
        match to {
            RackDestination::Drawer(owner) if owner == from => {
                if let Some(slot) = self.drawer_mut(owner).and_then(|drawer| drawer.rack_mut(rack)) {
                    *slot = moved;
                }
            }
            RackDestination::Drawer(owner) => {
                if let Some(drawer) = self.drawer_mut(from) {
                    drawer.take(rack);
                }
                if let Some(drawer) = self.drawer_mut(owner) {
                    drawer.push(moved);
                }
                self.take_away_the_crate_if_it_has_no_rack(from);
            }
            RackDestination::NewCrate { track, start, length } => {
                // (The crate is made before the rack leaves where it is, so that if it could not be made, which the plan says
                // it can, the rack would not be lost.)
                let crate_clip = self.insert_crate(track, TimelineFrameIndex::new(start), length)?;
                if let Some(drawer) = self.drawer_mut(from) {
                    drawer.take(rack);
                }
                if let Some(drawer) = self.drawer_mut(AnimationTarget::Clip(crate_clip)) {
                    drawer.push(moved);
                }
                self.take_away_the_crate_if_it_has_no_rack(from);
            }
        }
        Ok(())
    }

    /// Where the drawer that a rack is to go to is on the timeline: that of the clip or track, or that of the crate that would
    /// be made, if it could be.
    fn extent_of_the_destination(&self, to: RackDestination) -> Result<DrawerExtent, EditError> {
        match to {
            RackDestination::Drawer(owner) => self.extent_of_the_drawer_of(owner).ok_or(match owner {
                AnimationTarget::Clip(clip) => EditError::Clip(ClipEditError::UnknownClip { clip }),
                AnimationTarget::Track(track) => EditError::UnknownTrack { track },
            }),
            RackDestination::NewCrate { track, start, length } => {
                CrateClip::new(length)?;
                self.track(track)
                    .ok_or(EditError::UnknownTrack { track })?
                    .start_of_the_free_stretch_that_has_room_for(TimelineFrameIndex::new(start), length)?;
                Ok(DrawerExtent {
                    start,
                    length: Some(length),
                })
            }
        }
    }

    /// How many frames later (earlier, if negative) than where they are the keyframes of the rack `rack` can be put when it is
    /// moved to `to`, if it is wanted that they are `wanted_shift` frames later: that, or as near to it as keeps all of them
    /// at frames the drawer it goes to has. (It is where a rack is dropped that decides where it goes in the drawer, as
    /// far as the drawer has room: it is not refused for being a little too far along.) Says why not if it cannot go there:
    /// [`EditError::NoRoomForTheKeyframes`] if the first and the last of its keyframes are further from one another than
    /// the drawer is long, and any reason that [`Self::check_the_move_of_a_rack`] would give.
    pub fn shift_that_fits(&self, rack: RackId, to: RackDestination, wanted_shift: i64) -> Result<i64, EditError> {
        let located = self.located_rack(rack).ok_or(EditError::UnknownRack { rack })?;
        let extent = self.extent_of_the_destination(to)?;
        let (Some(first), Some(last)) = (located.rack.first_frame(), located.rack.last_frame()) else {
            return Err(EditError::NoRoomForTheKeyframes);
        };
        let (first, last) = ((located.start + first) as i64, (located.start + last) as i64);
        let (start, span) = (extent.start as i64, last - first);
        let (earliest, latest) = match extent.length {
            Some(length) if span >= length as i64 => return Err(EditError::NoRoomForTheKeyframes),
            Some(length) => (start - first, start + length as i64 - 1 - last),
            None => (start - first, i64::MAX),
        };
        let shift = wanted_shift.clamp(earliest, latest);
        self.check_the_move_of_a_rack(rack, to, shift)?;
        Ok(shift)
    }

    /// Works out whether a rack can be moved as [`Self::move_rack`] says, and if it can, what it will be like once it has.
    fn plan_the_move_of_a_rack(
        &self,
        rack_id: RackId,
        to: RackDestination,
        shift_in_frames: i64,
    ) -> Result<RackMove, EditError> {
        let located = self
            .located_rack(rack_id)
            .ok_or(EditError::UnknownRack { rack: rack_id })?;
        let rack = located.rack;
        let destination_extent = self.extent_of_the_destination(to)?;
        // Where a frame of the rack will be: on the timeline it is `located.start` + the frame, shifted, and the drawer it
        // goes to counts from where it starts. (The shift is the same for every keyframe, so if the first and the last are in
        // the drawer, so are all of them.)
        let new_frame = |frame: u64| -> Option<u64> {
            (located.start + frame)
                .checked_add_signed(shift_in_frames)?
                .checked_sub(destination_extent.start)
                .filter(|new_frame| destination_extent.length.is_none_or(|length| *new_frame < length))
        };
        let fits = rack
            .first_frame()
            .zip(rack.last_frame())
            .is_some_and(|(first, last)| new_frame(first).is_some() && new_frame(last).is_some());
        if !fits {
            return Err(EditError::NoRoomForTheKeyframes);
        }
        if let RackDestination::Drawer(owner) = to
            && let Some(other) = self
                .drawer_of(owner)
                .and_then(|drawer| drawer.rack_for(rack.target(), rack.property()))
            && other.id() != rack_id
        {
            return Err(EditError::TheDrawerHasARackForThatAlready {
                owner,
                property: rack.property(),
            });
        }
        Ok(RackMove {
            rack: rack_id,
            from: located.owner,
            to,
            moved: rack.with_frames_mapped(|frame| new_frame(frame).unwrap_or(frame)),
        })
    }

    /// Puts a keyframe crate on the track `track`, starting at `timeline_frame` and lasting `length` frames, which holds no
    /// rack yet. There must be nothing on the track at any of those frames.
    pub fn insert_crate(
        &mut self,
        track: TrackId,
        timeline_frame: TimelineFrameIndex,
        length: u64,
    ) -> Result<ClipId, EditError> {
        let crate_clip = CrateClip::new(length)?;
        Ok(self
            .track_mut_or_error(track)?
            .insert_clip_at(timeline_frame, Clip::Crate(crate_clip))?)
    }

    /// Takes the clip `owner` off its track if it is a keyframe crate that has no rack in it (nothing else is taken away).
    fn take_away_the_crate_if_it_has_no_rack(&mut self, owner: AnimationTarget) {
        let AnimationTarget::Clip(clip) = owner else {
            return;
        };
        let has_no_rack = self
            .placement_of(clip)
            .is_some_and(|placement| matches!(placement.clip, Clip::Crate(_)) && placement.drawer.is_empty());
        if has_no_rack {
            self.delete_clip(clip)
                .expect("the crate was found on a track a moment ago");
        }
    }
}
