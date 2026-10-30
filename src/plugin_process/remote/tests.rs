// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The copy that runs in a process of its own, tried against a program that is played from a thread, so that each thing a program can do (answer, not answer, end, say nonsense) is
//! exactly what is tried. The real program, with real plugins, is tried in `tests/plugin_process.rs`.

use std::collections::VecDeque;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::*;
use crate::sound_engine::NoteEventKind;

/// The limits of a copy whose program is played from a thread: long, except for the one test that is about the limit (a slow computer must not make a good answer look late).
fn limits_of_a_test() -> Limits {
    Limits {
        to_connect: Duration::from_secs(60),
        to_make: Duration::from_secs(60),
        to_run: Duration::from_secs(60),
    }
}

fn a_short_limit_to_run() -> Limits {
    Limits {
        to_run: Duration::from_millis(400),
        ..limits_of_a_test()
    }
}

fn a_plugin_like_this() -> Made {
    Made {
        latency: 64,
        takes_values_at_samples: true,
        most_samples_at_once: Some(512),
        state_not_taken: None,
        done_between_blocks: WhatWasDoneBetweenBlocks::default(),
    }
}

/// A copy whose program is `play`, which is given its end of the connection and is run from a thread.
fn a_copy_of_a_program_that(
    made: &Made,
    play: impl FnOnce(TcpStream) + Send + 'static,
) -> (RemoteCopy, thread::JoinHandle<()>) {
    a_copy_of_a_program_that_with(limits_of_a_test(), made, play)
}

/// The same, with limits.
fn a_copy_of_a_program_that_with(
    limits: Limits,
    made: &Made,
    play: impl FnOnce(TcpStream) + Send + 'static,
) -> (RemoteCopy, thread::JoinHandle<()>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let program = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        play(stream);
    });
    let stream = TcpStream::connect(address).unwrap();
    (
        RemoteCopy::over_a_stream_that_has_made_a_plugin_like(stream, made, limits),
        program,
    )
}

/// A program that answers every run with what `answer` makes of the request and the sound that came with it, until the connection ends.
fn answering(
    answer: impl Fn(&RunRequest, &[u8]) -> (Ran, Vec<u8>) + Send + 'static,
) -> impl FnOnce(TcpStream) + Send + 'static {
    move |mut stream| {
        while let Ok(frame) = read_frame(&mut stream) {
            let request = RunRequest::of_the_header(&frame.header).expect("the editor says a good request");
            let (ran, raw) = answer(&request, &frame.raw);
            if write_frame(&mut stream, FrameKind::Ran, &ran.header(), &raw).is_err() {
                return;
            }
        }
    }
}

fn went_well(raw_of_the_outputs: Vec<u8>) -> (Ran, Vec<u8>) {
    (
        Ran {
            result: Ok(()),
            latency: 64,
            meters: Vec::new(),
            done_between_blocks: WhatWasDoneBetweenBlocks::default(),
        },
        raw_of_the_outputs,
    )
}

fn samples_as_bytes(channels: &[&[f32]]) -> Vec<u8> {
    let mut raw = Vec::new();
    put_samples(&mut raw, channels.iter().copied(), channels[0].len());
    raw
}

#[test]
fn a_run_gives_the_program_the_commands_since_the_last_run_in_order_and_the_sound_and_puts_back_what_it_made() {
    let (seen, requests) = mpsc::channel();
    let (mut copy, program) = a_copy_of_a_program_that(
        &a_plugin_like_this(),
        answering(move |request, raw| {
            seen.send((request.clone(), raw.to_vec())).unwrap();
            // The program makes the sound of its first input twice as loud, in both outputs.
            let loud: Vec<f32> = raw
                .as_chunks::<4>()
                .0
                .iter()
                .take(request.frames)
                .map(|bytes| f32::from_bits(u32::from_le_bytes(*bytes)) * 2.0)
                .collect();
            went_well(samples_as_bytes(&[&loud, &loud]))
        }),
    );
    copy.set_value(7, 0.5, 3);
    copy.reset();
    copy.set_value(8, -1.0, 100);
    let mut input = [1.0_f32, 2.0, 3.0, 4.0];
    let (mut left, mut right) = ([0.0_f32; 4], [0.0_f32; 4]);

    copy.run(&mut [&mut input], &mut [&mut left, &mut right], 4).unwrap();
    // The second run has nothing to do first.
    copy.run(&mut [&mut input], &mut [&mut left, &mut right], 4).unwrap();
    drop(copy);
    program.join().unwrap();

    let (first, sound) = requests.recv().unwrap();
    assert_eq!(
        first.commands,
        vec![
            Command::SetValue {
                handle: 7,
                value: 0.5,
                at_sample: 3
            },
            Command::Reset,
            Command::SetValue {
                handle: 8,
                value: -1.0,
                at_sample: 100
            },
        ]
    );
    assert_eq!(
        (first.frames, first.inputs, first.outputs, first.notes),
        (4, 1, 2, None)
    );
    assert_eq!(sound, samples_as_bytes(&[&[1.0, 2.0, 3.0, 4.0]]));
    assert_eq!(requests.recv().unwrap().0.commands, Vec::new());
    assert_eq!(left, [2.0, 4.0, 6.0, 8.0]);
    assert_eq!(right, [2.0, 4.0, 6.0, 8.0]);
}

