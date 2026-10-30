// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A copy of a plugin in a process of its own (phase 14), tried with the real program (`fastforwardrs-plugin-process`) and the plugins that are written for the tests.
//!
//! * **The same blocks through a copy in this process and a copy in the program give the same bits** (`PluginInstance` is the seam: the copy in this process is made the way the
//!   editor makes one, by hand and not by the program's own code, so that the two are not the same code twice).
//! * **What a plugin can do to its process, it does to the program**: crash it, never return, end it, damage nothing here. The test program is still there after each.

use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use clap_test_plugins as clap_plugins;
use fastforwardrs::plugin_host::{
    BetweenBlocks, ClapCopy, ClapMaker, LadspaCopy, Lives, Lv2Copy, PluginInstance, WhatWasScanned,
};
use fastforwardrs::plugin_process::protocol::{
    Address, FrameKind, Hello, Make, Refused, TOKEN_VARIABLE, read_frame, write_frame,
};
use fastforwardrs::plugin_process::remote::{Launcher, Limits, NotStarted, RemoteCopy};
use fastforwardrs::plugin_scan::{effect_of_a_plugin, effect_of_an_lv2_plugin};
use fastforwardrs::project::PluginAddress;
use fastforwardrs::sound_engine::{NoteEvent, NoteEventKind};

fn the_program() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fastforwardrs-plugin-process"))
}

fn launcher() -> Launcher {
    Launcher::new(the_program())
}

/// The limits for a plugin that misbehaves as `how` says: **short only for the one a test is about** (a slow computer must not make a good start look like a hung one).
fn limits_for(how: &str) -> Limits {
    let short = Duration::from_secs(2);
    let long = Duration::from_secs(60);
    Limits {
        to_connect: long,
        to_make: if how.starts_with("hang_when_made") || how.starts_with("hang_when_loaded") {
            short
        } else {
            long
        },
        to_run: if how == "hang_when_run" { short } else { long },
    }
}

// --- what is played ---

#[derive(Clone)]
enum Step {
    Value {
        handle: u32,
        value: f32,
        at_sample: usize,
    },
    Reset,
    Block {
        frames: usize,
        inputs: usize,
        outputs: usize,
        notes: Option<Vec<NoteEvent>>,
    },
}

fn block(frames: usize, inputs: usize, outputs: usize) -> Step {
    Step::Block {
        frames,
        inputs,
        outputs,
        notes: None,
    }
}

fn block_with_notes(frames: usize, outputs: usize, notes: Vec<NoteEvent>) -> Step {
    Step::Block {
        frames,
        inputs: 0,
        outputs,
        notes: Some(notes),
    }
}

fn on(time: u32, key: u8, velocity: f32) -> NoteEvent {
    NoteEvent {
        time,
        what: NoteEventKind::On {
            channel: 0,
            key,
            velocity,
            id: u32::from(key),
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
            id: u32::from(key),
        },
    }
}

/// What was seen of a block: everything an effect looks at.
#[derive(Debug, PartialEq)]
struct Seen {
    between_blocks: BetweenBlocks,
    /// The sound that came out, as bits (a number that is not a number is only the same if its bits are), or why the run failed.
    sound: Result<Vec<Vec<u32>>, String>,
    latency: usize,
    meters: Vec<(u32, u32)>,
}

/// The input of a block: a different number at every sample of every channel.
fn input_of(channel: usize, frames: usize, block_number: usize) -> Vec<f32> {
    (0..frames)
        .map(|sample| ((sample + 7 * channel + 31 * block_number) % 101) as f32 / 100.0 - 0.5)
        .collect()
}

/// Plays a script through a copy the way the effect does: asks what it did between blocks, gives the values, runs the block, and looks at what is there after it.
fn play(copy: &mut dyn PluginInstance, script: &[Step]) -> Vec<Seen> {
    let mut seen = Vec::new();
    for (number, step) in script.iter().enumerate() {
        match step {
            Step::Value {
                handle,
                value,
                at_sample,
            } => copy.set_value(*handle, *value, *at_sample),
            Step::Reset => copy.reset(),
            Step::Block {
                frames,
                inputs,
                outputs,
                notes,
            } => {
                let between_blocks = copy.between_blocks();
                let mut input: Vec<Vec<f32>> = (0..*inputs).map(|channel| input_of(channel, *frames, number)).collect();
                let mut output: Vec<Vec<f32>> = vec![vec![0.0; *frames]; *outputs];
                let result = {
                    let mut input_slices: Vec<&mut [f32]> = input.iter_mut().map(Vec::as_mut_slice).collect();
                    let mut output_slices: Vec<&mut [f32]> = output.iter_mut().map(Vec::as_mut_slice).collect();
                    match notes {
                        Some(notes) => copy.run_with_notes(&mut input_slices, &mut output_slices, *frames, notes),
                        None => copy.run(&mut input_slices, &mut output_slices, *frames),
                    }
                };
                seen.push(Seen {
                    between_blocks,
                    sound: result.map(|()| {
                        output
                            .iter()
                            .map(|channel| channel.iter().map(|sample| sample.to_bits()).collect())
                            .collect()
                    }),
                    latency: copy.latency(),
                    meters: copy
                        .outputs()
                        .into_iter()
                        .map(|(handle, value)| (handle, value.to_bits()))
                        .collect(),
                });
            }
        }
    }
    seen
}

// --- the plugins ---

/// A plugin of one of the libraries for the tests, as far as making a copy of it goes.
trait Subject {
    /// A copy in this process, made the way the editor makes one, and what was said of a state that it did not take.
    fn in_this_process(&self, state: Option<&[u8]>) -> (Box<dyn PluginInstance>, Option<String>);
    fn what_to_make(&self, has_state: bool) -> Make;
}

struct ClapPlugin {
    library: PathBuf,
    id: String,
    scanned: WhatWasScanned,
}

fn a_clap_plugin(id: &str, inputs: usize, outputs: usize, value_handles: &[u32]) -> ClapPlugin {
    ClapPlugin {
        library: clap_plugins::library_file_for_tests(),
        id: id.to_owned(),
        scanned: WhatWasScanned {
            audio_inputs: inputs,
            audio_outputs: outputs,
            value_handles: value_handles.to_vec(),
            ladspa_label: None,
        },
    }
}

impl Subject for ClapPlugin {
    fn in_this_process(&self, state: Option<&[u8]>) -> (Box<dyn PluginInstance>, Option<String>) {
        let library = clap_host::Library::open(&self.library).expect("the test library is loaded");
        let lives = Arc::<Mutex<Lives>>::default();
        let alive = Lives::is_alive(&lives, &self.library, 0);
        let maker = ClapMaker {
            worker: clap_host::MainThread::start(),
            library,
            id: self.id.clone(),
            state: state.map(Arc::from),
        };
        let (instance, state_not_taken) = maker
            .make_and_say_if_the_state_was_not_taken()
            .expect("the test plugin is made");
        (Box::new(ClapCopy::new(instance, maker, alive)), state_not_taken)
    }

