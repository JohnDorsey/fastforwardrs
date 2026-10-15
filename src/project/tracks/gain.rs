// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! How much louder or quieter than it is a sound is made, in decibels.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A change in loudness, in decibels. 0 leaves the sound as it is; every 6 more makes it about
/// twice as loud and every 6 fewer about half as loud. A clip has one, and so does a track, and
/// both are applied to the sound of every clip on the track.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "f32", into = "f32")]
pub struct Gain {
    decibels: f32,
}

impl Gain {
    /// No change: the sound is as loud as it was recorded.
    pub const UNCHANGED: Gain = Gain { decibels: 0.0 };

    /// The quietest a gain can make a sound (about 3% of its loudness). Anything quieter than
    /// that is what muting is for.
    pub const SMALLEST_DECIBELS: f32 = -30.0;

    /// The loudest a gain can make a sound (about 32 times its loudness).
    pub const LARGEST_DECIBELS: f32 = 30.0;

    /// A gain of `decibels`. A number outside what is allowed (for example from a hand-edited
    /// project file) is brought back inside it, and one that is not a number means no change.
    pub fn from_decibels(decibels: f32) -> Self {
        if decibels.is_nan() {
            return Self::UNCHANGED;
        }
        Self {
            decibels: decibels.clamp(Self::SMALLEST_DECIBELS, Self::LARGEST_DECIBELS),
        }
    }

    pub fn decibels(self) -> f32 {
        self.decibels
    }

    /// Whether the sound is left as it is.
    pub fn is_unchanged(self) -> bool {
        self.decibels == 0.0
    }

    /// The number the sound's samples are multiplied by: 1 for no change, 2 for about +6 decibels.
    pub fn linear_factor(self) -> f64 {
        decibels_to_linear_factor(f64::from(self.decibels))
    }

    /// The number the samples are multiplied by when both this gain and `other` are applied. (The
    /// decibels add up, which can go beyond what one gain can be, so this is not itself a gain.)
    pub fn combined_linear_factor(self, other: Gain) -> f64 {
        decibels_to_linear_factor(f64::from(self.decibels) + f64::from(other.decibels))
    }

    /// The number the samples are multiplied by to change their loudness by `decibels`, whatever that is (it is not
    /// limited to what one gain can be: it may be that of two together).
    pub fn linear_factor_of_decibels(decibels: f64) -> f64 {
        decibels_to_linear_factor(decibels)
    }
}

fn decibels_to_linear_factor(decibels: f64) -> f64 {
    10f64.powf(decibels / 20.0)
}

impl Default for Gain {
    fn default() -> Self {
        Self::UNCHANGED
    }
}

impl fmt::Display for Gain {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:+.1} dB", self.decibels)
    }
}

impl From<f32> for Gain {
    fn from(decibels: f32) -> Self {
        Self::from_decibels(decibels)
    }
}

impl From<Gain> for f32 {
    fn from(gain: Gain) -> Self {
        gain.decibels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_change_multiplies_by_exactly_one() {
        assert_eq!(Gain::UNCHANGED.linear_factor(), 1.0);
        assert_eq!(Gain::default(), Gain::UNCHANGED);
        assert!(Gain::UNCHANGED.is_unchanged());
        assert!(!Gain::from_decibels(0.5).is_unchanged());
    }

    #[test]
    fn six_decibels_is_about_twice_as_loud_and_minus_six_about_half() {
        assert!((Gain::from_decibels(6.0).linear_factor() - 1.995).abs() < 0.001);
        assert!((Gain::from_decibels(-6.0).linear_factor() - 0.501).abs() < 0.001);
        assert!((Gain::from_decibels(20.0).linear_factor() - 10.0).abs() < 1e-9);
        assert!((Gain::from_decibels(-20.0).linear_factor() - 0.1).abs() < 1e-9);
    }

    #[test]
    fn a_gain_beyond_what_is_allowed_is_brought_back_inside_and_nothing_is_not_a_change() {
        assert_eq!(Gain::from_decibels(500.0).decibels(), Gain::LARGEST_DECIBELS);
        assert_eq!(Gain::from_decibels(-500.0).decibels(), Gain::SMALLEST_DECIBELS);
        assert_eq!(Gain::from_decibels(f32::NAN), Gain::UNCHANGED);
        assert_eq!(Gain::from_decibels(f32::INFINITY).decibels(), Gain::LARGEST_DECIBELS);
        assert_eq!(
            Gain::from_decibels(f32::NEG_INFINITY).decibels(),
            Gain::SMALLEST_DECIBELS
        );
        assert_eq!(
            Gain::from_decibels(7.25).decibels(),
            7.25,
            "what is allowed is kept exactly"
        );
    }

    #[test]
    fn the_range_is_the_same_distance_either_side_of_no_change_so_that_zero_is_the_middle_of_a_slider() {
        assert_eq!(Gain::SMALLEST_DECIBELS, -Gain::LARGEST_DECIBELS);
    }

    #[test]
    fn two_gains_applied_together_add_their_decibels_even_beyond_what_one_can_be() {
        let loud = Gain::from_decibels(30.0);
        let combined = loud.combined_linear_factor(loud);
        assert!(
            (combined - 1000.0).abs() < 1e-6,
            "+60 decibels is 1000 times: {combined}"
        );
        let cancelling = Gain::from_decibels(12.0).combined_linear_factor(Gain::from_decibels(-12.0));
        assert!((cancelling - 1.0).abs() < 1e-12);
        assert_eq!(
            Gain::UNCHANGED.combined_linear_factor(Gain::UNCHANGED),
            1.0,
            "exactly one, so that nothing needs to be done to the sound"
        );
    }

    #[test]
    fn it_is_written_as_a_plain_number_of_decibels_and_read_back_inside_the_allowed_range() {
        assert_eq!(serde_json::to_string(&Gain::from_decibels(-3.5)).unwrap(), "-3.5");
        assert_eq!(serde_json::from_str::<Gain>("12.0").unwrap(), Gain::from_decibels(12.0));
        assert_eq!(
            serde_json::from_str::<Gain>("999").unwrap().decibels(),
            Gain::LARGEST_DECIBELS
        );
        assert!(serde_json::from_str::<Gain>("\"loud\"").is_err());
    }

    #[test]
    fn it_is_shown_with_its_sign_and_one_decimal() {
        assert_eq!(Gain::from_decibels(3.0).to_string(), "+3.0 dB");
        assert_eq!(Gain::from_decibels(-12.25).to_string(), "-12.2 dB");
        assert_eq!(Gain::UNCHANGED.to_string(), "+0.0 dB");
    }
}
