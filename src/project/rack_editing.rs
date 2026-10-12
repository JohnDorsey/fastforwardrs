//! Editing racks and the keyframes in them: moving keyframes in time and from one rack to another, taking them away, moving a
//! rack to another drawer (or to another place among the racks of the drawer it is in) or onto a track, and making keyframe
//! crates.
//!
//! A keyframe never changes what it animates, and neither does a rack: what moves is the keyframes in time and from a rack to
//! another rack of the same property of the same thing, and the rack from one drawer to another. A rack keeps its name (see
//! [`RackId`]) wherever it goes, and so does a keyframe (see [`KeyframeId`]).

use std::collections::{BTreeMap, HashSet};

use super::animating::AnimationTarget;
use super::racks::DrawerExtent;
use super::tracks::{
    Clip, ClipEditError, ClipId, CrateClip, KeyframeId, Rack, RackEditError, RackId, RackKeyframe, TrackId,
};
use super::{EditError, Project, PropertyId, PropertyValue, TimelineFrameIndex};

/// A keyframe to move, and where to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyframeMove {
    pub keyframe: KeyframeId,
    /// The rack the keyframe goes to: the one it is in to leave it there, or another that animates the same property of the
    /// same thing.
    pub to_rack: RackId,
    /// Counted from the start of the drawer that `to_rack` is in.
    pub to_frame: u64,
}

/// A keyframe, and the rack it is to be moved to, wherever in time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyframeDestination {
    pub keyframe: KeyframeId,
    pub rack: RackId,
}

