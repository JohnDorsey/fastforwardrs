//! The editor's own little sounds: signs that something was done (a clip was cut, say), as opposed to the sound of
//! the project, which the preview plays ([`super::AudioSupplier`]).
//!
//! They come out of speakers of their own ([`EffectSpeakers`]), so that they are heard whether or not the video
//! is playing, and so that they neither disturb the sound of the preview nor wait for it. Each effect is made
//! from nothing ([`SoundEffect::synthesize`]: the editor has no sound files of its own) once for each rate the
//! speakers play at, and is played by being mixed into whatever else is sounding.

mod device;
#[cfg(test)]
mod logging_speakers;
mod snip;
mod voices;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

pub use device::{DeviceEffectSpeakers, opener_of_the_default_effect_speakers};
#[cfg(test)]
pub use logging_speakers::{SpeakersLog, speakers_that_only_keep_a_log};

use super::audio_supplier::MINIMUM_TIME_BETWEEN_FAILED_ATTEMPTS;

/// A sound the editor makes by itself, to show that something happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SoundEffect {
    /// Clips were cut in two.
    Snip,
}

impl SoundEffect {
    /// The sound, as moments (one channel, each between -1 and 1) for speakers that play `sample_rate` of them
    /// a second. It is the same every time.
    pub fn synthesize(self, sample_rate: u32) -> Vec<f32> {
        match self {
            Self::Snip => snip::snip(sample_rate),
        }
    }
}

/// Where the sound effects go: the speakers, or something standing in for them.
pub trait EffectSpeakers {
    /// How many moments of sound the speakers play a second.
    fn sample_rate(&self) -> u32;
    /// Starts playing `sound` (one channel, which the speakers give to all of theirs) from its start, over
    /// whatever else is playing. It does not wait for the sound to finish.
    fn play(&mut self, sound: Arc<[f32]>);
    /// Whether the speakers still work (a sound device can be unplugged while it is in use).
    fn is_working(&self) -> bool {
        true
    }
}

/// Opens the speakers when they are first needed. (They are not opened at startup, so that a program that never
/// plays an effect does not take hold of the sound device.)
pub type EffectSpeakersOpener = Box<dyn FnMut() -> Result<Box<dyn EffectSpeakers>, String>>;

/// Plays the editor's sound effects. It is silent until given speakers to play through.
#[derive(Default)]
pub struct SoundEffects {
    open_speakers: Option<EffectSpeakersOpener>,
    speakers: Option<Box<dyn EffectSpeakers>>,
    /// The sounds that have been made, for each rate of speakers that has asked for them.
    sounds: HashMap<(SoundEffect, u32), Arc<[f32]>>,
    last_failed_attempt_at: Option<Instant>,
    last_problem: Option<String>,
}

impl SoundEffects {
    /// Plays the effects through speakers made by `open_speakers`, which is called when an effect is first played
    /// (and again, after a failure, a little later).
    pub fn play_through(&mut self, open_speakers: EffectSpeakersOpener) {
        self.speakers = None;
        self.open_speakers = Some(open_speakers);
        self.last_failed_attempt_at = None;
    }

    /// Whether it has been told where to play the effects (see [`Self::play_through`]); until it has, it is silent.
    pub fn is_given_speakers_to_play_through(&self) -> bool {
        self.open_speakers.is_some()
    }

    /// What last went wrong with the speakers, if anything has. The effects are a convenience, so a problem with
    /// them is for the program to remember and not to interrupt the user with.
    pub fn last_problem(&self) -> Option<&str> {
        self.last_problem.as_deref()
    }

    /// Plays `effect` now, over anything else that is playing, if there are speakers to play it.
    pub fn play(&mut self, effect: SoundEffect, now: Instant) {
        if self.speakers.as_ref().is_some_and(|speakers| !speakers.is_working()) {
            self.speakers = None;
            self.last_failed_attempt_at = Some(now);
            self.last_problem = Some("the speakers for the editor's own sounds stopped working".to_owned());
        }
        if !self.make_sure_there_are_speakers(now) {
            return;
        }
        let Some(speakers) = &mut self.speakers else {
            return;
        };
        let sample_rate = speakers.sample_rate();
        let sound = self
            .sounds
            .entry((effect, sample_rate))
            .or_insert_with(|| effect.synthesize(sample_rate).into());
        speakers.play(Arc::clone(sound));
    }

