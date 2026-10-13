//! The pieces that the editor's sound effects are made from: noise, filters that take the high or the low parts out of it, and
//! the shaping of the end and the loudness of a sound. (Every effect is made from nothing: the editor has no sound files of its
//! own.) Everything here gives the same result every time, so the effects do.

use std::f32::consts::{PI, TAU};

/// White noise: a different value between -1 and 1 each time, in an order that is always the same.
pub(super) struct Noise {
    state: u32,
}

impl Noise {
    pub(super) fn new() -> Self {
        Self { state: 0x2545_F491 }
    }

    /// The next value (a xorshift generator).
    pub(super) fn next(&mut self) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        self.state as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
}

/// Two simple filters one after the other, which let through what is higher than a pitch and take out what
/// is lower (more and more of it the lower it is).
pub(super) struct HighPass {
    /// How much of the difference from what the low parts have followed is followed on each moment.
    follow: f32,
    first_low: f32,
    second_low: f32,
}

impl HighPass {
    pub(super) fn new(lowest_pitch_hertz: f32, sample_rate: f32) -> Self {
        Self {
            follow: 1.0 - (-TAU * lowest_pitch_hertz / sample_rate).exp(),
            first_low: 0.0,
            second_low: 0.0,
        }
    }

    pub(super) fn filter(&mut self, value: f32) -> f32 {
        self.first_low += self.follow * (value - self.first_low);
        let without_the_lowest = value - self.first_low;
        self.second_low += self.follow * (without_the_lowest - self.second_low);
        without_the_lowest - self.second_low
    }
}

/// Two simple filters one after the other, which let through what is lower than a pitch and take out what
/// is higher (more and more of it the higher it is): the other way round from [`HighPass`].
pub(super) struct LowPass {
    /// How much of the difference from what has been let through so far is followed on each moment.
    follow: f32,
    first: f32,
    second: f32,
}

impl LowPass {
    pub(super) fn new(highest_pitch_hertz: f32, sample_rate: f32) -> Self {
        Self {
            follow: 1.0 - (-TAU * highest_pitch_hertz / sample_rate).exp(),
            first: 0.0,
            second: 0.0,
        }
    }

    pub(super) fn filter(&mut self, value: f32) -> f32 {
        self.first += self.follow * (value - self.first);
        self.second += self.follow * (self.first - self.second);
        self.second
    }
}

/// How loud a note of `pitch_hertz` that dies away is `seconds` after it began, when it is `loudness` at first and takes
/// `seconds_to_fade` to become `1/e` (about 0.37) of that: the ring of something that has been struck.
pub(super) fn ring(pitch_hertz: f32, loudness: f32, seconds_to_fade: f32, seconds: f32) -> f32 {
    loudness * (TAU * pitch_hertz * seconds).sin() * (-seconds / seconds_to_fade).exp()
}

/// Makes the last `seconds` of `sound` (which is for speakers that play `sample_rate` moments a second) die away to nothing, along
/// half a wave of cosine.
pub(super) fn fade_out(sound: &mut [f32], seconds: f32, sample_rate: f32) {
    let fade_length = ((seconds * sample_rate) as usize).min(sound.len());
    let fade_start = sound.len() - fade_length;
    for (position, value) in sound[fade_start..].iter_mut().enumerate() {
        let how_far = (position + 1) as f32 / fade_length as f32;
        *value *= 0.5 + 0.5 * (PI * how_far).cos();
    }
}

