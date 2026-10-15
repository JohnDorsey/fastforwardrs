// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The speakers for the editor's own sounds: a stream of the default output device of their own, which plays
//! what is started on it, over one another, and is silent otherwise.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use cpal::{FromSample, SizedSample, Stream};

use super::voices::Voices;
use super::{EffectSpeakers, EffectSpeakersOpener};
use crate::playback::output_stream::{AudioDeviceError, FillsOutput, OutputShape, open_default_output};

/// The default output device, playing the sounds that were started on it.
pub struct DeviceEffectSpeakers {
    // Stopped (and the device let go of) when dropped.
    _stream: Stream,
    voices: Arc<Mutex<Voices>>,
    sample_rate: u32,
    has_failed: Arc<AtomicBool>,
}

/// What gives the device the sounds that are playing.
struct PlayingSounds {
    voices: Arc<Mutex<Voices>>,
    shape: OutputShape,
}

impl FillsOutput for PlayingSounds {
    fn fill<Sample: SizedSample + FromSample<f32>>(&mut self, output: &mut [Sample]) {
        lock(&self.voices).mix_into(output, self.shape);
    }
}

impl DeviceEffectSpeakers {
    /// Opens the default output device and starts it playing silence, ready for sounds to be started.
    pub fn open() -> Result<Self, AudioDeviceError> {
        let voices: Arc<Mutex<Voices>> = Arc::default();
        let opened = open_default_output({
            let voices = Arc::clone(&voices);
            move |shape| PlayingSounds { voices, shape }
        })?;
        Ok(Self {
            _stream: opened.stream,
            voices,
            sample_rate: opened.format.sample_rate,
            has_failed: opened.has_failed,
        })
    }

    /// How many of the sounds that were started have not finished.
    #[cfg(test)]
    fn sounds_playing(&self) -> usize {
        lock(&self.voices).count()
    }
}

impl EffectSpeakers for DeviceEffectSpeakers {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn play(&mut self, sound: Arc<[f32]>) {
        lock(&self.voices).start(sound);
    }

    fn is_working(&self) -> bool {
        !self.has_failed.load(Ordering::Relaxed)
    }
}

/// The way to open the default speakers for the editor's own sounds, for [`super::SoundEffects::play_through`].
pub fn opener_of_the_default_effect_speakers() -> EffectSpeakersOpener {
    Box::new(|| {
        DeviceEffectSpeakers::open()
            .map(|speakers| Box::new(speakers) as Box<dyn EffectSpeakers>)
            .map_err(|error| error.to_string())
    })
}

/// The sounds, even if the thread that held them last panicked: a sound that is wrong is better than none, and
/// better than the sound device's thread dying too.
fn lock(voices: &Mutex<Voices>) -> MutexGuard<'_, Voices> {
    voices.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    /// Opens the real speakers and has them play a sound of silence, to see that they take it at the speed of
    /// real time. (Silence, so that running the tests makes no noise.) Skipped on a computer without a sound
    /// output.
    ///
    /// A computer that is busy makes the sound thread late now and then, and one try is spoiled by that: the speakers have to be on time
    /// in one of three tries (speakers that do not go at the speed of real time are not, in any of them).
    #[test]
    fn the_real_speakers_play_a_sound_to_its_end_in_real_time() {
        let mut speakers = match DeviceEffectSpeakers::open() {
            Ok(speakers) => speakers,
            Err(error) => {
                eprintln!("SKIPPED: no sound output to try ({error})");
                return;
            }
        };
        assert!(speakers.sample_rate() >= 8000, "{}", speakers.sample_rate());
        assert!(speakers.is_working());
        assert_eq!(speakers.sounds_playing(), 0, "silent until a sound is started");

        let seconds = 0.4;
        let silence: Arc<[f32]> = vec![0.0; (seconds * f64::from(speakers.sample_rate())) as usize].into();
        let mut complaints = Vec::new();
        for _attempt in 0..3 {
            let started = Instant::now();
            speakers.play(silence.clone());
            assert_eq!(speakers.sounds_playing(), 1);

            std::thread::sleep(Duration::from_millis(200));
            assert_eq!(speakers.sounds_playing(), 1, "not over after half the time");
            while speakers.sounds_playing() > 0 && started.elapsed() < Duration::from_secs(3) {
                std::thread::sleep(Duration::from_millis(10));
            }
            let heard = started.elapsed().as_secs_f64();
            assert_eq!(speakers.sounds_playing(), 0, "over after {heard:.2} s");
            if (seconds - 0.1..seconds + 0.25).contains(&heard) {
                break;
            }
            complaints.push(format!("a sound of {seconds} s took {heard:.3} s to play"));
            assert!(complaints.len() < 3, "{complaints:?}");
        }
        assert!(speakers.is_working());
    }
}
