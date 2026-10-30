// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Surge XT, with its factory and third-party patches, played through programs of their own (phase 14), to look at the crash that it was measured to have when it is run in the editor's
//! process (`docs/bundleable-synthesizers.md`, "Playing the patches"): **Surge XT dies at random when a patch is loaded as its state and notes are played.** Every play here is a
//! program of its own, so a crash ends the program and not this test; each is counted, with the patch and the block it happened in.
//!
//! Ignored: it needs the folder that holds `surge-xt-synthesizer` (the plugin) and `SurgeXTData` (the patches), as `tools/bundled_plugins/fetch_bundled_plugins.py` makes it
//! (`FFRS_BUNDLED_PLUGINS`). What it plays is told by environment variables (all optional):
//!
//! * `SURGE_PATCHES`: `all`, `random:N` (default `random:200`), or names (a word of the path of a patch, separated by `;`: `Quiz;Toms 1`).
//! * `SURGE_PLAYS`: how many plays (default: one for each patch chosen). A list that is shorter is gone through again.
//! * `SURGE_BLOCKS`: blocks of 512 samples in a play (default 100); `SURGE_NOTE_AT_BLOCK`: the block the note begins in (default 0); `SURGE_NOTE_LENGTH`: blocks it is held (default 50).
//! * `SURGE_THREADS`: programs at the same time (default 6).
//! * `SURGE_DELAY_MS`: how long to wait between the plugin being made and the first block (default 0).
//! * `SURGE_IN_THIS_PROCESS=1`: the plays are made one after the other **in this process, as the editor made them before phase 14** (a crash ends the test: the lines that say `in this process: play N` show
//!   how far it got).
//! * `SURGE_NO_STATE=1`: no patch is given (the default patch), `SURGE_NO_NOTES=1`: no note is played.
//! * `SURGE_START_AGAIN=1`: the program is started again when it ends (as the editor does), and the play goes on.
//! * `SURGE_SEED`: for `random`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fastforwardrs::plugin_host::{PluginInstance, WhatWasScanned};
use fastforwardrs::plugin_process::protocol::{Address, Make};
use fastforwardrs::plugin_process::remote::{Launcher, Limits};
use fastforwardrs::plugin_scan::effect_of_a_clap_plugin;
use fastforwardrs::project::PluginAddress;
use fastforwardrs::sound_engine::{NoteEvent, NoteEventKind};

fn the_root() -> PathBuf {
    PathBuf::from(
        std::env::var("FFRS_BUNDLED_PLUGINS")
            .expect("FFRS_BUNDLED_PLUGINS names the folder with Surge XT and its data"),
    )
}

fn find(folder: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(folder).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|file| file == name) {
            return Some(path);
        }
    }
    None
}

fn patches_in(folder: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            patches_in(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "fxp") {
            found.push(path);
        }
    }
}

fn all_the_patches() -> Vec<PathBuf> {
    let data = the_root().join("SurgeXTData");
    let mut found = Vec::new();
    patches_in(&data.join("patches_factory"), &mut found);
    patches_in(&data.join("patches_3rdparty"), &mut found);
    assert!(found.len() > 3000, "the patches are there: {}", found.len());
    found
}

fn number_from(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// A number for the nth thing, the same every time for one seed (xorshift).
fn random_order(count: usize, how_many: usize, seed: u64) -> Vec<usize> {
    let mut state = seed | 1;
    (0..how_many)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 11) as usize % count
        })
        .collect()
}

fn the_patches_chosen() -> Vec<PathBuf> {
    let everything = all_the_patches();
    let wanted = std::env::var("SURGE_PATCHES").unwrap_or_else(|_| "random:200".to_owned());
    if wanted == "all" {
        return everything;
    }
    if let Some(count) = wanted.strip_prefix("random:") {
        let seed = std::env::var("SURGE_SEED")
            .ok()
            .and_then(|seed| seed.parse().ok())
            .unwrap_or(12345);
        return random_order(everything.len(), count.parse().expect("a number after random:"), seed)
            .into_iter()
            .map(|place| everything[place].clone())
            .collect();
    }
    wanted
        .split(';')
        .map(|word| {
            everything
                .iter()
                .find(|path| path.to_string_lossy().replace('\\', "/").contains(word))
                .unwrap_or_else(|| panic!("no patch has {word:?} in its path"))
                .clone()
        })
        .collect()
}

/// The chunk of a `.fxp` (what follows its header of 60 bytes), which is what Surge XT takes as its state.
fn chunk_of(patch: &Path) -> Vec<u8> {
    let bytes = std::fs::read(patch).expect("the patch is read");
    assert!(bytes.len() > 64 && &bytes[0..4] == b"CcnK", "{patch:?} is a patch");
    bytes[60..].to_vec()
}

fn the_name_of(patch: &Path) -> String {
    let parts: Vec<_> = patch
        .components()
        .rev()
        .take(3)
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    format!("{}/{}/{}", parts[2], parts[1], parts[0].trim_end_matches(".fxp"))
}

