// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The keyframes of one property: the values it has at some frames, from which it is worked out at every other.
//!
//! Frames are counted from the start of whatever the keyframes belong to (a clip, or the whole timeline for a track).
//! Before the first keyframe the property has the value of the first, and after the last keyframe the value of the
//! last. Between two keyframes it changes evenly from the one to the other, if what it is made of can (a number, two
//! numbers, a colour), and otherwise (yes or no, words) it stays what it was until the next keyframe.

use super::property::PropertyValue;

/// A value at a frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Keyframe {
    pub frame: u64,
    pub value: PropertyValue,
}

/// The keyframes of one property, in order of frame, with no two at the same frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keyframes {
    keyframes: Vec<Keyframe>,
}

impl Keyframes {
    /// Keyframes made of `keyframes`, which may be in any order; where two are at the same frame the last one given
    /// is kept.
    pub fn from_unordered(keyframes: impl IntoIterator<Item = Keyframe>) -> Self {
        let mut made = Self::default();
        for keyframe in keyframes {
            made.set(keyframe.frame, keyframe.value);
        }
        made
    }

    pub fn is_empty(&self) -> bool {
        self.keyframes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.keyframes.len()
    }

    /// The keyframes, the earliest first.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Keyframe> {
        self.keyframes.iter()
    }

    /// The value of the keyframe at `frame`, if there is one exactly there.
    pub fn at(&self, frame: u64) -> Option<&PropertyValue> {
        self.position_of(frame).ok().map(|index| &self.keyframes[index].value)
    }

    fn position_of(&self, frame: u64) -> Result<usize, usize> {
        self.keyframes.binary_search_by_key(&frame, |keyframe| keyframe.frame)
    }

    /// The value at `frame`: that of the keyframe there, or what it is between the keyframes either side of it, or
    /// that of the first or the last if it is before or after them all. `None` if there are no keyframes.
    pub fn value_at(&self, frame: u64) -> Option<PropertyValue> {
        let after = self.keyframes.partition_point(|keyframe| keyframe.frame <= frame);
        let before = after.checked_sub(1).map(|index| &self.keyframes[index]);
        let next = self.keyframes.get(after);
        match (before, next) {
            (Some(before), Some(next)) => {
                let fraction = (frame - before.frame) as f32 / (next.frame - before.frame) as f32;
                Some(before.value.blended_towards(&next.value, fraction))
            }
            (Some(only), None) | (None, Some(only)) => Some(only.value.clone()),
            (None, None) => None,
        }
    }

    /// Puts a keyframe with `value` at `frame`, in place of the one there if there is one.
    pub fn set(&mut self, frame: u64, value: PropertyValue) {
        match self.position_of(frame) {
            Ok(index) => self.keyframes[index].value = value,
            Err(index) => self.keyframes.insert(index, Keyframe { frame, value }),
        }
    }

    /// The keyframes of the two parts that what they belong to is cut into when it is cut `frames_in` frames after its
    /// start (which must be more than 0): of the part before the cut, with the frames it has, and of the part from the
    /// cut on, with its frames counted from the cut. At every frame each part has what the whole had there.
    ///
    /// For that, a part that the curve goes on beyond (or comes from before) is given a keyframe at its edge with the
    /// value that the curve has there (the last frame of the first part, the first frame of the second). That also
    /// gives a part that had no keyframes of its own one, so that what was animated stays so. Nothing at all is cut into
    /// nothing at all.
    ///
    /// Nothing in the program cuts keyframes like this any more (racks cut their own keyframes: see [`crate::project::Rack::split_at`]);
    /// it is what a rack's cut is checked against, in the tests, as arithmetic written a second time.
    #[cfg(test)]
    #[must_use]
    pub fn split_at(&self, frames_in: u64) -> (Keyframes, Keyframes) {
        if self.is_empty() {
            return (Self::default(), Self::default());
        }
        let cut = self.keyframes.partition_point(|keyframe| keyframe.frame < frames_in);
        let mut first = Self {
            keyframes: self.keyframes[..cut].to_vec(),
        };
        let mut second = Self {
            keyframes: self.keyframes[cut..]
                .iter()
                .map(|keyframe| Keyframe {
                    frame: keyframe.frame - frames_in,
                    value: keyframe.value.clone(),
                })
                .collect(),
        };
        let last_frame_of_the_first = frames_in.saturating_sub(1);
        let curve_goes_on_beyond_the_cut = cut < self.keyframes.len();
        if curve_goes_on_beyond_the_cut
            && first.at(last_frame_of_the_first).is_none()
            && let Some(value) = self.value_at(last_frame_of_the_first)
        {
            first.set(last_frame_of_the_first, value);
        }
        let curve_comes_from_before_the_cut = cut > 0;
        if curve_comes_from_before_the_cut
            && second.at(0).is_none()
            && let Some(value) = self.value_at(frames_in)
        {
            second.set(0, value);
        }
        (first, second)
    }

    /// The frame of the last keyframe before `frame`.
    pub fn frame_before(&self, frame: u64) -> Option<u64> {
        let index = self.keyframes.partition_point(|keyframe| keyframe.frame < frame);
        index.checked_sub(1).map(|index| self.keyframes[index].frame)
    }

    /// The frame of the first keyframe after `frame`.
    pub fn frame_after(&self, frame: u64) -> Option<u64> {
        let index = self.keyframes.partition_point(|keyframe| keyframe.frame <= frame);
        self.keyframes.get(index).map(|keyframe| keyframe.frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::DeterministicRandomNumbers;

    fn number(value: f32) -> PropertyValue {
        PropertyValue::Number(value)
    }

    fn keyframes_of(frames_and_numbers: &[(u64, f32)]) -> Keyframes {
        Keyframes::from_unordered(frames_and_numbers.iter().map(|(frame, value)| Keyframe {
            frame: *frame,
            value: number(*value),
        }))
    }

    fn frames_of(keyframes: &Keyframes) -> Vec<u64> {
        keyframes.iter().map(|keyframe| keyframe.frame).collect()
    }

    fn as_number(value: Option<PropertyValue>) -> f32 {
        match value {
            Some(PropertyValue::Number(value)) => value,
            other => panic!("not a number: {other:?}"),
        }
    }

    // --- looking things up ---

    /// Found by the planted-mistakes check: working a keyframe's value out as the end of the stretch before it (the
    /// whole of the way from the keyframe before) is the same value but for the last bit of a number with a fraction, and
    /// nothing noticed. Exactly the value is what keeps a control that shows the value, and gives it back when it is
    /// touched, from changing it by a rounding error.
    #[test]
    fn at_a_keyframe_the_value_is_exactly_the_keyframes_whatever_the_numbers_around_it_are() {
        for seed in 1..=200u64 {
            let mut random = DeterministicRandomNumbers::with_seed(seed);
            let mut keyframes = Keyframes::default();
            for _ in 0..2 + random.next_below(5) {
                keyframes.set(
                    random.next_below(300),
                    number(random.next_below(1000) as f32 / 7.0 - 50.0),
                );
            }
            for keyframe in keyframes.iter() {
                assert_eq!(
                    keyframes.value_at(keyframe.frame).as_ref(),
                    Some(&keyframe.value),
                    "seed {seed}, frame {}",
                    keyframe.frame
                );
            }
        }
    }

    #[test]
    fn no_keyframes_have_no_value() {
        let none = Keyframes::default();
        assert!(none.is_empty());
        assert_eq!(none.value_at(5), None);
        assert_eq!(none.frame_before(5), None);
        assert_eq!(none.frame_after(5), None);
    }

    #[test]
    fn before_the_first_keyframe_the_value_is_the_first_and_after_the_last_it_is_the_last() {
        let keyframes = keyframes_of(&[(10, 1.0), (20, 3.0)]);
        assert_eq!(keyframes.value_at(0), Some(number(1.0)));
        assert_eq!(keyframes.value_at(9), Some(number(1.0)));
        assert_eq!(keyframes.value_at(21), Some(number(3.0)));
        assert_eq!(keyframes.value_at(5000), Some(number(3.0)));
    }

    #[test]
    fn at_a_keyframe_the_value_is_its_own_and_between_two_it_changes_evenly() {
        let keyframes = keyframes_of(&[(10, 1.0), (20, 3.0), (40, -1.0)]);
        assert_eq!(keyframes.value_at(10), Some(number(1.0)));
        assert_eq!(keyframes.value_at(20), Some(number(3.0)));
        assert_eq!(keyframes.value_at(15), Some(number(2.0)));
        assert_eq!(keyframes.value_at(30), Some(number(1.0)));
        assert_eq!(keyframes.value_at(40), Some(number(-1.0)));
        assert_eq!(as_number(keyframes.value_at(11)), 1.2);
    }

    #[test]
    fn a_single_keyframe_is_the_value_everywhere() {
        let keyframes = keyframes_of(&[(7, 4.0)]);
        for frame in [0, 6, 7, 8, 100] {
            assert_eq!(keyframes.value_at(frame), Some(number(4.0)));
        }
    }

    #[test]
    fn yes_or_no_and_words_stay_what_they_were_until_the_next_keyframe() {
        let flags = Keyframes::from_unordered([
            Keyframe {
                frame: 0,
                value: PropertyValue::Flag(false),
            },
            Keyframe {
                frame: 10,
                value: PropertyValue::Flag(true),
            },
        ]);
        assert_eq!(flags.value_at(9), Some(PropertyValue::Flag(false)));
        assert_eq!(flags.value_at(10), Some(PropertyValue::Flag(true)));
        assert_eq!(flags.value_at(11), Some(PropertyValue::Flag(true)));
    }

    #[test]
    fn the_frames_before_and_after_a_frame_are_the_keyframes_either_side_of_it_not_including_it() {
        let keyframes = keyframes_of(&[(10, 1.0), (20, 3.0), (40, -1.0)]);
        assert_eq!(keyframes.frame_before(10), None);
        assert_eq!(keyframes.frame_before(11), Some(10));
        assert_eq!(keyframes.frame_before(20), Some(10));
        assert_eq!(keyframes.frame_before(99), Some(40));
        assert_eq!(keyframes.frame_after(0), Some(10));
        assert_eq!(keyframes.frame_after(10), Some(20));
        assert_eq!(keyframes.frame_after(39), Some(40));
        assert_eq!(keyframes.frame_after(40), None);
    }

    // --- changing them ---

    #[test]
    fn setting_a_keyframe_puts_it_in_order_and_replaces_the_one_at_the_same_frame() {
        let mut keyframes = keyframes_of(&[(30, 3.0), (10, 1.0)]);
        assert_eq!(frames_of(&keyframes), vec![10, 30], "however they were given");
        keyframes.set(20, number(2.0));
        assert_eq!(frames_of(&keyframes), vec![10, 20, 30]);
        keyframes.set(20, number(9.0));
        assert_eq!(keyframes.len(), 3);
        assert_eq!(keyframes.at(20), Some(&number(9.0)));
    }

    #[test]
    fn of_two_keyframes_given_at_one_frame_the_last_is_kept() {
        let keyframes = keyframes_of(&[(5, 1.0), (5, 2.0)]);
        assert_eq!(keyframes.len(), 1);
        assert_eq!(keyframes.at(5), Some(&number(2.0)));
    }

    // --- cutting ---

    #[test]
    fn cutting_nothing_gives_nothing() {
        let (first, second) = Keyframes::default().split_at(10);
        assert!(first.is_empty() && second.is_empty());
    }

    #[test]
    fn the_part_before_a_cut_keeps_its_keyframes_and_gets_one_at_its_last_frame_with_the_value_there() {
        let keyframes = keyframes_of(&[(0, 0.0), (20, 20.0)]);
        let (first, second) = keyframes.split_at(10);
        assert_eq!(frames_of(&first), vec![0, 9]);
        assert_eq!(first.at(9), Some(&number(9.0)), "the value the whole had at frame 9");
        assert_eq!(frames_of(&second), vec![0, 10], "frames are counted from the cut");
        assert_eq!(second.at(0), Some(&number(10.0)));
        assert_eq!(second.at(10), Some(&number(20.0)));
    }

    #[test]
    fn a_keyframe_just_where_the_cut_is_needs_no_new_one_and_one_just_before_it_neither() {
        let keyframes = keyframes_of(&[(0, 0.0), (9, 9.0), (10, 10.0), (20, 20.0)]);
        let (first, second) = keyframes.split_at(10);
        assert_eq!(frames_of(&first), vec![0, 9]);
        assert_eq!(frames_of(&second), vec![0, 10]);
    }

    #[test]
    fn a_part_that_has_no_keyframes_of_its_own_is_given_one_so_that_it_stays_animated() {
        let keyframes = keyframes_of(&[(2, 5.0), (4, 7.0)]);
        // The cut is after both: the second part has the last value all through.
        let (first, second) = keyframes.split_at(10);
        assert_eq!(frames_of(&first), vec![2, 4]);
        assert_eq!(frames_of(&second), vec![0]);
        assert_eq!(second.at(0), Some(&number(7.0)));
        // The cut is before both: the first part has the first value all through.
        let (first, second) = keyframes.split_at(1);
        assert_eq!(frames_of(&first), vec![0]);
        assert_eq!(first.at(0), Some(&number(5.0)));
        assert_eq!(frames_of(&second), vec![1, 3]);
    }

    fn random_keyframes(random: &mut DeterministicRandomNumbers) -> Keyframes {
        Keyframes::from_unordered((0..1 + random.next_below(6)).map(|_| Keyframe {
            frame: random.next_below(60),
            value: number(random.next_below(200) as f32 - 100.0),
        }))
    }

    fn is_near(first: f32, second: f32) -> bool {
        (first - second).abs() < 1e-3
    }

    #[test]
    fn the_two_parts_of_a_cut_have_at_every_frame_what_the_whole_had_for_random_keyframes_and_cuts() {
        let mut random = DeterministicRandomNumbers::with_seed(12);
        for round in 0..300 {
            let whole = random_keyframes(&mut random);
            let cut = 1 + random.next_below(70);
            let (first, second) = whole.split_at(cut);
            assert!(
                !first.is_empty() && !second.is_empty(),
                "round {round}: both stay animated"
            );
            for frame in 0..cut {
                assert!(
                    is_near(as_number(first.value_at(frame)), as_number(whole.value_at(frame))),
                    "round {round}: frame {frame} of the first part of a cut at {cut}: {first:?} of {whole:?}"
                );
            }
            for frame in cut..cut + 40 {
                assert!(
                    is_near(
                        as_number(second.value_at(frame - cut)),
                        as_number(whole.value_at(frame))
                    ),
                    "round {round}: frame {frame} in the second part of a cut at {cut}: {second:?} of {whole:?}"
                );
            }
            assert!(first.iter().all(|keyframe| keyframe.frame < cut), "round {round}");
        }
    }
}
