//! The speakers: playing the preview's sound through the operating system's default output device.
//!
//! The device asks for sound in small pieces, on a thread of its own, many times a second. The
//! sound waits in a queue that the editor fills a little ahead of when it is heard; when the queue
//! runs dry the device is given silence, never a pause.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream};
use thiserror::Error;

use super::audio_splice::splice_waiting_sound;
use super::audio_supplier::{AudioSink, AudioSinkOpener};
use crate::media::AudioFormat;

/// More than two channels (surround) are not made use of: the sound is stereo, and the other
/// channels of a device that has them are left silent.
const MOST_CHANNELS_WE_MAKE_SOUND_FOR: u16 = 2;

#[derive(Debug, Error)]
pub enum AudioDeviceError {
    #[error("this computer has no sound output")]
    NoOutputDevice,
    #[error("the sound output could not be set up: {0}")]
    CouldNotSetUp(String),
    #[error("the sound output wants samples of a kind this program cannot make ({0})")]
    UnsupportedSampleFormat(String),
}

/// The default output device, playing what is queued.
pub struct DeviceAudioSink {
    // Stopped (and the device let go of) when dropped.
    _stream: Stream,
    queue: Arc<Mutex<VecDeque<f32>>>,
    format: AudioFormat,
    has_failed: Arc<AtomicBool>,
}

impl DeviceAudioSink {
    /// Opens the default output device and starts it playing silence, ready for sound to be queued.
    pub fn open() -> Result<Self, AudioDeviceError> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or(AudioDeviceError::NoOutputDevice)?;
        let supported_configuration = device
            .default_output_config()
            .map_err(|error| AudioDeviceError::CouldNotSetUp(error.to_string()))?;
        let sample_format = supported_configuration.sample_format();
        let configuration = supported_configuration.config();

        let device_channels = usize::from(configuration.channels);
        let sound_channels = usize::from(configuration.channels.min(MOST_CHANNELS_WE_MAKE_SOUND_FOR));
        let queue: Arc<Mutex<VecDeque<f32>>> = Arc::default();
        let has_failed = Arc::new(AtomicBool::new(false));

        let stream = {
            let queue = Arc::clone(&queue);
            let has_failed = Arc::clone(&has_failed);
            let on_error = move |_error| has_failed.store(true, Ordering::Relaxed);
            // The same code for each kind of sample the device can want.
            macro_rules! stream_of {
                ($sample:ty) => {
                    device.build_output_stream(
                        configuration,
                        move |output: &mut [$sample], _| {
                            fill_output(output, &mut lock(&queue), device_channels, sound_channels);
                        },
                        on_error,
                        None,
                    )
                };
            }
            match sample_format {
                SampleFormat::F32 => stream_of!(f32),
                SampleFormat::F64 => stream_of!(f64),
                SampleFormat::I16 => stream_of!(i16),
                SampleFormat::I32 => stream_of!(i32),
                SampleFormat::U16 => stream_of!(u16),
                other => return Err(AudioDeviceError::UnsupportedSampleFormat(other.to_string())),
            }
            .map_err(|error| AudioDeviceError::CouldNotSetUp(error.to_string()))?
        };
        stream
            .play()
            .map_err(|error| AudioDeviceError::CouldNotSetUp(error.to_string()))?;

        Ok(Self {
            _stream: stream,
            queue,
            format: AudioFormat {
                sample_rate: configuration.sample_rate,
                channel_count: sound_channels as u16,
            },
            has_failed,
        })
    }
}

impl AudioSink for DeviceAudioSink {
    fn format(&self) -> AudioFormat {
        self.format
    }

    fn queue(&mut self, samples: &[f32]) {
        lock(&self.queue).extend(samples);
    }

    fn clear(&mut self) {
        lock(&self.queue).clear();
    }

    fn queued_moment_count(&self) -> u64 {
        (lock(&self.queue).len() / usize::from(self.format.channel_count)) as u64
    }

    fn replace_waiting_sound(&mut self, keep_moments: u64, fade_moments: u64, new_sound: &[f32]) {
        // The queue stays locked while the join is made, so the device cannot take sound out of the
        // middle of it.
        splice_waiting_sound(
            &mut lock(&self.queue),
            usize::from(self.format.channel_count),
            keep_moments,
            fade_moments,
            new_sound,
        );
    }

    fn is_working(&self) -> bool {
        !self.has_failed.load(Ordering::Relaxed)
    }
}

/// The way to open the speakers, for [`crate::playback::AudioSupplier::play_through`].
pub fn opener_of_the_default_speakers() -> AudioSinkOpener {
    Box::new(|| {
        DeviceAudioSink::open()
            .map(|sink| Box::new(sink) as Box<dyn AudioSink>)
            .map_err(|error| error.to_string())
    })
}

/// The queue, even if the thread that held it last panicked: a piece of sound that is wrong is
/// better than none, and better than the sound device's thread dying too.
fn lock(queue: &Mutex<VecDeque<f32>>) -> MutexGuard<'_, VecDeque<f32>> {
    queue.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Fills what the device asked for from the queue. `output` holds whole moments of
