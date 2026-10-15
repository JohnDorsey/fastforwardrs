// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A rack: the keyframes of one property of one clip or track, which are kept together so that they can be moved as a group.
//!
//! A rack is in a drawer (see [`super::drawer::Drawer`]) of a clip or of a track, or in a keyframe crate, and what it
//! animates need not be the thing that has the drawer: the keyframes that lower the sound of a big video can be kept in the
//! drawer of a small clip, and go wherever that clip goes. What a rack holds is only the keyframes: what they come to for the
//! property they are of, when there are the keyframes of other racks as well, is the project's to say.
//!
//! The frames of a rack are counted from the start of what holds it (a clip's first frame, or the start of the timeline for
//! a track's drawer), so that they go along with it.

use thiserror::Error;

use super::ids::{KeyframeId, RackId};
use super::keyframe_fitting::{WhichAreLost, frames_for_keyframes_wanted_at};
use crate::project::{AnimationTarget, Keyframe, Keyframes, PropertyId, PropertyValue};

/// A value at a frame, in a rack. It has a name of its own (see [`KeyframeId`]), which is how it is told from every other
/// keyframe whichever frame it is moved to and whichever rack it is moved to.
#[derive(Clone, Debug, PartialEq)]
pub struct RackKeyframe {
    id: KeyframeId,
    /// Counted from the start of what holds the rack.
    pub frame: u64,
    pub value: PropertyValue,
}

impl RackKeyframe {
    /// The name of the keyframe.
    pub fn id(&self) -> KeyframeId {
        self.id
    }
}

/// Why a change to a rack was not made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum RackEditError {
    #[error("the rack has no such keyframe")]
    NoSuchKeyframe,
    #[error("the rack has that keyframe already")]
    AlreadyHasThatKeyframe,
    #[error("two keyframes of the rack would be at the same frame")]
    FramesWouldCoincide,
    #[error("that is not the kind of value the property has")]
    NotTheKindOfValueThePropertyHas,
}

/// The keyframes of one property of one clip or track.
#[derive(Clone, Debug, PartialEq)]
pub struct Rack {
    id: RackId,
    target: AnimationTarget,
    property: PropertyId,
    /// In the order of their frames, with no two at the same frame.
    keyframes: Vec<RackKeyframe>,
    /// The number the next keyframe made in the rack is given (see [`KeyframeId`]).
    next_keyframe_number: u32,
}

impl Rack {
    /// A rack with no keyframes, for `property` of `target`.
    pub fn new(id: RackId, target: AnimationTarget, property: PropertyId) -> Self {
        Self {
            id,
            target,
            property,
            keyframes: Vec::new(),
            next_keyframe_number: 0,
        }
    }

    pub fn id(&self) -> RackId {
        self.id
    }

    /// The clip or track whose property the keyframes are of.
    pub fn target(&self) -> AnimationTarget {
        self.target
    }

    pub fn property(&self) -> PropertyId {
        self.property
    }

    /// The keyframes, the earliest first.
    pub fn keyframes(&self) -> &[RackKeyframe] {
        &self.keyframes
    }

    pub fn is_empty(&self) -> bool {
        self.keyframes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.keyframes.len()
    }

    /// The frame of the first keyframe.
    pub fn first_frame(&self) -> Option<u64> {
        self.keyframes.first().map(|keyframe| keyframe.frame)
    }

    /// The frame of the last keyframe.
    pub fn last_frame(&self) -> Option<u64> {
        self.keyframes.last().map(|keyframe| keyframe.frame)
    }

    fn position_of(&self, frame: u64) -> Result<usize, usize> {
        self.keyframes.binary_search_by_key(&frame, |keyframe| keyframe.frame)
    }

    /// The keyframe at `frame`, if there is one exactly there.
    pub fn at(&self, frame: u64) -> Option<&RackKeyframe> {
        self.position_of(frame).ok().map(|index| &self.keyframes[index])
    }

    /// The keyframe called `id`, if it is in this rack (whichever rack made it).
    pub fn keyframe(&self, id: KeyframeId) -> Option<&RackKeyframe> {
        self.keyframes.iter().find(|keyframe| keyframe.id == id)
    }

    /// What the keyframes say by themselves, as the keyframes of a property are worked out from: with no names.
    pub fn curve(&self) -> Keyframes {
        Keyframes::from_unordered(self.keyframes.iter().map(|keyframe| Keyframe {
            frame: keyframe.frame,
            value: keyframe.value.clone(),
        }))
    }

