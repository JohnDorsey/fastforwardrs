// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Measuring how long an effect rings on (docs/engine-phase-15-vst3-design.md, section 16): the effect is run on **half a second of noise and then silence**, and what comes out is
//! listened to until it has been quiet for long enough.
//!
//! * **The ring is what is heard after the input ends**: from the end of the noise to the end of the last stretch of sound that is louder than **60 decibels under the loudest the
//!   effect was** (the noise going through it, dry and wet, is the loudest, normally). 60 dB is what a reverb's decay time means, and under it is under what a clip's own sound hides.
//! * **Sound is told from a hum by what changes in it**: the power of a stretch of ten milliseconds is that of what is left when its average is taken off, so that a constant
//!   (an effect that makes a level of its own whatever goes in) is not a ring that never ends.
//! * **Quiet is not the end until it has lasted [`SECONDS_OF_QUIET_THAT_ENDS_A_RING`]**: a delay has nothing between its echoes. Past that nothing is waited for.
//! * **An effect that is still loud [`MOST_SECONDS_MEASURED`] seconds after its input ended** rings that long and says it is [still ringing](RingOut::is_still_ringing).
//! * **The latency of the effect is not part of the ring**: the graph makes up for it (see [`BlockProcessor::latency`]), so the sound is where the input is.
//!
//! This makes the sound of the effect and listens to it and does nothing else: the effect is made by whoever has it (`plugin_host::ring_outs`), as it is in the preview.

use super::{BlockProcessor, SAMPLE_RATE};

/// How long the noise that the effect is given lasts. (Long enough for a reverb to be full, as a clip of sound is; short enough that what rings on is told from it.)
pub const SECONDS_OF_BURST: f64 = 0.5;

/// The most seconds after the input ends that an effect is run for: the longest tail that the program measures (the menu of the tail goes to the same).
pub const MOST_SECONDS_MEASURED: u64 = 30;

/// How long a quiet lasts before it is the end of the ring.
pub const SECONDS_OF_QUIET_THAT_ENDS_A_RING: f64 = 5.0;

/// How far under the loudest the effect was a stretch of sound is when it is no longer counted as ringing. 60 decibels of power: a millionth.
pub const DECIBELS_THAT_IS_SILENCE: f64 = 60.0;

/// How many samples are listened to at once: ten milliseconds.
const SAMPLES_IN_A_WINDOW: usize = SAMPLE_RATE as usize / 100;

/// How many samples the effect is given at once: eight windows (a block as plugins are given blocks, and no longer than the graph gives them).
const SAMPLES_IN_A_BLOCK: usize = 8 * SAMPLES_IN_A_WINDOW;

/// How long an effect was found to ring on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingOut {
    /// The samples from where the input ended to where the effect was last louder than a millionth of its loudest power, to a window; at most [`MOST_SECONDS_MEASURED`] seconds.
    pub samples: u64,
    /// Whether the effect was still that loud when it was stopped: it does not die away in the time that is measured (a feedback that does not lose anything), and `samples` is the most.
    pub is_still_ringing: bool,
}

/// Runs `processor`, which is new (it has heard nothing), on noise and then on silence, and says how long it rings on after the noise.
pub fn measure_the_ring_out(processor: &mut dyn BlockProcessor) -> RingOut {
    let burst = (SECONDS_OF_BURST * f64::from(SAMPLE_RATE)) as usize;
    let latency = processor.latency();
    let end_of_the_input = burst + latency;
    let most_windows = (end_of_the_input + MOST_SECONDS_MEASURED as usize * SAMPLE_RATE as usize) / SAMPLES_IN_A_WINDOW;
    let quiet_that_ends_a_ring =
        (SECONDS_OF_QUIET_THAT_ENDS_A_RING * f64::from(SAMPLE_RATE)) as usize / SAMPLES_IN_A_WINDOW;
    let silent = power_ratio_of(DECIBELS_THAT_IS_SILENCE);

    let mut noise = [Noise(0x9E37_79B9), Noise(0x85EB_CA6B)];
    let mut powers: Vec<f64> = Vec::new();
    let (mut loudest, mut quiet_windows) = (0.0_f64, 0_usize);
    let mut given = 0_usize;
    let (mut left, mut right) = (vec![0.0_f32; SAMPLES_IN_A_BLOCK], vec![0.0_f32; SAMPLES_IN_A_BLOCK]);
    let mut stopped_by_the_end = true;
    while powers.len() < most_windows {
        let length = SAMPLES_IN_A_BLOCK.min(most_windows * SAMPLES_IN_A_WINDOW - given);
        let (left, right) = (&mut left[..length], &mut right[..length]);
        for (channel, samples) in [&mut *left, &mut *right].into_iter().enumerate() {
            for (place, sample) in samples.iter_mut().enumerate() {
                *sample = if given + place < burst {
                    noise[channel].next()
                } else {
                    0.0
                };
            }
        }
        processor.process(left, right);
        for window in 0..length / SAMPLES_IN_A_WINDOW {
            let range = window * SAMPLES_IN_A_WINDOW..(window + 1) * SAMPLES_IN_A_WINDOW;
            let power = power_of(&left[range.clone()]) + power_of(&right[range]);
            loudest = loudest.max(power);
            quiet_windows = if power > loudest * silent { 0 } else { quiet_windows + 1 };
            powers.push(power);
        }
        given += length;
        if given >= end_of_the_input && quiet_windows >= quiet_that_ends_a_ring {
            stopped_by_the_end = false;
            break;
        }
    }

    let threshold = loudest * silent;
    let Some(last_loud) = powers
        .iter()
        .rposition(|power| *power > threshold)
        .filter(|_| loudest > 0.0)
    else {
        return RingOut {
            samples: 0,
            is_still_ringing: false,
        };
    };
    let end_of_the_ring = (last_loud + 1) * SAMPLES_IN_A_WINDOW;
    RingOut {
        samples: end_of_the_ring.saturating_sub(end_of_the_input) as u64,
        is_still_ringing: stopped_by_the_end && last_loud + 1 == powers.len(),
    }
}

/// The ratio of powers that `decibels` decibels under is.
fn power_ratio_of(decibels: f64) -> f64 {
    10.0_f64.powf(-decibels / 10.0)
}

/// The power of the change in a stretch of samples: the squares of the samples with their average taken off.
fn power_of(samples: &[f32]) -> f64 {
    let average = samples.iter().map(|sample| f64::from(*sample)).sum::<f64>() / samples.len() as f64;
    samples
        .iter()
        .map(|sample| (f64::from(*sample) - average).powi(2))
        .sum()
}

/// Noise that is the same every time (a measurement of the same effect is the same number), as loud as a quarter of the loudest sound there is (12 dB under it, as music is).
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        // (xorshift: its period is not short, and its numbers are not related for this.)
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0) * 0.25
    }
}
