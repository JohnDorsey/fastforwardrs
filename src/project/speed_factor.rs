//! How many times faster than normal a clip plays.

use std::fmt;
use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A whole-number speed-up of at least 1: 1 is normal speed, 4 shows every fourth frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct SpeedFactor(NonZeroU32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("a speed factor must be a whole number of at least 1, but it was 0")]
pub struct ZeroSpeedFactor;

impl SpeedFactor {
    /// Normal speed: the clip is not accelerated.
    pub const NORMAL: SpeedFactor = SpeedFactor(NonZeroU32::MIN);

    /// Creates a speed factor, or `None` if `factor` is zero.
    pub fn new(factor: u32) -> Option<Self> {
        NonZeroU32::new(factor).map(Self)
    }

    pub fn value(self) -> u32 {
        self.0.get()
    }

    /// Whether the clip plays faster than normal (and so gets the fast-forward symbol).
    pub fn is_accelerated(self) -> bool {
        self.value() > 1
    }
}

impl Default for SpeedFactor {
    fn default() -> Self {
        Self::NORMAL
    }
}

impl fmt::Display for SpeedFactor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}x", self.value())
    }
}

impl TryFrom<u32> for SpeedFactor {
    type Error = ZeroSpeedFactor;

    fn try_from(factor: u32) -> Result<Self, Self::Error> {
        Self::new(factor).ok_or(ZeroSpeedFactor)
    }
}

impl From<SpeedFactor> for u32 {
    fn from(speed_factor: SpeedFactor) -> Self {
        speed_factor.value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_not_a_speed_factor() {
        assert_eq!(SpeedFactor::new(0), None);
        assert_eq!(SpeedFactor::try_from(0), Err(ZeroSpeedFactor));
    }

    #[test]
    fn only_factors_above_one_count_as_accelerated() {
        assert!(!SpeedFactor::NORMAL.is_accelerated());
        assert!(!SpeedFactor::new(1).unwrap().is_accelerated());
        assert!(SpeedFactor::new(2).unwrap().is_accelerated());
        assert!(SpeedFactor::new(1000).unwrap().is_accelerated());
    }

    #[test]
    fn displays_the_number_followed_by_an_x() {
        assert_eq!(SpeedFactor::new(4).unwrap().to_string(), "4x");
        assert_eq!(SpeedFactor::NORMAL.to_string(), "1x");
    }

    #[test]
    fn serialises_as_a_plain_number_and_rejects_zero_when_loading() {
        let factor = SpeedFactor::new(8).unwrap();
        assert_eq!(serde_json::to_string(&factor).unwrap(), "8");
        assert_eq!(serde_json::from_str::<SpeedFactor>("8").unwrap(), factor);
        assert!(serde_json::from_str::<SpeedFactor>("0").is_err());
        assert!(serde_json::from_str::<SpeedFactor>("-2").is_err());
        assert!(serde_json::from_str::<SpeedFactor>("2.5").is_err());
    }
}
