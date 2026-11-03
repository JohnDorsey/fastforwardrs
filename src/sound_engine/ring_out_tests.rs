// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the meter of a ring says of effects whose ring is known (docs/engine-phase-15-vst3-design.md, section 16): echoes of a known length and decay, a gain, a hum, a delay with a
//! long way to go, an effect that never stops, and one that delays the sound.

use super::ring_out::{
    DECIBELS_THAT_IS_SILENCE, MOST_SECONDS_MEASURED, RingOut, SECONDS_OF_BURST, SECONDS_OF_QUIET_THAT_ENDS_A_RING,
    measure_the_ring_out,
};
use super::{BlockProcessor, SAMPLE_RATE};

const SECOND: u64 = SAMPLE_RATE as u64;

/// A comb: what comes out is what goes in plus `feedback` times what came out `delay` samples ago (an echo that dies away by `feedback` at each turn), plus `hum` that is there
/// whatever goes in.
struct Comb {
    delay: usize,
    feedback: f32,
    hum: f32,
    remembered: [Vec<f32>; 2],
    at: usize,
}

impl Comb {
    fn new(delay: usize, feedback: f32) -> Self {
        Self {
            delay,
            feedback,
            hum: 0.0,
            remembered: [vec![0.0; delay.max(1)], vec![0.0; delay.max(1)]],
            at: 0,
        }
    }
}

impl BlockProcessor for Comb {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (channel, samples) in [left, right].into_iter().enumerate() {
            let mut at = self.at;
            for sample in samples.iter_mut() {
                let out = *sample + self.remembered[channel][at] * self.feedback;
                self.remembered[channel][at] = if self.delay == 0 { 0.0 } else { out };
                at = (at + 1) % self.remembered[channel].len();
                *sample = out + self.hum;
            }
            if channel == 1 {
                self.at = at;
            }
        }
    }

    fn reset(&mut self) {
        panic!("a meter does not reset an effect: it runs it from the start");
    }
}

/// An effect that makes a copy of what goes in, `delay` samples later, at `gain`, and nothing else (a delay that is all wet, with one echo).
struct Wet {
    gain: f32,
    remembered: [std::collections::VecDeque<f32>; 2],
}

impl Wet {
    fn new(delay: usize, gain: f32) -> Self {
        Self {
            gain,
            remembered: std::array::from_fn(|_| std::iter::repeat_n(0.0, delay).collect()),
        }
    }
}

impl BlockProcessor for Wet {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (channel, samples) in [left, right].into_iter().enumerate() {
            for sample in samples {
                self.remembered[channel].push_back(*sample);
                *sample = self.remembered[channel].pop_front().unwrap() * self.gain;
            }
        }
    }

    fn reset(&mut self) {
        panic!("a meter does not reset an effect");
    }
}

/// The dry sound at `dry` and a copy of it `delay` samples later at `echo`.
struct DryAndOneEcho {
    wet: Wet,
    dry: f32,
}

impl BlockProcessor for DryAndOneEcho {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let (mut echo_left, mut echo_right) = (left.to_vec(), right.to_vec());
        self.wet.process(&mut echo_left, &mut echo_right);
        for (sample, echo) in left.iter_mut().zip(&echo_left) {
            *sample = *sample * self.dry + echo;
        }
        for (sample, echo) in right.iter_mut().zip(&echo_right) {
            *sample = *sample * self.dry + echo;
        }
    }

    fn reset(&mut self) {
        panic!("a meter does not reset an effect");
    }
}

/// An effect that does nothing to the sound, and says it makes it `latency` samples later (the graph makes up for it).
struct Late {
    inner: Wet,
    latency: usize,
}

impl BlockProcessor for Late {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.inner.process(left, right);
    }

    fn reset(&mut self) {}

    fn latency(&self) -> usize {
        self.latency
    }
}

/// An effect that counts the samples it is given and the blocks they came in.
struct Counting {
    samples: u64,
    longest_block: usize,
}

impl BlockProcessor for Counting {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        assert_eq!(left.len(), right.len(), "the two channels are as long as each other");
        self.samples += left.len() as u64;
        self.longest_block = self.longest_block.max(left.len());
    }

    fn reset(&mut self) {}
}

fn seconds(ring: &RingOut) -> f64 {
    ring.samples as f64 / f64::from(SAMPLE_RATE)
}

#[test]
fn an_echo_that_dies_away_rings_for_as_long_as_it_takes_to_be_a_millionth_of_the_power_it_began_with() {
    // 0.1 s between echoes, each half as loud (6 dB less) as the one before: 60 dB under in about ten turns, a second.
    let ring = measure_the_ring_out(&mut Comb::new(4_800, 0.5));
    assert!(!ring.is_still_ringing);
    assert!(
        (0.9..=1.1).contains(&seconds(&ring)),
        "about a second for sixty decibels at six decibels in a tenth of a second: {}",
        seconds(&ring)
    );
    // A third of the turns: the same decay in a third of the time (a quarter as loud, 12 dB).
    let quicker = measure_the_ring_out(&mut Comb::new(4_800, 0.25));
    assert!((0.4..=0.6).contains(&seconds(&quicker)), "{}", seconds(&quicker));
    // Ten times the time at the same decay per turn: ten times the ring.
    let slower = measure_the_ring_out(&mut Comb::new(48_000, 0.5));
    assert!((9.0..=11.0).contains(&seconds(&slower)), "{}", seconds(&slower));
}

