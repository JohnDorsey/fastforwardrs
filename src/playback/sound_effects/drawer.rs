//! The sounds of a drawer: wood sliding on wood, and a knock of wood when the drawer comes to the end of its way. Made from
//! nothing, like the snip (the editor has no sound files of its own), and quiet: they are there to go with what moves on the
//! screen, not to be noticed.
//!
//! A drawer that opens starts quickly and slows as it arrives (the one on the screen does: see `timeline_panel::drawers`), so
//! its slide is loudest at first and dies away, and it ends with a short, soft knock of the drawer against its stops. A drawer
//! that shuts starts slowly and is quickest at the end (the one on the screen is too), so its slide swells, and ends with the
//! heavier, lower thunk of the drawer meeting the frame.
//!
//! The slide is noise kept between a few hundred hertz and a thousand or so, which is where the sound of rubbing wood is: nothing
//! high that could squeak, and no pitch to speak of, which a creak has. It is roughened with a slow wobble, so that it has the
//! grain of wood and not the smoothness of a hiss. The knocks are a few pitches that are not multiples of one another, which die
//! away quickly (as with anything wooden), with a little low click in front of them.
//!
//! The noise comes from a generator that always starts the same, so the sounds are the same every time.

use std::f32::consts::TAU;

use super::synthesis::{HighPass, LowPass, Noise, fade_out, ring, scale_to_the_peak};

/// How long the slide of the drawer lasts, in seconds, before the knock. It is as long as the drawer on the screen takes to slide
/// (a test of the timeline checks that the two are the same).
pub const SLIDE_SECONDS: f32 = 0.18;

/// How long the knock of a drawer that opens rings after it, in seconds, and the sound lasts after the slide.
const OPENING_TAIL_SECONDS: f32 = 0.07;
/// How long the thunk of a drawer that shuts rings after it, in seconds, and the sound lasts after the slide.
const CLOSING_TAIL_SECONDS: f32 = 0.14;

/// How loud the loudest moment is, as a fraction of the loudest sound the speakers can make. (The sounds are scaled to
/// exactly this, so it does not depend on how the noise happened to come out.) The snip is more than twice as loud as either.
pub(super) const OPENING_PEAK_LOUDNESS: f32 = 0.2;
pub(super) const CLOSING_PEAK_LOUDNESS: f32 = 0.24;

/// Nothing in the sounds is lower than this pitch, in hertz, to speak of: the small speakers of a computer cannot play it, and
/// what they cannot play they distort, so it would only make a quiet sound rattle.
const LOWEST_PITCH_HERTZ: f32 = 60.0;

/// The slide is noise with what is lower than this pitch (in hertz) taken out of it, and what is higher than that one.
const SLIDE_LOWEST_PITCH_HERTZ: f32 = 180.0;
const SLIDE_HIGHEST_PITCH_HERTZ: f32 = 1100.0;
/// How loud the slide is at its loudest, compared with the knock at its loudest.
const SLIDE_LOUDNESS: f32 = 0.3;
/// How long the slide takes to begin, in seconds, so that it does not start with a click.
const SLIDE_ATTACK_SECONDS: f32 = 0.006;
/// How long the slide takes to stop when the drawer does, in seconds (it is covered by the knock).
const SLIDE_RELEASE_SECONDS: f32 = 0.002;

/// The wobble of the slide, which gives it the grain of wood: two slow waves, each with its pitch in hertz, how far it takes the
/// loudness up and down (as a fraction of it), and where in its wave it starts. (They are not multiples of one another, so the
/// wobble does not repeat evenly.)
const GRAIN: [(f32, f32, f32); 2] = [(41.0, 0.35, 0.0), (67.0, 0.25, 1.1)];

/// A knock of wood: for each of its pitches, its pitch in hertz, how loud it is, and how many seconds it takes to become `1/e`
/// (about 0.37) of that; and how loud the click in front of it is, how long that takes to become a tenth of what it was (in seconds),
/// and the highest pitch in it.
struct Knock {
    rings: [(f32, f32, f32); 3],
    click_loudness: f32,
    click_seconds: f32,
    click_highest_pitch_hertz: f32,
}

