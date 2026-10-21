// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The check of a plugin's start: does the sound a new plugin makes depend on what was in memory before it was made?
//!
//! Found with a reverb (Aura Lite) that roared, warbled and went silent at random: **its sound at the start depended on the bytes the memory held when it was made** (made after
//! the memory was filled with `0xFF` and let go of, 86 of 100 sounded different from the first; made in memory that was cleared, none did). A plugin that reads memory it did not
//! write sounds as the leftovers of the program make it, and the editor's memory is full of old pictures. Seven other plugins did not do it.
//!
//! The check: the plugin is made and run on a fixed noise **twice with the memory cleared** (if those two sound different, the plugin differs by itself, as one with random
//! modulation does, and nothing can be said), and then **after the memory was filled with other bytes and let go of**, which is what the application's memory is like. A plugin
//! that sounds different then **is unreliable at its start**. It runs the code of the plugin many times, so it is done by the program that scans.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::instance::Instance;
use crate::library::Library;
use crate::main_thread::MainThread;

/// What the check found of a plugin.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum StartCheck {
    /// It was not checked (it has no audio in and out to run, it could not be made or run, or the scan is from before the check existed).
    #[default]
    NotChecked,
    /// Made again in memory that held other bytes, it sounded as it did in cleared memory.
    Steady,
    /// Made twice in cleared memory, it sounded different each time (it varies by itself): nothing can be said of its start.
    DiffersByItself,
    /// Made in memory that held other bytes, it sounded different from how it did in cleared memory: **its start depends on memory it did not write**.
    Unreliable {
        /// What the memory had been filled with (`0xFF`).
        filled_with: u8,
        /// How different, the greatest difference of a sample over the loudness (rms) of the sound, as a fraction.
        relative_difference: f32,
    },
}

impl StartCheck {
    /// What to tell a person about the plugin, if the check found something.
    pub fn warning(&self) -> Option<String> {
        match self {
            Self::Unreliable {
                filled_with,
                relative_difference,
            } => Some(format!(
                "its sound at the start depends on what was in memory before it was made (made again after memory was filled with the byte 0x{filled_with:02X}, it sounded {:.0}% different, where the greatest difference of a sample is told as a share of its loudness): \
                 it may start with a roar, a warble or silence, and that is a fault of the plugin and not of the editor",
                relative_difference * 100.0
            )),
            Self::NotChecked | Self::Steady | Self::DiffersByItself => None,
        }
    }
}

/// How many blocks of noise the plugin is run on.
const BLOCKS: usize = 24;

/// The frames of a block.
const BLOCK_FRAMES: usize = crate::scan::PROBE_BLOCK_FRAMES;

/// The longest that the checks of the plugins of one library may take all together: a library of hundreds of plugins (a collection) is not checked to the end, and the plugins
/// after the time is up are [`StartCheck::NotChecked`]. (A plugin takes about half a second.)
pub const LONGEST_FOR_A_LIBRARY: std::time::Duration = std::time::Duration::from_secs(12);

/// The bytes that the memory is filled with, besides zero, and the order they are tried in. (A plugin that reads a flag or a gain from memory it did not write gets a different one from
/// each.)
const FILLS: [u8; 2] = [0xFF, 0xCD];

/// How many times the memory is filled with each byte before the plugin is said to be steady: where a plugin lands in memory depends on the allocator, and a plugin that reads
/// garbage is given it by a fill about half the time (measured with the plugin that roared: 43 of 60 fills with `0xFF`, 21 of 60 with `0xCD`).
const FILLS_OF_EACH_BYTE: usize = 5;

/// A difference of a sample from one sound to the other, over the loudness of the sound, above which two sounds are said to be different.
const SAME_WITHIN: f32 = 0.01;

/// A loudness below which a sound is taken to be this loud (so that two silences are not told apart by their rounding).
const LEAST_LOUDNESS: f32 = 0.01;