    fn what_to_make(&self, has_state: bool) -> Make {
        Make {
            address: Address::Clap { id: self.id.clone() },
            library: self.library.clone(),
            scanned: self.scanned.clone(),
            has_state,
        }
    }
}

fn started(
    launcher: &Launcher,
    subject: &dyn Subject,
    state: Option<&[u8]>,
) -> Result<(RemoteCopy, fastforwardrs::plugin_process::protocol::Made), NotStarted> {
    launcher.start(&subject.what_to_make(state.is_some()), state)
}

/// Plays `script` through a copy in this process and through a copy in the program, and demands that everything seen is the same. Gives what was seen.
fn the_same_in_the_program(subject: &dyn Subject, state: Option<&[u8]>, script: &[Step]) -> Vec<Seen> {
    let (mut local, local_state_not_taken) = subject.in_this_process(state);
    let (mut remote, made) = started(&launcher(), subject, state).expect("the program makes the plugin");

    let in_this_process = play(local.as_mut(), script);
    let in_the_program = play(&mut remote, script);

    assert_eq!(made.state_not_taken, local_state_not_taken, "what is said of the state");
    assert_eq!(made.takes_values_at_samples, local.takes_values_at_samples());
    assert_eq!(made.most_samples_at_once, local.most_samples_at_once());
    assert_eq!(in_the_program, in_this_process);
    assert!(!in_this_process.is_empty(), "something was played");
    in_this_process
}

fn the_sound(seen: &Seen) -> Vec<Vec<f32>> {
    seen.sound
        .as_ref()
        .expect("the run went well")
        .iter()
        .map(|channel| channel.iter().map(|bits| f32::from_bits(*bits)).collect())
        .collect()
}

const GAIN: u32 = 1;

// --- CLAP: the same in the program ---

#[test]
fn an_instrument_plays_the_same_notes_in_the_program_as_in_this_process_through_holds_releases_resets_and_values() {
    let instrument = a_clap_plugin(clap_plugins::INSTRUMENT, 0, 2, &[GAIN]);
    let script = [
        Step::Value {
            handle: GAIN,
            value: 0.5,
            at_sample: 0,
        },
        block_with_notes(512, 2, vec![on(10, 60, 0.5), on(200, 72, 1.0)]),
        // (Held through a block with no notes, and a value in the middle of it.)
        Step::Value {
            handle: GAIN,
            value: 2.0,
            at_sample: 100,
        },
        block_with_notes(512, 2, Vec::new()),
        block_with_notes(512, 2, vec![off(50, 60)]),
        Step::Reset,
        block_with_notes(512, 2, vec![on(0, 48, 0.25)]),
        block_with_notes(
            512,
            2,
            vec![
                NoteEvent {
                    time: 3,
                    what: NoteEventKind::AllOff,
                },
                on(4, 40, 0.75),
            ],
        ),
        block_with_notes(64, 2, Vec::new()),
    ];

    let seen = the_same_in_the_program(&instrument, None, &script);

    // (Not a test of silence against silence: the first note is key 60 at velocity 0.5, 0.5 * 60 / 100 = 0.3, times the gain 0.5.)
    let first = the_sound(&seen[0]);
    assert_eq!(first[0][0], 0.0);
    assert!((first[0][20] - 0.15).abs() < 1e-6, "{}", first[0][20]);
    assert_eq!(first[0], first[1], "both channels");
}

#[test]
fn an_effect_gives_the_same_sound_in_the_program_as_in_this_process_with_values_at_samples() {
    let gain = a_clap_plugin(clap_plugins::GAIN, 2, 2, &[GAIN]);
    let script = [
        block(512, 2, 2),
        Step::Value {
            handle: GAIN,
            value: 0.25,
            at_sample: 0,
        },
        Step::Value {
            handle: GAIN,
            value: 3.0,
            at_sample: 200,
        },
        block(512, 2, 2),
        block(100, 2, 2),
        Step::Reset,
        block(512, 2, 2),
    ];

    let seen = the_same_in_the_program(&gain, None, &script);

    // (The gain changes at sample 200 of the second block: 0.25 before it and 3 from it.)
    let input = input_of(0, 512, 3);
    let sound = the_sound(&seen[1]);
    assert_eq!(sound[0][199], input[199] * 0.25);
    assert_eq!(sound[0][200], input[200] * 3.0);
}

#[test]
fn a_plugin_with_a_latency_and_one_with_a_tail_say_and_do_the_same_in_the_program() {
    let latency = a_clap_plugin(clap_plugins::LATENCY, 2, 2, &[]);
    let seen = the_same_in_the_program(&latency, None, &[block(512, 2, 2), block(512, 2, 2)]);
    assert_eq!(seen[0].latency, clap_plugins::LATENCY_SAMPLES as usize);

    let tail = a_clap_plugin(clap_plugins::TAIL, 2, 2, &[]);
    let seen = the_same_in_the_program(
        &tail,
        None,
        &[block(512, 2, 2), block(512, 2, 2), Step::Reset, block(512, 2, 2)],
    );
    // (An echo rings: there is sound in the second block, and the reset between the second and the third was given in its place.)
    assert!(the_sound(&seen[1])[0].iter().any(|sample| *sample != 0.0));
}

#[test]
fn a_plugin_that_asks_to_be_made_again_is_made_again_in_the_program_and_it_is_said_between_the_blocks_it_would_have_been()
 {
    let callbacks = a_clap_plugin(clap_plugins::CALLBACKS, 2, 2, &[GAIN]);
    let script: Vec<Step> = std::iter::repeat_n(block(512, 2, 2), 9).collect();

    let seen = the_same_in_the_program(&callbacks, None, &script);

    // (Not a test of nothing against nothing: it was made again, and then again, until it was not any more.)
    let restarts = seen.iter().filter(|block| block.between_blocks.was_restarted).count();
    assert!(restarts >= 2, "{restarts} restarts");
    assert!(
        seen.iter().any(|block| block
            .between_blocks
            .note
            .as_deref()
            .is_some_and(|note| note.contains("more than"))),
        "it was said once that it was not made again any more"
    );

    let latency_changes = a_clap_plugin(clap_plugins::LATENCY_CHANGES, 2, 2, &[]);
    let seen = the_same_in_the_program(
        &latency_changes,
        None,
        &[block(512, 2, 2), block(512, 2, 2), block(512, 2, 2)],
    );
    // (It says that its latency changed and it is the same number when it is made again: so it is made again and there is no word of a latency.)
    assert!(seen.iter().any(|block| block.between_blocks.was_restarted));
}