/// The soft knock of a drawer that has come to the end of its way.
const OPENING_KNOCK: Knock = Knock {
    rings: [(250.0, 0.55, 0.018), (590.0, 0.30, 0.011), (1080.0, 0.16, 0.007)],
    click_loudness: 0.25,
    click_seconds: 0.0015,
    click_highest_pitch_hertz: 2500.0,
};

/// The thunk of a drawer that has met the frame: lower, and ringing for longer.
const CLOSING_KNOCK: Knock = Knock {
    rings: [(140.0, 0.90, 0.040), (330.0, 0.50, 0.026), (610.0, 0.28, 0.015)],
    click_loudness: 0.35,
    click_seconds: 0.002,
    click_highest_pitch_hertz: 1800.0,
};

/// The last of the sound dies away over this long, in seconds, so that it does not end in a click.
const FADE_OUT_SECONDS: f32 = 0.02;

/// The sound of a drawer that opens, as moments of sound (one channel) for speakers that play `sample_rate` of them a second.
pub(super) fn opening(sample_rate: u32) -> Vec<f32> {
    // A slide that is loudest at first and dies away as the drawer slows: the speed of a drawer that goes by a cubic curve that
    // slows as it arrives is as the square of how much of its way is left.
    drawer(
        sample_rate,
        OPENING_TAIL_SECONDS,
        &OPENING_KNOCK,
        |how_far| (1.0 - how_far).powi(2),
        OPENING_PEAK_LOUDNESS,
    )
}

/// The sound of a drawer that shuts, as moments of sound (one channel) for speakers that play `sample_rate` of them a second.
pub(super) fn closing(sample_rate: u32) -> Vec<f32> {
    // A slide that swells as the drawer speeds up: the speed of a drawer that goes by the same curve backwards is as the square of
    // how much of its way it has gone.
    drawer(
        sample_rate,
        CLOSING_TAIL_SECONDS,
        &CLOSING_KNOCK,
        |how_far| how_far.powi(2),
        CLOSING_PEAK_LOUDNESS,
    )
}

/// A drawer's sound: its slide, as loud as `speed` says at each moment of it (`speed` is given how far along the slide the moment
/// is, from 0 to 1, and gives a number from 0 to 1), and then the knock, the whole of it as loud as `peak_loudness` at the most.
fn drawer(
    sample_rate: u32,
    tail_seconds: f32,
    knock: &Knock,
    speed: impl Fn(f32) -> f32,
    peak_loudness: f32,
) -> Vec<f32> {
    let rate = sample_rate as f32;
    let length = ((SLIDE_SECONDS + tail_seconds) * rate).round() as usize;
    let mut noise = Noise::new();
    let mut slide_high_pass = HighPass::new(SLIDE_LOWEST_PITCH_HERTZ, rate);
    let mut slide_low_pass = LowPass::new(SLIDE_HIGHEST_PITCH_HERTZ, rate);
    let mut click_low_pass = LowPass::new(knock.click_highest_pitch_hertz, rate);
    let mut without_the_lowest = HighPass::new(LOWEST_PITCH_HERTZ, rate);

    let mut sound: Vec<f32> = (0..length)
        .map(|moment| {
            let seconds = moment as f32 / rate;
            let rubbing = slide_low_pass.filter(slide_high_pass.filter(noise.next()));
            let click_noise = click_low_pass.filter(noise.next());
            let slide = if seconds < SLIDE_SECONDS {
                let attack = (seconds / SLIDE_ATTACK_SECONDS).min(1.0);
                SLIDE_LOUDNESS * attack * speed(seconds / SLIDE_SECONDS) * grain_at(seconds) * rubbing
            } else {
                // (The slide stops with the drawer; it is let go of quickly enough to be hidden by the knock.)
                let since_the_stop = seconds - SLIDE_SECONDS;
                SLIDE_LOUDNESS
                    * speed(1.0)
                    * grain_at(SLIDE_SECONDS)
                    * rubbing
                    * (-since_the_stop / SLIDE_RELEASE_SECONDS).exp()
            };
            let since_the_stop = seconds - SLIDE_SECONDS;
            if since_the_stop < 0.0 {
                return slide;
            }
            let click =
                knock.click_loudness * click_noise * (-since_the_stop * 10.0_f32.ln() / knock.click_seconds).exp();
            let wood: f32 = knock
                .rings
                .iter()
                .map(|&(pitch, loudness, seconds_to_fade)| ring(pitch, loudness, seconds_to_fade, since_the_stop))
                .sum();
            slide + click + wood
        })
        .map(|value| without_the_lowest.filter(value))
        .collect();

    fade_out(&mut sound, FADE_OUT_SECONDS, rate);
    scale_to_the_peak(&mut sound, peak_loudness);
    sound
}

