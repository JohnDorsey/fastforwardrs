//! Merging racks: the keyframes of several racks of one property of one thing become the keyframes of one rack.
//!
//! Racks of one property of one thing are not merged by being put in the same drawer (see [`super::tracks::Drawer`]): each keeps
//! its own keyframes and they say the property together (see [`super::racks`]). Merging is asked for, and does not change what
//! the property is at any frame: it only changes which rack holds the keyframes.

use std::collections::BTreeMap;

use super::animating::AnimationTarget;
use super::racks::LocatedRack;
use super::tracks::{Rack, RackId, RackKeyframe};
use super::{EditError, Project};

/// A merging of racks that was found to be possible: the racks as they are once it is done.
struct Merging {
    /// The rack that has all the keyframes. (It is the one of the racks that was in the drawer that holds them all.)
    survivor: RackId,
    /// The racks that change, with the clip or track whose drawer each is in: the survivor with all the keyframes, and the others
    /// with none (which is how they are taken away).
    racks: Vec<(AnimationTarget, Rack)>,
}

impl Project {
    /// Whether [`Self::merge_racks`] would merge the racks, and if not, why not.
    pub fn check_the_merging_of_racks(&self, racks: &[RackId]) -> Result<(), EditError> {
        self.plan_the_merging_of_racks(racks).map(|_| ())
    }

    /// Makes one rack of the racks `racks`, which have the keyframes of all of them. It is the rack of them that is in the drawer
    /// that is first, of those that have room for every keyframe at the frame of the timeline it is at (so that no keyframe
    /// changes frame, and the property is what it was at every frame): it keeps its place in its drawer and its name, and the
    /// others are taken away, with a keyframe crate that is left with no rack. Every keyframe keeps its name. Where two keyframes
    /// are at one frame, the one that counted is kept (that of the rack that comes later: see [`Self::located_racks`]), so that
    /// what the property is does not change. Gives the name of the rack that was kept.
    ///
    /// Nothing is changed, and the error says why, if there are fewer than two racks, or any is not there, or they are not all of
    /// the same property of the same thing, or no drawer one of them is in has room for all the keyframes.
    pub fn merge_racks(&mut self, racks: &[RackId]) -> Result<RackId, EditError> {
        let Merging { survivor, racks } = self.plan_the_merging_of_racks(racks)?;
        self.put_back_the_racks(racks);
        Ok(survivor)
    }

    fn plan_the_merging_of_racks(&self, racks: &[RackId]) -> Result<Merging, EditError> {
        let mut wanted: Vec<RackId> = Vec::with_capacity(racks.len());
        for rack in racks {
            self.located_rack(*rack).ok_or(EditError::UnknownRack { rack: *rack })?;
            if !wanted.contains(rack) {
                wanted.push(*rack);
            }
        }
        if wanted.len() < 2 {
            return Err(EditError::TooFewRacksToMerge);
        }
        // (In the order that says which of two keyframes at one frame counts: the later is the one that does.)
        let located: Vec<LocatedRack<'_>> = self
            .located_racks()
            .filter(|located| wanted.contains(&located.rack.id()))
            .collect();
        let of_the_same_thing = |first: &LocatedRack<'_>, other: &LocatedRack<'_>| {
            first.rack.target() == other.rack.target() && first.rack.property() == other.rack.property()
        };
        if !located.iter().all(|other| of_the_same_thing(&located[0], other)) {
            return Err(EditError::RacksOfDifferentThingsCannotBeMerged);
        }
        let first_frame = located
            .iter()
            .filter_map(|located| located.rack.first_frame().map(|frame| located.timeline_frame_of(frame)))
            .min();
        let last_frame = located
            .iter()
            .filter_map(|located| located.rack.last_frame().map(|frame| located.timeline_frame_of(frame)))
            .max();
        let (Some(first_frame), Some(last_frame)) = (first_frame, last_frame) else {
            return Err(EditError::NoRoomForTheKeyframes);
        };
        let survivor = located
            .iter()
            .find(|candidate| {
                candidate.start <= first_frame
                    && candidate
                        .length
                        .is_none_or(|length| last_frame < candidate.start + length)
            })
            .ok_or(EditError::NoRoomForTheKeyframes)?;

        let mut by_frame: BTreeMap<u64, RackKeyframe> = BTreeMap::new();
        for rack in &located {
            for keyframe in rack.rack.keyframes() {
                let mut kept = keyframe.clone();
                kept.frame = rack.timeline_frame_of(keyframe.frame) - survivor.start;
                // (A later rack's keyframe at a frame is put over an earlier one's.)
                by_frame.insert(kept.frame, kept);
            }
        }
        let mut changed = vec![(
            survivor.owner,
            survivor.rack.with_keyframes(by_frame.into_values().collect()),
        )];
        changed.extend(
            located
                .iter()
                .filter(|other| other.rack.id() != survivor.rack.id())
                .map(|other| (other.owner, other.rack.emptied())),
        );
        Ok(Merging {
            survivor: survivor.rack.id(),
            racks: changed,
        })
    }
}
