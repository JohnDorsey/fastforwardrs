// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The gate of the watchdog: a shader that does not finish, given to a real engine on the software adapter, in **a process of its own**.
//!
//! On a software adapter (a graphics card that is the processor) the work of a draw that does not finish cannot be stopped, and it goes on using the processor
//! after the device it was given to has been thrown away, until the program ends (the findings of phase 0.5, S2: 8 cores' worth on WARP, 1 on lavapipe). So the
//! test that makes one happen is run by another test as a child: the test program starts itself (`current_exe`), runs the one test that is marked `#[ignore]`
//! (so that it is never run on its own by accident), and **kills it by its process id** if it is not done at a limit of time, as it does when it is done: what
//! was abandoned ends with the child, and no test that comes after is slowed down by it.
//!
//! What the child does, and prints a line for as it goes, which the parent looks for: it makes a frame, gives the engine the shader of an effect that has a
//! loop of two thousand million passes (with the limit on loops put out of the way: this is the case where the guard does not help), asks for a frame that
//! uses it, and sees the frame fail at the limit of time, the device replaced and the effect switched off, and then the frame made without the effect on
//! the new device. A mistake in what the child sees is a panic in it, which the parent reports with what the child said.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::engine_tests::{events_of, software_engine_with};
use super::foreign_effect_tests::{call, effect_source};
use super::oracle_tests::TestPicture;
use super::testing::software_graphics;
use super::{
    AdapterChoice, CORNERS_OF_THE_WHOLE_PICTURE, EffectCall, EngineEvent, EngineSettings, FrameDescription,
    LayerDescription, LayerSource, PictureId,
};

/// The name of the test that the child runs.
const THE_CHILD: &str = "engine::hang_tests::the_child_that_gives_the_engine_a_shader_that_does_not_finish";

/// What the parent sets so that the child knows it is one, and ends the process itself when it is done (the work that was abandoned is not waited for).
const THE_CHILD_IS_A_CHILD: &str = "FASTFORWARDRS_HANG_TEST_CHILD";

/// How long the engine of the child gives the card to make a frame. (More than the engine's own two seconds, so that a computer that is slow to make the first
/// frame, when the card compiles its shaders, is not taken for a frame that does not finish.)
const THE_CHILD_WAITS_FOR_A_FRAME: Duration = Duration::from_secs(4);

/// How long the parent lets the child take before it kills it. (It takes a few seconds: the frame that does not finish is waited for for four.)
const THE_PARENT_WAITS_FOR_THE_CHILD: Duration = Duration::from_secs(120);

/// A shader whose loop has as many passes as the first number of the call says, and does something to a number in each of them.
const A_LOOP_OF_AS_MANY_PASSES_AS_ASKED: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let passes = u32(numbers[0].x);
    var value = position.x * 0.001;
    for (var round_number = 0u; round_number < passes; round_number++) {
        value = fract(value * 1.0001 + 0.37);
    }
    return vec4<f32>(value, 0.5, 0.25, 1.0);
}
";

fn the_picture() -> TestPicture {
    TestPicture::gradient(16, 16)
}

fn frame_with(effects: Vec<EffectCall>) -> FrameDescription {
    let mut description = FrameDescription::black((16, 16));
    description.layers.push(LayerDescription {
        source: LayerSource::Picture(PictureId(1)),
        corners: CORNERS_OF_THE_WHOLE_PICTURE,
        effects,
    });
    description
}

/// What a child that was run said and came to.
struct WhatTheChildDid {
    /// How it ended, if it did: `None` if the parent had to kill it at the limit.
    ended_well: Option<bool>,
    stdout: String,
    stderr: String,
    took: Duration,
}

impl WhatTheChildDid {
    fn report(&self) -> String {
        format!(
            "ended well: {:?} (None: it was killed), after {:?}\n--- what it said ---\n{}\n--- what it said was wrong ---\n{}",
            self.ended_well, self.took, self.stdout, self.stderr
        )
    }
}