#[test]
fn an_effect_that_makes_nothing_after_its_input_ends_has_no_ring() {
    let mut no_effect = Wet::new(0, 1.0);
    assert_eq!(
        measure_the_ring_out(&mut no_effect),
        RingOut {
            samples: 0,
            is_still_ringing: false
        }
    );
}

#[test]
fn a_hum_that_is_there_whatever_goes_in_is_not_a_ring() {
    let mut humming = Comb::new(0, 0.0);
    humming.hum = 0.3;
    assert_eq!(
        measure_the_ring_out(&mut humming),
        RingOut {
            samples: 0,
            is_still_ringing: false
        },
        "a constant is not heard as ringing on: the meter listens to what changes"
    );
}

#[test]
fn an_echo_that_comes_late_after_a_long_quiet_is_a_ring_that_lasts_until_it_ends() {
    // A copy of the burst three seconds later, and nothing in between: the quiet is not the end.
    let ring = measure_the_ring_out(&mut Wet::new(3 * SECOND as usize, 1.0));
    assert!(
        (2.95..=3.05).contains(&seconds(&ring)),
        "the copy ends three seconds after the burst does: {}",
        seconds(&ring)
    );
    // ... and one that comes later than the quiet that the meter waits for is not heard (it is a delay of more than anyone sets for a tail, and the maintainer can say).
    let too_late = measure_the_ring_out(&mut Wet::new(8 * SECOND as usize, 1.0));
    assert_eq!(too_late.samples, 0, "the meter gave up on the quiet before it came");
}

#[test]
fn an_effect_that_does_not_die_away_in_the_most_seconds_that_are_measured_rings_for_that_long_and_says_it_is_still_ringing()
 {
    let ring = measure_the_ring_out(&mut Comb::new(4_800, 1.0));
    assert!(ring.is_still_ringing);
    assert_eq!(
        ring.samples,
        MOST_SECONDS_MEASURED * SECOND,
        "the most seconds that are measured, counted from where the input ended"
    );
}

#[test]
fn a_ring_is_measured_against_the_loudest_the_effect_is_and_not_against_the_input() {
    // The dry sound at full, and a copy a second later at 0.01 (40 dB under): heard.
    let heard = measure_the_ring_out(&mut DryAndOneEcho {
        wet: Wet::new(SECOND as usize, 0.01),
        dry: 1.0,
    });
    assert!((0.95..=1.05).contains(&seconds(&heard)), "{}", seconds(&heard));
    // ... at 0.0005 (66 dB under) it is under the sixty decibels that count as silence.
    const { assert!(DECIBELS_THAT_IS_SILENCE < 66.0) };
    let not_heard = measure_the_ring_out(&mut DryAndOneEcho {
        wet: Wet::new(SECOND as usize, 0.0005),
        dry: 1.0,
    });
    assert_eq!(not_heard.samples, 0);
}

#[test]
fn the_latency_of_an_effect_is_not_part_of_its_ring() {
    // The sound comes out 4800 samples late and the graph makes up for it: the ring is what comes after the sound ends where the graph puts it.
    let mut late = Late {
        inner: Wet::new(4_800, 1.0),
        latency: 4_800,
    };
    assert_eq!(measure_the_ring_out(&mut late).samples, 0);
}

#[test]
fn the_effect_is_run_on_whole_blocks_of_both_channels_and_for_no_longer_than_the_burst_and_the_quiet_that_ends_a_ring()
{
    let mut counting = Counting {
        samples: 0,
        longest_block: 0,
    };
    let ring = measure_the_ring_out(&mut counting);
    assert_eq!(ring.samples, 0);
    assert!(counting.longest_block <= 4_096, "{}", counting.longest_block);
    let burst_and_quiet = ((SECONDS_OF_BURST + SECONDS_OF_QUIET_THAT_ENDS_A_RING) * f64::from(SAMPLE_RATE)) as u64;
    assert!(
        (burst_and_quiet..burst_and_quiet + 4_096).contains(&counting.samples),
        "a burst and five seconds of quiet, to a block: {}",
        counting.samples
    );
}

/// Some copies of the input, each later than the one before, added together.
struct SomeEchoes(Vec<Wet>);

impl BlockProcessor for SomeEchoes {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let (heard_left, heard_right) = (left.to_vec(), right.to_vec());
        left.fill(0.0);
        right.fill(0.0);
        for echo in &mut self.0 {
            let (mut echo_left, mut echo_right) = (heard_left.clone(), heard_right.clone());
            echo.process(&mut echo_left, &mut echo_right);
            for (sample, made) in left.iter_mut().zip(&echo_left) {
                *sample += made;
            }
            for (sample, made) in right.iter_mut().zip(&echo_right) {
                *sample += made;
            }
        }
    }

    fn reset(&mut self) {
        panic!("a meter does not reset an effect");
    }
}

#[test]
fn echoes_with_less_quiet_between_them_than_the_end_of_a_ring_are_one_ring_however_many_there_are() {
    // Echoes two, six and ten seconds after the burst begins: 3.5 seconds of quiet between each and the next, which is not the five seconds that end a ring.
    let echoes = [2, 6, 10].map(|seconds| Wet::new(seconds * SECOND as usize, 1.0));
    let ring = measure_the_ring_out(&mut SomeEchoes(echoes.into()));
    assert!(
        (9.95..=10.05).contains(&seconds(&ring)),
        "the last echo ends ten seconds after the burst does: {}",
        seconds(&ring)
    );
}
