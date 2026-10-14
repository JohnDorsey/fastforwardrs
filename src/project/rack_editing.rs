//! Editing racks and the keyframes in them: moving keyframes in time and from one rack to another, taking them away, moving
//! racks to another drawer (or to another place among the racks of the drawer they are in) or onto a track, and making keyframe
//! crates (see the `crates` module, which keeps each crate as long as its keyframes need).
//!
//! A keyframe never changes what it animates, and neither does a rack: what moves is the keyframes in time and from a rack to
//! another rack of the same property of the same thing, and the racks from one drawer to another. A rack keeps its name (see
//! [`RackId`]) wherever it goes, and so does a keyframe (see [`KeyframeId`]).

use std::collections::{BTreeMap, HashSet};

use super::animating::AnimationTarget;
use super::racks::{DrawerExtent, FrameRoom};
use super::tracks::{ClipEditError, KeyframeId, Rack, RackEditError, RackId, RackKeyframe, TrackId};
use super::{EditError, Project, PropertyId, PropertyValue, TimelineFrameIndex};

/// A keyframe to move, and where to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyframeMove {
    pub keyframe: KeyframeId,
    /// The rack the keyframe goes to: the one it is in to leave it there, or another that animates the same property of the
    /// same thing.
    pub to_rack: RackId,
    /// Counted from the start of the drawer that `to_rack` is in, which is the first frame of the clip that has it (or, for a
    /// keyframe crate, of the crate, which is made to begin earlier if the frame is before it, and to end later if it is after, where
    /// there is room for that). Only a frame of a crate can be before the start of the drawer.
    pub to_frame: i64,
}

/// A keyframe, and the rack it is to be moved to, wherever in time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyframeDestination {
    pub keyframe: KeyframeId,
    pub rack: RackId,
}

/// Where [`Project::move_racks`] is to put racks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RackDestination {
    /// In the drawer of a clip (a keyframe crate is one) or of a track. (In a keyframe crate they are put where they are, and the
    /// crate is made as long as it takes to hold them, as far as there is room.)
    Drawer(AnimationTarget),
    /// In a new keyframe crate on `track`, which lasts from the first keyframe of the racks to the last, and is somewhere in the
    /// empty stretch of the track that goes from the frame `from_frame` up to (not including) the frame `until_frame`.
    NewCrate {
        track: TrackId,
        from_frame: u64,
        until_frame: u64,
    },
}

/// A move of racks that was found to be possible: everything that is needed to make it.
struct RackMoves {
    /// The racks, in the order they were asked for: each as it is once it has gone, with its keyframes at the frames of the timeline
    /// they go to, and the clip or track whose drawer it was in.
    racks: Vec<(AnimationTarget, Rack)>,
    /// The first frame of the timeline that a keyframe of any of them goes to, and the last.
    first_frame: u64,
    last_frame: u64,
}

impl Project {
    /// Where the drawer of `owner` is on the timeline, if there is such a clip or track.
    pub fn extent_of_the_drawer_of(&self, owner: AnimationTarget) -> Option<DrawerExtent> {
        match owner {
            AnimationTarget::Clip(clip) => {
                let placement = self.placement_of(clip)?;
                let room = if self.is_a_crate(owner) {
                    self.room_of_the_crate(clip)?
                } else {
                    FrameRoom {
                        first_frame: placement.first_timeline_frame.value(),
                        end_frame: Some(placement.end_timeline_frame.value()),
                    }
                };
                Some(DrawerExtent {
                    start: placement.first_timeline_frame.value(),
                    room,
                })
            }
            AnimationTarget::Track(track) => {
                self.track(track)?;
                Some(DrawerExtent {
                    start: 0,
                    room: FrameRoom {
                        first_frame: 0,
                        end_frame: None,
                    },
                })
            }
        }
    }

    /// Whether [`Self::move_keyframes`] would move the keyframes, and if not, why not.
    pub fn check_the_move_of_keyframes(&self, moves: &[KeyframeMove]) -> Result<(), EditError> {
        self.would_be_allowed(|project| project.move_keyframes_unchecked(moves))
    }

    /// Moves keyframes, each to the rack and the frame given for it (counted from the start of the drawer that rack is in). A
    /// keyframe goes from a rack to another that animates the same property of the same thing, which keeps its name. Either
    /// all of them move or, if one cannot (it is not there, or would go to a rack that animates something else, or to a
    /// frame its drawer does not have room for, or two keyframes of a rack would be at the same frame), none does. A rack that is left
    /// with no keyframe is taken away, and so is a keyframe crate that is left with no rack; a keyframe crate is made shorter or longer
    /// as the keyframes in it need (see the `crates` module).
    pub fn move_keyframes(&mut self, moves: &[KeyframeMove]) -> Result<(), EditError> {
        self.keeping_the_groups_valid(|project| project.move_keyframes_unchecked(moves))
    }