/// `device_channels` samples each; the first `sound_channels` of each moment get sound and any
/// others silence. When the queue has run dry the rest is silence.
fn fill_output<Sample: SizedSample + FromSample<f32>>(
    output: &mut [Sample],
    queue: &mut VecDeque<f32>,
    device_channels: usize,
    sound_channels: usize,
) {
    for moment in output.chunks_mut(device_channels) {
        // Whole moments only: a moment of sound is taken from the queue or none of it is.
        let sound_is_there = queue.len() >= sound_channels;
        for (channel, sample) in moment.iter_mut().enumerate() {
            let value = if sound_is_there && channel < sound_channels {
                queue.pop_front().unwrap_or(0.0)
            } else {
                0.0
            };
            *sample = Sample::from_sample_(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    fn queue_of(samples: &[f32]) -> VecDeque<f32> {
        samples.iter().copied().collect()
    }

    #[test]
    fn stereo_sound_goes_to_a_stereo_device_in_order() {
        let mut queue = queue_of(&[0.1, -0.1, 0.2, -0.2, 0.3, -0.3]);
        let mut output = [0.0_f32; 4];
        fill_output(&mut output, &mut queue, 2, 2);
        assert_eq!(output, [0.1, -0.1, 0.2, -0.2]);
        assert_eq!(queue, queue_of(&[0.3, -0.3]), "what was not asked for waits");
    }

    #[test]
    fn a_device_with_more_channels_gets_sound_on_the_first_two_and_silence_on_the_rest() {
        let mut queue = queue_of(&[0.1, -0.1, 0.2, -0.2]);
        let mut output = [9.0_f32; 12]; // two moments of six channels... and more
        fill_output(&mut output, &mut queue, 6, 2);
        assert_eq!(
            output,
            [0.1, -0.1, 0.0, 0.0, 0.0, 0.0, 0.2, -0.2, 0.0, 0.0, 0.0, 0.0],
            "and the sound's own channels were not shifted by the extra ones"
        );
        assert!(queue.is_empty());
    }

    #[test]
    fn mono_sound_goes_to_a_mono_device() {
        let mut queue = queue_of(&[0.5, 0.25]);
        let mut output = [0.0_f32; 2];
        fill_output(&mut output, &mut queue, 1, 1);
        assert_eq!(output, [0.5, 0.25]);
    }

    #[test]
    fn when_the_queue_runs_dry_the_rest_is_silence_and_nothing_goes_wrong_afterwards() {
        let mut queue = queue_of(&[0.1, -0.1]);
        let mut output = [9.0_f32; 6];
        fill_output(&mut output, &mut queue, 2, 2);
        assert_eq!(output, [0.1, -0.1, 0.0, 0.0, 0.0, 0.0]);
        // Later, sound arrives again: it starts at the start of a moment.
        queue.extend([0.7, -0.7]);
        let mut output = [9.0_f32; 2];
        fill_output(&mut output, &mut queue, 2, 2);
        assert_eq!(output, [0.7, -0.7]);
    }

    #[test]
    fn sound_is_converted_to_whatever_kind_of_sample_the_device_wants() {
        let mut queue = queue_of(&[0.5, -0.5, 1.0, -1.0, 0.0, 0.0]);
        let mut as_integers = [0_i16; 6];
        fill_output(&mut as_integers, &mut queue, 2, 2);
        assert_eq!(as_integers[0], 16384, "half of the largest");
        assert_eq!(as_integers[1], -16384);
        assert_eq!(as_integers[2], i16::MAX);
        assert_eq!(as_integers[4], 0);

        let mut queue = queue_of(&[0.0, 1.0]);
        let mut as_unsigned = [0_u16; 2];
        fill_output(&mut as_unsigned, &mut queue, 2, 2);
        assert_eq!(as_unsigned[0], 32768, "silence is the middle for unsigned samples");
        assert_eq!(as_unsigned[1], u16::MAX);
    }

    /// Opens the real speakers and gives them silence to play, to see that they take it at the
    /// speed of real time. (Silence, so that running the tests makes no noise.) Skipped on a
    /// computer without a sound output.
    #[test]
    fn the_real_speakers_use_up_what_is_queued_in_real_time() {
        let mut sink = match DeviceAudioSink::open() {
            Ok(sink) => sink,
            Err(error) => {
                eprintln!("SKIPPED: no sound output to try ({error})");
                return;
            }
        };
        let format = sink.format();
        assert!(format.sample_rate >= 8000, "{format:?}");
        assert!((1..=2).contains(&format.channel_count));
        let channels = usize::from(format.channel_count);
        let one_second = vec![0.0_f32; format.sample_rate as usize * channels];

        sink.queue(&one_second);
        assert_eq!(sink.queued_moment_count(), u64::from(format.sample_rate));
        let started = Instant::now();
        std::thread::sleep(Duration::from_millis(500));
        let left = sink.queued_moment_count();
        let heard = started.elapsed().as_secs_f64();

        let used = f64::from(format.sample_rate) - left as f64;
        let expected = heard * f64::from(format.sample_rate);
        assert!(
            (used - expected).abs() < 0.12 * f64::from(format.sample_rate),
            "in {heard:.3} s the speakers used up {used} moments, about {expected} were expected"
        );
        sink.clear();
        assert_eq!(sink.queued_moment_count(), 0);
        assert!(sink.is_working());
    }
}