fn on(time: u32, key: u8) -> NoteEvent {
    NoteEvent {
        time,
        what: NoteEventKind::On {
            channel: 0,
            key,
            velocity: 0.8,
            id: 1,
        },
    }
}

fn off(time: u32, key: u8) -> NoteEvent {
    NoteEvent {
        time,
        what: NoteEventKind::Off {
            channel: 0,
            key,
            velocity: 0.5,
            id: 1,
        },
    }
}

/// What happened in one play.
#[derive(Debug)]
struct Play {
    patch: String,
    /// The block the program ended in and the words, if it did and the copy was gone; and how many times it was started again.
    ended_in: Option<(usize, String)>,
    started_again: usize,
    /// How loud the sound was (the peak) when the note was held.
    peak: f32,
    seconds_to_start: f64,
}

fn play_one(launcher: &Launcher, make: &Make, patch: &Path, start_again: bool) -> Play {
    let state: Option<Arc<[u8]>> = (std::env::var("SURGE_NO_STATE").is_err()).then(|| Arc::from(chunk_of(patch)));
    let blocks = number_from("SURGE_BLOCKS", 100);
    let note_at = number_from("SURGE_NOTE_AT_BLOCK", 0);
    let note_for = number_from("SURGE_NOTE_LENGTH", 50);
    let no_notes = std::env::var("SURGE_NO_NOTES").is_ok();
    let began = Instant::now();
    let started = if start_again {
        launcher.start_and_start_again_when_it_ends(make, state)
    } else {
        launcher.start(make, state.as_deref())
    };
    let seconds_to_start = began.elapsed().as_secs_f64();
    std::thread::sleep(Duration::from_millis(number_from("SURGE_DELAY_MS", 0) as u64));
    let mut result = Play {
        patch: the_name_of(patch),
        ended_in: None,
        started_again: 0,
        peak: 0.0,
        seconds_to_start,
    };
    let (mut copy, _) = match started {
        Ok(started) => started,
        Err(not_started) => {
            result.ended_in = Some((0, format!("not started: {not_started}")));
            return result;
        }
    };
    let (mut left, mut right) = (vec![0.0_f32; 512], vec![0.0_f32; 512]);
    for block in 0..blocks {
        let notes: Vec<NoteEvent> = if no_notes {
            Vec::new()
        } else if block == note_at {
            vec![on(0, 60)]
        } else if block == note_at + note_for {
            vec![off(0, 60)]
        } else {
            Vec::new()
        };
        left.fill(0.0);
        right.fill(0.0);
        // (What the effect does between blocks: it asks the copy what happened.)
        let _ = copy.between_blocks();
        match copy.run_with_notes(&mut [], &mut [&mut left, &mut right], 512, &notes) {
            Ok(()) => {
                if block >= note_at && block < note_at + note_for {
                    result.peak = left
                        .iter()
                        .chain(right.iter())
                        .fold(result.peak, |peak, sample| peak.max(sample.abs()));
                }
            }
            Err(why) => {
                result.ended_in = Some((block, why));
                break;
            }
        }
    }
    result.started_again = copy.times_started_again();
    result
}

/// Plays the patches one after another in this process, as the editor did before phase 14. A crash ends the test.
fn play_in_this_process(library: &Path, id: &str, patches: &[PathBuf], plays: usize) {
    let library_loaded = clap_host::Library::open(library).expect("Surge XT is loaded");
    let worker = clap_host::MainThread::start();
    let blocks = number_from("SURGE_BLOCKS", 100);
    let note_at = number_from("SURGE_NOTE_AT_BLOCK", 0);
    let note_for = number_from("SURGE_NOTE_LENGTH", 50);
    let began = Instant::now();
    for number in 0..plays {
        let patch = &patches[number % patches.len()];
        if number % 100 == 0 {
            eprintln!("in this process: play {number} ({:.0?})", began.elapsed());
        }
        let state: Arc<[u8]> = Arc::from(chunk_of(patch));
        let lives = Arc::<Mutex<fastforwardrs::plugin_host::Lives>>::default();
        let alive = fastforwardrs::plugin_host::Lives::is_alive(&lives, library, 0);
        let maker = fastforwardrs::plugin_host::ClapMaker {
            worker: Arc::clone(&worker),
            library: library_loaded.clone(),
            id: id.to_owned(),
            state: Some(state),
        };
        let (instance, _) = maker
            .make_and_say_if_the_state_was_not_taken()
            .expect("Surge XT is made");
        let mut copy = fastforwardrs::plugin_host::ClapCopy::new(instance, maker, alive);
        let (mut left, mut right) = (vec![0.0_f32; 512], vec![0.0_f32; 512]);
        for block in 0..blocks {
            let notes: Vec<NoteEvent> = if block == note_at {
                vec![on(0, 60)]
            } else if block == note_at + note_for {
                vec![off(0, 60)]
            } else {
                Vec::new()
            };
            let _ = copy.between_blocks();
            copy.run_with_notes(&mut [], &mut [&mut left, &mut right], 512, &notes)
                .expect("the block is run");
        }
    }
    eprintln!(
        "SURGE IN THIS PROCESS: {plays} plays, none ended the test ({:.0?})",
        began.elapsed()
    );
}