#[test]
fn a_plugin_that_says_it_could_not_run_says_the_same_words_from_the_program() {
    let fails = a_clap_plugin(clap_plugins::FAILS_TO_PROCESS, 2, 2, &[]);

    let seen = the_same_in_the_program(&fails, None, &[block(512, 2, 2), block(512, 2, 2)]);

    assert!(seen[0].sound.is_err(), "it said that it failed");
}

#[test]
fn the_state_of_a_plugin_reaches_the_copy_in_the_program_and_what_it_says_of_a_state_it_cannot_read_is_the_same() {
    let editor = a_clap_plugin(clap_plugins::EDITOR, 2, 2, &[GAIN]);
    let script = [block(512, 2, 2)];

    let with_edits = the_same_in_the_program(&editor, Some(b"created=0 edits=2"), &script);
    let without = the_same_in_the_program(&editor, None, &script);

    // (The state says two edits, and the plugin's sound is multiplied by 1 + 0.5 edits: twice as loud.)
    let (loud, plain) = (the_sound(&with_edits[0]), the_sound(&without[0]));
    assert_eq!(loud[0][5], plain[0][5] * 2.0);
    // A state that is not text is a state that the plugin cannot read: it says so, in the words of the copy in this process.
    let unreadable = the_same_in_the_program(&editor, Some(&[0xFF, 0xFE, 0xFD]), &script);
    let (_, said) = editor.in_this_process(Some(&[0xFF, 0xFE, 0xFD]));
    assert!(said.is_some(), "the plugin said that it could not read it");
    assert_eq!(
        the_sound(&unreadable[0])[0][5],
        plain[0][5],
        "the plugin goes on as it was"
    );
    // And a state of no bytes at all is a state (the plugin reads nothing from it), and is not no state.
    let empty = the_same_in_the_program(&editor, Some(&[]), &script);
    assert_eq!(the_sound(&empty[0])[0][5], plain[0][5]);
}

#[test]
fn the_side_chain_instrument_and_the_midi_instrument_are_the_same_in_the_program() {
    let notes = vec![on(0, 64, 0.5), on(100, 67, 0.5), off(300, 64)];
    let side_chain = a_clap_plugin(clap_plugins::INSTRUMENT_WITH_A_SIDE_CHAIN, 0, 2, &[GAIN]);
    the_same_in_the_program(
        &side_chain,
        None,
        &[
            block_with_notes(512, 2, notes.clone()),
            block_with_notes(512, 2, Vec::new()),
        ],
    );
    let midi = a_clap_plugin(clap_plugins::INSTRUMENT_MIDI, 0, 2, &[GAIN]);
    the_same_in_the_program(
        &midi,
        None,
        &[block_with_notes(512, 2, notes), block_with_notes(512, 2, Vec::new())],
    );
}

#[test]
fn two_copies_of_a_plugin_that_allows_no_other_alive_do_not_disturb_each_other_in_processes_of_their_own() {
    let single = a_clap_plugin(clap_plugins::SINGLE_INSTANCE, 2, 2, &[]);
    let script = [block(64, 2, 2)];

    // In this process the second copy is made while the first is alive, and gives its sound with the sign changed.
    let (mut first_here, _) = single.in_this_process(None);
    let (mut second_here, _) = single.in_this_process(None);
    let first_here = play(first_here.as_mut(), &script);
    let second_here = play(second_here.as_mut(), &script);
    assert_eq!(the_sound(&second_here[0])[0][3], -the_sound(&first_here[0])[0][3]);

    // In two programs they are two plugins, and both are as the first was.
    let (mut first, _) = started(&launcher(), &single, None).unwrap();
    let (mut second, _) = started(&launcher(), &single, None).unwrap();
    let first = play(&mut first, &script);
    let second = play(&mut second, &script);
    assert_eq!(first, first_here);
    assert_eq!(second, first_here, "the second is as the first");
}

// --- what a plugin can do to its process ---

/// Whether the system has a process with this number.
fn a_process_is_there(id: u32) -> bool {
    if cfg!(windows) {
        let listed = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {id}"), "/NH"])
            .output()
            .expect("tasklist is there");
        String::from_utf8_lossy(&listed.stdout).contains(&id.to_string())
    } else {
        Command::new("kill")
            .args(["-0", &id.to_string()])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }
}

fn wait_until_the_process_is_gone(id: u32) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while a_process_is_there(id) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    !a_process_is_there(id)
}

fn a_launcher_for_a_plugin_that_misbehaves(how: &str) -> Launcher {
    launcher()
        .with_environment(vec![("FFRS_TEST_PLUGIN".to_owned(), how.to_owned())])
        .with_limits(limits_for(how))
}

#[test]
fn a_plugin_that_crashes_its_process_while_it_runs_is_an_error_of_that_run_and_the_editor_is_still_here() {
    let gain = a_clap_plugin(clap_plugins::GAIN, 2, 2, &[GAIN]);
    let (mut copy, _) = started(&a_launcher_for_a_plugin_that_misbehaves("crash_when_run"), &gain, None).unwrap();
    let pid = copy.process_id().expect("the program is there");
    assert!(a_process_is_there(pid));
    let (mut left, mut right) = ([0.0_f32; 512], [0.0_f32; 512]);

    let error = copy
        .run(
            &mut [&mut [0.5; 512], &mut [0.5; 512]],
            &mut [&mut left, &mut right],
            512,
        )
        .unwrap_err();

    assert!(error.contains("the program that runs the plugin ended"), "{error}");
    assert!(copy.gone_because().is_some());
    assert_eq!(
        copy.run(
            &mut [&mut [0.5; 512], &mut [0.5; 512]],
            &mut [&mut left, &mut right],
            512
        )
        .unwrap_err(),
        error,
        "every call after that says the same"
    );
    assert!(wait_until_the_process_is_gone(pid), "the program is gone");
    assert_eq!(
        copy.latency(),
        0,
        "a copy that is gone can still be asked what it is like"
    );
    copy.reset();
    assert_eq!(copy.between_blocks(), BetweenBlocks::default());
}

#[test]
fn a_plugin_that_ends_its_process_while_it_runs_and_one_that_hangs_it_are_each_an_error_and_a_hung_program_is_stopped()
{
    let gain = a_clap_plugin(clap_plugins::GAIN, 2, 2, &[GAIN]);
    let (mut left, mut right) = ([0.0_f32; 512], [0.0_f32; 512]);

    let (mut copy, _) = started(&a_launcher_for_a_plugin_that_misbehaves("exit_when_run"), &gain, None).unwrap();
    let error = copy
        .run(
            &mut [&mut [0.5; 512], &mut [0.5; 512]],
            &mut [&mut left, &mut right],
            512,
        )
        .unwrap_err();
    assert!(error.contains("ended (exit code 0)"), "{error}");

    let (mut copy, _) = started(&a_launcher_for_a_plugin_that_misbehaves("hang_when_run"), &gain, None).unwrap();
    let pid = copy.process_id().unwrap();
    let began = Instant::now();
    let error = copy
        .run(
            &mut [&mut [0.5; 512], &mut [0.5; 512]],
            &mut [&mut left, &mut right],
            512,
        )
        .unwrap_err();
    assert!(
        error.contains("did not answer within 2 seconds, and was stopped"),
        "{error}"
    );
    assert!(
        began.elapsed() >= Duration::from_secs(2) && began.elapsed() < Duration::from_secs(60),
        "{:?}",
        began.elapsed()
    );
    assert!(wait_until_the_process_is_gone(pid), "the hung program is stopped");
}

