//! The sound of a cut: the snip of scissors, made from nothing (the editor has no sound files of its own).
//!
//! Scissors close with a quick slide of the blades, "shh", that ends in the hard little knock of the blades
//! meeting, "tk", with a bright ring after it. Here the slide is a swell of high-pitched noise, the knock is a
//! click of noise, and the ring is a few pitches that are not multiples of one another (as with anything metal).
//! Everything is high: there is nothing below a few hundred hertz to make it boom on a loud computer.
//!
//! The noise comes from a generator that always starts the same, so the sound is the same every time.

use std::f32::consts::TAU;

use super::synthesis::{HighPass, Noise, fade_out, scale_to_the_peak};

/// How long the whole sound lasts, in seconds.
pub(super) const DURATION_SECONDS: f32 = 0.14;

/// How loud the loudest moment is, as a fraction of the loudest sound the speakers can make. (The sound is
/// scaled to exactly this, so it does not depend on how the noise happened to come out.)
pub(super) const PEAK_LOUDNESS: f32 = 0.45;

/// How long the slide of the blades lasts, in seconds, before they meet.
const SLIDE_SECONDS: f32 = 0.045;
/// The slide is noise with what is lower than this pitch (in hertz) taken out of it.
const SLIDE_LOWEST_PITCH_HERTZ: f32 = 3500.0;
/// How loud the slide is when it ends, compared with the knock.
const SLIDE_LOUDNESS: f32 = 0.5;

/// How long the click of the knock lasts, in seconds (the time for it to be a tenth of what it was).
const KNOCK_SECONDS: f32 = 0.003;
const KNOCK_LOUDNESS: f32 = 1.0;

/// The ring after the knock: for each of its pitches, its pitch in hertz, how loud it is, and how many seconds
/// it takes to become `1/e` (about 0.37) of that.
const RING: [(f32, f32, f32); 3] = [(2630.0, 0.45, 0.050), (4180.0, 0.32, 0.038), (6420.0, 0.22, 0.026)];

/// The last of the sound dies away over this long, in seconds, so that it does not end in a click.
const FADE_OUT_SECONDS: f32 = 0.02;