#[test]
fn a_run_with_notes_gives_them_and_a_plain_run_gives_none_and_a_run_of_no_notes_is_not_a_plain_run() {
    let (seen, requests) = mpsc::channel();
    let (mut copy, program) = a_copy_of_a_program_that(
        &a_plugin_like_this(),
        answering(move |request, _| {
            seen.send(request.notes.clone()).unwrap();
            went_well(vec![0; request.outputs * request.frames * 4])
        }),
    );
    let note = NoteEvent {
        time: 3,
        what: NoteEventKind::On {
            channel: 1,
            key: 60,
            velocity: 0.5,
            id: 9,
        },
    };
    let mut out = [0.0_f32; 8];

    copy.run(&mut [], &mut [&mut out], 8).unwrap();
    copy.run_with_notes(&mut [], &mut [&mut out], 8, &[note]).unwrap();
    copy.run_with_notes(&mut [], &mut [&mut out], 8, &[]).unwrap();
    drop(copy);
    program.join().unwrap();

    assert_eq!(requests.recv().unwrap(), None);
    assert_eq!(requests.recv().unwrap(), Some(vec![note]));
    assert_eq!(requests.recv().unwrap(), Some(Vec::new()));
}

#[test]
fn the_latency_the_meters_and_what_was_done_between_blocks_are_kept_from_the_answers_and_given_once() {
    let mut made = a_plugin_like_this();
    made.done_between_blocks = WhatWasDoneBetweenBlocks {
        was_restarted: false,
        notes: vec!["while it was made".to_owned()],
    };
    let runs = std::sync::atomic::AtomicU32::new(0);
    let (mut copy, program) = a_copy_of_a_program_that(
        &made,
        answering(move |request, _| {
            let run = runs.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            (
                Ran {
                    result: Ok(()),
                    latency: 100 + run,
                    meters: vec![(4, run as f32)],
                    done_between_blocks: WhatWasDoneBetweenBlocks {
                        was_restarted: run == 0,
                        notes: vec![format!("after run {run}")],
                    },
                },
                vec![0; request.outputs * request.frames * 4],
            )
        }),
    );
    let mut out = [0.0_f32; 2];
    assert_eq!(copy.latency(), 64, "the latency it was made with");
    assert_eq!(
        copy.between_blocks(),
        BetweenBlocks {
            was_restarted: false,
            note: Some("while it was made".to_owned())
        },
        "what the program said it did while it made the plugin is given at the first question"
    );

    copy.run(&mut [], &mut [&mut out], 2).unwrap();
    copy.run(&mut [], &mut [&mut out], 2).unwrap();

    assert_eq!(copy.latency(), 101, "the latency the last run said");
    assert_eq!(copy.outputs(), vec![(4, 1.0)], "the meters of the last run");
    assert_eq!(
        copy.between_blocks(),
        BetweenBlocks {
            was_restarted: true,
            note: Some("after run 0; after run 1".to_owned())
        },
        "both runs' words, and the restart of the first"
    );
    assert_eq!(copy.between_blocks(), BetweenBlocks::default(), "given once");
    drop(copy);
    program.join().unwrap();
}

#[test]
fn what_the_program_says_of_the_plugin_is_what_the_copy_says_of_itself() {
    let made = Made {
        latency: 7,
        takes_values_at_samples: false,
        most_samples_at_once: None,
        state_not_taken: None,
        done_between_blocks: WhatWasDoneBetweenBlocks::default(),
    };
    let (copy, program) = a_copy_of_a_program_that(&made, answering(|_, _| went_well(Vec::new())));
    assert!(!copy.takes_values_at_samples());
    assert_eq!(copy.most_samples_at_once(), None);
    assert_eq!(copy.latency(), 7);
    drop(copy);
    program.join().unwrap();

    let (copy, program) = a_copy_of_a_program_that(&a_plugin_like_this(), answering(|_, _| went_well(Vec::new())));
    assert!(copy.takes_values_at_samples());
    assert_eq!(copy.most_samples_at_once(), Some(512));
    drop(copy);
    program.join().unwrap();
}