/// Runs the test `name` of this test program in a process of its own, and kills the process (by its id) if it has not ended at `limit`.
fn run_in_a_process_of_its_own(name: &str, limit: Duration) -> WhatTheChildDid {
    let mut child = Command::new(std::env::current_exe().expect("the test program knows where it is"))
        .args([name, "--exact", "--ignored", "--nocapture", "--test-threads=1"])
        .env(THE_CHILD_IS_A_CHILD, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the test program starts itself");
    let read_all = |mut pipe: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = pipe.read_to_string(&mut text);
            text
        })
    };
    let stdout = read_all(Box::new(child.stdout.take().expect("the output is piped")));
    let stderr = read_all(Box::new(child.stderr.take().expect("the errors are piped")));
    let started = Instant::now();
    let ended_well = loop {
        match child.try_wait().expect("the child can be asked") {
            Some(status) => break Some(status.success()),
            None if started.elapsed() >= limit => {
                // (By the handle of the child, which is its process id: nothing is looked for by name.)
                child.kill().expect("the child can be killed");
                let _ = child.wait();
                break None;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    WhatTheChildDid {
        ended_well,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
        took: started.elapsed(),
    }
}

/// Whether `lines_in_order` are all in `said`, one after another.
fn has_the_lines_in_order(said: &str, lines_in_order: &[&str]) -> bool {
    let mut from = 0;
    for line in lines_in_order {
        match said[from..].find(line) {
            Some(place) => from += place + line.len(),
            None => return false,
        }
    }
    true
}

#[test]
fn a_shader_that_does_not_finish_in_a_frame_is_caught_by_the_watchdog_the_device_is_replaced_and_the_engine_makes_the_next_frame()
 {
    // (The right to use the graphics device is held while the child runs, as it is for any test that draws, so that nothing else uses the processor with it.)
    let Some(_graphics) = software_graphics() else { return };
    let outcome = run_in_a_process_of_its_own(THE_CHILD, THE_PARENT_WAITS_FOR_THE_CHILD);
    assert_eq!(outcome.ended_well, Some(true), "{}", outcome.report());
    assert!(
        has_the_lines_in_order(
            &outcome.stdout,
            &[
                "CHILD: a plain frame was made",
                "CHILD: the frame that does not finish failed",
                "CHILD: the device was replaced and the effect was switched off",
                "CHILD: the frame was made on the new device without the effect",
                "CHILD: done",
            ]
        ),
        "{}",
        outcome.report()
    );
}

#[test]
fn a_child_that_is_not_done_at_the_limit_is_killed_and_what_it_said_is_kept() {
    // The test of the test: a program that does not end is killed at the limit, and the limit is kept. (It is the parent's own test program, told to run a test
    // that waits.)
    let started = Instant::now();
    let outcome = run_in_a_process_of_its_own(
        "engine::hang_tests::the_child_that_waits_for_ever",
        Duration::from_secs(3),
    );
    assert_eq!(outcome.ended_well, None, "{}", outcome.report());
    assert!(outcome.took >= Duration::from_secs(3), "{:?}", outcome.took);
    assert!(
        started.elapsed() < Duration::from_secs(60),
        "it was killed at the limit and not waited for: {:?}",
        started.elapsed()
    );
    assert!(outcome.stdout.contains("CHILD: waiting"), "{}", outcome.report());
}

#[test]
#[ignore = "is run by the test that starts it, in a process of its own"]
fn the_child_that_waits_for_ever() {
    if std::env::var_os(THE_CHILD_IS_A_CHILD).is_none() {
        // (Run by hand with all the tests that are marked to be ignored, it would be a test that never ends.)
        return;
    }
    println!("CHILD: waiting");
    loop {
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
#[ignore = "is run by the test that starts it, in a process of its own: the work of the shader it gives the engine is not stopped by anything but the end of the process"]
fn the_child_that_gives_the_engine_a_shader_that_does_not_finish() {
    let Some(_graphics) = software_graphics() else {
        println!("CHILD: no software adapter");
        return;
    };
    let mut settings = EngineSettings::new(AdapterChoice::SoftwareOnly);
    settings.frame_timeout = THE_CHILD_WAITS_FOR_A_FRAME;
    // The guard's limit on loops is put out of the way: this is the shader that the guard does not help with, which only the watchdog catches.
    settings.most_passes_of_a_loop = u32::MAX;
    let (mut engine, _) = software_engine_with(settings);
    engine
        .compile_effect_shader("test:slow", &effect_source(A_LOOP_OF_AS_MANY_PASSES_AS_ASKED))
        .expect("the guard takes it");
    engine.upload_picture(PictureId(1), Arc::new(the_picture().as_picture_data()));

    // A frame that the card makes in no time.
    engine.request_frame(frame_with(Vec::new()));
    let plain = match events_of(&engine, 1).remove(0) {
        EngineEvent::Frame { frame, .. } => frame,
        other => panic!("not a frame: {other:?}"),
    };
    println!("CHILD: a plain frame was made");

    // A frame whose effect has two thousand million passes of a loop in each of its 256 pixels: more than any card makes in the time a frame is given.
    let slow = frame_with(vec![call("test:slow", &[2.0e9])]);
    let asked = Instant::now();
    engine.request_frame(slow.clone());
    let events = events_of(&engine, 2);
    let took = asked.elapsed();
    let EngineEvent::FrameFailed { serial: 2, problem } = &events[0] else {
        panic!("the frame did not fail first: {events:?}")
    };
    assert!(problem.contains("did not finish"), "{problem}");
    assert!(
        took >= THE_CHILD_WAITS_FOR_A_FRAME,
        "the frame was given up on before the time it was given: {took:?}"
    );
    println!("CHILD: the frame that does not finish failed after {took:?}");
    let EngineEvent::DeviceReplaced {
        why,
        effects_switched_off,
    } = &events[1]
    else {
        panic!("the device was not replaced next: {events:?}")
    };
    assert!(why.contains("did not finish"), "{why}");
    assert_eq!(effects_switched_off, &["test:slow".to_owned()]);
    assert!(
        took < THE_CHILD_WAITS_FOR_A_FRAME + Duration::from_secs(30),
        "the device was replaced, but in {took:?}"
    );
    println!("CHILD: the device was replaced and the effect was switched off, {took:?} after the frame was asked for");

    // The window sends its pictures again, asks for the same frame again, and it is made, without the effect.
    engine.upload_picture(PictureId(1), Arc::new(the_picture().as_picture_data()));
    let asked_again = Instant::now();
    engine.request_frame(slow);
    let again = events_of(&engine, 1).remove(0);
    let EngineEvent::Frame { frame, statistics, .. } = again else {
        panic!("not a frame: {again:?}")
    };
    assert_eq!(
        statistics.effects_not_run, 1,
        "the effect that does not finish is not run again"
    );
    assert_eq!(*frame, *plain, "the frame is the picture as it is");
    println!(
        "CHILD: the frame was made on the new device without the effect, {:?} after it was asked for",
        asked_again.elapsed()
    );
    println!("CHILD: done");
    if std::env::var_os(THE_CHILD_IS_A_CHILD).is_some() {
        // (What was abandoned is still running on the processor, and the engine's own thread may be waiting for it: the process ends here, and it ends with it.)
        std::process::exit(0);
    }
}
