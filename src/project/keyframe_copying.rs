//! Copying keyframes: a keyframe, or a group of them, is copied to the frames after it or the frames before it, in the rack it is
//! in, so that what was made once can be made again at another time.
//!
//! A group is copied as a whole: the copies are as far from one another as the keyframes they are copies of, and they are all
//! as far from the keyframes that are copied as the first and the last of those are from each other (and then a gap), so that
//! the copies of a group come where the group would be if it were put next to itself.

use std::collections::BTreeMap;

use super::animating::AnimationTarget;
use super::tracks::{KeyframeId, Rack, RackEditError, RackId};
use super::{EditError, Project};

/// Which way along the timeline: towards its start, or towards its end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeDirection {
    Earlier,
    Later,
}

/// Copies of keyframes that were found to be possible to make: everything that is needed to make them.
struct Copying {
    /// The racks that get copies, as they are once they have them, each with the clip or track that has its drawer.
    racks: Vec<(AnimationTarget, Rack)>,
    /// The names of the copies, in the order of the keyframes they are copies of.
    copies: Vec<KeyframeId>,
}

impl Project {
    /// Whether [`Self::duplicate_keyframes`] would make the copies, and if not, why not.
    pub fn check_the_duplicating_of_keyframes(
        &self,
        keyframes: &[KeyframeId],
        towards: TimeDirection,
        gap_in_frames: u64,
    ) -> Result<(), EditError> {
        self.plan_the_duplicating_of_keyframes(keyframes, towards, gap_in_frames)
            .map(|_| ())
    }

    /// Makes a copy of each of the keyframes in the rack it is in: the copy has the same value, and is as far from where the
    /// keyframe is, along the timeline, as the first of the keyframes is from the last, and then `gap_in_frames` more (at least
    /// one: the copies do not come to the frames of the keyframes that are copied), later or earlier, by `towards`. Each copy is a
    /// new keyframe, with a name of its own: the names are given, in the order of the keyframes that are copied.
    ///
    /// Either all the copies are made or, if one cannot be (a keyframe is not there, or the copy would be at a frame that the
    /// drawer of its rack does not have, or at the frame of a keyframe the rack has), none is, and the error says why.
    pub fn duplicate_keyframes(
        &mut self,
        keyframes: &[KeyframeId],
        towards: TimeDirection,
        gap_in_frames: u64,
    ) -> Result<Vec<KeyframeId>, EditError> {
        let Copying { racks, copies } = self.plan_the_duplicating_of_keyframes(keyframes, towards, gap_in_frames)?;
        self.put_back_the_racks(racks);
        Ok(copies)
    }

    fn plan_the_duplicating_of_keyframes(
        &self,
        keyframes: &[KeyframeId],
        towards: TimeDirection,
        gap_in_frames: u64,
    ) -> Result<Copying, EditError> {
        let mut wanted: Vec<KeyframeId> = Vec::with_capacity(keyframes.len());
        for keyframe in keyframes {
            if !wanted.contains(keyframe) {
                wanted.push(*keyframe);
            }
        }
        let located = wanted
            .iter()
            .map(|keyframe| {
                self.locate_keyframe(*keyframe)
                    .ok_or(EditError::Rack(RackEditError::NoSuchKeyframe))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (Some(first), Some(last)) = (
            located.iter().map(|located| located.timeline_frame()).min(),
            located.iter().map(|located| located.timeline_frame()).max(),
        ) else {
            return Ok(Copying {
                racks: Vec::new(),
                copies: Vec::new(),
            });
        };
        let distance = last - first + gap_in_frames.max(1);

        let mut racks: BTreeMap<RackId, (AnimationTarget, Rack)> = BTreeMap::new();
        let mut copies = Vec::with_capacity(located.len());
        for original in &located {
            let copy_is_at = match towards {
                TimeDirection::Later => original.timeline_frame().checked_add(distance),
                TimeDirection::Earlier => original.timeline_frame().checked_sub(distance),
            };
            let frame_in_the_rack = copy_is_at
                .and_then(|frame| frame.checked_sub(original.rack.start))
                .filter(|frame| original.rack.length.is_none_or(|length| *frame < length))
                .ok_or(EditError::NoRoomForTheKeyframes)?;
            let (_, rack) = racks
                .entry(original.rack.rack.id())
                .or_insert_with(|| (original.rack.owner, original.rack.rack.clone()));
            copies.push(rack.add_a_copy_of(original.keyframe.id(), frame_in_the_rack)?);
        }
        Ok(Copying {
            racks: racks.into_values().collect(),
            copies,
        })
    }
}