#[test]
fn a_plugin_that_says_it_could_not_run_is_an_error_of_that_run_and_the_program_and_the_copy_go_on() {
    let calls = std::sync::atomic::AtomicU32::new(0);
    let (mut copy, program) = a_copy_of_a_program_that(
        &a_plugin_like_this(),
        answering(move |request, _| {
            if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                (
                    Ran {
                        result: Err("process failed".to_owned()),
                        latency: 64,
                        meters: Vec::new(),
                        done_between_blocks: WhatWasDoneBetweenBlocks::default(),
                    },
                    Vec::new(),
                )
            } else {
                went_well(vec![0; request.outputs * request.frames * 4])
            }
        }),
    );
    let mut out = [1.0_f32; 2];

    let first = copy.run(&mut [], &mut [&mut out], 2);
    let second = copy.run(&mut [], &mut [&mut out], 2);

    assert_eq!(first, Err("process failed".to_owned()));
    assert_eq!(copy.gone_because(), None);
    assert_eq!(second, Ok(()));
    assert_eq!(out, [0.0, 0.0]);
    drop(copy);
    program.join().unwrap();
}

#[test]
fn a_program_that_never_answers_is_waited_for_no_longer_than_the_limit_and_then_every_call_is_the_same_error_at_once() {
    let (release, wait) = mpsc::channel::<()>();
    let (mut copy, program) =
        a_copy_of_a_program_that_with(a_short_limit_to_run(), &a_plugin_like_this(), move |mut stream| {
            let _ = read_frame(&mut stream);
            // (It has the request, and does not answer, until the test lets it go.)
            let _ = wait.recv();
        });
    let mut out = [0.0_f32; 2];

    let began = Instant::now();
    let first = copy.run(&mut [], &mut [&mut out], 2).unwrap_err();
    let waited = began.elapsed();
    let again = Instant::now();
    let second = copy.run(&mut [], &mut [&mut out], 2).unwrap_err();
    let third = copy.run_with_notes(&mut [], &mut [&mut out], 2, &[]).unwrap_err();

    assert!(first.contains("did not answer within 0.4 seconds"), "{first}");
    assert!(
        waited >= Duration::from_millis(350) && waited < Duration::from_secs(30),
        "{waited:?}"
    );
    assert_eq!((&second, &third), (&first, &first));
    assert!(
        again.elapsed() < Duration::from_millis(100),
        "the later calls do not wait"
    );
    assert_eq!(copy.gone_because(), Some(first.as_str()));
    release.send(()).unwrap();
    program.join().unwrap();
}

#[test]
fn a_program_whose_connection_ends_in_the_middle_of_a_run_is_gone_and_says_so() {
    let (mut copy, program) = a_copy_of_a_program_that(&a_plugin_like_this(), |mut stream| {
        let _ = read_frame(&mut stream);
        // (Dropped here: the connection ends with a request unanswered.)
    });
    let mut out = [0.0_f32; 2];

    let error = copy.run(&mut [], &mut [&mut out], 2).unwrap_err();

    assert!(
        error.contains("connection to the program that runs the plugin ended"),
        "{error}"
    );
    assert!(copy.gone_because().is_some());
    assert_eq!(copy.run(&mut [], &mut [&mut out], 2).unwrap_err(), error);
    program.join().unwrap();
}

#[test]
fn a_program_that_says_what_is_not_a_frame_is_gone_and_a_frame_that_is_not_an_answer_is_too() {
    for (what_it_says, words) in [
        (u32::MAX.to_le_bytes().to_vec(), "not a frame"),
        // (A frame of the right length, of a kind that there is not.)
        (vec![5, 0, 0, 0, 77, 0, 0, 0, 0], "not a frame"),
    ] {
        let (mut copy, program) = a_copy_of_a_program_that(&a_plugin_like_this(), move |mut stream| {
            let _ = read_frame(&mut stream);
            stream.write_all(&what_it_says).unwrap();
            let _ = read_frame(&mut stream);
        });
        let mut out = [0.0_f32; 2];

        let error = copy.run(&mut [], &mut [&mut out], 2).unwrap_err();

        assert!(error.contains(words), "{error}");
        assert!(copy.gone_because().is_some());
        drop(copy);
        program.join().unwrap();
    }

    let (mut copy, program) = a_copy_of_a_program_that(&a_plugin_like_this(), |mut stream| {
        let _ = read_frame(&mut stream);
        write_frame(&mut stream, FrameKind::Made, b"{}", &[]).unwrap();
        let _ = read_frame(&mut stream);
    });
    let mut out = [0.0_f32; 2];
    let error = copy.run(&mut [], &mut [&mut out], 2).unwrap_err();
    assert!(error.contains("said Made when it was given sound to run"), "{error}");
    assert!(copy.gone_because().is_some());
    drop(copy);
    program.join().unwrap();
}

