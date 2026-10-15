// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Opening the default output device, so that something fills what it asks for.
//!
//! The device asks for sound in small pieces, on a thread of its own, many times a second, in whatever kind of
//! sample it likes (decimals of 32 or 64 bits, or integers of 16 or 32). What fills those pieces is a
//! [`FillsOutput`], which is made once the shape of the output is known. The sound of the preview
//! ([`super::DeviceAudioSink`]) and the editor's own little sounds ([`super::DeviceEffectSpeakers`]) both
//! come out of the speakers this way, each through a stream of its own.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream};
use thiserror::Error;

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

/// How the output is laid out: it is asked for in whole moments of `device_channels` samples each, of which
/// the first `sound_channels` get sound (the others are left silent).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OutputShape {
    pub device_channels: usize,
    pub sound_channels: usize,
}

/// What fills the pieces of output that the device asks for.
pub(super) trait FillsOutput: Send + 'static {
    /// Fills `output`, which is made of whole moments (see [`OutputShape`]), with what is to be heard now.
    fn fill<Sample: SizedSample + FromSample<f32>>(&mut self, output: &mut [Sample]);
}

/// An output that is open and playing, whatever fills it.
pub(super) struct OpenedOutput {
    /// The device is let go of when this is dropped.
    pub stream: Stream,
    pub format: AudioFormat,
    /// Set when the device reports an error (it can be unplugged while it is in use).
    pub has_failed: Arc<AtomicBool>,
}

/// Opens the default output device and starts it playing what the filler `make_filler` makes (once it is told
/// the shape of the output) gives it.
pub(super) fn open_default_output<Filler: FillsOutput>(
    make_filler: impl FnOnce(OutputShape) -> Filler,
) -> Result<OpenedOutput, AudioDeviceError> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or(AudioDeviceError::NoOutputDevice)?;
    let supported_configuration = device
        .default_output_config()
        .map_err(|error| AudioDeviceError::CouldNotSetUp(error.to_string()))?;
    let sample_format = supported_configuration.sample_format();
    let configuration = supported_configuration.config();

    let shape = OutputShape {
        device_channels: usize::from(configuration.channels),
        sound_channels: usize::from(configuration.channels.min(MOST_CHANNELS_WE_MAKE_SOUND_FOR)),
    };
    let mut filler = make_filler(shape);
    let has_failed = Arc::new(AtomicBool::new(false));

    let stream = {
        let has_failed = Arc::clone(&has_failed);
        let on_error = move |_error| has_failed.store(true, Ordering::Relaxed);
        // The same code for each kind of sample the device can want.
        macro_rules! stream_of {
            ($sample:ty) => {
                device.build_output_stream(
                    configuration,
                    move |output: &mut [$sample], _| filler.fill(output),
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

    Ok(OpenedOutput {
        stream,
        format: AudioFormat {
            sample_rate: configuration.sample_rate,
            channel_count: shape.sound_channels as u16,
        },
        has_failed,
    })
}