/// The snip, as moments of sound (one channel) for speakers that play `sample_rate` of them a second.
pub(super) fn snip(sample_rate: u32) -> Vec<f32> {
    let rate = sample_rate as f32;
    let length = (DURATION_SECONDS * rate).round() as usize;
    let mut noise = Noise::new();
    let mut slide_filter = HighPass::new(SLIDE_LOWEST_PITCH_HERTZ, rate);
    let mut knock_filter = HighPass::new(SLIDE_LOWEST_PITCH_HERTZ / 2.0, rate);

    let mut sound: Vec<f32> = (0..length)
        .map(|moment| {
            let seconds = moment as f32 / rate;
            let slide_noise = slide_filter.filter(noise.next());
            let knock_noise = knock_filter.filter(noise.next());
            if seconds < SLIDE_SECONDS {
                // The swell is slow at first and quickest at the end, where the blades meet.
                let swell = (seconds / SLIDE_SECONDS).powi(2);
                SLIDE_LOUDNESS * swell * slide_noise
            } else {
                let since_the_knock = seconds - SLIDE_SECONDS;
                let knock = KNOCK_LOUDNESS * knock_noise * (-since_the_knock * 10.0_f32.ln() / KNOCK_SECONDS).exp();
                let ring: f32 = RING
                    .iter()
                    .map(|&(pitch, loudness, seconds_to_fade)| {
                        loudness * (TAU * pitch * since_the_knock).sin() * (-since_the_knock / seconds_to_fade).exp()
                    })
                    .sum();
                knock + ring
            }
        })
        .collect();

    fade_out(&mut sound, FADE_OUT_SECONDS, rate);
    scale_to_the_peak(&mut sound, PEAK_LOUDNESS);
    sound
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATES: [u32; 4] = [8000, 22050, 44100, 48000];

    fn energy(sound: &[f32]) -> f32 {
        sound.iter().map(|value| value * value).sum()
    }

    /// How much of the energy of `sound` is in pitches lower than `pitch_hertz`, as a fraction of all of it
    /// (found by working out each pitch that a sound of that length can have there, which is slow but exact).
    fn share_of_the_energy_below(sound: &[f32], pitch_hertz: f32, rate: f32) -> f32 {
        let length = sound.len();
        let strength_of = |step: usize| -> f64 {
            let (mut cosine, mut sine) = (0.0_f64, 0.0_f64);
            for (moment, value) in sound.iter().enumerate() {
                let angle = std::f64::consts::TAU * (step * moment) as f64 / length as f64;
                cosine += f64::from(*value) * angle.cos();
                sine += f64::from(*value) * angle.sin();
            }
            cosine * cosine + sine * sine
        };
        let highest_low_step = (pitch_hertz * length as f32 / rate) as usize;
        // Each step above the first has a twin at the other end of the whole list of them.
        let low: f64 = strength_of(0) + (1..=highest_low_step).map(|step| 2.0 * strength_of(step)).sum::<f64>();
        (low / (length as f64 * f64::from(energy(sound)))) as f32
    }

    #[test]
    fn the_snip_lasts_the_same_time_whatever_the_speakers_rate_is() {
        for rate in RATES {
            let sound = snip(rate);
            let seconds = sound.len() as f32 / rate as f32;
            assert!(
                (seconds - DURATION_SECONDS).abs() <= 1.0 / rate as f32,
                "{seconds} s at {rate} moments a second"
            );
            assert!(
                (0.08..=0.25).contains(&seconds),
                "a snip is a short sound, not {seconds} s"
            );
        }
    }

    #[test]
    fn the_snip_is_clearly_heard_without_being_close_to_the_loudest_the_speakers_can_make() {
        for rate in RATES {
            let sound = snip(rate);
            let peak = sound.iter().fold(0.0_f32, |peak, value| peak.max(value.abs()));
            assert!((peak - PEAK_LOUDNESS).abs() < 1e-6, "peak {peak} at {rate}");
            assert!(sound.iter().all(|value| value.is_finite()));
        }
    }

    #[test]
    fn the_snip_neither_starts_nor_ends_with_a_click() {
        for rate in RATES {
            let sound = snip(rate);
            assert_eq!(sound[0], 0.0, "starts from silence at {rate}");
            let last = *sound.last().unwrap();
            assert!(
                last.abs() < 1e-3 * PEAK_LOUDNESS,
                "ends in silence at {rate}, not {last}"
            );
            // The last millisecond is much quieter than the sound was.
            let one_millisecond = (rate / 1000) as usize;
            let tail = &sound[sound.len() - one_millisecond..];
            assert!(tail.iter().all(|value| value.abs() < 0.05 * PEAK_LOUDNESS), "{tail:?}");
        }
    }

    #[test]
    fn the_snip_has_a_slide_then_a_knock_which_is_the_loudest_part() {
        let rate = 48000;
        let sound = snip(rate);
        let slide_end = (SLIDE_SECONDS * rate as f32) as usize;
        let slide_peak = sound[..slide_end]
            .iter()
            .fold(0.0_f32, |peak, value| peak.max(value.abs()));
        let after_the_knock = &sound[slide_end..slide_end + (0.01 * rate as f32) as usize];
        let knock_peak = after_the_knock
            .iter()
            .fold(0.0_f32, |peak, value| peak.max(value.abs()));
        assert!(knock_peak > 2.0 * slide_peak, "knock {knock_peak}, slide {slide_peak}");
        assert!(slide_peak > 0.0, "the slide is heard");
        // The slide swells: its last tenth is louder than its first tenth.
        let tenth = slide_end / 10;
        assert!(energy(&sound[slide_end - tenth..slide_end]) > 10.0 * energy(&sound[..tenth]));
    }

    #[test]
    fn the_knock_rings_on_for_a_while_and_dies_away() {
        for rate in [44100_u32, 48000] {
            let sound = snip(rate);
            let energy_between =
                |from: f32, to: f32| energy(&sound[(from * rate as f32) as usize..(to * rate as f32) as usize]);
            let total = energy(&sound);
            let ring = energy_between(SLIDE_SECONDS + 0.015, SLIDE_SECONDS + 0.045);
            let later = energy_between(SLIDE_SECONDS + 0.055, SLIDE_SECONDS + 0.075);
            // (Measured: the ring is about a third of all the energy, and what is left later on about a twentieth.)
            assert!(
                ring > 0.15 * total,
                "the ring is {:.0} % of the energy at {rate}",
                100.0 * ring / total
            );
            assert!(later < 0.5 * ring, "it dies away: {later} after {ring} at {rate}");
        }
    }

    #[test]
    fn the_snip_is_bright_with_next_to_nothing_low_in_it() {
        for rate in RATES {
            let sound = snip(rate);
            let share = share_of_the_energy_below(&sound, 1000.0, rate as f32);
            // (Measured: under 0.1 % at each of these rates.)
            assert!(share < 0.005, "{:.2} % of the energy is low at {rate}", share * 100.0);
            let mean = sound.iter().sum::<f32>() / sound.len() as f32;
            assert!(mean.abs() < 0.005, "mean {mean} at {rate}");
        }
    }

    #[test]
    fn the_snip_is_the_same_every_time() {
        assert_eq!(snip(48000), snip(48000));
    }

    /// A fingerprint of every moment of `sound`, to a thousandth of the loudest the speakers can make. (Not to the last bit: `sin`, `exp` and `ln`
    /// come from the maths library of the operating system, and the same sound differs by a few parts in a hundred million between Windows and
    /// Linux, in about one moment in a hundred. A thousandth is far finer than anything that can be heard, and a change of the sound that is
    /// meant, even a small one, moves many moments across the lines between thousandths.)
    fn fingerprint_of(sound: &[f32]) -> u64 {
        sound.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, value| {
            let thousandths = (value * 1000.0).round() as i32;
            (hash ^ u64::from(thousandths as u32)).wrapping_mul(0x0100_0000_01b3)
        })
    }

    #[test]
    fn the_snip_is_the_sound_it_was_to_a_thousandth_when_the_parts_other_effects_share_were_taken_out_of_it() {
        let found: Vec<(u32, usize, u64)> = RATES
            .iter()
            .map(|&rate| {
                let sound = snip(rate);
                (rate, sound.len(), fingerprint_of(&sound))
            })
            .collect();
        let expected = [
            (8000_u32, 1120_usize, 0x99a6_d9c9_2318_e04b_u64),
            (22050, 3087, 0x4aa4_5354_2bec_c86f_u64),
            (44100, 6174, 0x59cd_1f57_5c30_16a8_u64),
            (48000, 6720, 0x7ea9_c977_7a57_04eb_u64),
        ];
        assert_eq!(found, expected);
    }
}