#[test]
fn an_answer_that_is_not_a_message_and_sound_that_is_not_what_was_asked_for_are_a_program_that_is_gone() {
    let (mut copy, program) = a_copy_of_a_program_that(&a_plugin_like_this(), |mut stream| {
        let _ = read_frame(&mut stream);
        write_frame(&mut stream, FrameKind::Ran, &[1, 2, 3], &[]).unwrap();
        let _ = read_frame(&mut stream);
    });
    let mut out = [0.0_f32; 2];
    let error = copy.run(&mut [], &mut [&mut out], 2).unwrap_err();
    assert!(error.contains("what is not a message"), "{error}");
    assert!(copy.gone_because().is_some());
    drop(copy);
    program.join().unwrap();

    // Sound for one sample when two were asked for, and sound for three channels when two were asked for.
    for raw_samples in [1_usize, 3] {
        let (mut copy, program) = a_copy_of_a_program_that(
            &a_plugin_like_this(),
            answering(move |_, _| went_well(vec![0; raw_samples * 4])),
        );
        let mut out = [0.0_f32; 2];
        let error = copy.run(&mut [], &mut [&mut out], 2).unwrap_err();
        assert!(error.contains("not what was asked for"), "{raw_samples}: {error}");
        assert!(copy.gone_because().is_some());
        drop(copy);
        program.join().unwrap();
    }
}

#[test]
fn sound_that_is_shorter_than_the_run_or_has_too_many_channels_is_refused_without_the_program_being_asked() {
    let (seen, requests) = mpsc::channel();
    let (mut copy, program) = a_copy_of_a_program_that(
        &a_plugin_like_this(),
        answering(move |request, _| {
            seen.send(()).unwrap();
            went_well(vec![0; request.outputs * request.frames * 4])
        }),
    );
    let mut short = [0.0_f32; 3];
    let mut full = [0.0_f32; 4];
    let mut channels = vec![[0.0_f32; 4]; 256];

    let output_too_short = copy.run(&mut [], &mut [&mut short], 4);
    let input_too_short = copy.run(&mut [&mut short], &mut [&mut full], 4);
    let mut many: Vec<&mut [f32]> = channels.iter_mut().map(|channel| channel.as_mut_slice()).collect();
    let too_many = copy.run(&mut [], &mut many, 4);

    assert!(output_too_short.is_err() && input_too_short.is_err() && too_many.is_err());
    assert_eq!(
        copy.gone_because(),
        None,
        "it is the caller's mistake and not the program's"
    );
    copy.run(&mut [], &mut [&mut full], 4).unwrap();
    drop(copy);
    program.join().unwrap();
    assert_eq!(requests.try_iter().count(), 1, "only the good run reached the program");
}

#[test]
fn only_the_first_frames_of_longer_channels_are_given_and_put_back() {
    let (mut copy, program) = a_copy_of_a_program_that(
        &a_plugin_like_this(),
        answering(|request, raw| {
            assert_eq!((request.frames, request.inputs, raw.len()), (2, 1, 8));
            went_well(samples_as_bytes(&[&[5.0, 6.0]]))
        }),
    );
    let mut input = [1.0_f32, 2.0, 3.0];
    let mut output = [9.0_f32, 9.0, 9.0];

    copy.run(&mut [&mut input], &mut [&mut output], 2).unwrap();

    assert_eq!(output, [5.0, 6.0, 9.0]);
    drop(copy);
    program.join().unwrap();
}

#[test]
fn a_copy_that_is_dropped_closes_its_connection_which_is_what_tells_the_program_to_end() {
    let (ended, ends) = mpsc::channel();
    let (copy, program) = a_copy_of_a_program_that(&a_plugin_like_this(), move |mut stream| {
        let error = read_frame(&mut stream).unwrap_err();
        ended.send(matches!(error, FrameError::Connection(_))).unwrap();
    });

    drop(copy);

    assert_eq!(ends.recv_timeout(Duration::from_secs(5)), Ok(true));
    program.join().unwrap();
}

#[test]
fn a_value_at_a_sample_past_what_a_number_of_32_bits_holds_is_given_at_the_last_it_holds() {
    let (seen, requests) = mpsc::channel();
    let (mut copy, program) = a_copy_of_a_program_that(
        &a_plugin_like_this(),
        answering(move |request, _| {
            seen.send(request.commands.clone()).unwrap();
            went_well(vec![0; request.outputs * request.frames * 4])
        }),
    );
    copy.set_value(1, 1.0, usize::MAX);
    let mut out = [0.0_f32; 1];

    copy.run(&mut [], &mut [&mut out], 1).unwrap();
    drop(copy);
    program.join().unwrap();

    assert_eq!(
        requests.recv().unwrap(),
        vec![Command::SetValue {
            handle: 1,
            value: 1.0,
            at_sample: u32::MAX
        }]
    );
}

fn a_hello(token: &str) -> Vec<u8> {
    serde_json::to_vec(&Hello {
        token: token.to_owned(),
    })
    .unwrap()
}