/// Fills the memory that the allocator has to give with `byte` and lets go of it: buffers are taken one after another, each filled and let go of, so that the allocator hands the
/// same memory out again and the memory it has holds `byte`. The sizes are those of the buffers of a plugin: a hundred from 64 KiB to 2 MiB (delay lines and wave tables are
/// large, and a plugin that landed on small buffers only did not read garbage in the experiments), and some hundreds from 16 bytes to 64 KiB. They are different each time
/// (where the plugin lands is decided by what the allocator has, which the sizes shape).
#[must_use = "what is given back keeps the filled memory from being given back to the system, and is to be kept until the plugin has been made and run"]
pub fn fill_the_memory_with(byte: u8) -> Vec<u8> {
    static TIMES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let mut seed = TIMES
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut next = move |least: usize, span: usize| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        least + (seed >> 33) as usize % span
    };
    // Taken together and kept until all are taken (so that every free piece the allocator has is used and filled), then let go of together.
    let mut taken: Vec<Vec<u8>> = Vec::new();
    for _ in 0..160 {
        taken.push(vec![byte; next(64 << 10, 448 << 10)]);
    }
    for _ in 0..40 {
        taken.push(vec![byte; next(512 << 10, 1536 << 10)]);
    }
    for _ in 0..2000 {
        taken.push(vec![byte; next(16, 64 << 10)]);
    }
    // A small buffer taken last stands above all the others (the free pieces are all in use, so it comes from the top): while it is kept, the allocator cannot give the filled memory
    // back to the system, as glibc does with the top of its heap, and what the plugin is given is what was filled.
    let pin = vec![byte; 64];
    std::hint::black_box(&taken);
    drop(taken);
    pin
}

/// The noise that is given to the plugin: the same every time.
fn the_noise(channels: usize) -> Vec<Vec<f32>> {
    let mut seed = 99_u64;
    (0..channels)
        .map(|_| {
            (0..BLOCKS * BLOCK_FRAMES)
                .map(|_| {
                    seed = seed
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1_442_695_040_888_963_407);
                    ((seed >> 40) as f32 / (1_u64 << 24) as f32) - 0.5
                })
                .collect()
        })
        .collect()
}

/// What the plugin gives, every output channel after the other, when it is made now and run on the noise (`None`: it could not be made or run).
fn what_a_new_plugin_makes(
    worker: &Arc<MainThread>,
    library: &Library,
    id: &str,
    inputs: usize,
    outputs: usize,
) -> Option<Vec<f32>> {
    let mut instance = Instance::new(worker, library, id, crate::scan::PROBE_SAMPLE_RATE, BLOCK_FRAMES as u32).ok()?;
    let noise = the_noise(inputs);
    let mut made: Vec<Vec<f32>> = vec![Vec::new(); outputs];
    for block in 0..BLOCKS {
        let span = block * BLOCK_FRAMES..(block + 1) * BLOCK_FRAMES;
        let mut input: Vec<Vec<f32>> = noise.iter().map(|channel| channel[span.clone()].to_vec()).collect();
        let mut output = vec![vec![0.0_f32; BLOCK_FRAMES]; outputs];
        {
            let mut input_slices: Vec<&mut [f32]> = input.iter_mut().map(Vec::as_mut_slice).collect();
            let mut output_slices: Vec<&mut [f32]> = output.iter_mut().map(Vec::as_mut_slice).collect();
            instance.run(&mut input_slices, &mut output_slices, &[]).ok()?;
        }
        for (all, part) in made.iter_mut().zip(output) {
            all.extend(part);
        }
    }
    Some(made.concat())
}

/// How different two sounds are: the greatest difference of a sample, over the loudness (rms) of the first (but not less than a least loudness); a sample that is not a number in one
/// and is in the other is infinitely different.
pub fn how_different(first: &[f32], second: &[f32]) -> f32 {
    if first.len() != second.len() {
        return f32::INFINITY;
    }
    if first.is_empty() {
        return 0.0;
    }
    let loudness = (first.iter().map(|sample| sample * sample).sum::<f32>() / first.len() as f32)
        .sqrt()
        .max(LEAST_LOUDNESS);
    let mut greatest = 0.0_f32;
    for (a, b) in first.iter().zip(second) {
        let difference = if a.is_nan() && b.is_nan() { 0.0 } else { (a - b).abs() };
        if difference.is_nan() {
            return f32::INFINITY;
        }
        greatest = greatest.max(difference);
    }
    greatest / loudness
}

