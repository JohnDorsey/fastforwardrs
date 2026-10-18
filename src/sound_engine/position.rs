// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where a frame of the timeline is, in samples of sound.

use super::SAMPLE_RATE;
use crate::project::FrameRate;

/// The number of the sample that frame number `frame` begins at: the moment the frame starts at, in samples, **rounded to the nearest sample** (half a sample is
/// rounded up). Exact integer arithmetic, and worked out from the frame itself, so that 30000 frames of 29.97 frames a second are exactly 1001 seconds of sound and
/// there is no drift to add up item by item. (Where a frame is a whole number of samples, which it is at 24, 25, 30, 50 and 60 frames a second, this is exact.)
pub fn sample_at_frame(frame_rate: FrameRate, frame: u64) -> u64 {
    let numerator = u128::from(frame_rate.numerator());
    let twice_the_samples_times_the_numerator =
        2 * u128::from(frame) * u128::from(frame_rate.denominator()) * u128::from(SAMPLE_RATE);
    ((twice_the_samples_times_the_numerator + numerator) / (2 * numerator)) as u64
}