#[test]
fn a_connection_that_says_another_token_or_none_is_let_go_and_the_one_that_says_it_is_taken() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let impostors = thread::spawn(move || {
        // One that says another word, one that says another kind of frame, one that says nothing at all (kept open), and then the right one.
        let mut wrong = TcpStream::connect(address).unwrap();
        write_frame(&mut wrong, FrameKind::Hello, &a_hello("not the token"), &[]).unwrap();
        let mut another_kind = TcpStream::connect(address).unwrap();
        write_frame(&mut another_kind, FrameKind::Made, &a_hello("the token"), &[]).unwrap();
        let silent = TcpStream::connect(address).unwrap();
        let mut right = TcpStream::connect(address).unwrap();
        write_frame(&mut right, FrameKind::Hello, &a_hello("the token"), &[]).unwrap();
        (wrong, another_kind, silent, right)
    });

    let accepted = accept_the_program(
        &listener,
        None,
        "the token",
        Duration::from_secs(10),
        Duration::from_millis(200),
    )
    .unwrap();

    let (_wrong, _another_kind, _silent, right) = impostors.join().unwrap();
    assert_eq!(accepted.peer_addr().unwrap(), right.local_addr().unwrap());
}

#[test]
fn nobody_connecting_within_the_time_is_an_error_that_says_how_long_it_waited() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();

    let began = Instant::now();
    let error =
        accept_the_program(&listener, None, "x", Duration::from_millis(300), Duration::from_secs(1)).unwrap_err();

    assert!(error.contains("did not connect within 0.3 seconds"), "{error}");
    assert!(began.elapsed() >= Duration::from_millis(300) && began.elapsed() < Duration::from_secs(30));
}

#[test]
fn the_tokens_are_not_the_same_twice_and_are_long_enough_not_to_be_guessed() {
    let tokens: Vec<String> = (0..50).map(|_| a_token()).collect();

    for token in &tokens {
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|digit| digit.is_ascii_hexdigit()));
    }
    let mut sorted = tokens.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), tokens.len());
}

#[test]
fn a_program_that_cannot_be_started_is_said_with_where_it_was_looked_for() {
    let launcher = Launcher::new(PathBuf::from("/this/program/is/not/there"));
    let make = Make {
        address: super::super::protocol::Address::Clap { id: "x".to_owned() },
        library: PathBuf::from("x"),
        scanned: crate::plugin_host::WhatWasScanned {
            audio_inputs: 0,
            audio_outputs: 2,
            value_handles: Vec::new(),
            ladspa_label: None,
        },
        has_state: false,
    };

    let Err(NotStarted::TheProgram(why)) = launcher.start(&make, None) else {
        panic!("a program that is not there is not started")
    };

    assert!(
        why.contains("could not be started") && why.contains("is/not/there"),
        "{why}"
    );
}

// --- a program that ends is started again ---

type Program = Box<dyn FnOnce(TcpStream) + Send>;

/// What a program was given in a run, and which program (counted from 0) was given it.
type Given = (usize, RunRequest);

/// Starts a program that is played from a thread, and gives the editor's end of its connection.
fn start_a_program(play: Program) -> (TcpStream, thread::JoinHandle<()>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        play(stream);
    });
    (TcpStream::connect(address).unwrap(), handle)
}

/// What a program does with the runs it is given: it records each (as program number `number`) and answers it, except that **it ends without a word at run number `ends_at_run`**
/// (counted from 1), or never answers at `hangs_at_run` until it is let go, or answers with a failure at `says_it_failed_at_run`.
struct Behaviour {
    number: usize,
    ends_at_run: Option<usize>,
    hangs_at_run: Option<usize>,
    says_it_failed_at_run: Option<usize>,
}

impl Behaviour {
    fn well(number: usize) -> Self {
        Self {
            number,
            ends_at_run: None,
            hangs_at_run: None,
            says_it_failed_at_run: None,
        }
    }

    fn ending_at_run(number: usize, run: usize) -> Self {
        Self {
            ends_at_run: Some(run),
            ..Self::well(number)
        }
    }
}

fn a_program_that(
    behaviour: Behaviour,
    given: &mpsc::Sender<Given>,
    release: Option<Arc<Mutex<mpsc::Receiver<()>>>>,
) -> Program {
    let given = given.clone();
    Box::new(move |mut stream| {
        let mut run = 0;
        while let Ok(frame) = read_frame(&mut stream) {
            run += 1;
            let request = RunRequest::of_the_header(&frame.header).expect("the editor says a good request");
            let (outputs, frames) = (request.outputs, request.frames);
            let _ = given.send((behaviour.number, request));
            if behaviour.ends_at_run == Some(run) {
                return;
            }
            if behaviour.hangs_at_run == Some(run) {
                if let Some(release) = &release {
                    let _ = release.lock().unwrap().recv();
                }
                return;
            }
            let (ran, raw) = if behaviour.says_it_failed_at_run == Some(run) {
                (
                    Ran {
                        result: Err("process failed".to_owned()),
                        latency: 64,
                        meters: Vec::new(),
                        done_between_blocks: WhatWasDoneBetweenBlocks::default(),
                    },
                    Vec::new(),
                )
            } else {
                went_well(vec![0; outputs * frames * 4])
            };
            if write_frame(&mut stream, FrameKind::Ran, &ran.header(), &raw).is_err() {
                return;
            }
        }
    })
}

