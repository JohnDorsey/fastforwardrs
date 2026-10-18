// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! How loud a clip is, sample by sample.

use super::SAMPLE_RATE;
use crate::media::export::LoudnessToExport;
use crate::project::FrameRate;

/// The number a clip's samples are multiplied by, at every sample of it.
///
/// A constant loudness is one number. A changing one is given in decibels at some frames, between which it changes **evenly in decibels, and so evenly in time**; before
/// the first and after the last it is as at that one. The decibels become a number by `10^(decibels / 20)` at every sample (in `f64`, then multiplied in `f32`).
#[derive(Clone, Debug, PartialEq)]
pub struct GainCurve {
    shape: Shape,
}

#[derive(Clone, Debug, PartialEq)]
enum Shape {
    Constant(f32),
    Changing {
        /// The moments, in seconds from the start of the clip, and the decibels at them: at least two, in order.
        times: Vec<f64>,
        decibels: Vec<f64>,
    },
}

impl GainCurve {
    pub fn new(loudness: &LoudnessToExport, frame_rate: FrameRate) -> Self {
        let shape = match loudness {
            LoudnessToExport::Constant(factor) => Shape::Constant(*factor as f32),
            LoudnessToExport::Changing(points) => {
                assert!(points.len() >= 2, "a changing loudness has at least two points");
                Shape::Changing {
                    times: points
                        .iter()
                        .map(|&(frame, _)| frame_rate.seconds_at_frame(frame))
                        .collect(),
                    decibels: points.iter().map(|&(_, decibels)| f64::from(decibels)).collect(),
                }
            }
        };
        Self { shape }
    }

    /// Whether the clip is as loud as its file (nothing is done to it).
    pub fn changes_nothing(&self) -> bool {
        matches!(self.shape, Shape::Constant(factor) if factor == 1.0)
    }

    /// The number that sample number `sample` of the clip (counted from the first sample of the clip) is multiplied by. This is the arithmetic; [`Self::apply`] does the same
    /// for a stretch of samples, quicker, and the tests hold the one to the other.
    pub fn factor_at_sample(&self, sample: u64) -> f64 {
        match &self.shape {
            Shape::Constant(factor) => f64::from(*factor),
            Shape::Changing { times, decibels } => {
                let time = sample as f64 / f64::from(SAMPLE_RATE);
                let segment = times.partition_point(|point| *point <= time).saturating_sub(1);
                factor_of_decibels(decibels_in_segment(times, decibels, segment, time))
            }
        }
    }

    /// Multiplies the samples of `left` and `right` (as long as each other), which are samples number `first_sample`, `first_sample + 1`... of the clip.
    pub fn apply(&mut self, first_sample: u64, left: &mut [f32], right: &mut [f32]) {
        match &mut self.shape {
            Shape::Constant(factor) => {
                if *factor != 1.0 {
                    for sample in left.iter_mut().chain(right.iter_mut()) {
                        *sample *= *factor;
                    }
                }
            }
            Shape::Changing { times, decibels } => {
                let first_time = first_sample as f64 / f64::from(SAMPLE_RATE);
                // (The segment moves forward as the samples go by.)
                let mut segment = times.partition_point(|point| *point <= first_time).saturating_sub(1);
                for (place, (left, right)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
                    let time = (first_sample + place as u64) as f64 / f64::from(SAMPLE_RATE);
                    while segment + 2 < times.len() && times[segment + 1] <= time {
                        segment += 1;
                    }
                    let factor = factor_of_decibels(decibels_in_segment(times, decibels, segment, time)) as f32;
                    *left *= factor;
                    *right *= factor;
                }
            }
        }
    }
}

/// How loud, in decibels, it is at `time` if the segment that begins at point `segment` is the one `time` is in (before the first point and after the last it is as at that one).
fn decibels_in_segment(times: &[f64], decibels: &[f64], segment: usize, time: f64) -> f64 {
    let last = times.len() - 1;
    if time <= times[0] {
        return decibels[0];
    }
    if time >= times[last] {
        return decibels[last];
    }
    let segment = segment.min(last - 1);
    let (from_time, to_time) = (times[segment], times[segment + 1]);
    let (from_decibels, to_decibels) = (decibels[segment], decibels[segment + 1]);
    from_decibels + (to_decibels - from_decibels) * (time - from_time) / (to_time - from_time)
}

fn factor_of_decibels(decibels: f64) -> f64 {
    10f64.powf(decibels / 20.0)
}
