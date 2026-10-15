// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Exact video frame rates.

use std::fmt;
use std::num::NonZeroU32;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A tolerance, in frames, used when turning a time in seconds into a frame number. A time that
/// was itself computed from a frame number carries a tiny floating point error; without this
/// tolerance such a time could land a hair below the frame it came from and map to the frame
/// before it.
const FRAME_ROUNDING_TOLERANCE_IN_FRAMES: f64 = 1.0e-6;

/// An exact frame rate: `numerator` frames every `denominator` seconds.
///
/// Fractions are used (instead of a floating point number) because common rates such as
/// 30000/1001 ("29.97") cannot be written exactly as a decimal, and frame arithmetic must stay
/// exact over hours of video.
///
/// The fraction is always stored in lowest terms, so `60/2` and `30/1` compare equal. In project
/// files it is written as text, for example `"30000/1001"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct FrameRate {
    numerator: u32,
    denominator: u32,
}

/// Returned when a frame rate has a zero numerator or denominator, or cannot be parsed.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum InvalidFrameRate {
    #[error(
        "a frame rate needs a numerator and a denominator that are both greater than zero (got {numerator}/{denominator})"
    )]
    ZeroComponent { numerator: u32, denominator: u32 },
    #[error("\"{text}\" is not a frame rate; expected a whole number or a fraction such as \"30000/1001\"")]
    Unparseable { text: String },
}

impl FrameRate {
    /// Creates a frame rate of `numerator` frames every `denominator` seconds.
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, InvalidFrameRate> {
        if numerator == 0 || denominator == 0 {
            return Err(InvalidFrameRate::ZeroComponent { numerator, denominator });
        }
        let common_divisor = greatest_common_divisor(numerator, denominator);
        Ok(Self {
            numerator: numerator / common_divisor,
            denominator: denominator / common_divisor,
        })
    }

    /// Creates a frame rate of a whole number of frames per second, such as 30 or 60.
    pub fn from_whole_frames_per_second(frames_per_second: u32) -> Result<Self, InvalidFrameRate> {
        Self::new(frames_per_second, 1)
    }

    /// A frame rate of a whole number of frames per second that is known not to be zero, so that
    /// it can be made where an error cannot be handled: in a constant.
    pub const fn from_non_zero_whole_frames_per_second(frames_per_second: NonZeroU32) -> Self {
        Self {
            numerator: frames_per_second.get(),
            denominator: 1,
        }
    }

    pub fn numerator(self) -> u32 {
        self.numerator
    }

    pub fn denominator(self) -> u32 {
        self.denominator
    }

    /// The rate as a (possibly inexact) decimal number, for example 29.97002997...
    pub fn frames_per_second(self) -> f64 {
        f64::from(self.numerator) / f64::from(self.denominator)
    }

    /// The most frames that any single second can contain: 30 for both 30 and 29.97.
    pub fn most_frames_in_one_second(self) -> u32 {
        self.numerator.div_ceil(self.denominator)
    }

    /// The moment, in seconds, at which frame number `frame_number` starts (frame 0 starts at 0).
    pub fn seconds_at_frame(self, frame_number: u64) -> f64 {
        let frames_times_denominator = u128::from(frame_number) * u128::from(self.denominator);
        frames_times_denominator as f64 / f64::from(self.numerator)
    }

    /// The moment, in whole microseconds, at which frame number `frame_number` starts, to the nearest
    /// microsecond (half a microsecond is rounded up). Exact integer arithmetic. A microsecond is what
    /// ffmpeg's own sums are made in when it is asked for them (`AVTB`), so a moment given in them is
    /// given exactly as it will be used.
    pub fn microseconds_at_frame(self, frame_number: u64) -> u64 {
        let numerator = u128::from(self.numerator);
        let scaled = u128::from(frame_number) * u128::from(self.denominator) * 1_000_000;
        u64::try_from((2 * scaled + numerator) / (2 * numerator)).unwrap_or(u64::MAX)
    }