    fn move_keyframes_unchecked(&mut self, moves: &[KeyframeMove]) -> Result<(), EditError> {
        self.atomically(|project| {
            let to_frames_of_the_timeline = project.frames_of_the_timeline_that_keyframes_are_moved_to(moves)?;
            project.make_the_crates_begin_in_time_for(
                &to_frames_of_the_timeline
                    .iter()
                    .map(|(requested, frame)| (requested.to_rack, *frame))
                    .collect::<Vec<_>>(),
            )?;
            // (The drawers of the crates that were made to begin earlier are counted from a frame that is earlier now.)
            let moves = project.moves_to_frames_of_the_timeline(&to_frames_of_the_timeline)?;
            let racks = project.plan_the_move_of_keyframes(&moves)?;
            project.put_back_the_racks(racks)
        })
    }

    /// Where each of `moves` takes its keyframe to, as a frame of the timeline, which is that frame of the drawer of the rack it goes
    /// to as that drawer is now; or why that is not somewhere keyframes can go.
    fn frames_of_the_timeline_that_keyframes_are_moved_to(
        &self,
        moves: &[KeyframeMove],
    ) -> Result<Vec<(KeyframeMove, u64)>, EditError> {
        moves
            .iter()
            .map(|requested| {
                let destination = self.located_rack(requested.to_rack).ok_or(EditError::UnknownRack {
                    rack: requested.to_rack,
                })?;
                let frame = destination
                    .start
                    .checked_add_signed(requested.to_frame)
                    .filter(|frame| destination.room.contains(*frame))
                    .ok_or(EditError::NoRoomForTheKeyframes)?;
                Ok((*requested, frame))
            })
            .collect()
    }

    /// Makes the keyframe crates that keyframes are to be put in begin early enough to have the frames of the timeline they are put at
    /// (see [`Self::make_the_crate_begin_by`]): `to` has each rack that keyframes are put in, and the frame of the timeline one of them
    /// is put at. (They are made as long as they need at the end once the keyframes are in them, by [`Self::fit_the_crates`].)
    pub(super) fn make_the_crates_begin_in_time_for(&mut self, to: &[(RackId, u64)]) -> Result<(), EditError> {
        let mut first_frames_by_owner: Vec<(AnimationTarget, u64)> = Vec::new();
        for (rack, frame) in to {
            let owner = self
                .located_rack(*rack)
                .ok_or(EditError::UnknownRack { rack: *rack })?
                .owner;
            match first_frames_by_owner.iter_mut().find(|(other, _)| *other == owner) {
                Some((_, first_frame)) => *first_frame = (*first_frame).min(*frame),
                None => first_frames_by_owner.push((owner, *frame)),
            }
        }
        for (owner, first_frame) in first_frames_by_owner {
            self.make_the_crate_begin_by(owner, first_frame)?;
        }
        Ok(())
    }

    /// The moves that put each keyframe at its frame of the timeline in the rack it goes to, counted from where that rack's
    /// drawer begins now.
    fn moves_to_frames_of_the_timeline(&self, to: &[(KeyframeMove, u64)]) -> Result<Vec<KeyframeMove>, EditError> {
        to.iter()
            .map(|(requested, frame)| {
                let destination = self.located_rack(requested.to_rack).ok_or(EditError::UnknownRack {
                    rack: requested.to_rack,
                })?;
                let to_frame = frame
                    .checked_sub(destination.start)
                    .ok_or(EditError::NoRoomForTheKeyframes)?;
                Ok(KeyframeMove {
                    to_frame: i64::try_from(to_frame).map_err(|_| EditError::NoRoomForTheKeyframes)?,
                    ..*requested
                })
            })
            .collect()
    }

