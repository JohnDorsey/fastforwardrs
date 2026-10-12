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
use crate::project::{AnimationTarget, Keyframe, Keyframes, PropertyId, PropertyValue};

/// A value at a frame, in a rack. It has a number of its own, which is how it is told from the others in the rack
/// whichever frame it is moved to.
#[derive(Clone, Debug, PartialEq)]
pub struct RackKeyframe {
    number: u32,
    /// Counted from the start of what holds the rack.
    pub frame: u64,
    pub value: PropertyValue,
}

impl RackKeyframe {
    /// Which of the keyframes made in its rack this is.
    pub fn number(&self) -> u32 {
        self.number
    }
}

/// Why a change to a rack was not made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum RackEditError {
    #[error("the rack has no such keyframe")]
    NoSuchKeyframe,
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

    /// The name of `keyframe`, which must be one of this rack's.
    pub fn id_of(&self, keyframe: &RackKeyframe) -> KeyframeId {
        KeyframeId::new(self.id, keyframe.number)
    }

    /// The keyframe called `id`, if it is in this rack.
    pub fn keyframe(&self, id: KeyframeId) -> Option<&RackKeyframe> {
        if id.rack() != self.id {
            return None;
        }
        self.keyframes.iter().find(|keyframe| keyframe.number == id.number())
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
                Ok(KeyframeId::new(self.id, self.keyframes[index].number))
            }
            Err(index) => {
                let number = self.next_keyframe_number;
                self.next_keyframe_number += 1;
                self.keyframes.insert(index, RackKeyframe { number, frame, value });
                Ok(KeyframeId::new(self.id, number))
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
        if id.rack() != self.id {
            return None;
        }
        let index = self
            .keyframes
            .iter()
            .position(|keyframe| keyframe.number == id.number())?;
        Some(self.keyframes.remove(index))
    }

    /// Moves keyframes, each to the frame given for it: all of them, or (if one is not in the rack, or two would be at the
    /// same frame, counting those that stay) none.
    pub fn move_keyframes(&mut self, moves: &[(KeyframeId, u64)]) -> Result<(), RackEditError> {
        let mut frames: Vec<(u32, u64)> = self
            .keyframes
            .iter()
            .map(|keyframe| (keyframe.number, keyframe.frame))
            .collect();
        for (id, to_frame) in moves {
            if id.rack() != self.id {
                return Err(RackEditError::NoSuchKeyframe);
            }
            let (_, frame) = frames
                .iter_mut()
                .find(|(number, _)| *number == id.number())
                .ok_or(RackEditError::NoSuchKeyframe)?;
            *frame = *to_frame;
        }
        let mut seen: Vec<u64> = frames.iter().map(|(_, frame)| *frame).collect();
        seen.sort_unstable();
        if seen.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(RackEditError::FramesWouldCoincide);
        }
        for keyframe in &mut self.keyframes {
            keyframe.frame = frames
                .iter()
                .find(|(number, _)| *number == keyframe.number)
                .map_or(keyframe.frame, |(_, frame)| *frame);
        }
        self.keyframes.sort_by_key(|keyframe| keyframe.frame);
        Ok(())
    }

    /// A copy of this rack that has the name `id` and has the keyframes animate `target`. (Its keyframes keep their numbers:
    /// they are told from those of this rack by the rack they are in.)
    #[must_use]
    pub fn copy_as(&self, id: RackId, target: AnimationTarget) -> Rack {
        Rack {
            id,
            target,
            ..self.clone()
        }
    }

    /// The same rack, with its keyframes animating `target`.
    #[must_use]
    pub fn retargeted(mut self, target: AnimationTarget) -> Rack {
        self.target = target;
        self
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
                    number: keyframe.number,
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

    /// This rack when what holds it is only `length` frames long and was longer: the keyframes it still has a frame for, and
    /// one at its last frame with `value_at_the_end` if that says what it is to be (unless there is one there already).
    #[must_use]
    pub fn truncated_to(&self, length: u64, value_at_the_end: Option<PropertyValue>) -> Rack {
        let mut truncated = Rack {
            keyframes: self
                .keyframes
                .iter()
                .filter(|keyframe| keyframe.frame < length)
                .cloned()
                .collect(),
            ..self.clone()
        };
        if let Some(value) = value_at_the_end
            && let Some(last_frame) = length.checked_sub(1)
        {
            truncated.set_if_there_is_none_there(last_frame, value);
        }
        truncated
    }

    /// This rack when what holds it is only `length` frames long and was longer, by what its own keyframes say (see
    /// [`Self::truncated_to`]): a keyframe is added at its last frame if some were cut off, with the value that the rack's own
    /// keyframes had there.
    #[must_use]
    pub fn truncated_by_its_own_curve(&self, length: u64) -> Rack {
        let some_were_cut_off = self.keyframes.iter().any(|keyframe| keyframe.frame >= length);
        let value_at_the_end = some_were_cut_off
            .then(|| length.checked_sub(1).and_then(|last| self.curve().value_at(last)))
            .flatten();
        self.truncated_to(length, value_at_the_end)
    }

    /// This rack when what holds it is `to_length` frames long and was `from_length`: each keyframe is as far through it as it
    /// was, as near as the frames allow. Keyframes that land on the same frame are one: the later is kept. Nothing is kept of
    /// what is `to_length` frames or more through.
    #[must_use]
    pub fn rescaled(&self, from_length: u64, to_length: u64) -> Rack {
        if to_length == 0 {
            return Rack {
                keyframes: Vec::new(),
                ..self.clone()
            };
        }
        if from_length == 0 || from_length == to_length {
            return self.clone();
        }
        self.with_frames_mapped(|frame| {
            let frame = (frame as f64 * to_length as f64 / from_length as f64).round() as u64;
            frame.min(to_length - 1)
        })
    }

    /// This rack with every keyframe at the frame that `to_frame` says its frame becomes. (Keyframes that land on one frame
    /// are one: the later is kept.)
    #[must_use]
    pub fn with_frames_mapped(&self, to_frame: impl Fn(u64) -> u64) -> Rack {
        let mut mapped: Vec<RackKeyframe> = self
            .keyframes
            .iter()
            .map(|keyframe| RackKeyframe {
                number: keyframe.number,
                frame: to_frame(keyframe.frame),
                value: keyframe.value.clone(),
            })
            .collect();
        // (A stable sort: of two at one frame the one that was later stays later.)
        mapped.sort_by_key(|keyframe| keyframe.frame);
        let mut kept: Vec<RackKeyframe> = Vec::with_capacity(mapped.len());
        for keyframe in mapped {
            match kept.last_mut() {
                Some(last) if last.frame == keyframe.frame => *last = keyframe,
                _ => kept.push(keyframe),
            }
        }
        Rack {
            keyframes: kept,
            ..self.clone()
        }
    }

    fn set_if_there_is_none_there(&mut self, frame: u64, value: PropertyValue) {
        if self.at(frame).is_none() {
            let _ = self.set(frame, value);
        }
    }
}

#[cfg(test)]
mod tests;
