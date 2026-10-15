// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sounds that are playing at once, mixed into what the speakers ask for.

use std::sync::Arc;

use cpal::{FromSample, SizedSample};

use crate::playback::output_stream::OutputShape;

/// How many sounds are played at once. When one more is started, the one that has been playing longest is cut
/// off, so that a person who holds down a key does not make a din.
pub(super) const MOST_SOUNDS_AT_ONCE: usize = 8;

/// A sound that is being played, and how much of it has been.
struct Voice {
    sound: Arc<[f32]>,
    moments_played: usize,
}

/// The sounds that are playing now.
#[derive(Default)]
pub(super) struct Voices {
    /// In the order they were started.
    voices: Vec<Voice>,
}

impl Voices {
    /// Starts playing `sound` (one channel), from its start, over what is playing.
    pub(super) fn start(&mut self, sound: Arc<[f32]>) {
        if self.voices.len() >= MOST_SOUNDS_AT_ONCE {
            self.voices.remove(0);
        }
        self.voices.push(Voice {
            sound,
            moments_played: 0,
        });
    }

    /// How many sounds have not finished.
    #[cfg(test)]
    pub(super) fn count(&self) -> usize {
        self.voices.len()
    }

    /// Fills `output`, which is made of whole moments of the shape given, with the sounds that are playing,
    /// added together (and never louder than the speakers can make). Each sound is heard on every channel
    /// that gets sound, and the other channels are silent. The sounds that end in it are forgotten.
    pub(super) fn mix_into<Sample: SizedSample + FromSample<f32>>(
        &mut self,
        output: &mut [Sample],
        shape: OutputShape,
    ) {
        for moment in output.chunks_mut(shape.device_channels) {
            let mut mixed = 0.0_f32;
            for voice in &mut self.voices {
                if let Some(value) = voice.sound.get(voice.moments_played) {
                    mixed += value;
                    voice.moments_played += 1;
                }
            }
            let mixed = mixed.clamp(-1.0, 1.0);
            for (channel, sample) in moment.iter_mut().enumerate() {
                let value = if channel < shape.sound_channels { mixed } else { 0.0 };
                *sample = Sample::from_sample_(value);
            }
        }
        self.voices.retain(|voice| voice.moments_played < voice.sound.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEREO: OutputShape = OutputShape {
        device_channels: 2,
        sound_channels: 2,
    };

    fn sound_of(samples: &[f32]) -> Arc<[f32]> {
        samples.into()
    }

    #[test]
    fn nothing_playing_is_silence() {
        let mut voices = Voices::default();
        let mut output = [9.0_f32; 6];
        voices.mix_into(&mut output, STEREO);
        assert_eq!(output, [0.0; 6]);
        assert_eq!(voices.count(), 0);
    }

    #[test]
    fn a_sound_is_heard_on_each_channel_that_gets_sound_and_on_no_other() {
        let mut voices = Voices::default();
        voices.start(sound_of(&[0.5, -0.25]));
        let mut output = [9.0_f32; 12];
        voices.mix_into(
            &mut output,
            OutputShape {
                device_channels: 6,
                sound_channels: 2,
            },
        );
        assert_eq!(output, [0.5, 0.5, 0.0, 0.0, 0.0, 0.0, -0.25, -0.25, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(voices.count(), 0, "it ended");

        voices.start(sound_of(&[0.5, -0.25]));
        let mut mono_output = [9.0_f32; 2];
        voices.mix_into(
            &mut mono_output,
            OutputShape {
                device_channels: 1,
                sound_channels: 1,
            },
        );
        assert_eq!(mono_output, [0.5, -0.25]);
    }

    #[test]
    fn a_sound_goes_on_where_it_left_off_when_the_speakers_ask_again() {
        let mut voices = Voices::default();
        voices.start(sound_of(&[0.1, 0.2, 0.3, 0.4, 0.5]));
        let mut first = [0.0_f32; 4];
        voices.mix_into(&mut first, STEREO);
        assert_eq!(first, [0.1, 0.1, 0.2, 0.2]);
        assert_eq!(voices.count(), 1);
        let mut second = [9.0_f32; 6];
        voices.mix_into(&mut second, STEREO);
        assert_eq!(
            second,
            [0.3, 0.3, 0.4, 0.4, 0.5, 0.5],
            "all of what was left, and then no more"
        );
        assert_eq!(voices.count(), 0);
    }

    #[test]
    fn sounds_playing_together_are_added() {
        let mut voices = Voices::default();
        voices.start(sound_of(&[0.25, 0.25, 0.25]));
        let mut output = [0.0_f32; 2];
        voices.mix_into(&mut output, STEREO);
        voices.start(sound_of(&[0.125, 0.5]));
        let mut output = [0.0_f32; 4];
        voices.mix_into(&mut output, STEREO);
        assert_eq!(output, [0.375, 0.375, 0.75, 0.75], "the second began at its own start");
    }

    #[test]
    fn what_is_louder_than_the_speakers_can_make_is_cut_off_at_their_loudest() {
        let mut voices = Voices::default();
        voices.start(sound_of(&[0.75, -0.75]));
        voices.start(sound_of(&[0.75, -0.75]));
        let mut output = [0.0_f32; 4];
        voices.mix_into(&mut output, STEREO);
        assert_eq!(output, [1.0, 1.0, -1.0, -1.0]);
    }

    #[test]
    fn the_sound_is_converted_to_whatever_kind_of_sample_the_speakers_want() {
        let mut voices = Voices::default();
        voices.start(sound_of(&[0.5, -0.5]));
        let mut as_integers = [0_i16; 4];
        voices.mix_into(&mut as_integers, STEREO);
        assert_eq!(as_integers, [16384, 16384, -16384, -16384]);
    }

    #[test]
    fn one_more_sound_than_can_play_at_once_cuts_off_the_one_that_has_played_longest() {
        let mut voices = Voices::default();
        for number in 0..MOST_SOUNDS_AT_ONCE {
            voices.start(sound_of(&[0.01 * (number + 1) as f32]));
        }
        assert_eq!(voices.count(), MOST_SOUNDS_AT_ONCE);
        voices.start(sound_of(&[0.5]));
        assert_eq!(voices.count(), MOST_SOUNDS_AT_ONCE);
        let mut output = [0.0_f32; 2];
        voices.mix_into(&mut output, STEREO);
        // 0.01 (the first sound) is gone: 0.02 + ... + 0.08 + 0.5.
        let expected = (2..=MOST_SOUNDS_AT_ONCE)
            .map(|number| 0.01 * number as f32)
            .sum::<f32>()
            + 0.5;
        assert!((output[0] - expected).abs() < 1e-6, "{output:?} {expected}");
    }

    #[test]
    fn an_odd_piece_of_output_does_not_go_wrong() {
        let mut voices = Voices::default();
        voices.start(sound_of(&[0.5, 0.5]));
        let mut output = [9.0_f32; 3]; // a moment and a half
        voices.mix_into(&mut output, STEREO);
        assert_eq!(output, [0.5, 0.5, 0.5]);
    }
}
