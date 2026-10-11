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