/// What to make of the sounds a plugin made: `cleared` twice in cleared memory, and `filled` in memory filled with each byte (in order).
pub fn judge(cleared: (&[f32], &[f32]), filled: &[(u8, Vec<f32>)]) -> StartCheck {
    if how_different(cleared.0, cleared.1) > SAME_WITHIN {
        return StartCheck::DiffersByItself;
    }
    let mut worst: Option<(u8, f32)> = None;
    for (byte, sound) in filled {
        let difference = how_different(cleared.0, sound);
        if difference > SAME_WITHIN && worst.is_none_or(|(_, before)| difference > before) {
            worst = Some((*byte, difference));
        }
    }
    match worst {
        Some((filled_with, relative_difference)) => StartCheck::Unreliable {
            filled_with,
            relative_difference,
        },
        None => StartCheck::Steady,
    }
}

/// The two sounds that agree, of the first two made in cleared memory and (only if those two do not agree) a third: the first two if they agree, else the third and the one of them it
/// agrees with. `Err` says what to make of the plugin if none agree ([`StartCheck::DiffersByItself`]) or the third could not be made ([`StartCheck::NotChecked`]).
pub fn the_pair_that_agrees(
    first: Vec<f32>,
    second: Vec<f32>,
    third: impl FnOnce() -> Option<Vec<f32>>,
) -> Result<(Vec<f32>, Vec<f32>), StartCheck> {
    if how_different(&first, &second) <= SAME_WITHIN {
        return Ok((first, second));
    }
    let Some(third) = third() else {
        return Err(StartCheck::NotChecked);
    };
    if how_different(&second, &third) <= SAME_WITHIN {
        Ok((second, third))
    } else if how_different(&first, &third) <= SAME_WITHIN {
        Ok((first, third))
    } else {
        Err(StartCheck::DiffersByItself)
    }
}

/// Checks the plugin `id` of `library`, which has `inputs` audio input channels and `outputs` audio output channels on its main ports.
pub fn check_the_start(
    worker: &Arc<MainThread>,
    library: &Library,
    id: &str,
    inputs: usize,
    outputs: usize,
) -> StartCheck {
    if inputs == 0 || outputs == 0 {
        return StartCheck::NotChecked;
    }
    let make = |byte: u8| {
        // (On the thread that makes and activates plugins, which is where a plugin allocates what it keeps: with glibc each thread has a heap of its own, and filling the memory of
        // another thread would not reach the plugin.)
        let pin = worker.call(move |_| fill_the_memory_with(byte))?;
        let made = what_a_new_plugin_makes(worker, library, id, inputs, outputs);
        drop(pin);
        made
    };
    // The sound in cleared memory is the one that two of three runs agree on (the first run may land in what the plugin itself left; a plugin that does not agree with itself in
    // three runs varies by itself).
    let (Some(first), Some(second)) = (make(0), make(0)) else {
        return StartCheck::NotChecked;
    };
    let (first, second) = match the_pair_that_agrees(first, second, || make(0)) {
        Ok(pair) => pair,
        Err(found) => return found,
    };
    let mut filled = Vec::new();
    for _ in 0..FILLS_OF_EACH_BYTE {
        for byte in FILLS {
            let Some(sound) = make(byte) else {
                return StartCheck::NotChecked;
            };
            let differs = how_different(&first, &sound) > SAME_WITHIN;
            filled.push((byte, sound));
            if differs {
                // (One is enough to say it, and the rest is time that a library of hundreds of plugins is not given.)
                return judge((&first, &second), &filled);
            }
        }
    }
    judge((&first, &second), &filled)
}
