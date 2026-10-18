// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sound files for the tests of the sound engine: files of noise (a different number at every sample, so that a clip that is a sample out of place shows) and a steady one.

use std::fs;
use std::path::{Path, PathBuf};

use fastforwardrs::sound_engine::SAMPLE_RATE;

use super::RandomNumbers;

/// The lengths of the three files of noise that `the_sources` makes, in seconds.
pub const FILE_SECONDS: [f64; 3] = [6.0, 3.0, 9.0];

/// A stereo WAV file of 32-bit floating point samples, 48000 a second, that has `value(channel, sample)` for the samples.
pub fn write_wav(path: &Path, seconds: f64, value: impl Fn(usize, usize) -> f32) {
    let samples = (seconds * f64::from(SAMPLE_RATE)) as usize;
    let data_bytes = samples * 2 * 4;
    let mut bytes = Vec::with_capacity(44 + data_bytes);
    bytes.extend(b"RIFF");
    bytes.extend(((36 + data_bytes) as u32).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16_u32.to_le_bytes());
    bytes.extend(3_u16.to_le_bytes()); // IEEE floating point
    bytes.extend(2_u16.to_le_bytes());
    bytes.extend(SAMPLE_RATE.to_le_bytes());
    bytes.extend((SAMPLE_RATE * 8).to_le_bytes());
    bytes.extend(8_u16.to_le_bytes());
    bytes.extend(32_u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend((data_bytes as u32).to_le_bytes());
    for sample in 0..samples {
        for channel in 0..2 {
            bytes.extend(value(channel, sample).to_le_bytes());
        }
    }
    fs::write(path, bytes).expect("the sound file is written");
}

/// A file of noise: a number in -0.5..0.5 at every sample, different in the two channels, the same for the same `seed`.
pub fn noise_file(folder: &Path, name: &str, seconds: f64, seed: u64) -> PathBuf {
    let path = folder.join(name);
    let mut random = RandomNumbers::with_seed(seed);
    let mut numbers: Vec<f32> = Vec::new();
    for _ in 0..(seconds * f64::from(SAMPLE_RATE)) as usize * 2 {
        numbers.push(random.next_below(1_000_000) as f32 / 1_000_000.0 - 0.5);
    }
    write_wav(&path, seconds, |channel, sample| numbers[sample * 2 + channel]);
    path
}

/// A file that is the same number all the way through.
pub fn steady_file(folder: &Path, name: &str, seconds: f64, value: f32) -> PathBuf {
    let path = folder.join(name);
    write_wav(&path, seconds, |_, _| value);
    path
}

/// The sound of `specification` as the filter graph of ffmpeg makes it (the oracle), interleaved.
/// Three files of noise in `folder`, of the lengths in `FILE_SECONDS`.
pub fn the_sources(folder: &Path) -> Vec<PathBuf> {
    FILE_SECONDS
        .iter()
        .enumerate()
        .map(|(number, seconds)| noise_file(folder, &format!("noise_{number}.wav"), *seconds, 1000 + number as u64))
        .collect()
}