    /// The frame that is on screen at `seconds`. Negative and not-a-number inputs give frame 0.
    pub fn frame_at_seconds(self, seconds: f64) -> u64 {
        if seconds.is_nan() || seconds <= 0.0 {
            return 0;
        }
        let frames = seconds * self.frames_per_second() + FRAME_ROUNDING_TOLERANCE_IN_FRAMES;
        frames.floor() as u64
    }

    /// The frame whose start is closest to `seconds`. Negative and not-a-number inputs give 0.
    pub fn nearest_frame_to_seconds(self, seconds: f64) -> u64 {
        if seconds.is_nan() || seconds <= 0.0 {
            return 0;
        }
        (seconds * self.frames_per_second()).round() as u64
    }

    /// The number of the whole second (counting from 0) that contains `frame_number`.
    /// Exact integer arithmetic.
    pub fn whole_second_containing_frame(self, frame_number: u64) -> u64 {
        let numerator_of_seconds = u128::from(frame_number) * u128::from(self.denominator);
        (numerator_of_seconds / u128::from(self.numerator)) as u64
    }

    /// The number of the first frame that starts inside whole second `whole_second`.
    /// Exact integer arithmetic.
    pub fn first_frame_in_whole_second(self, whole_second: u64) -> u64 {
        let frames_numerator = u128::from(whole_second) * u128::from(self.numerator);
        frames_numerator.div_ceil(u128::from(self.denominator)) as u64
    }

    /// How many frames at the rate `other` take as long as `frames` frames at this rate, to the nearest
    /// whole frame (half a frame is rounded up). Exact integer arithmetic.
    ///
    /// This is also how many pictures a video of `frames` pictures comes to when it is made to run at
    /// `other`: that is what ffmpeg's `fps` filter does when it is asked for the nearest picture to each
    /// moment (checked against ffmpeg 9.0.2 for videos of 12 to 60 frames per second, of 1 to 300 pictures).
    pub fn convert_frame_count_to(self, frames: u64, other: FrameRate) -> u64 {
        let numerator = u128::from(frames) * u128::from(other.numerator) * u128::from(self.denominator);
        let denominator = u128::from(other.denominator) * u128::from(self.numerator);
        let rounded = numerator
            .checked_mul(2)
            .and_then(|twice| twice.checked_add(denominator))
            .map(|sum| sum / (2 * denominator));
        rounded.and_then(|count| u64::try_from(count).ok()).unwrap_or(u64::MAX)
    }
}

impl fmt::Display for FrameRate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.numerator, self.denominator)
    }
}

impl FromStr for FrameRate {
    type Err = InvalidFrameRate;

    /// Accepts a whole number ("30") or a fraction ("30000/1001").
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let unparseable = || InvalidFrameRate::Unparseable { text: text.to_owned() };
        let (numerator_text, denominator_text) = match text.trim().split_once('/') {
            Some((numerator_text, denominator_text)) => (numerator_text, denominator_text),
            None => (text.trim(), "1"),
        };
        let numerator = numerator_text.trim().parse().map_err(|_| unparseable())?;
        let denominator = denominator_text.trim().parse().map_err(|_| unparseable())?;
        Self::new(numerator, denominator)
    }
}