#[test]
fn a_program_that_crashes_when_the_library_is_loaded_or_when_the_plugin_is_made_is_a_copy_that_was_not_started() {
    let gain = a_clap_plugin(clap_plugins::GAIN, 2, 2, &[GAIN]);
    for (how, words) in [
        ("crash_when_loaded", "ended"),
        ("crash_when_made", "ended"),
        ("crash_when_activated", "ended"),
        ("hang_when_made", "did not answer within 2 seconds"),
        ("hang_when_loaded", "did not answer within 2 seconds"),
    ] {
        let Err(not_started) = started(&a_launcher_for_a_plugin_that_misbehaves(how), &gain, None) else {
            panic!("{how}: the copy was made")
        };
        let NotStarted::TheProgram(why) = not_started else {
            panic!("{how}: {not_started:?}")
        };
        assert!(why.contains(words), "{how}: {why}");
    }
    // A library whose entry says no is a library that could not be loaded, which the program says in its own words.
    let Err(NotStarted::Refused(Refused::CouldNotBeLoaded(why))) =
        started(&a_launcher_for_a_plugin_that_misbehaves("init_says_no"), &gain, None)
    else {
        panic!("init says no")
    };
    assert!(!why.is_empty());
}

#[test]
fn what_a_plugin_prints_while_it_is_loaded_and_made_does_not_get_in_the_way() {
    let instrument = a_clap_plugin(clap_plugins::INSTRUMENT, 0, 2, &[GAIN]);
    let (mut copy, _) = started(
        &a_launcher_for_a_plugin_that_misbehaves("print_noise"),
        &instrument,
        None,
    )
    .unwrap();
    let (mut left, mut right) = ([0.0_f32; 512], [0.0_f32; 512]);

    copy.run_with_notes(&mut [], &mut [&mut left, &mut right], 512, &[on(0, 60, 1.0)])
        .unwrap();

    assert!((left[10] - 0.6).abs() < 1e-6);
}

#[test]
fn dropping_the_copy_ends_the_program_and_so_does_a_program_that_is_stopped_from_outside_a_copy_that_says_so() {
    let gain = a_clap_plugin(clap_plugins::GAIN, 2, 2, &[GAIN]);

    let (copy, _) = started(&launcher(), &gain, None).unwrap();
    let pid = copy.process_id().unwrap();
    assert!(a_process_is_there(pid));
    drop(copy);
    assert!(
        !a_process_is_there(pid),
        "the program is gone when the copy is dropped, not some time after"
    );

    let (mut copy, _) = started(&launcher(), &gain, None).unwrap();
    let pid = copy.process_id().unwrap();
    let (mut left, mut right) = ([0.0_f32; 64], [0.0_f32; 64]);
    copy.run(&mut [&mut [0.5; 64], &mut [0.5; 64]], &mut [&mut left, &mut right], 64)
        .unwrap();
    // Stopped from outside, by something that is not the editor (the operating system's own tool).
    let stopped = if cfg!(windows) {
        Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .status()
    } else {
        Command::new("kill").args(["-9", &pid.to_string()]).status()
    };
    assert!(stopped.unwrap().success());
    let error = copy
        .run(&mut [&mut [0.5; 64], &mut [0.5; 64]], &mut [&mut left, &mut right], 64)
        .unwrap_err();
    assert!(error.contains("the program that runs the plugin ended"), "{error}");
}