/// How much louder or quieter than its middle the rubbing is `seconds` into the slide: the grain of the wood.
fn grain_at(seconds: f32) -> f32 {
    GRAIN
        .iter()
        .map(|&(pitch, depth, start)| depth * (TAU * pitch * seconds + start).sin())
        .sum::<f32>()
        + 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATES: [u32; 4] = [8000, 22050, 44100, 48000];

    fn energy(sound: &[f32]) -> f32 {
        sound.iter().map(|value| value * value).sum()
    }

    fn peak(sound: &[f32]) -> f32 {
        sound.iter().fold(0.0_f32, |peak, value| peak.max(value.abs()))
    }

    /// The part of `sound` from `from` to `to` seconds in, for speakers that play `rate` moments a second.
    fn between(sound: &[f32], from: f32, to: f32, rate: u32) -> &[f32] {
        &sound[(from * rate as f32) as usize..((to * rate as f32) as usize).min(sound.len())]
    }

    /// How strong the pure note of `pitch_hertz` is in `sound`: the energy of what in it goes along with that note.
    fn strength_of_the_note(sound: &[f32], pitch_hertz: f64, rate: f64) -> f64 {
        let (mut cosine, mut sine) = (0.0_f64, 0.0_f64);
        for (moment, value) in sound.iter().enumerate() {
            let angle = std::f64::consts::TAU * pitch_hertz * moment as f64 / rate;
            cosine += f64::from(*value) * angle.cos();
            sine += f64::from(*value) * angle.sin();
        }
        (cosine * cosine + sine * sine) / sound.len() as f64
    }

    /// How much of the energy of `sound` is in pitches lower than `pitch_hertz`, as a fraction of all of it (found by working out
    /// each pitch that a sound of that length can have there, which is slow but exact).
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

    fn both(rate: u32) -> [(&'static str, Vec<f32>); 2] {
        [("opening", opening(rate)), ("closing", closing(rate))]
    }

    // --- how long, how loud ---

    #[test]
    fn each_sound_lasts_the_slide_and_its_own_tail_whatever_the_speakers_rate_is() {
        for rate in RATES {
            for (name, sound, tail) in [
                ("opening", opening(rate), OPENING_TAIL_SECONDS),
                ("closing", closing(rate), CLOSING_TAIL_SECONDS),
            ] {
                let seconds = sound.len() as f32 / rate as f32;
                assert!(
                    (seconds - (SLIDE_SECONDS + tail)).abs() <= 1.0 / rate as f32,
                    "{name}: {seconds} s at {rate} moments a second"
                );
            }
        }
    }

    #[test]
    fn the_sounds_are_exactly_as_loud_as_they_are_meant_to_be_and_much_quieter_than_the_snip() {
        for rate in RATES {
            for (name, sound, loudness) in [
                ("opening", opening(rate), OPENING_PEAK_LOUDNESS),
                ("closing", closing(rate), CLOSING_PEAK_LOUDNESS),
            ] {
                assert!(
                    (peak(&sound) - loudness).abs() < 1e-6,
                    "{name}: peak {} at {rate}",
                    peak(&sound)
                );
                assert!(sound.iter().all(|value| value.is_finite()), "{name} at {rate}");
                assert!(
                    loudness < 0.55 * super::super::snip::PEAK_LOUDNESS,
                    "{name} is quiet by the side of the snip"
                );
            }
        }
    }

    #[test]
    fn the_sounds_neither_start_nor_end_with_a_click() {
        for rate in RATES {
            for (name, sound) in both(rate) {
                assert_eq!(sound[0], 0.0, "{name} starts from silence at {rate}");
                let one_millisecond = (rate / 1000) as usize;
                let tail = &sound[sound.len() - one_millisecond..];
                assert!(
                    tail.iter().all(|value| value.abs() < 0.02 * peak(&sound)),
                    "{name} ends in silence at {rate}: {tail:?}"
                );
            }
        }
    }

    #[test]
    fn the_sounds_are_the_same_every_time_and_not_the_same_as_one_another() {
        assert_eq!(opening(48000), opening(48000));
        assert_eq!(closing(48000), closing(48000));
        assert_ne!(opening(48000).len(), closing(48000).len());
        assert_ne!(opening(48000)[..4000], closing(48000)[..4000]);
    }

    // --- the shape of each ---

    #[test]
    fn a_drawer_that_opens_is_loudest_at_the_start_of_its_slide_and_one_that_shuts_is_loudest_at_the_end_of_it() {
        for rate in [22050_u32, 48000] {
            let opening = opening(rate);
            let closing = closing(rate);
            let first = |sound: &[f32]| energy(between(sound, 0.02, 0.06, rate));
            let last = |sound: &[f32]| energy(between(sound, SLIDE_SECONDS - 0.06, SLIDE_SECONDS - 0.02, rate));
            assert!(
                first(&opening) > 5.0 * last(&opening),
                "opening at {rate}: first {} last {}",
                first(&opening),
                last(&opening)
            );
            assert!(
                last(&closing) > 5.0 * first(&closing),
                "closing at {rate}: first {} last {}",
                first(&closing),
                last(&closing)
            );
        }
    }

    #[test]
    fn each_slide_ends_in_a_knock_that_is_louder_than_all_of_the_slide() {
        for rate in [22050_u32, 48000] {
            for (name, sound) in both(rate) {
                let slide = peak(between(&sound, 0.0, SLIDE_SECONDS - 0.005, rate));
                let knock = peak(between(&sound, SLIDE_SECONDS, SLIDE_SECONDS + 0.02, rate));
                assert!(knock > 2.0 * slide, "{name} at {rate}: knock {knock}, slide {slide}");
                assert!(slide > 0.0, "{name}: the slide is heard");
            }
        }
    }

    #[test]
    fn the_knock_dies_away_after_it() {
        for rate in [22050_u32, 48000] {
            for (name, sound, tail) in [
                ("opening", opening(rate), OPENING_TAIL_SECONDS),
                ("closing", closing(rate), CLOSING_TAIL_SECONDS),
            ] {
                let near = energy(between(&sound, SLIDE_SECONDS, SLIDE_SECONDS + 0.02, rate));
                let far = energy(between(&sound, SLIDE_SECONDS + tail - 0.03, SLIDE_SECONDS + tail, rate));
                assert!(far < 0.02 * near, "{name} at {rate}: near {near}, far {far}");
            }
        }
    }

    /// The pitch (in hertz, to within ten) that the knock at the end of `sound` has the most of in it.
    fn strongest_pitch_of_the_knock(sound: &[f32], rate: u32) -> f64 {
        let knock = between(sound, SLIDE_SECONDS, SLIDE_SECONDS + 0.06, rate);
        (8..150)
            .map(|step| f64::from(step) * 10.0)
            .map(|pitch| (pitch, strength_of_the_note(knock, pitch, f64::from(rate))))
            .fold((0.0, 0.0), |best, now| if now.1 > best.1 { now } else { best })
            .0
    }

    #[test]
    fn the_thunk_of_a_drawer_that_shuts_is_lower_than_the_knock_of_one_that_opens() {
        for rate in [22050_u32, 48000] {
            let opening = strongest_pitch_of_the_knock(&opening(rate), rate);
            let closing = strongest_pitch_of_the_knock(&closing(rate), rate);

            // (Measured: 250 and 140 hertz, which are the lowest pitches of the two knocks.)
            assert!(
                (200.0..=300.0).contains(&opening),
                "the knock of a drawer that opens: {opening} Hz at {rate}"
            );
            assert!(
                (100.0..=180.0).contains(&closing),
                "the thunk of a drawer that shuts: {closing} Hz at {rate}"
            );
            assert!(
                closing < 0.7 * opening,
                "opening {opening} Hz, closing {closing} Hz at {rate}"
            );
        }
    }

    // --- not a squeak, not a creak, not a boom ---

    #[test]
    fn the_sounds_have_next_to_nothing_high_in_them_which_a_squeak_would_have() {
        for rate in [22050_u32, 48000] {
            for (name, sound) in both(rate) {
                let below = share_of_the_energy_below(&sound, 3000.0, rate as f32);
                // (Measured: at most 0.15 % of the energy is above 3000 Hz.)
                assert!(
                    1.0 - below < 0.01,
                    "{name} at {rate}: {:.2} % of the energy is high",
                    100.0 * (1.0 - below)
                );
            }
        }
    }

    #[test]
    fn the_sounds_have_next_to_nothing_very_low_in_them_which_would_rattle_the_small_speakers_of_a_computer() {
        for rate in [22050_u32, 48000] {
            for (name, sound) in both(rate) {
                let below_forty = share_of_the_energy_below(&sound, 40.0, rate as f32);
                let below_sixty = share_of_the_energy_below(&sound, LOWEST_PITCH_HERTZ, rate as f32);
                // (Measured: at most 0.05 % of the energy is below 40 Hz and at most 0.3 % is below 60 Hz. Without the filter that
                // takes it out, 1 % to 1.6 % was below 40 Hz and 1.5 % to 2.6 % below 60 Hz.)
                assert!(
                    below_forty < 0.002,
                    "{name} at {rate}: {:.3} % is below 40 Hz",
                    100.0 * below_forty
                );
                assert!(
                    below_sixty < 0.01,
                    "{name} at {rate}: {:.3} % is below 60 Hz",
                    100.0 * below_sixty
                );
                let mean = sound.iter().sum::<f32>() / sound.len() as f32;
                assert!(mean.abs() < 0.01 * peak(&sound), "{name} at {rate}: mean {mean}");
            }
        }
    }

    #[test]
    fn the_slide_is_rubbing_and_not_a_note_so_no_one_pitch_in_it_stands_out_as_a_creak_does() {
        let rate = 48000_u32;
        for (name, sound) in both(rate) {
            // The middle of the slide, where it is neither beginning nor stopping, from where the rubbing is to where it is not.
            let rubbing = between(&sound, 0.03, SLIDE_SECONDS - 0.03, rate);
            let strengths: Vec<f64> = (0..90)
                .map(|step| strength_of_the_note(rubbing, 250.0 + 8.0 * step as f64, f64::from(rate)))
                .collect();
            let mean = strengths.iter().sum::<f64>() / strengths.len() as f64;
            let strongest = strengths.iter().copied().fold(0.0_f64, f64::max);
            // (Measured: 3.7 to 6.9 times. Noise has its strongest pitch a few times the average one; a note that is held
            // would be hundreds of times.)
            assert!(
                strongest < 20.0 * mean,
                "{name}: the strongest pitch is {:.1} times the average",
                strongest / mean
            );
        }
    }

    // --- the grain ---

    #[test]
    fn the_grain_wobbles_the_rubbing_but_never_takes_it_away_or_turns_it_over() {
        let values: Vec<f32> = (0..2000).map(|moment| grain_at(moment as f32 / 1000.0)).collect();
        let (lowest, highest) = values.iter().fold((f32::MAX, f32::MIN), |(low, high), value| {
            (low.min(*value), high.max(*value))
        });
        assert!(lowest >= 0.39, "never quiet: {lowest}");
        assert!(highest <= 1.61, "never twice as loud: {highest}");
        assert!(highest - lowest > 0.5, "it does wobble: {lowest} to {highest}");
        assert!((grain_at(0.0) - (1.0 + 0.25 * 1.1_f32.sin())).abs() < 1e-6);
    }
}