impl TryFrom<String> for FrameRate {
    type Error = InvalidFrameRate;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<FrameRate> for String {
    fn from(frame_rate: FrameRate) -> Self {
        frame_rate.to_string()
    }
}

fn greatest_common_divisor(mut first: u32, mut second: u32) -> u32 {
    while second != 0 {
        (first, second) = (second, first % second);
    }
    first
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_rate(numerator: u32, denominator: u32) -> FrameRate {
        FrameRate::new(numerator, denominator).unwrap()
    }

    #[test]
    fn fractions_are_stored_in_lowest_terms() {
        assert_eq!(frame_rate(60, 2), frame_rate(30, 1));
        assert_eq!(frame_rate(60000, 2002), frame_rate(30000, 1001));
        assert_eq!(frame_rate(60, 2).numerator(), 30);
        assert_eq!(frame_rate(60, 2).denominator(), 1);
    }

    #[test]
    fn a_zero_numerator_or_denominator_is_rejected() {
        assert_eq!(
            FrameRate::new(0, 1),
            Err(InvalidFrameRate::ZeroComponent {
                numerator: 0,
                denominator: 1
            })
        );
        assert!(FrameRate::new(30, 0).is_err());
    }

    #[test]
    fn text_parsing_accepts_whole_numbers_and_fractions_and_rejects_everything_else() {
        assert_eq!("30".parse::<FrameRate>().unwrap(), frame_rate(30, 1));
        assert_eq!("30000/1001".parse::<FrameRate>().unwrap(), frame_rate(30000, 1001));
        assert_eq!(" 25 / 1 ".parse::<FrameRate>().unwrap(), frame_rate(25, 1));
        for bad_text in ["", "abc", "30/", "/30", "30/0", "0/0", "-30", "29.97", "1/2/3"] {
            assert!(bad_text.parse::<FrameRate>().is_err(), "{bad_text:?} should not parse");
        }
    }

    #[test]
    fn text_form_round_trips_through_display_and_serde() {
        let original = frame_rate(30000, 1001);
        assert_eq!(original.to_string(), "30000/1001");
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(json, "\"30000/1001\"");
        assert_eq!(serde_json::from_str::<FrameRate>(&json).unwrap(), original);
        assert!(serde_json::from_str::<FrameRate>("\"0/1\"").is_err());
    }

    #[test]
    fn most_frames_in_one_second_rounds_fractional_rates_up() {
        assert_eq!(frame_rate(30, 1).most_frames_in_one_second(), 30);
        assert_eq!(frame_rate(30000, 1001).most_frames_in_one_second(), 30);
        assert_eq!(frame_rate(24000, 1001).most_frames_in_one_second(), 24);
        assert_eq!(frame_rate(60, 1).most_frames_in_one_second(), 60);
    }

    #[test]
    fn seconds_at_frame_matches_the_definition() {
        assert_eq!(frame_rate(30, 1).seconds_at_frame(0), 0.0);
        assert_eq!(frame_rate(30, 1).seconds_at_frame(45), 1.5);
        let seconds = frame_rate(30000, 1001).seconds_at_frame(30000);
        assert!(
            (seconds - 1001.0).abs() < 1.0e-9,
            "30000 frames at 29.97 are 1001 s, got {seconds}"
        );
    }

    #[test]
    fn microseconds_at_frame_are_the_exact_moment_to_the_nearest_microsecond_with_halves_rounded_up() {
        assert_eq!(frame_rate(30, 1).microseconds_at_frame(0), 0);
        assert_eq!(frame_rate(30, 1).microseconds_at_frame(45), 1_500_000);
        assert_eq!(
            frame_rate(30, 1).microseconds_at_frame(1),
            33_333,
            "33333.33 microseconds"
        );
        assert_eq!(
            frame_rate(30, 1).microseconds_at_frame(2),
            66_667,
            "66666.67 microseconds"
        );
        assert_eq!(frame_rate(30000, 1001).microseconds_at_frame(30000), 1_001_000_000);
        assert_eq!(
            frame_rate(2_000_000, 1).microseconds_at_frame(1),
            1,
            "half a microsecond is rounded up"
        );
        assert_eq!(frame_rate(4_000_000, 1).microseconds_at_frame(1), 0, "a quarter is not");
        assert_eq!(
            frame_rate(1, 1).microseconds_at_frame(u64::MAX),
            u64::MAX,
            "no moment is too far to say"
        );
    }

    #[test]
    fn microseconds_at_frame_agree_with_seconds_at_frame_to_within_half_a_microsecond() {
        for rate in [
            frame_rate(12, 1),
            frame_rate(24000, 1001),
            frame_rate(25, 1),
            frame_rate(30, 1),
            frame_rate(30000, 1001),
            frame_rate(60000, 1001),
            frame_rate(120, 1),
        ] {
            for frame in [0, 1, 2, 3, 29, 30, 31, 1000, 123_456, 10_000_000] {
                let exact = rate.seconds_at_frame(frame) * 1_000_000.0;
                let microseconds = rate.microseconds_at_frame(frame) as f64;
                assert!(
                    (microseconds - exact).abs() <= 0.5 + 1.0e-3,
                    "frame {frame} at {rate}: {microseconds} against {exact}"
                );
            }
        }
    }

    #[test]
    fn every_frame_maps_back_to_itself_through_seconds() {
        for rate in [
            frame_rate(24, 1),
            frame_rate(24000, 1001),
            frame_rate(25, 1),
            frame_rate(30, 1),
            frame_rate(30000, 1001),
            frame_rate(60, 1),
            frame_rate(60000, 1001),
            frame_rate(120, 1),
        ] {
            for frame_number in 0..300_000u64 {
                let seconds = rate.seconds_at_frame(frame_number);
                assert_eq!(
                    rate.frame_at_seconds(seconds),
                    frame_number,
                    "frame_at_seconds(seconds_at_frame({frame_number})) at {rate}"
                );
                assert_eq!(rate.nearest_frame_to_seconds(seconds), frame_number);
            }
        }
    }

    #[test]
    fn a_time_just_before_a_frame_boundary_belongs_to_the_previous_frame() {
        let rate = frame_rate(30, 1);
        assert_eq!(rate.frame_at_seconds(1.0 - 0.001), 29);
        assert_eq!(rate.frame_at_seconds(1.0), 30);
        assert_eq!(rate.frame_at_seconds(1.0 + 0.001), 30);
    }

    #[test]
    fn negative_and_nan_times_map_to_the_first_frame() {
        let rate = frame_rate(30, 1);
        assert_eq!(rate.frame_at_seconds(-5.0), 0);
        assert_eq!(rate.frame_at_seconds(f64::NAN), 0);
        assert_eq!(rate.nearest_frame_to_seconds(-5.0), 0);
        assert_eq!(rate.nearest_frame_to_seconds(f64::NAN), 0);
    }

    #[test]
    fn frames_are_counted_again_at_another_rate_as_the_same_time_to_the_nearest_whole_frame() {
        let (thirty, twenty_five, twenty_four, sixty) = (
            frame_rate(30, 1),
            frame_rate(25, 1),
            frame_rate(24, 1),
            frame_rate(60, 1),
        );
        assert_eq!(twenty_five.convert_frame_count_to(100, thirty), 120);
        assert_eq!(twenty_four.convert_frame_count_to(24, thirty), 30);
        assert_eq!(thirty.convert_frame_count_to(30, sixty), 60);
        assert_eq!(sixty.convert_frame_count_to(60, thirty), 30);
        assert_eq!(thirty.convert_frame_count_to(0, sixty), 0);
        assert_eq!(
            thirty.convert_frame_count_to(77, thirty),
            77,
            "the same rate changes nothing"
        );
        // Half a frame is rounded up: 5 frames at 60 are 2.5 at 30.
        assert_eq!(sixty.convert_frame_count_to(5, thirty), 3);
        assert_eq!(sixty.convert_frame_count_to(1, thirty), 1);
        // Less than half a frame is nothing.
        assert_eq!(frame_rate(50, 1).convert_frame_count_to(1, twenty_four), 0);
        // The fractional rates are exact: 300 frames at 29.97 last 10.01 seconds, which is 300.3 at 30.
        assert_eq!(frame_rate(30000, 1001).convert_frame_count_to(300, thirty), 300);
        assert_eq!(frame_rate(30000, 1001).convert_frame_count_to(30000, thirty), 30030);
    }

    #[test]
    fn the_number_of_frames_that_ffmpeg_makes_when_it_changes_the_rate_of_a_video_is_what_is_counted() {
        // What `fps=<rate>:round=near` was measured to make out of videos of these rates and lengths.
        let measured = [
            // (the video's rate, its pictures, the rate asked for, the pictures that came out)
            ((50, 1), 241, (30, 1), 145),
            ((60, 1), 1, (30, 1), 1),
            ((60, 1), 5, (30, 1), 3),
            ((12, 1), 1, (30, 1), 3),
            ((24, 1), 1, (25, 1), 1),
            ((50, 1), 1, (24, 1), 0),
            ((60, 1), 1, (24, 1), 0),
            ((60, 1), 2, (24, 1), 1),
            ((30000, 1001), 300, (30, 1), 300),
        ];
        for ((from_numerator, from_denominator), pictures, (to_numerator, to_denominator), expected) in measured {
            assert_eq!(
                frame_rate(from_numerator, from_denominator)
                    .convert_frame_count_to(pictures, frame_rate(to_numerator, to_denominator)),
                expected,
                "{pictures} pictures at {from_numerator}/{from_denominator} made to run at {to_numerator}/{to_denominator}"
            );
        }
    }

    #[test]
    fn counting_at_another_rate_and_back_gives_the_same_number_when_the_other_rate_is_a_multiple() {
        let (thirty, sixty, hundred_and_twenty) = (frame_rate(30, 1), frame_rate(60, 1), frame_rate(120, 1));
        for frames in 0..2000 {
            assert_eq!(
                sixty.convert_frame_count_to(thirty.convert_frame_count_to(frames, sixty), thirty),
                frames
            );
            assert_eq!(
                hundred_and_twenty
                    .convert_frame_count_to(thirty.convert_frame_count_to(frames, hundred_and_twenty), thirty),
                frames
            );
        }
    }

    #[test]
    fn counting_at_another_rate_never_goes_back_in_time_and_is_never_more_than_a_frame_off_the_exact_time() {
        let rates = [
            frame_rate(24, 1),
            frame_rate(25, 1),
            frame_rate(30000, 1001),
            frame_rate(60, 1),
            frame_rate(12, 1),
        ];
        for from in rates {
            for to in rates {
                let mut previous = 0;
                for frames in 0..1200u64 {
                    let converted = from.convert_frame_count_to(frames, to);
                    assert!(converted >= previous, "{from} to {to} at {frames}");
                    previous = converted;
                    let exact = frames as f64 * to.frames_per_second() / from.frames_per_second();
                    assert!(
                        (converted as f64 - exact).abs() <= 0.5 + 1e-9,
                        "{from} to {to}: {frames} is {exact}, not {converted}"
                    );
                }
            }
        }
    }

    #[test]
    fn counting_a_huge_number_of_frames_at_another_rate_does_not_overflow() {
        assert_eq!(
            frame_rate(1, 4_000_000_000).convert_frame_count_to(u64::MAX, frame_rate(4_000_000_000, 1)),
            u64::MAX
        );
    }

    #[test]
    fn whole_seconds_and_their_first_frames_agree_for_fractional_rates() {
        let rate = frame_rate(30000, 1001);
        // 29.97 fps: second 1 starts at frame 30 (29.97 rounded up), second 2 at frame 60.
        assert_eq!(rate.first_frame_in_whole_second(0), 0);
        assert_eq!(rate.first_frame_in_whole_second(1), 30);
        assert_eq!(rate.first_frame_in_whole_second(2), 60);
        assert_eq!(rate.whole_second_containing_frame(29), 0);
        assert_eq!(rate.whole_second_containing_frame(30), 1);
        assert_eq!(rate.whole_second_containing_frame(59), 1);
        assert_eq!(rate.whole_second_containing_frame(60), 2);
    }
}