    /// Puts a keyframe with `value` at `frame`, in place of the one there if there is one (which keeps its name). Gives the
    /// name of the keyframe, or says why not (and changes nothing) if `value` is not the kind of value the property has.
    pub fn set(&mut self, frame: u64, value: PropertyValue) -> Result<KeyframeId, RackEditError> {
        if value.kind() != self.property.kind() {
            return Err(RackEditError::NotTheKindOfValueThePropertyHas);
        }
        match self.position_of(frame) {
            Ok(index) => {
                self.keyframes[index].value = value;
                Ok(self.keyframes[index].id)
            }
            Err(index) => {
                let id = KeyframeId::new(self.id, self.next_keyframe_number);
                self.next_keyframe_number += 1;
                self.keyframes.insert(index, RackKeyframe { id, frame, value });
                Ok(id)
            }
        }
    }

    /// Takes away the keyframe at `frame`, if there is one, and gives it.
    pub fn remove_at(&mut self, frame: u64) -> Option<RackKeyframe> {
        let index = self.position_of(frame).ok()?;
        Some(self.keyframes.remove(index))
    }

    /// Takes away the keyframe called `id`, if the rack has it, and gives it.
    pub fn remove(&mut self, id: KeyframeId) -> Option<RackKeyframe> {
        let index = self.keyframes.iter().position(|keyframe| keyframe.id == id)?;
        Some(self.keyframes.remove(index))
    }

    /// Puts a keyframe that was made anywhere, with the name it has, in this rack, at the frame it has. Says why not, and
    /// changes nothing, if the rack has a keyframe at that frame, or that keyframe already, or the value is not the kind the
    /// property has.
    pub fn put_in(&mut self, keyframe: RackKeyframe) -> Result<(), RackEditError> {
        if keyframe.value.kind() != self.property.kind() {
            return Err(RackEditError::NotTheKindOfValueThePropertyHas);
        }
        if self.keyframe(keyframe.id).is_some() {
            return Err(RackEditError::AlreadyHasThatKeyframe);
        }
        match self.position_of(keyframe.frame) {
            Ok(_) => Err(RackEditError::FramesWouldCoincide),
            Err(index) => {
                self.keyframes.insert(index, keyframe);
                Ok(())
            }
        }
    }

    /// Makes a new keyframe at `frame` that has the value of the keyframe called `id`, which this rack must have. Gives the name
    /// of the new keyframe, or says why not (and changes nothing): the rack does not have that keyframe, or has a keyframe at
    /// `frame` already.
    pub fn add_a_copy_of(&mut self, id: KeyframeId, frame: u64) -> Result<KeyframeId, RackEditError> {
        let value = self.keyframe(id).ok_or(RackEditError::NoSuchKeyframe)?.value.clone();
        if self.at(frame).is_some() {
            return Err(RackEditError::FramesWouldCoincide);
        }
        self.set(frame, value)
    }

    /// A copy of this rack that has the name `id` and has the keyframes animate `target`. Its keyframes are new ones, made in
    /// it, at the same frames and with the same values: they are not the keyframes of this rack, which keep their names.
    #[must_use]
    pub fn copy_as(&self, id: RackId, target: AnimationTarget) -> Rack {
        let keyframes: Vec<RackKeyframe> = self
            .keyframes
            .iter()
            .zip(0u32..)
            .map(|(keyframe, number)| RackKeyframe {
                id: KeyframeId::new(id, number),
                frame: keyframe.frame,
                value: keyframe.value.clone(),
            })
            .collect();
        Rack {
            id,
            target,
            property: self.property,
            next_keyframe_number: u32::try_from(keyframes.len()).unwrap_or(u32::MAX),
            keyframes,
        }
    }

    /// This rack with no keyframes at all. (A project does not keep a rack like that: it is what is put in place of a rack that is
    /// to be taken away.)
    #[must_use]
    pub fn emptied(&self) -> Rack {
        self.with_keyframes(Vec::new())
    }

    /// This rack with `keyframes` in place of its own, each with the name it has. They must be in the order of their frames, with no
    /// two at one frame, and of the kind of value that the property has.
    #[must_use]
    pub fn with_keyframes(&self, keyframes: Vec<RackKeyframe>) -> Rack {
        debug_assert!(
            keyframes.windows(2).all(|pair| pair[0].frame < pair[1].frame),
            "the keyframes of a rack are in the order of their frames, each at a frame of its own"
        );
        debug_assert!(
            keyframes
                .iter()
                .all(|keyframe| keyframe.value.kind() == self.property.kind())
        );
        Rack {
            id: self.id,
            target: self.target,
            property: self.property,
            keyframes,
            next_keyframe_number: self.next_keyframe_number,
        }
    }