/// Where [`Project::move_rack`] is to put a rack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RackDestination {
    /// In the drawer of a clip (a keyframe crate is one) or of a track.
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

    /// Whether [`Self::move_keyframes`] would move the keyframes, and if not, why not.
    pub fn check_the_move_of_keyframes(&self, moves: &[KeyframeMove]) -> Result<(), EditError> {
        self.plan_the_move_of_keyframes(moves).map(|_| ())
    }

    /// Moves keyframes, each to the rack and the frame given for it (counted from the start of the drawer that rack is in). A
    /// keyframe goes from a rack to another that animates the same property of the same thing, which keeps its name. Either
    /// all of them move or, if one cannot (it is not there, or would go to a rack that animates something else, or to a
    /// frame its drawer does not have, or two keyframes of a rack would be at the same frame), none does. A rack that is left
    /// with no keyframe is taken away, and so is a keyframe crate that is left with no rack.
    pub fn move_keyframes(&mut self, moves: &[KeyframeMove]) -> Result<(), EditError> {
        let racks = self.plan_the_move_of_keyframes(moves)?;
        self.put_back_the_racks(racks);
        Ok(())
    }

    /// The racks that are changed by `moves`, as they are once they are, each with the clip or track that has the drawer it is in;
    /// or why the keyframes cannot all be moved.
    fn plan_the_move_of_keyframes(&self, moves: &[KeyframeMove]) -> Result<Vec<(AnimationTarget, Rack)>, EditError> {
        let mut racks: BTreeMap<RackId, (AnimationTarget, Rack)> = BTreeMap::new();
        let mut to_put_in = Vec::with_capacity(moves.len());
        // (All the keyframes are taken out of their racks before any is put in one, so that a keyframe can go to the frame
        // that another leaves.)
        for requested in moves {
            let located = self
                .locate_keyframe(requested.keyframe)
                .ok_or(RackEditError::NoSuchKeyframe)?;
            let (_, rack) = racks
                .entry(located.rack.rack.id())
                .or_insert_with(|| (located.rack.owner, located.rack.rack.clone()));
            let keyframe = rack.remove(requested.keyframe).ok_or(RackEditError::NoSuchKeyframe)?;
            let of = (located.rack.rack.target(), located.rack.rack.property());
            to_put_in.push((keyframe, *requested, of));
        }
        for (mut keyframe, requested, of) in to_put_in {
            let destination = self.located_rack(requested.to_rack).ok_or(EditError::UnknownRack {
                rack: requested.to_rack,
            })?;
            if (destination.rack.target(), destination.rack.property()) != of {
                return Err(EditError::KeyframeCannotGoToThatRack {
                    keyframe: requested.keyframe,
                    rack: requested.to_rack,
                });
            }
            if destination.length.is_some_and(|length| requested.to_frame >= length) {
                return Err(EditError::NoRoomForTheKeyframes);
            }
            keyframe.frame = requested.to_frame;
            let (_, rack) = racks
                .entry(requested.to_rack)
                .or_insert_with(|| (destination.owner, destination.rack.clone()));
            rack.put_in(keyframe)?;
        }
        Ok(racks.into_values().collect())
    }

    /// Puts racks that were changed (each with the clip or track whose drawer it is in) in place of those that they were made
    /// from. A rack that has no keyframe is taken away instead, and a keyframe crate that is left with no rack is taken
    /// away with them.
    pub(super) fn put_back_the_racks(&mut self, racks: Vec<(AnimationTarget, Rack)>) {
        let mut owners = Vec::new();
        for (owner, rack) in racks {
            if let Some(drawer) = self.drawer_mut(owner) {
                if let Some(slot) = drawer.rack_mut(rack.id()) {
                    *slot = rack;
                }
                drawer.remove_the_racks_with_no_keyframes();
            }
            if !owners.contains(&owner) {
                owners.push(owner);
            }
        }
        for owner in owners {
            self.take_away_the_crate_if_it_has_no_rack(owner);
        }
    }

    /// How far later (earlier, if negative) than where they are the keyframes `moving` can be moved all together, in frames,
    /// when `wanted` is how far they are wanted: `wanted` itself, or as much of it as they can. Each keeps to the frames the
    /// drawer of the rack it goes to has, and stops short of a keyframe of that rack that is not moving (it does not pass it
    /// nor come to its frame); keyframes that move together are never in one another's way. (A keyframe that is at the very
    /// frame the one that moves is at, which it can only be in a rack that the keyframe is going to and is not in, is in nobody's
    /// way: the one that moves can go either way from it, and where it can be put at is for the caller to have checked. The
    /// names of keyframes that are not there, and of racks that are not, are left out.)
    pub fn allowed_shift_of_keyframes(&self, moving: &[KeyframeDestination], wanted: i64) -> i64 {
        let moving_set: HashSet<KeyframeId> = moving.iter().map(|destination| destination.keyframe).collect();
        let mut furthest_earlier = i64::MIN;
        let mut furthest_later = i64::MAX;
        for destination in moving {
            let (Some(keyframe), Some(rack)) = (
                self.locate_keyframe(destination.keyframe),
                self.located_rack(destination.rack),
            ) else {
                continue;
            };
            let at = keyframe.timeline_frame() as i64;
            furthest_earlier = furthest_earlier.max(rack.start as i64 - at);
            if let Some(length) = rack.length {
                furthest_later = furthest_later.min((rack.start + length) as i64 - 1 - at);
            }
            for other in rack.rack.keyframes() {
                if moving_set.contains(&other.id()) {
                    continue;
                }
                let there = rack.timeline_frame_of(other.frame) as i64;
                match there.cmp(&at) {
                    std::cmp::Ordering::Greater => furthest_later = furthest_later.min(there - at - 1),
                    std::cmp::Ordering::Less => furthest_earlier = furthest_earlier.max(there - at + 1),
                    std::cmp::Ordering::Equal => {}
                }
            }
        }
        // (A keyframe is where it is, so no limit is on the wrong side of nothing: the range holds 0.)
        wanted.clamp(furthest_earlier.min(0), furthest_later.max(0))
    }

    /// The moves that put each of the keyframes `moving` in the rack that goes with it, `shift` frames later (earlier, if
    /// negative) than it is. (The names of keyframes that are not there are left out. Use
    /// [`Self::allowed_shift_of_keyframes`] to find a shift that they can all be moved by.)
    pub fn moves_of_keyframes_by(&self, moving: &[KeyframeDestination], shift: i64) -> Vec<KeyframeMove> {
        moving
            .iter()
            .filter_map(|destination| {
                let keyframe = self.locate_keyframe(destination.keyframe)?;
                let rack = self.located_rack(destination.rack)?;
                let to_timeline_frame = keyframe.timeline_frame().checked_add_signed(shift)?;
                Some(KeyframeMove {
                    keyframe: destination.keyframe,
                    to_rack: destination.rack,
                    to_frame: to_timeline_frame.checked_sub(rack.start)?,
                })
            })
            .collect()
    }

    /// The frame of the timeline that the keyframe `keyframe` is at, if it is there.
    pub fn timeline_frame_of_keyframe(&self, keyframe: KeyframeId) -> Option<u64> {
        self.locate_keyframe(keyframe).map(|located| located.timeline_frame())
    }

    /// Takes away the keyframes (those that are there: a name of one that is not is left alone). A rack that has none left is
    /// taken away with them, and so is a keyframe crate that has no rack left in it. A property that has no keyframe at all
    /// any more, in any rack, is not animated any more, and stays what its last keyframe in time said.
    pub fn remove_keyframes(&mut self, keyframes: &[KeyframeId]) -> Result<(), EditError> {
        // What each property that loses a keyframe ends as, by the keyframes it has now: if it loses them all, it stays that.
        let mut what_they_end_as: Vec<(AnimationTarget, PropertyId, PropertyValue)> = Vec::new();
        let mut drawers_that_lost_a_rack: Vec<AnimationTarget> = Vec::new();
        for id in keyframes {
            let Some(located) = self.locate_keyframe(*id) else {
                continue;
            };
            let (owner, rack_id) = (located.rack.owner, located.rack.rack.id());
            let (target, property) = (located.rack.rack.target(), located.rack.rack.property());
            let is_new_to_the_list = !what_they_end_as
                .iter()
                .any(|(listed_target, listed_property, _)| *listed_target == target && *listed_property == property);
            if is_new_to_the_list && let Some(last) = self.keyframes_of(target, property).iter().last() {
                what_they_end_as.push((target, property, last.value.clone()));
            }
            if let Some(drawer) = self.drawer_mut(owner) {
                if let Some(rack) = drawer.rack_mut(rack_id) {
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

    /// Takes away the racks (those that are there) with all the keyframes in them: see [`Self::remove_keyframes`].
    pub fn remove_racks(&mut self, racks: &[RackId]) -> Result<(), EditError> {
        let keyframes: Vec<KeyframeId> = racks
            .iter()
            .filter_map(|rack| self.located_rack(*rack))
            .flat_map(|located| located.rack.keyframes().iter().map(RackKeyframe::id))
            .collect();
        self.remove_keyframes(&keyframes)
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
    /// them where they are in time, whatever drawer they are in.)
    ///
    /// `slot` says where it goes among the racks of the drawer it goes to: it has that many racks above it (which does not
    /// count the rack itself, if it is in that drawer already). Without one it keeps its place if it is in that drawer
    /// already, and goes under the others if it is not. (Nothing is said about where in a new crate: it is alone there.)
    ///
    /// A rack always lasts as long as the drawer it is in, so it can only go where all of its keyframes (the first and the
    /// last, and so those between) are at frames the drawer has: nothing is changed, and the error says so
    /// ([`EditError::NoRoomForTheKeyframes`]), if they are not. It can go to a drawer that has a rack of the same property of
    /// the same thing: they are not merged (see [`Self::merge_racks`]). A keyframe crate that is left with no rack by a rack
    /// that goes is taken away.
    pub fn move_rack(
        &mut self,
        rack: RackId,
        to: RackDestination,
        shift_in_frames: i64,
        slot: Option<usize>,
    ) -> Result<(), EditError> {
        let plan = self.plan_the_move_of_a_rack(rack, to, shift_in_frames)?;
        let RackMove { rack, from, to, moved } = plan;
        match to {
            RackDestination::Drawer(owner) if owner == from => {
                if let Some(drawer) = self.drawer_mut(owner) {
                    let place = slot.or_else(|| drawer.position_of(rack));
                    drawer.take(rack);
                    drawer.insert(place.unwrap_or(drawer.len()), moved);
                }
            }
            RackDestination::Drawer(owner) => {
                if let Some(drawer) = self.drawer_mut(from) {
                    drawer.take(rack);
                }
                if let Some(drawer) = self.drawer_mut(owner) {
                    drawer.insert(slot.unwrap_or(drawer.len()), moved);
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
    pub(super) fn take_away_the_crate_if_it_has_no_rack(&mut self, owner: AnimationTarget) {
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