    /// Opens the speakers if that has not been done. Returns whether there are speakers.
    fn make_sure_there_are_speakers(&mut self, now: Instant) -> bool {
        if self.speakers.is_some() {
            return true;
        }
        let Some(open_speakers) = &mut self.open_speakers else {
            return false;
        };
        if self
            .last_failed_attempt_at
            .is_some_and(|failed_at| now.saturating_duration_since(failed_at) < MINIMUM_TIME_BETWEEN_FAILED_ATTEMPTS)
        {
            return false;
        }
        match open_speakers() {
            Ok(speakers) => {
                self.speakers = Some(speakers);
                true
            }
            Err(problem) => {
                self.last_failed_attempt_at = Some(now);
                self.last_problem = Some(format!(
                    "the speakers for the editor's own sounds could not be opened: {problem}"
                ));
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;
    use std::time::Duration;

    use super::*;

    /// Effects that are to play through speakers that make no sound (opened when first needed) at `sample_rate`,
    /// and the log of what happens to the speakers.
    fn effects_playing_through_logging_speakers_at(sample_rate: u32) -> (SoundEffects, Rc<SpeakersLog>) {
        let (opener, log) = speakers_that_only_keep_a_log(sample_rate);
        let mut effects = SoundEffects::default();
        effects.play_through(opener);
        (effects, log)
    }

    #[test]
    fn effects_with_no_speakers_to_play_through_make_no_sound_and_no_trouble() {
        let mut effects = SoundEffects::default();
        effects.play(SoundEffect::Snip, Instant::now());
        assert_eq!(effects.last_problem(), None);
    }

    #[test]
    fn the_speakers_are_not_opened_until_an_effect_is_played_and_are_then_kept() {
        let (mut effects, log) = effects_playing_through_logging_speakers_at(48000);
        assert_eq!(log.times_opened(), 0);
        let now = Instant::now();
        effects.play(SoundEffect::Snip, now);
        effects.play(SoundEffect::Snip, now + Duration::from_secs(5));
        assert_eq!(log.times_opened(), 1);
        assert_eq!(log.count_of_sounds_played(), 2);
    }

    #[test]
    fn the_sound_that_is_played_is_the_effect_made_for_the_rate_of_the_speakers() {
        for rate in [22050, 48000] {
            let (mut effects, log) = effects_playing_through_logging_speakers_at(rate);
            effects.play(SoundEffect::Snip, Instant::now());
            let played = log.sounds_played();
            assert_eq!(played.len(), 1);
            assert_eq!(*played[0], *SoundEffect::Snip.synthesize(rate), "at {rate}");
            assert!(!played[0].is_empty());
        }
    }

    #[test]
    fn an_effect_is_made_once_and_the_same_sound_is_played_again() {
        let (mut effects, log) = effects_playing_through_logging_speakers_at(48000);
        let now = Instant::now();
        effects.play(SoundEffect::Snip, now);
        effects.play(SoundEffect::Snip, now);
        let played = log.sounds_played();
        assert!(Arc::ptr_eq(&played[0], &played[1]));
    }

    #[test]
    fn speakers_that_cannot_be_opened_are_tried_again_only_after_a_while() {
        let (mut effects, log) = effects_playing_through_logging_speakers_at(48000);
        log.make_opening_fail(true);
        let start = Instant::now();
        effects.play(SoundEffect::Snip, start);
        assert!(
            effects
                .last_problem()
                .is_some_and(|problem| problem.contains("no thank you"))
        );
        assert_eq!(log.count_of_sounds_played(), 0);

        // Things get better, but it is too soon to look.
        log.make_opening_fail(false);
        effects.play(SoundEffect::Snip, start + MINIMUM_TIME_BETWEEN_FAILED_ATTEMPTS / 2);
        assert_eq!(log.times_opened(), 0);
        assert_eq!(log.count_of_sounds_played(), 0);

        effects.play(SoundEffect::Snip, start + MINIMUM_TIME_BETWEEN_FAILED_ATTEMPTS);
        assert_eq!(log.times_opened(), 1);
        assert_eq!(log.count_of_sounds_played(), 1);
    }

    #[test]
    fn speakers_that_stop_working_are_replaced_by_new_ones_a_while_later() {
        let (mut effects, log) = effects_playing_through_logging_speakers_at(48000);
        log.break_the_first_speakers_opened();
        let start = Instant::now();
        effects.play(SoundEffect::Snip, start);
        // (The sound went to speakers that were already unusable: the first play only finds that out next time.)
        assert_eq!(log.times_opened(), 1);
        assert_eq!(log.count_of_sounds_played(), 1);

        let found_out_at = start + Duration::from_secs(1);
        effects.play(SoundEffect::Snip, found_out_at);
        assert!(
            effects
                .last_problem()
                .is_some_and(|problem| problem.contains("stopped working"))
        );
        assert_eq!(log.times_opened(), 1, "too soon after the failure to try again");
        assert_eq!(log.count_of_sounds_played(), 1);

        effects.play(SoundEffect::Snip, found_out_at + MINIMUM_TIME_BETWEEN_FAILED_ATTEMPTS);
        assert_eq!(log.times_opened(), 2);
        assert_eq!(log.count_of_sounds_played(), 2);
    }

    #[test]
    fn being_given_other_speakers_lets_go_of_the_old_ones() {
        let (mut effects, first) = effects_playing_through_logging_speakers_at(48000);
        effects.play(SoundEffect::Snip, Instant::now());
        let (opener, second) = speakers_that_only_keep_a_log(44100);
        effects.play_through(opener);
        effects.play(SoundEffect::Snip, Instant::now());
        assert_eq!(first.count_of_sounds_played(), 1);
        assert_eq!(second.count_of_sounds_played(), 1);
        assert_eq!(
            second.sounds_played()[0].len(),
            SoundEffect::Snip.synthesize(44100).len()
        );
    }
}