#[test]
fn a_program_ends_when_the_editor_closes_its_end_even_when_the_plugin_is_in_the_middle_of_a_call_that_never_returns() {
    let hang = a_clap_plugin(clap_plugins::GAIN, 2, 2, &[GAIN]);
    for how in ["", "hang_when_run"] {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let mut program = Command::new(the_program())
            .env(TOKEN_VARIABLE, "the token")
            .env("FFRS_TEST_PLUGIN", how)
            .args(["--connect", &listener.local_addr().unwrap().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        let hello = read_frame(&mut stream).unwrap();
        assert_eq!(hello.kind, FrameKind::Hello);
        assert_eq!(
            serde_json::from_slice::<Hello>(&hello.header).unwrap().token,
            "the token"
        );
        let make = serde_json::to_vec(&hang.what_to_make(false)).unwrap();
        write_frame(&mut stream, FrameKind::Make, &make, &[]).unwrap();
        assert_eq!(read_frame(&mut stream).unwrap().kind, FrameKind::Made);
        if !how.is_empty() {
            // A run that the plugin never returns from.
            let request = fastforwardrs::plugin_process::protocol::RunRequest {
                commands: Vec::new(),
                frames: 4,
                inputs: 2,
                outputs: 2,
                notes: None,
            };
            write_frame(&mut stream, FrameKind::Run, &request.header(), &[0; 2 * 4 * 4]).unwrap();
            std::thread::sleep(Duration::from_millis(300));
        }

        drop(stream);

        let deadline = Instant::now() + Duration::from_secs(10);
        let ended = loop {
            if let Some(status) = program.try_wait().unwrap() {
                break Some(status);
            }
            if Instant::now() > deadline {
                let _ = program.kill();
                break None;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(
            ended.is_some_and(|status| status.success()),
            "[{how}] the program ended by itself when its connection was closed"
        );
    }
}

#[test]
fn the_program_says_which_code_it_was_built_from_and_refuses_what_it_is_not_told_to_do() {
    let asked = Command::new(the_program()).arg("--build-id").output().unwrap();
    assert!(asked.status.success());
    assert!(
        fastforwardrs::plugin_scan::protocol::build_id_in(&String::from_utf8_lossy(&asked.stdout))
            .is_some_and(|id| id == fastforwardrs::plugin_scan::protocol::BUILD_ID)
    );
    for arguments in [
        vec![],
        vec!["--connect"],
        vec!["--connect", "not an address"],
        vec!["--something"],
    ] {
        let output = Command::new(the_program()).args(&arguments).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("Usage"),
            "{arguments:?}"
        );
    }
    // An address nobody listens at: the program has nothing to do and says so by its exit code.
    let nobody = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = nobody.local_addr().unwrap();
    drop(nobody);
    let status = Command::new(the_program())
        .args(["--connect", &address.to_string()])
        .status()
        .unwrap();
    assert_eq!(
        status.code(),
        Some(fastforwardrs::plugin_process::program::exit::NOT_CONNECTED)
    );
}

#[test]
fn a_plugin_that_is_not_there_is_not_what_was_scanned_or_cannot_be_made_is_refused_with_the_reason() {
    let gain = a_clap_plugin(clap_plugins::GAIN, 2, 2, &[GAIN]);
    let refused = |subject: &ClapPlugin| match started(&launcher(), subject, None) {
        Err(NotStarted::Refused(refused)) => refused,
        other => panic!("{:?}", other.map(|_| ())),
    };

    let mut no_such_library = a_clap_plugin(clap_plugins::GAIN, 2, 2, &[GAIN]);
    no_such_library.library = PathBuf::from("/there/is/no/such/library.clap");
    assert!(matches!(refused(&no_such_library), Refused::CouldNotBeLoaded(_)));

    let no_such_plugin = a_clap_plugin("org.example.nobody", 2, 2, &[GAIN]);
    assert!(matches!(refused(&no_such_plugin), Refused::CouldNotBeMade(_)));

    assert_eq!(
        refused(&a_clap_plugin(clap_plugins::GAIN, 1, 2, &[GAIN])),
        Refused::NotWhatWasScanned,
        "other inputs"
    );
    assert_eq!(
        refused(&a_clap_plugin(clap_plugins::GAIN, 2, 1, &[GAIN])),
        Refused::NotWhatWasScanned,
        "other outputs"
    );
    assert_eq!(
        refused(&a_clap_plugin(clap_plugins::GAIN, 2, 2, &[GAIN, 99])),
        Refused::NotWhatWasScanned,
        "a value it has no parameter for"
    );
    let _ = gain;
}

#[test]
fn an_lv2_or_a_ladspa_plugin_that_is_not_what_was_scanned_is_refused_as_a_clap_plugin_is() {
    let folder = tempfile::tempdir().unwrap();
    let mut lv2 = an_lv2_plugin(folder.path(), lv2_test_plugins::GAIN);
    lv2.scanned.audio_outputs += 1;
    let mut ladspa = a_ladspa_plugin("ffrs_test_gain");
    ladspa.scanned.audio_outputs += 1;
    let mut ladspa_with_another_label = a_ladspa_plugin("ffrs_test_gain");
    ladspa_with_another_label.scanned.ladspa_label = Some("ffrs_test_swap".to_owned());

    for (what, subject) in [
        ("lv2 ports", &lv2 as &dyn Subject),
        ("ladspa ports", &ladspa),
        ("a ladspa label", &ladspa_with_another_label),
    ] {
        match started(&launcher(), subject, None) {
            Err(NotStarted::Refused(refused)) => assert_eq!(refused, Refused::NotWhatWasScanned, "{what}"),
            other => panic!("{what}: {:?}", other.map(|_| ())),
        }
    }
}

#[test]
fn a_big_state_is_sent_whole() {
    // (The state of a synthesizer is megabytes; the test plugin reads text and takes the edits from it, so the edits are at the far end of 20 MiB of spaces.)
    let editor = a_clap_plugin(clap_plugins::EDITOR, 2, 2, &[GAIN]);
    let mut state = vec![b' '; 20 * 1024 * 1024];
    state.extend_from_slice(b"edits=2");

    let seen = the_same_in_the_program(&editor, Some(&state), &[block(64, 2, 2)]);

    let plain = the_same_in_the_program(&editor, None, &[block(64, 2, 2)]);
    assert_eq!(the_sound(&seen[0])[0][5], the_sound(&plain[0])[0][5] * 2.0);
}

// --- LV2 and LADSPA: the same in the program ---

struct Lv2Plugin {
    bundle: PathBuf,
    uri: String,
    scanned: WhatWasScanned,
}

fn an_lv2_plugin(folder: &Path, uri: &str) -> Lv2Plugin {
    let bundle = lv2_test_plugins::write_bundle(folder);
    let scan = lv2_host::scan_bundle(&bundle, &mut |_| {}).expect("the bundle is scanned");
    let installed = scan
        .plugins
        .iter()
        .filter_map(|plugin| effect_of_an_lv2_plugin(&bundle, plugin).ok())
        .find(|installed| matches!(&installed.address, PluginAddress::Lv2 { uri: found } if found == uri))
        .unwrap_or_else(|| panic!("no plugin called {uri}"));
    Lv2Plugin {
        bundle,
        uri: uri.to_owned(),
        scanned: WhatWasScanned::of(&installed),
    }
}

impl Lv2Plugin {
    /// An instance that is not wrapped in a copy: for what only the host crate can do to it.
    fn bare_instance(&self) -> lv2_host::Instance {
        let bundle = lv2_host::describe_bundle(&self.bundle).expect("the bundle is described");
        let description = bundle
            .plugins
            .iter()
            .find(|plugin| plugin.uri == self.uri)
            .expect("the plugin is there");
        // (Kept for the rest of the test run, as the editor keeps its libraries.)
        let library: &'static lv2_host::Library = Box::leak(Box::new(
            lv2_host::Library::open(&description.binary).expect("the library is loaded"),
        ));
        lv2_host::Instance::new(library, description, 48_000.0, 512).expect("the plugin is made")
    }
}

impl Subject for Lv2Plugin {
    fn in_this_process(&self, state: Option<&[u8]>) -> (Box<dyn PluginInstance>, Option<String>) {
        let mut instance = self.bare_instance();
        let state_not_taken = state
            .filter(|_| instance.has_state())
            .and_then(|bytes| instance.restore_state(bytes).err())
            .map(|error| error.to_string());
        (Box::new(Lv2Copy(instance)), state_not_taken)
    }

    fn what_to_make(&self, has_state: bool) -> Make {
        Make {
            address: Address::Lv2 { uri: self.uri.clone() },
            library: self.bundle.clone(),
            scanned: self.scanned.clone(),
            has_state,
        }
    }
}

struct LadspaPlugin {
    library: PathBuf,
    index: usize,
    scanned: WhatWasScanned,
}

fn a_ladspa_plugin(label: &str) -> LadspaPlugin {
    let library = ladspa_test_plugins::library_file_for_tests();
    let scan = ladspa_host::scan_library(&library, |_| {}).expect("the test library is scanned");
    let installed = scan
        .plugins
        .iter()
        .filter_map(|plugin| effect_of_a_plugin(&library, plugin).ok())
        .find(|installed| installed.descriptor.key.ends_with(label))
        .unwrap_or_else(|| panic!("no plugin called {label}"));
    LadspaPlugin {
        library,
        index: installed.address.ladspa_index().expect("a LADSPA plugin"),
        scanned: WhatWasScanned::of(&installed),
    }
}

impl Subject for LadspaPlugin {
    fn in_this_process(&self, _state: Option<&[u8]>) -> (Box<dyn PluginInstance>, Option<String>) {
        let library = ladspa_host::Library::open(&self.library).expect("the library is loaded");
        let instance = library.instantiate(self.index, 48_000).expect("the plugin is made");
        (Box::new(LadspaCopy(instance)), None)
    }

    fn what_to_make(&self, has_state: bool) -> Make {
        Make {
            address: Address::Ladspa { index: self.index },
            library: self.library.clone(),
            scanned: self.scanned.clone(),
            has_state,
        }
    }
}

#[test]
fn an_lv2_instrument_plays_the_same_in_the_program_as_in_this_process_and_its_control_output_is_the_same() {
    let folder = tempfile::tempdir().unwrap();
    let instrument = an_lv2_plugin(folder.path(), lv2_test_plugins::INSTRUMENT);
    let script = [
        block_with_notes(512, 2, vec![on(10, 60, 0.5), on(200, 72, 1.0)]),
        block_with_notes(512, 2, Vec::new()),
        block_with_notes(512, 2, vec![off(50, 60)]),
        Step::Reset,
        block_with_notes(256, 2, vec![on(0, 48, 0.25)]),
    ];

    let seen = the_same_in_the_program(&instrument, None, &script);

    // (Not a test of silence against silence: there is sound after the first note, and the control output counts the events of the block.)
    assert!(the_sound(&seen[0])[0].iter().any(|sample| *sample > 0.1));
    assert!(
        !seen[0].meters.is_empty(),
        "the control output is the meter that the window shows"
    );
    assert_eq!(
        f32::from_bits(seen[0].meters[0].1),
        2.0,
        "two events in the first block"
    );
}

#[test]
fn an_lv2_effect_gives_the_same_sound_in_the_program_with_values_set_between_pieces_of_a_block() {
    let folder = tempfile::tempdir().unwrap();
    let gain = an_lv2_plugin(folder.path(), lv2_test_plugins::GAIN);
    let handle = gain.scanned.value_handles[0];
    let script = [
        block(32, 2, 2),
        Step::Value {
            handle,
            value: 0.5,
            at_sample: 0,
        },
        block(32, 2, 2),
        Step::Value {
            handle,
            value: 3.0,
            at_sample: 0,
        },
        block(32, 2, 2),
        block(512, 2, 2),
    ];

    let seen = the_same_in_the_program(&gain, None, &script);

    // (The third block was run with the gain 3, the value that was given just before it.)
    let input = input_of(0, 32, 4);
    assert_eq!(the_sound(&seen[2])[0][4], input[4] * 3.0);
}

#[test]
fn an_lv2_plugin_with_a_latency_says_the_same_latency_in_the_program() {
    let folder = tempfile::tempdir().unwrap();
    let latency = an_lv2_plugin(folder.path(), lv2_test_plugins::LATENCY);

    let seen = the_same_in_the_program(&latency, None, &[block(512, 2, 2), block(512, 2, 2)]);

    assert_eq!(seen[0].latency, lv2_test_plugins::LATENCY_FRAMES);
}

#[test]
fn an_lv2_state_reaches_the_program_and_what_is_said_of_a_state_that_is_not_one_is_the_same() {
    let folder = tempfile::tempdir().unwrap();
    let stateful = an_lv2_plugin(folder.path(), lv2_test_plugins::STATE);
    let script = [block(64, 2, 2)];
    // A state that the plugin saved itself, and bytes that are not a state.
    let saved = stateful
        .bare_instance()
        .save_state()
        .expect("the plugin saves its state");

    the_same_in_the_program(&stateful, Some(&saved), &script);

    let (_, said) = stateful.in_this_process(Some(b"this is not a state"));
    assert!(said.is_some(), "the plugin said that it could not take it");
    the_same_in_the_program(&stateful, Some(b"this is not a state"), &script);
}

#[test]
fn ladspa_plugins_play_the_same_in_the_program_the_effect_that_remembers_and_the_one_with_a_control_output() {
    let gain = a_ladspa_plugin("ffrs_test_gain");
    let handle = gain.scanned.value_handles[0];
    the_same_in_the_program(
        &gain,
        None,
        &[
            block(32, 1, 1),
            Step::Value {
                handle,
                value: 2.5,
                at_sample: 0,
            },
            block(32, 1, 1),
            block(512, 1, 1),
        ],
    );

    let remembers = a_ladspa_plugin("ffrs_test_one_pole");
    let seen = the_same_in_the_program(
        &remembers,
        None,
        &[block(512, 1, 1), block(512, 1, 1), Step::Reset, block(512, 1, 1)],
    );
    assert!(the_sound(&seen[0])[0].iter().any(|sample| *sample != 0.0));

    let switches = a_ladspa_plugin("ffrs_test_switches");
    // (A LADSPA copy has no meters: the host does not read the control outputs of a LADSPA plugin, in this process or in the program; the same is seen.)
    let seen = the_same_in_the_program(&switches, None, &[block(64, 1, 1), block(64, 1, 1)]);
    assert!(seen[0].meters.is_empty());
}

// --- a program that ends is started again ---

fn started_again_when_it_ends(
    launcher: &Launcher,
    subject: &dyn Subject,
    state: Option<&[u8]>,
) -> Result<(RemoteCopy, fastforwardrs::plugin_process::protocol::Made), NotStarted> {
    launcher.start_and_start_again_when_it_ends(&subject.what_to_make(state.is_some()), state.map(Arc::from))
}

fn a_note(time: u32, what: NoteEventKind) -> NoteEvent {
    NoteEvent { time, what }
}

#[test]
fn an_instrument_whose_program_crashes_at_every_fourth_run_is_started_again_and_makes_the_sound_that_it_makes_without_a_crash()
 {
    let pedal = |time, value| {
        a_note(
            time,
            NoteEventKind::Controller {
                channel: 0,
                number: 64,
                value,
            },
        )
    };
    let wheel = |time, value| a_note(time, NoteEventKind::PitchBend { channel: 0, value });
    let script = [
        Step::Value {
            handle: GAIN,
            value: 0.5,
            at_sample: 0,
        },
        // 1: two things that are kept by the plugin (the pedal and the wheel) and a note
        block_with_notes(512, 2, vec![on(10, 60, 0.5), pedal(20, 127), wheel(30, 12_000)]),
        block_with_notes(512, 2, Vec::new()),
        block_with_notes(512, 2, vec![on(100, 72, 1.0)]),
        // 4: the program crashes here; two notes are held, and the pedal and the wheel are where they were put
        block_with_notes(512, 2, vec![off(50, 60)]),
        block_with_notes(512, 2, Vec::new()),
        Step::Value {
            handle: GAIN,
            value: 2.0,
            at_sample: 100,
        },
        block_with_notes(512, 2, Vec::new()),
        // 7: it crashes again, in the block that has a new note
        block_with_notes(512, 2, vec![on(0, 48, 0.25)]),
        block_with_notes(512, 2, vec![off(300, 72)]),
        block_with_notes(512, 2, vec![pedal(10, 0)]),
        // 10: a seek (a reset), and then it crashes in the block with the first note after it
        Step::Reset,
        block_with_notes(512, 2, vec![on(5, 55, 0.8)]),
        block_with_notes(512, 2, Vec::new()),
        block_with_notes(512, 2, vec![off(100, 55)]),
    ];
    for id in [clap_plugins::INSTRUMENT, clap_plugins::INSTRUMENT_MIDI] {
        let instrument = a_clap_plugin(id, 0, 2, &[GAIN]);
        let (mut without_a_crash, _) = instrument.in_this_process(None);
        let crashing = launcher().with_environment(vec![(
            "FFRS_TEST_PLUGIN".to_owned(),
            "crash_at_every_fourth_run".to_owned(),
        )]);
        let (mut with_crashes, _) =
            started_again_when_it_ends(&crashing, &instrument, None).expect("the program makes the plugin");
        let first_program = with_crashes.process_id().unwrap();

        let wanted = play(without_a_crash.as_mut(), &script);
        let heard = play(&mut with_crashes, &script);

        let sounds = |seen: &[Seen]| seen.iter().map(|block| block.sound.clone()).collect::<Vec<_>>();
        assert_eq!(
            sounds(&heard),
            sounds(&wanted),
            "{id}: the sound is the sound of a program that does not crash, bit for bit"
        );
        assert_eq!(heard.len(), 12);
        assert_eq!(
            with_crashes.times_started_again(),
            3,
            "{id}: runs 4, 7 and 10 of the program that counts its own"
        );
        assert_ne!(
            with_crashes.process_id().unwrap(),
            first_program,
            "{id}: it is another program"
        );
        let said: Vec<&str> = heard
            .iter()
            .filter_map(|block| block.between_blocks.note.as_deref())
            .collect();
        assert_eq!(
            said.len(),
            3,
            "{id}: it is said between the blocks, once for each: {said:?}"
        );
        assert!(
            said.iter()
                .all(|words| words.contains("the program that runs the plugin ended")
                    && words.contains("and was started again; the notes that were held are played again from here")),
            "{id}: {said:?}"
        );
        // (Not silence against silence: the first notes are heard in the blocks the crashes were in.)
        assert!(the_sound(&heard[3])[0].iter().any(|sample| *sample > 0.1), "{id}");
        assert!(the_sound(&heard[9])[0].iter().any(|sample| *sample > 0.05), "{id}");
    }
}

#[test]
fn an_effect_whose_program_crashes_is_started_again_with_its_values_and_makes_the_sound_that_it_makes_without_a_crash()
{
    let gain = a_clap_plugin(clap_plugins::GAIN, 2, 2, &[GAIN]);
    let mut script = vec![Step::Value {
        handle: GAIN,
        value: 0.25,
        at_sample: 0,
    }];
    for number in 0..9 {
        if number == 4 {
            script.push(Step::Value {
                handle: GAIN,
                value: 3.0,
                at_sample: 200,
            });
        }
        script.push(block(512, 2, 2));
    }
    let (mut without_a_crash, _) = gain.in_this_process(None);
    let crashing = launcher().with_environment(vec![(
        "FFRS_TEST_PLUGIN".to_owned(),
        "crash_at_every_fourth_run".to_owned(),
    )]);
    let (mut with_crashes, _) =
        started_again_when_it_ends(&crashing, &gain, None).expect("the program makes the plugin");

    let wanted = play(without_a_crash.as_mut(), &script);
    let heard = play(&mut with_crashes, &script);

    assert_eq!(
        heard.iter().map(|block| &block.sound).collect::<Vec<_>>(),
        wanted.iter().map(|block| &block.sound).collect::<Vec<_>>()
    );
    assert_eq!(with_crashes.times_started_again(), 2, "runs 4 and 7");
}

#[test]
fn a_program_that_crashes_at_every_run_is_started_again_so_often_and_then_the_copy_is_gone_and_the_editor_is_still_here()
 {
    let gain = a_clap_plugin(clap_plugins::GAIN, 2, 2, &[GAIN]);
    let (mut copy, _) =
        started_again_when_it_ends(&a_launcher_for_a_plugin_that_misbehaves("crash_when_run"), &gain, None).unwrap();
    let (mut left, mut right) = ([0.0_f32; 512], [0.0_f32; 512]);

    let error = copy
        .run(
            &mut [&mut [0.5; 512], &mut [0.5; 512]],
            &mut [&mut left, &mut right],
            512,
        )
        .unwrap_err();

    assert!(
        error.contains("the program that runs the plugin ended")
            && error.contains("it was started again 5 times, and is not any more"),
        "{error}"
    );
    assert_eq!(copy.times_started_again(), 5);
    assert_eq!(copy.gone_because(), Some(error.as_str()));
    assert!(copy.process_id().is_none(), "no program is left");
}

// --- a plugin that applies its state in the audio thread (Surge XT does) ---

#[test]
fn a_note_in_the_first_block_of_a_plugin_that_applies_its_state_in_the_audio_thread_does_not_end_the_program() {
    // (The test instrument that is made like Surge XT applies the state it is given in the first block that follows, and a note in that block ends its program: Surge XT does that at random,
    // in some 4 plays of a hundred for some patches, and the test instrument always does.)
    let like_surge = a_clap_plugin(clap_plugins::INSTRUMENT_WITH_A_SIDE_CHAIN, 0, 2, &[GAIN]);
    let (mut copy, _) = started(&launcher(), &like_surge, Some(b"edits=2")).expect("the program makes the plugin");
    let (mut left, mut right) = ([0.0_f32; 512], [0.0_f32; 512]);

    copy.run_with_notes(&mut [], &mut [&mut left, &mut right], 512, &[on(0, 50, 1.0)])
        .expect("the first block, with a note, is run");

    // Key 50 at full velocity is 0.5; the state's two edits make it twice as loud: the state was applied, and the note was heard.
    assert_eq!(left[10], 1.0);
    assert_eq!(copy.times_started_again(), 0);
}

#[test]
fn a_state_and_a_note_in_the_first_block_make_the_same_sound_in_the_program_and_in_this_process_which_are_both_given_a_silent_block_first()
 {
    // (If the copy in this process were not given the silent block, this test program would end with the plugin's abort: that is how the lack of it is noticed.)
    let like_surge = a_clap_plugin(clap_plugins::INSTRUMENT_WITH_A_SIDE_CHAIN, 0, 2, &[GAIN]);

    let seen = the_same_in_the_program(
        &like_surge,
        Some(b"edits=2"),
        &[
            block_with_notes(512, 2, vec![on(0, 50, 1.0)]),
            block_with_notes(512, 2, Vec::new()),
        ],
    );

    assert_eq!(the_sound(&seen[0])[0][10], 1.0);
    let without_a_state = the_same_in_the_program(&like_surge, None, &[block_with_notes(512, 2, vec![on(0, 50, 1.0)])]);
    assert_eq!(
        the_sound(&without_a_state[0])[0][10],
        0.5,
        "no state, no edits, and the sound is half as loud"
    );
}

// --- what it costs (a report, not a check) ---

/// How long the program takes to start and make a plugin, and how long a block takes in it and in this process. Run with
/// `cargo test --test plugin_process what_a_process_costs -- --ignored --nocapture`.
#[test]
#[ignore = "a report of times, not a check"]
fn what_a_process_costs() {
    let instrument = a_clap_plugin(clap_plugins::INSTRUMENT, 0, 2, &[GAIN]);
    let gain = a_clap_plugin(clap_plugins::GAIN, 2, 2, &[GAIN]);
    for (name, subject, inputs) in [("instrument", &instrument as &dyn Subject, 0), ("gain", &gain, 2)] {
        let began = Instant::now();
        let (local, _) = subject.in_this_process(None);
        let made_here = began.elapsed();
        let began = Instant::now();
        let (remote, _) = started(&launcher(), subject, None).expect("the program makes the plugin");
        let made_in_the_program = began.elapsed();
        let notes = [on(0, 60, 0.5)];
        let time_blocks = |copy: &mut dyn PluginInstance| {
            let mut input = vec![vec![0.1_f32; 512]; inputs];
            let mut output = vec![vec![0.0_f32; 512]; 2];
            let blocks = 3000;
            let began = Instant::now();
            for _ in 0..blocks {
                let mut input_slices: Vec<&mut [f32]> = input.iter_mut().map(Vec::as_mut_slice).collect();
                let mut output_slices: Vec<&mut [f32]> = output.iter_mut().map(Vec::as_mut_slice).collect();
                if inputs == 0 {
                    copy.run_with_notes(&mut input_slices, &mut output_slices, 512, &notes)
                        .unwrap();
                } else {
                    copy.run(&mut input_slices, &mut output_slices, 512).unwrap();
                }
            }
            began.elapsed() / blocks
        };
        let (mut local, mut remote) = (local, remote);
        let here = time_blocks(local.as_mut());
        let there = time_blocks(&mut remote);
        println!(
            "{name}: made in this process {made_here:?}, made in the program (start, connect, load, make) {made_in_the_program:?}; a block of 512 samples (10.7 ms of sound): {here:?} here, {there:?} in the program"
        );
    }
}

// --- what the program does about an editor that says what it should not ---

/// Starts the program, plays the editor's part with `say` (given the connection after the program has said its token), and gives the code the program ends with: `None` if it
/// has not ended in ten seconds (and is stopped).
fn how_a_program_ends_when_the_editor(say: impl FnOnce(&mut std::net::TcpStream)) -> Option<i32> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let mut program = Command::new(the_program())
        .env(TOKEN_VARIABLE, "the token")
        .args(["--connect", &listener.local_addr().unwrap().to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let (mut stream, _) = listener.accept().unwrap();
    assert_eq!(read_frame(&mut stream).unwrap().kind, FrameKind::Hello);
    say(&mut stream);
    let deadline = Instant::now() + Duration::from_secs(10);
    // (The connection stays open: the program ends because of what it was told, and not because the editor went.)
    loop {
        if let Some(status) = program.try_wait().unwrap() {
            return status.code();
        }
        if Instant::now() > deadline {
            let _ = program.kill();
            let _ = program.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn a_make_of_the_instrument() -> Vec<u8> {
    serde_json::to_vec(&a_clap_plugin(clap_plugins::INSTRUMENT, 0, 2, &[GAIN]).what_to_make(false)).unwrap()
}

#[test]
fn a_program_that_is_not_asked_to_make_a_plugin_or_is_asked_for_nonsense_ends_with_its_own_word_for_it() {
    use fastforwardrs::plugin_process::program::exit;
    use fastforwardrs::plugin_process::protocol::RunRequest;

    // A run before any plugin was asked for, and a request to make that is not a request.
    assert_eq!(
        how_a_program_ends_when_the_editor(|stream| write_frame(stream, FrameKind::Run, &[], &[]).unwrap()),
        Some(exit::NOT_ASKED_TO_MAKE_A_COPY)
    );
    assert_eq!(
        how_a_program_ends_when_the_editor(|stream| write_frame(stream, FrameKind::Make, b"not json", &[]).unwrap()),
        Some(exit::NOT_ASKED_TO_MAKE_A_COPY)
    );
    // After the plugin is made: a frame that is not a run, a run that is not a request, and a run for more sound than a block can be.
    let after_it_is_made = |then: Box<dyn FnOnce(&mut std::net::TcpStream)>| {
        how_a_program_ends_when_the_editor(move |stream| {
            write_frame(stream, FrameKind::Make, &a_make_of_the_instrument(), &[]).unwrap();
            assert_eq!(read_frame(stream).unwrap().kind, FrameKind::Made);
            then(stream);
        })
    };
    assert_eq!(
        after_it_is_made(Box::new(
            |stream| write_frame(stream, FrameKind::Made, b"{}", &[]).unwrap()
        )),
        Some(exit::SAID_SOMETHING_BROKEN)
    );
    assert_eq!(
        after_it_is_made(Box::new(
            |stream| write_frame(stream, FrameKind::Run, &[1, 2, 3], &[]).unwrap()
        )),
        Some(exit::SAID_SOMETHING_BROKEN)
    );
    let too_long = RunRequest {
        commands: Vec::new(),
        frames: fastforwardrs::plugin_process::program::MOST_FRAMES_OF_A_RUN + 1,
        inputs: 0,
        outputs: 2,
        notes: None,
    };
    assert_eq!(
        after_it_is_made(Box::new(move |stream| write_frame(
            stream,
            FrameKind::Run,
            &too_long.header(),
            &[]
        )
        .unwrap())),
        Some(exit::SAID_SOMETHING_BROKEN),
        "a run of more frames than a block can be is refused and is not made room for"
    );
    let too_many_channels = RunRequest {
        commands: Vec::new(),
        frames: 4,
        inputs: 0,
        outputs: fastforwardrs::plugin_process::program::MOST_CHANNELS_OF_A_RUN + 1,
        notes: None,
    };
    assert_eq!(
        after_it_is_made(Box::new(move |stream| write_frame(
            stream,
            FrameKind::Run,
            &too_many_channels.header(),
            &[]
        )
        .unwrap())),
        Some(exit::SAID_SOMETHING_BROKEN)
    );
}

#[test]
fn the_sample_rate_of_the_sound_engine_reaches_the_plugin_in_the_program() {
    // (The test plugin multiplies the sound by the rate it was activated at over 1000.)
    let rate = a_clap_plugin(clap_plugins::RATE, 2, 2, &[]);

    let seen = the_same_in_the_program(&rate, None, &[block(64, 2, 2)]);

    let input = input_of(0, 64, 0);
    assert_eq!(the_sound(&seen[0])[0][7], input[7] * 48.0);
}