/// A copy whose first program is `first` and whose later programs are `later`, in order, each made like the `Made` it comes with; when there is none left, a program cannot be started.
/// Gives the threads of the programs too.
fn a_copy_that_goes_through(
    made_by_the_first: &Made,
    first: Program,
    later: Vec<(Made, Program)>,
    limits: Limits,
) -> (RemoteCopy, Arc<Mutex<Vec<thread::JoinHandle<()>>>>) {
    let handles = Arc::new(Mutex::new(Vec::new()));
    let (stream, handle) = start_a_program(first);
    handles.lock().unwrap().push(handle);
    let later = Arc::new(Mutex::new(VecDeque::from(later)));
    let copy = RemoteCopy::over_a_stream_that_has_made_a_plugin_like(stream, made_by_the_first, limits)
        .with_a_way_to_start_again({
            let handles = Arc::clone(&handles);
            move || {
                let Some((made, play)) = later.lock().unwrap().pop_front() else {
                    return Err(NotStarted::TheProgram("there is no program to start".to_owned()));
                };
                let (stream, handle) = start_a_program(play);
                handles.lock().unwrap().push(handle);
                Ok((stream, made))
            }
        });
    (copy, handles)
}

/// A copy with `first` and one more program that is well behaved (number 1).
fn a_copy_that_is_started_again_once(
    first: Behaviour,
    given: &mpsc::Sender<Given>,
    limits: Limits,
) -> (RemoteCopy, Arc<Mutex<Vec<thread::JoinHandle<()>>>>) {
    a_copy_that_goes_through(
        &a_plugin_like_this(),
        a_program_that(first, given, None),
        vec![(a_plugin_like_this(), a_program_that(Behaviour::well(1), given, None))],
        limits,
    )
}

fn the_given(given: &mpsc::Receiver<Given>, how_many: usize) -> Vec<Given> {
    (0..how_many)
        .map(|_| {
            given
                .recv_timeout(Duration::from_secs(20))
                .expect("the program was given a run")
        })
        .collect()
}

fn note(time: u32, what: NoteEventKind) -> NoteEvent {
    NoteEvent { time, what }
}

fn on(time: u32, key: u8, id: u32) -> NoteEvent {
    note(
        time,
        NoteEventKind::On {
            channel: 0,
            key,
            velocity: 0.5,
            id,
        },
    )
}

fn off(time: u32, key: u8, id: u32) -> NoteEvent {
    note(
        time,
        NoteEventKind::Off {
            channel: 0,
            key,
            velocity: 0.5,
            id,
        },
    )
}

fn pedal(time: u32, value: u8) -> NoteEvent {
    note(
        time,
        NoteEventKind::Controller {
            channel: 0,
            number: 64,
            value,
        },
    )
}

fn set(handle: u32, value: f32, at_sample: u32) -> Command {
    Command::SetValue {
        handle,
        value,
        at_sample,
    }
}

fn let_the_programs_end(copy: RemoteCopy, handles: &Mutex<Vec<thread::JoinHandle<()>>>) {
    drop(copy);
    for handle in handles.lock().unwrap().drain(..) {
        handle.join().unwrap();
    }
}

#[test]
fn a_program_that_ends_in_the_middle_of_a_run_is_started_again_and_the_run_is_given_to_the_new_one_with_what_it_must_know()
 {
    let (given, seen) = mpsc::channel();
    let (mut copy, handles) =
        a_copy_that_is_started_again_once(Behaviour::ending_at_run(0, 2), &given, limits_of_a_test());
    let mut out = [0.0_f32; 16];
    // The first run: two notes, one of them let go of in the run, the pedal down, a value.
    copy.set_value(5, 0.5, 10);
    copy.run_with_notes(
        &mut [],
        &mut [&mut out],
        16,
        &[on(3, 60, 1), on(3, 64, 2), pedal(1, 127), off(9, 64, 2)],
    )
    .unwrap();
    // The second run: a value and a note. The program ends in the middle of it.
    copy.set_value(6, 1.0, 0);
    copy.run_with_notes(&mut [], &mut [&mut out], 16, &[on(5, 67, 3)])
        .unwrap();
    // And a third, which goes to the new program as it is.
    copy.set_value(7, 2.0, 4);
    copy.run_with_notes(&mut [], &mut [&mut out], 16, &[off(2, 60, 1)])
        .unwrap();

    let runs = the_given(&seen, 4);
    assert_eq!(
        runs.iter().map(|(program, _)| *program).collect::<Vec<_>>(),
        vec![0, 0, 1, 1],
        "two runs for the first program, then the second run again and the third for the second"
    );
    assert_eq!(
        runs[2].1.commands,
        vec![set(5, 0.5, 0), set(6, 1.0, 0)],
        "the value that was set before the run, at the first sample, then the commands of the run"
    );
    assert_eq!(
        runs[2].1.notes,
        Some(vec![pedal(0, 127), on(0, 60, 1), on(5, 67, 3)]),
        "the pedal where it was, the note that is held (the other was let go of), then the notes of the run"
    );
    assert_eq!(
        runs[3].1.commands,
        vec![set(7, 2.0, 4)],
        "the third run is as it was asked for"
    );
    assert_eq!(runs[3].1.notes, Some(vec![off(2, 60, 1)]));
    assert_eq!(copy.times_started_again(), 1);
    assert_eq!(copy.gone_because(), None);
    let said = copy.between_blocks();
    assert!(
        !said.was_restarted,
        "the values were given again by the copy: the effect has nothing to give"
    );
    let words = said.note.expect("it is said");
    assert!(
        words.contains("the connection to the program that runs the plugin ended")
            && words.contains("and was started again; the notes that were held are played again from here"),
        "{words}"
    );
    assert!(
        !words.contains("latency"),
        "the latency did not change, and nothing is said of it: {words}"
    );
    let_the_programs_end(copy, &handles);
}

