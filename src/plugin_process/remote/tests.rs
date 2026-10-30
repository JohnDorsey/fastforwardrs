// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The copy that runs in a process of its own, tried against a program that is played from a thread, so that each thing a program can do (answer, not answer, end, say nonsense) is
//! exactly what is tried. The real program, with real plugins, is tried in `tests/plugin_process.rs`.

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
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