#[test]
#[ignore = "needs Surge XT and its patches (FFRS_BUNDLED_PLUGINS) and plays hundreds of times"]
fn surge_xt_played_through_programs_of_its_own_says_how_often_and_where_it_ends() {
    let library = find(&the_root().join("surge-xt-synthesizer"), "Surge XT.clap").expect("Surge XT is there");
    let scan = clap_host::scan::scan_library(&library, |_| {}).expect("Surge XT is scanned");
    let installed = scan
        .plugins
        .iter()
        .filter_map(|plugin| effect_of_a_clap_plugin(&library, plugin).ok())
        .find(|plugin| plugin.descriptor.makes_sound_from_notes)
        .expect("Surge XT is a synthesizer");
    let PluginAddress::Clap { id } = &installed.address else {
        unreachable!("a CLAP plugin")
    };
    let make = Make {
        address: Address::Clap { id: id.clone() },
        library,
        scanned: WhatWasScanned::of(&installed),
        has_state: std::env::var("SURGE_NO_STATE").is_err(),
    };
    let launcher =
        Launcher::new(PathBuf::from(env!("CARGO_BIN_EXE_fastforwardrs-plugin-process"))).with_limits(Limits {
            to_connect: Duration::from_secs(60),
            to_make: Duration::from_secs(60),
            to_run: Duration::from_secs(10),
        });
    let patches = the_patches_chosen();
    let plays = number_from("SURGE_PLAYS", patches.len());
    if std::env::var("SURGE_IN_THIS_PROCESS").is_ok() {
        let PluginAddress::Clap { id } = &installed.address else {
            unreachable!()
        };
        play_in_this_process(&make.library, id, &patches, plays);
        return;
    }
    let threads = number_from("SURGE_THREADS", 6).max(1);
    let start_again = std::env::var("SURGE_START_AGAIN").is_ok();
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::<Play>::new());
    let began = Instant::now();

    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let number = next.fetch_add(1, Ordering::SeqCst);
                    if number >= plays {
                        return;
                    }
                    let play = play_one(&launcher, &make, &patches[number % patches.len()], start_again);
                    if let Some((block, why)) = &play.ended_in {
                        eprintln!("play {number} ENDED in block {block}: {} :: {why}", play.patch);
                    }
                    results.lock().unwrap().push(play);
                }
            });
        }
    });

    let results = results.into_inner().unwrap();
    let ended: Vec<&Play> = results.iter().filter(|play| play.ended_in.is_some()).collect();
    let started_again: usize = results.iter().map(|play| play.started_again).sum();
    let mut kinds = std::collections::BTreeMap::<String, usize>::new();
    for play in &ended {
        let words = &play.ended_in.as_ref().unwrap().1;
        let kind = if let (Some(open), Some(close)) = (words.find('('), words.find(')')) {
            words[open..=close].to_owned()
        } else {
            words.chars().take(60).collect()
        };
        *kinds.entry(kind).or_default() += 1;
    }
    let quiet = results
        .iter()
        .filter(|play| play.ended_in.is_none() && play.peak < 1e-4)
        .count();
    eprintln!(
        "SURGE SUMMARY: {} plays of {} patches in {:.0?} ({} programs at a time{}); {} plays ended in the copy being gone ({:.2}%); {} times a program was started again; \
         {} plays that went on to the end made no sound while the note was held; how they ended: {kinds:?}; mean start {:.0?} ms",
        results.len(),
        patches.len(),
        began.elapsed(),
        threads,
        if start_again {
            ", started again when they end"
        } else {
            ""
        },
        ended.len(),
        100.0 * ended.len() as f64 / results.len() as f64,
        started_again,
        quiet,
        1000.0 * results.iter().map(|play| play.seconds_to_start).sum::<f64>() / results.len() as f64,
    );
    let mut by_patch = std::collections::BTreeMap::<&str, (usize, usize)>::new();
    for play in &results {
        let entry = by_patch.entry(play.patch.as_str()).or_default();
        entry.0 += 1;
        entry.1 += usize::from(play.ended_in.is_some() || play.started_again > 0);
    }
    let mut worst: Vec<_> = by_patch.into_iter().filter(|(_, (_, ended))| *ended > 0).collect();
    worst.sort_by_key(|(_, (plays, ended))| std::cmp::Reverse((*ended * 1000 / plays, *ended)));
    for (patch, (plays, ended)) in worst.iter().take(15) {
        eprintln!("  {ended} of {plays}: {patch}");
    }
}