/// Makes the loudest moment of `sound` exactly `peak_loudness` (a fraction of the loudest sound the speakers can make).
pub(super) fn scale_to_the_peak(sound: &mut [f32], peak_loudness: f32) {
    let peak = sound.iter().fold(0.0_f32, |peak, value| peak.max(value.abs()));
    if peak > 0.0 {
        let scale = peak_loudness / peak;
        sound.iter_mut().for_each(|value| *value *= scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn energy(sound: &[f32]) -> f32 {
        sound.iter().map(|value| value * value).sum()
    }

    /// `seconds` of noise through `filter`, at `rate`.
    fn filtered_noise(rate: f32, seconds: f32, mut filter: impl FnMut(f32) -> f32) -> Vec<f32> {
        let mut noise = Noise::new();
        (0..(seconds * rate) as usize).map(|_| filter(noise.next())).collect()
    }

    /// How strong a pure note of `pitch_hertz` is in `sound`: the energy of what in it goes along with that note.
    fn strength_of_the_note(sound: &[f32], pitch_hertz: f32, rate: f32) -> f32 {
        let (mut cosine, mut sine) = (0.0_f64, 0.0_f64);
        for (moment, value) in sound.iter().enumerate() {
            let angle = std::f64::consts::TAU * f64::from(pitch_hertz) * moment as f64 / f64::from(rate);
            cosine += f64::from(*value) * angle.cos();
            sine += f64::from(*value) * angle.sin();
        }
        ((cosine * cosine + sine * sine) / sound.len() as f64) as f32
    }

    /// A note of `pitch_hertz` (which the length of a second at `rate` is a whole number of waves of, or nearly) through `filter`.
    fn note_through(rate: f32, pitch_hertz: f32, mut filter: impl FnMut(f32) -> f32) -> Vec<f32> {
        (0..4800)
            .map(|moment| filter((TAU * pitch_hertz * moment as f32 / rate).sin()))
            .collect()
    }

    #[test]
    fn the_noise_is_spread_over_its_range_and_does_not_repeat_at_once() {
        let mut noise = Noise::new();
        let values: Vec<f32> = (0..10_000).map(|_| noise.next()).collect();
        assert!(values.iter().all(|value| (-1.0..=1.0).contains(value)));
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        assert!(mean.abs() < 0.03, "mean {mean}");
        let mean_square = energy(&values) / values.len() as f32;
        assert!(
            (mean_square - 1.0 / 3.0).abs() < 0.03,
            "{mean_square}: uniform noise has 1/3"
        );
        assert_ne!(values[..100], values[100..200]);
    }

    #[test]
    fn the_noise_is_the_same_every_time() {
        let (mut first, mut second) = (Noise::new(), Noise::new());
        assert!((0..1000).all(|_| first.next() == second.next()));
    }

    #[test]
    fn a_low_pass_lets_a_low_note_through_and_takes_a_high_one_out() {
        let rate = 48000.0;
        let mut low_pass = LowPass::new(500.0, rate);
        let low_note = note_through(rate, 100.0, |value| low_pass.filter(value));
        let mut low_pass = LowPass::new(500.0, rate);
        let high_note = note_through(rate, 5000.0, |value| low_pass.filter(value));

        // (After the filter has settled: the second half.)
        let low_energy = energy(&low_note[2400..]);
        let high_energy = energy(&high_note[2400..]);
        assert!(
            low_energy > 0.7 * 2400.0 / 2.0,
            "a note two octaves under it is let through: {low_energy}"
        );
        assert!(
            high_energy < 0.01 * low_energy,
            "a note a decade above it is taken out: {high_energy}"
        );
    }

    #[test]
    fn a_high_pass_lets_a_high_note_through_and_takes_a_low_one_out() {
        let rate = 48000.0;
        let mut high_pass = HighPass::new(2000.0, rate);
        let high_note = note_through(rate, 12000.0, |value| high_pass.filter(value));
        let mut high_pass = HighPass::new(2000.0, rate);
        let low_note = note_through(rate, 200.0, |value| high_pass.filter(value));

        assert!(energy(&low_note[2400..]) < 0.02 * energy(&high_note[2400..]));
    }

    #[test]
    fn noise_through_a_low_pass_has_little_above_the_pitch_it_lets_through_and_a_lot_below() {
        let rate = 48000.0;
        let mut low_pass = LowPass::new(1000.0, rate);
        let sound = filtered_noise(rate, 0.2, |value| low_pass.filter(value));

        let low = strength_of_the_note(&sound, 300.0, rate);
        let high = strength_of_the_note(&sound, 8000.0, rate);

        assert!(high < 0.01 * low, "300 Hz is {low}, 8000 Hz is {high}");
    }

    #[test]
    fn a_ring_begins_at_nothing_and_dies_away_by_the_time_it_says() {
        assert_eq!(ring(440.0, 1.0, 0.05, 0.0), 0.0);
        // A quarter of a wave in, a note of 250 Hz is at the top of its swing (and has faded a little, which is what is checked).
        let top = ring(250.0, 0.8, 0.05, 0.001);
        assert!((top - 0.8 * (-0.001_f32 / 0.05).exp()).abs() < 1e-5, "{top}");
        // Six times the time it takes to fade to 1/e later, no swing is more than 1/e to the sixth of the loudness.
        let loudest_later = (0..200)
            .map(|step| ring(250.0, 0.8, 0.05, 0.3 + step as f32 / 48000.0).abs())
            .fold(0.0, f32::max);
        assert!(loudest_later < 0.8 * (-6.0_f32).exp() * 1.001, "{loudest_later}");
    }

    #[test]
    fn fading_out_ends_in_silence_and_leaves_what_comes_before_it_alone() {
        let mut sound = vec![1.0_f32; 1000];

        fade_out(&mut sound, 0.01, 48000.0);

        let fade_length = 480;
        assert!(sound[..1000 - fade_length].iter().all(|value| *value == 1.0));
        assert!(sound.last().unwrap().abs() < 1e-6, "ends in silence");
        assert!(
            sound[1000 - fade_length..].windows(2).all(|pair| pair[1] <= pair[0]),
            "it only goes down"
        );
    }

    #[test]
    fn a_fade_out_longer_than_the_sound_fades_all_of_it() {
        let mut sound = vec![1.0_f32; 10];
        fade_out(&mut sound, 1.0, 48000.0);
        assert!(sound[0] < 1.0 && sound[0] > 0.9, "{}", sound[0]);
        assert!(sound[9].abs() < 1e-6);
    }

    #[test]
    fn scaling_to_the_peak_makes_the_loudest_moment_exactly_that_and_keeps_the_shape() {
        let mut sound = vec![0.1, -0.4, 0.2, 0.05];

        scale_to_the_peak(&mut sound, 0.2);

        assert!((sound[1] + 0.2).abs() < 1e-7, "{sound:?}");
        assert!((sound[0] - 0.05).abs() < 1e-7 && (sound[2] - 0.1).abs() < 1e-7);
    }

    #[test]
    fn a_sound_of_nothing_is_left_as_it_is_when_scaled() {
        let mut sound = vec![0.0; 5];
        scale_to_the_peak(&mut sound, 0.3);
        assert_eq!(sound, vec![0.0; 5]);
    }
}