    /// The racks that are changed by `moves`, as they are once they are, each with the clip or track that has the drawer it is in;
    /// or why the keyframes cannot all be moved. (The frames are those of the drawers as they are: a keyframe crate has been made
    /// long enough for them already.)
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
            keyframe.frame = u64::try_from(requested.to_frame).map_err(|_| EditError::NoRoomForTheKeyframes)?;
            let (_, rack) = racks
                .entry(requested.to_rack)
                .or_insert_with(|| (destination.owner, destination.rack.clone()));
            rack.put_in(keyframe)?;
        }
        Ok(racks.into_values().collect())
    }

    /// Puts racks that were changed (each with the clip or track whose drawer it is in) in place of those that they were made
    /// from. A rack that has no keyframe is taken away instead, and a keyframe crate is made as long as the keyframes in it need
    /// (and taken away with them if it has none left).
    pub(super) fn put_back_the_racks(&mut self, racks: Vec<(AnimationTarget, Rack)>) -> Result<(), EditError> {
        let mut owners = Vec::new();
        for (owner, rack) in racks {
            if let Some(drawer) = self.drawer_mut(owner) {
                if let Some(slot) = drawer.rack_mut(rack.id()) {
                    *slot = rack;
                }
                drawer.remove_the_racks_with_no_keyframes();
            }
            owners.push(owner);
        }
        self.fit_the_crates(&owners)
    }

    /// How far later (earlier, if negative) than where they are the keyframes `moving` can be moved all together, in frames,
    /// when `wanted` is how far they are wanted: `wanted` itself, or as much of it as they can. Each keeps to the frames the
    /// drawer of the rack it goes to has room for (a keyframe crate has room for the empty frames round it too), and stops short of a
    /// keyframe of that rack that is not moving (it does not pass it nor come to its frame); keyframes that move together are never
    /// in one another's way. (A keyframe that is at the very frame the one that moves is at, which it can only be in a rack that
    /// the keyframe is going to and is not in, is in nobody's way: the one that moves can go either way from it, and where it can be
    /// put at is for the caller to have checked. The names of keyframes that are not there, and of racks that are not, are left out.)
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
            furthest_earlier = furthest_earlier.max(rack.room.first_frame as i64 - at);
            if let Some(end) = rack.room.end_frame {
                furthest_later = furthest_later.min(end as i64 - 1 - at);
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
                    to_frame: i64::try_from(to_timeline_frame).ok()? - i64::try_from(rack.start).ok()?,
                })
            })
            .collect()
    }

    /// The frame of the timeline that the keyframe `keyframe` is at, if it is there.
    pub fn timeline_frame_of_keyframe(&self, keyframe: KeyframeId) -> Option<u64> {
        self.locate_keyframe(keyframe).map(|located| located.timeline_frame())
    }

    /// Takes away the keyframes (those that are there: a name of one that is not is left alone). A rack that has none left is
    /// taken away with them, and so is a keyframe crate that has no rack left in it; a keyframe crate that still has some is made as
    /// short as the keyframes in it allow. A property that has no keyframe at all
    /// any more, in any rack, is not animated any more, and stays what its last keyframe in time said.
    pub fn remove_keyframes(&mut self, keyframes: &[KeyframeId]) -> Result<(), EditError> {
        self.keeping_the_groups_valid(|project| project.remove_keyframes_unchecked(keyframes))
    }

    fn remove_keyframes_unchecked(&mut self, keyframes: &[KeyframeId]) -> Result<(), EditError> {
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
            drawers_that_lost_a_rack.push(owner);
        }
        for (target, property, value) in what_they_end_as {
            if !self.is_animated(target, property) {
                self.set_static_value(target, property, &value)?;
            }
        }
        self.fit_the_crates(&drawers_that_lost_a_rack)
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

    /// Whether [`Self::move_racks`] would move the racks, and if not, why not.
    pub fn check_the_move_of_racks(
        &self,
        racks: &[RackId],
        to: RackDestination,
        shift_in_frames: i64,
    ) -> Result<(), EditError> {
        self.would_be_allowed(|project| project.move_racks_unchecked(racks, to, shift_in_frames, None))
    }

    /// Moves the racks `racks` to another drawer, or to a new keyframe crate (one for all of them), shifting all of their keyframes
    /// `shift_in_frames` frames later (earlier if that is less than 0) than the frames of the timeline they are at now, so that they
    /// stay where they are in relation to one another. (A shift of 0 leaves them where they are in time, whatever drawer they are in.)
    ///
    /// `slot` says where they go among the racks of the drawer they go to, together, in the order they are given in: the first has
    /// that many racks above it (which does not count the racks that are moved, if they are in that drawer already). Without one
    /// each that is in that drawer already keeps its place, and the others go under the racks that are there. (Nothing is said about
    /// where in a new crate: they are the only ones there.)
    ///
    /// A rack always lasts as long as the drawer it is in, so they can only go where all of their keyframes (the first of them all and
    /// the last, and so those between) are at frames the drawer has room for: nothing is changed, and the error says so
    /// ([`EditError::NoRoomForTheKeyframes`]), if they are not. A keyframe crate has room for the empty frames round it too, and is
    /// made as long as they need; a new keyframe crate is as long as they need, and is where they are. They can go to a drawer
    /// that has a rack of the same property of the same thing: they are not merged (see [`Self::merge_racks`]). A keyframe crate that
    /// is left with no rack by racks that go is taken away, and one that is left with some is made as short as they need.
    pub fn move_racks(
        &mut self,
        racks: &[RackId],
        to: RackDestination,
        shift_in_frames: i64,
        slot: Option<usize>,
    ) -> Result<(), EditError> {
        self.keeping_the_groups_valid(|project| project.move_racks_unchecked(racks, to, shift_in_frames, slot))
    }

    fn move_racks_unchecked(
        &mut self,
        racks: &[RackId],
        to: RackDestination,
        shift_in_frames: i64,
        slot: Option<usize>,
    ) -> Result<(), EditError> {
        self.atomically(|project| {
            let moves = project.plan_the_move_of_racks(racks, to, shift_in_frames)?;
            let destination = match to {
                RackDestination::Drawer(owner) => {
                    project.make_the_crate_begin_by(owner, moves.first_frame)?;
                    owner
                }
                RackDestination::NewCrate { track, .. } => {
                    // (The crate is made before the racks leave where they are, so that if it could not be made, which the plan
                    // says it can, they would not be lost.)
                    let crate_clip = project.insert_crate(
                        track,
                        TimelineFrameIndex::new(moves.first_frame),
                        moves.last_frame - moves.first_frame + 1,
                    )?;
                    AnimationTarget::Clip(crate_clip)
                }
            };
            let origin = project
                .extent_of_the_drawer_of(destination)
                .map(|extent| extent.start)
                .ok_or(EditError::NoRoomForTheKeyframes)?;
            // Where each rack is in the drawer it goes to, if it is in it already (which is where it goes back to without a slot).
            let places_before: Vec<Option<usize>> = moves
                .racks
                .iter()
                .map(|(_, moved)| {
                    project
                        .drawer_of(destination)
                        .and_then(|drawer| drawer.position_of(moved.id()))
                })
                .collect();
            let mut sources = Vec::new();
            for (from, moved) in &moves.racks {
                if let Some(drawer) = project.drawer_mut(*from) {
                    drawer.take(moved.id());
                }
                sources.push(*from);
            }
            let arriving: Vec<(Option<usize>, Rack)> = moves
                .racks
                .iter()
                .zip(places_before)
                .map(|((_, moved), place)| (place, moved.with_frames_mapped(|frame| frame - origin)))
                .collect();
            if let Some(drawer) = project.drawer_mut(destination) {
                match slot {
                    Some(slot) => {
                        for (index, (_, rack)) in arriving.into_iter().enumerate() {
                            drawer.insert(slot + index, rack);
                        }
                    }
                    None => {
                        // (Those that were in it go back to their places, the first place first, and the others go under all.)
                        let (mut back_in_place, new): (Vec<_>, Vec<_>) =
                            arriving.into_iter().partition(|(place, _)| place.is_some());
                        back_in_place.sort_by_key(|(place, _)| *place);
                        for (place, rack) in back_in_place {
                            drawer.insert(place.unwrap_or(0), rack);
                        }
                        for (_, rack) in new {
                            drawer.push(rack);
                        }
                    }
                }
            }
            let mut to_fit = sources;
            to_fit.push(destination);
            project.fit_the_crates(&to_fit)
        })
    }

    /// The frames of the timeline that the keyframes of racks can be put at in the drawer that they go to when they are moved to `to`:
    /// those that the drawer has room for, or those of the empty stretch of the track a new crate is made in.
    fn room_of_the_destination(&self, to: RackDestination) -> Result<FrameRoom, EditError> {
        match to {
            RackDestination::Drawer(owner) => {
                self.extent_of_the_drawer_of(owner)
                    .map(|extent| extent.room)
                    .ok_or(match owner {
                        AnimationTarget::Clip(clip) => EditError::Clip(ClipEditError::UnknownClip { clip }),
                        AnimationTarget::Track(track) => EditError::UnknownTrack { track },
                    })
            }
            RackDestination::NewCrate {
                track,
                from_frame,
                until_frame,
            } => {
                self.track(track).ok_or(EditError::UnknownTrack { track })?;
                Ok(FrameRoom {
                    first_frame: from_frame,
                    end_frame: Some(until_frame),
                })
            }
        }
    }

    /// How many frames later (earlier, if negative) than where they are the keyframes of the racks `racks` can be put when they are
    /// moved to `to`, if it is wanted that they are `wanted_shift` frames later: that, or as near to it as keeps all of them at
    /// frames the drawer they go to has room for. (It is where racks are dropped that decides where they go in the drawer, as
    /// far as the drawer has room: they are not refused for being a little too far along.) Says why not if they cannot go there:
    /// [`EditError::NoRoomForTheKeyframes`] if the first of all of their keyframes and the last are further from one another than
    /// the room is long, and any reason that [`Self::check_the_move_of_racks`] would give.
    pub fn shift_that_fits(&self, racks: &[RackId], to: RackDestination, wanted_shift: i64) -> Result<i64, EditError> {
        let located: Vec<_> = racks
            .iter()
            .map(|rack| self.located_rack(*rack).ok_or(EditError::UnknownRack { rack: *rack }))
            .collect::<Result<_, _>>()?;
        let room = self.room_of_the_destination(to)?;
        let first = located
            .iter()
            .filter_map(|located| located.rack.first_frame().map(|frame| located.timeline_frame_of(frame)))
            .min();
        let last = located
            .iter()
            .filter_map(|located| located.rack.last_frame().map(|frame| located.timeline_frame_of(frame)))
            .max();
        let (Some(first), Some(last)) = (first, last) else {
            return Err(EditError::NoRoomForTheKeyframes);
        };
        let (first, last) = (first as i64, last as i64);
        let (start, span) = (room.first_frame as i64, last - first);
        let (earliest, latest) = match room.end_frame {
            Some(end) if span >= end as i64 - start => return Err(EditError::NoRoomForTheKeyframes),
            Some(end) => (start - first, end as i64 - 1 - last),
            None => (start - first, i64::MAX),
        };
        let shift = wanted_shift.clamp(earliest, latest);
        self.check_the_move_of_racks(racks, to, shift)?;
        Ok(shift)
    }

    /// Works out whether racks can be moved as [`Self::move_racks`] says, and if they can, what they will be like once they have.
    fn plan_the_move_of_racks(
        &self,
        racks: &[RackId],
        to: RackDestination,
        shift_in_frames: i64,
    ) -> Result<RackMoves, EditError> {
        let mut distinct: Vec<RackId> = Vec::with_capacity(racks.len());
        for rack in racks {
            if !distinct.contains(rack) {
                distinct.push(*rack);
            }
        }
        let room = self.room_of_the_destination(to)?;
        let mut moved_racks = Vec::with_capacity(distinct.len());
        let mut first_frame = u64::MAX;
        let mut last_frame = 0;
        for rack_id in distinct {
            let located = self
                .located_rack(rack_id)
                .ok_or(EditError::UnknownRack { rack: rack_id })?;
            let (Some(first), Some(last)) = (located.rack.first_frame(), located.rack.last_frame()) else {
                return Err(EditError::NoRoomForTheKeyframes);
            };
            // (The shift is the same for every keyframe, so if the first and the last are in the room, so are all of them.)
            let shifted = |frame: u64| (located.start + frame).checked_add_signed(shift_in_frames);
            let (Some(shifted_first), Some(shifted_last)) = (shifted(first), shifted(last)) else {
                return Err(EditError::NoRoomForTheKeyframes);
            };
            first_frame = first_frame.min(shifted_first);
            last_frame = last_frame.max(shifted_last);
            moved_racks.push((
                located.owner,
                located.rack.with_frames_mapped(|frame| shifted(frame).unwrap_or(frame)),
            ));
        }
        if moved_racks.is_empty() || !room.contains(first_frame) || !room.contains(last_frame) {
            return Err(EditError::NoRoomForTheKeyframes);
        }
        if let RackDestination::NewCrate { track, .. } = to {
            // (All the frames of the crate that is made have to be empty, which they are if they are in the stretch that was asked for,
            // and that is the stretch they are asked of.)
            self.track(track)
                .ok_or(EditError::UnknownTrack { track })?
                .start_of_the_free_stretch_that_has_room_for(
                    TimelineFrameIndex::new(first_frame),
                    last_frame - first_frame + 1,
                )?;
        }
        Ok(RackMoves {
            racks: moved_racks,
            first_frame,
            last_frame,
        })
    }
}