#[test]
fn a_program_that_does_not_answer_is_stopped_and_started_again_and_the_run_is_given_to_the_new_one() {
    let (given, seen) = mpsc::channel();
    let (release, wait) = mpsc::channel::<()>();
    let first = a_program_that(
        Behaviour {
            hangs_at_run: Some(1),
            ..Behaviour::well(0)
        },
        &given,
        Some(Arc::new(Mutex::new(wait))),
    );
    let (mut copy, handles) = a_copy_that_goes_through(
        &a_plugin_like_this(),
        first,
        vec![(a_plugin_like_this(), a_program_that(Behaviour::well(1), &given, None))],
        a_short_limit_to_run(),
    );
    let mut out = [0.0_f32; 8];

    copy.run_with_notes(&mut [], &mut [&mut out], 8, &[on(0, 60, 1)])
        .unwrap();

    assert_eq!(
        the_given(&seen, 2)
            .iter()
            .map(|(program, _)| *program)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(copy.times_started_again(), 1);
    let words = copy.between_blocks().note.expect("it is said");
    assert!(
        words.contains("did not answer within 0.4 seconds") && words.contains("started again"),
        "{words}"
    );
    release.send(()).unwrap();
    let_the_programs_end(copy, &handles);
}

#[test]
fn a_plugin_that_says_it_could_not_run_does_not_have_its_program_started_again() {
    let (given, seen) = mpsc::channel();
    let (mut copy, handles) = a_copy_that_is_started_again_once(
        Behaviour {
            says_it_failed_at_run: Some(1),
            ..Behaviour::well(0)
        },
        &given,
        limits_of_a_test(),
    );
    let mut out = [0.0_f32; 8];

    let error = copy.run(&mut [], &mut [&mut out], 8).unwrap_err();

    assert_eq!(error, "process failed");
    assert_eq!(copy.times_started_again(), 0);
    assert_eq!(the_given(&seen, 1)[0].0, 0);
    assert!(seen.try_recv().is_err(), "no other program was given anything");
    let_the_programs_end(copy, &handles);
}

#[test]
fn a_program_that_ends_every_time_is_started_again_so_often_and_then_the_copy_is_gone_and_says_how_often() {
    let (given, seen) = mpsc::channel();
    let (mut copy, handles) = a_copy_that_goes_through(
        &a_plugin_like_this(),
        a_program_that(Behaviour::ending_at_run(0, 1), &given, None),
        (1..=MOST_RESTARTS_OF_A_PROGRAM + 2)
            .map(|number| {
                (
                    a_plugin_like_this(),
                    a_program_that(Behaviour::ending_at_run(number, 1), &given, None),
                )
            })
            .collect(),
        limits_of_a_test(),
    );
    let mut out = [0.0_f32; 8];

    let error = copy.run(&mut [], &mut [&mut out], 8).unwrap_err();
    let again = copy.run(&mut [], &mut [&mut out], 8).unwrap_err();

    assert!(
        error.contains("ended")
            && error.contains(&format!(
                "it was started again {MOST_RESTARTS_OF_A_PROGRAM} times, and is not any more"
            )),
        "{error}"
    );
    assert_eq!(again, error, "every call after that says the same");
    assert_eq!(copy.gone_because(), Some(error.as_str()));
    assert_eq!(copy.times_started_again(), MOST_RESTARTS_OF_A_PROGRAM);
    assert_eq!(
        the_given(&seen, MOST_RESTARTS_OF_A_PROGRAM + 1)
            .iter()
            .map(|(program, _)| *program)
            .collect::<Vec<_>>(),
        (0..=MOST_RESTARTS_OF_A_PROGRAM).collect::<Vec<_>>(),
        "the first program and one for each time it was started again, and no more"
    );
    assert!(seen.try_recv().is_err());
    let_the_programs_end(copy, &handles);
}

#[test]
fn a_program_that_cannot_be_started_again_ends_the_copy_and_the_error_says_why() {
    let (given, _seen) = mpsc::channel();
    // (No program is left to start.)
    let (mut copy, handles) = a_copy_that_goes_through(
        &a_plugin_like_this(),
        a_program_that(Behaviour::ending_at_run(0, 1), &given, None),
        Vec::new(),
        limits_of_a_test(),
    );
    let mut out = [0.0_f32; 8];

    let error = copy.run(&mut [], &mut [&mut out], 8).unwrap_err();

    assert!(
        error.contains("the connection to the program that runs the plugin ended")
            && error.contains("it could not be started again: there is no program to start"),
        "{error}"
    );
    assert_eq!(copy.gone_because(), Some(error.as_str()));
    assert_eq!(copy.times_started_again(), 1, "it was tried");
    let_the_programs_end(copy, &handles);
}

#[test]
fn a_reset_before_the_run_that_the_program_ended_in_means_the_notes_that_were_held_are_not_played_again() {
    let (given, seen) = mpsc::channel();
    let (mut copy, handles) =
        a_copy_that_is_started_again_once(Behaviour::ending_at_run(0, 2), &given, limits_of_a_test());
    let mut out = [0.0_f32; 8];
    copy.set_value(5, 0.5, 0);
    copy.run_with_notes(&mut [], &mut [&mut out], 8, &[on(0, 60, 1), pedal(1, 127)])
        .unwrap();
    // A seek: the plugin is reset, then a note that begins at the seek is given. The program ends in this run.
    copy.reset();
    copy.run_with_notes(&mut [], &mut [&mut out], 8, &[on(2, 72, 2)])
        .unwrap();

    let runs = the_given(&seen, 3);
    assert_eq!(runs[2].0, 1);
    assert_eq!(
        runs[2].1.commands,
        vec![set(5, 0.5, 0), Command::Reset],
        "the value is kept by a reset, and the reset is done again"
    );
    assert_eq!(
        runs[2].1.notes,
        Some(vec![on(2, 72, 2)]),
        "what was held before the reset is not played again"
    );
    let_the_programs_end(copy, &handles);
}

#[test]
fn a_run_without_notes_is_given_again_without_notes_and_with_the_values() {
    let (given, seen) = mpsc::channel();
    let (mut copy, handles) =
        a_copy_that_is_started_again_once(Behaviour::ending_at_run(0, 2), &given, limits_of_a_test());
    let mut input = [0.25_f32; 8];
    let mut out = [0.0_f32; 8];
    copy.set_value(1, 3.0, 0);
    copy.run(&mut [&mut input], &mut [&mut out], 8).unwrap();

    copy.run(&mut [&mut input], &mut [&mut out], 8).unwrap();

    let runs = the_given(&seen, 3);
    assert_eq!((runs[2].0, runs[2].1.notes.clone()), (1, None));
    assert_eq!(runs[2].1.commands, vec![set(1, 3.0, 0)]);
    assert_eq!((runs[2].1.inputs, runs[2].1.outputs, runs[2].1.frames), (1, 1, 8));
    let_the_programs_end(copy, &handles);
}

#[test]
fn what_a_program_that_is_started_again_says_of_the_plugin_is_what_the_copy_says_of_itself_afterwards() {
    let (given, _seen) = mpsc::channel();
    let other = Made {
        latency: 128,
        takes_values_at_samples: false,
        most_samples_at_once: Some(256),
        state_not_taken: None,
        done_between_blocks: WhatWasDoneBetweenBlocks {
            was_restarted: true,
            notes: vec!["while it was made again".to_owned()],
        },
    };
    let (mut copy, handles) = a_copy_that_goes_through(
        &a_plugin_like_this(),
        a_program_that(Behaviour::ending_at_run(0, 1), &given, None),
        vec![(other, a_program_that(Behaviour::well(1), &given, None))],
        limits_of_a_test(),
    );
    assert_eq!(copy.latency(), 64);
    let mut out = [0.0_f32; 8];

    copy.run(&mut [], &mut [&mut out], 8).unwrap();

    // (The fake program answers its run with a latency of 64 again, and what the last answer said is what the copy has: what is looked at is what the program said when it was made.)
    assert!(!copy.takes_values_at_samples());
    assert_eq!(copy.most_samples_at_once(), Some(256));
    let said = copy.between_blocks();
    assert!(
        said.was_restarted,
        "the plugin was made again by the program as it was made"
    );
    let words = said.note.unwrap();
    assert!(words.starts_with("while it was made again; "), "{words}");
    assert!(
        words.contains("its latency changed, and the sound is not made up for the new latency until the next play"),
        "{words}"
    );
    let_the_programs_end(copy, &handles);
}

#[test]
fn the_limits_that_a_program_is_given_are_twenty_seconds_to_start_thirty_to_make_and_five_to_run_a_block() {
    let limits = Limits::default();

    assert_eq!(limits.to_connect, Duration::from_secs(20));
    assert_eq!(limits.to_make, Duration::from_secs(30));
    assert_eq!(
        limits.to_run,
        Duration::from_secs(5),
        "a block of 10.7 ms of sound that needs five seconds is a plugin that is hung; and a program that is stopped for being slow is started again, so a false alarm costs little"
    );
}