    /// This rack with `keyframes` in place of its own, in whatever order they are given and at whatever frames: a rack that nothing
    /// in the program makes, for the tests of what finds one.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_keyframes_in_any_order(&self, keyframes: Vec<RackKeyframe>) -> Rack {
        Rack {
            keyframes,
            ..self.clone()
        }
    }

    /// The two racks that what holds this one is cut into, `frames_in` frames after its start: the first is this rack with
    /// the keyframes from that frame on taken out, and the second is made of those, with the name `second_id`, of
    /// `second_target`, and with its frames counted from the cut.
    ///
    /// A keyframe is added at the last frame of the first rack if `value_before_the_cut` says what it is to be there, and
    /// at the first frame of the second if `value_after_the_cut` does (unless there is one there already): they are for
    /// when the curve that the rack is a part of goes on across the cut, so that each part has what the whole had at every
    /// frame it has.
    #[must_use]
    pub fn split_at(
        &self,
        frames_in: u64,
        value_before_the_cut: Option<PropertyValue>,
        value_after_the_cut: Option<PropertyValue>,
        (second_id, second_target): (RackId, AnimationTarget),
    ) -> (Rack, Rack) {
        let cut = self.keyframes.partition_point(|keyframe| keyframe.frame < frames_in);
        let mut first = Rack {
            keyframes: self.keyframes[..cut].to_vec(),
            ..self.clone()
        };
        let mut second = Rack {
            id: second_id,
            target: second_target,
            keyframes: self.keyframes[cut..]
                .iter()
                .map(|keyframe| RackKeyframe {
                    id: keyframe.id,
                    frame: keyframe.frame - frames_in,
                    value: keyframe.value.clone(),
                })
                .collect(),
            ..self.clone()
        };
        if let Some(value) = value_before_the_cut
            && let Some(last_frame_of_the_first) = frames_in.checked_sub(1)
        {
            first.set_if_there_is_none_there(last_frame_of_the_first, value);
        }
        if let Some(value) = value_after_the_cut {
            second.set_if_there_is_none_there(0, value);
        }
        (first, second)
    }

    /// This rack when what holds it is only `length` frames long and was longer: the keyframes it still has frames for stay where
    /// they are, and those that were at frames that are cut off come in to the last frames, in their order, where there is room
    /// for them, which the empty frames between the keyframes make. A keyframe is lost only when there are more keyframes than
    /// frames, and then it is the latest that are (see `keyframe_fitting`).
    #[must_use]
    pub fn truncated_to(&self, length: u64) -> Rack {
        self.with_frames_fitted(|frame| frame, Some(length), WhichAreLost::TheLatest)
    }

    /// This rack when what holds it is `to_length` frames long and was `from_length`: each keyframe is as far through it as it was,
    /// as near as the frames allow. Keyframes that would land on one frame are put at frames of their own beside it, which the
    /// empty frames between the keyframes make room for: a keyframe is lost only when there are more keyframes than frames, and
    /// then it is every so many of them that are, so that those kept are spread through the whole (see
    /// `keyframe_fitting`).
    #[must_use]
    pub fn rescaled(&self, from_length: u64, to_length: u64) -> Rack {
        if to_length == 0 {
            return self.emptied();
        }
        if from_length == 0 || from_length == to_length {
            return self.clone();
        }
        self.with_frames_fitted(
            |frame| (frame as f64 * to_length as f64 / from_length as f64).round() as u64,
            Some(to_length),
            WhichAreLost::SpreadThroughThem,
        )
    }

    /// This rack with every keyframe at the frame that `to_frame` says its frame becomes, which must not put a keyframe before
    /// one that was before it. Keyframes that would land on one frame are put at frames of their own beside it (see
    /// `keyframe_fitting`), and none is lost.
    #[must_use]
    pub fn with_frames_mapped(&self, to_frame: impl Fn(u64) -> u64) -> Rack {
        self.with_frames_fitted(to_frame, None, WhichAreLost::TheLatest)
    }

    /// This rack with its keyframes at the frames that `to_frame` says they want to be at, fitted into `length` frames (or as many
    /// as they need, if that is `None`): see `keyframe_fitting`.
    fn with_frames_fitted(
        &self,
        to_frame: impl Fn(u64) -> u64,
        length: Option<u64>,
        which_are_lost: WhichAreLost,
    ) -> Rack {
        let wanted: Vec<u64> = self.keyframes.iter().map(|keyframe| to_frame(keyframe.frame)).collect();
        let fitted = self
            .keyframes
            .iter()
            .zip(frames_for_keyframes_wanted_at(&wanted, length, which_are_lost))
            .filter_map(|(keyframe, frame)| {
                Some(RackKeyframe {
                    id: keyframe.id,
                    frame: frame?,
                    value: keyframe.value.clone(),
                })
            })
            .collect();
        self.with_keyframes(fitted)
    }

    fn set_if_there_is_none_there(&mut self, frame: u64, value: PropertyValue) {
        if self.at(frame).is_none() {
            let _ = self.set(frame, value);
        }
    }
}

#[cfg(test)]
mod tests;
