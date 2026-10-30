// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::{self, Cursor, Write};

use super::*;

fn a_frame_of_every_kind_of_part() -> Vec<u8> {
    let mut bytes = Vec::new();
    write_frame(&mut bytes, FrameKind::Run, b"the header", &[1, 2, 3, 4, 5]).unwrap();
    bytes
}

#[test]
fn a_frame_comes_back_as_it_was_said_with_its_header_and_its_raw_rest_whatever_each_is() {
    for (header, raw) in [
        (&b"the header"[..], &[1_u8, 2, 3][..]),
        (&b""[..], &[9_u8, 9][..]),
        (&b"only a header"[..], &[][..]),
        (&b""[..], &[][..]),
    ] {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, FrameKind::Ran, header, raw).unwrap();

        let frame = read_frame(&mut Cursor::new(bytes)).unwrap();

        assert_eq!(frame.kind, FrameKind::Ran);
        assert_eq!(frame.header, header);
        assert_eq!(frame.raw, raw);
    }
}

#[test]
fn every_kind_of_frame_comes_back_as_its_kind() {
    for kind in [
        FrameKind::Hello,
        FrameKind::Make,
        FrameKind::Made,
        FrameKind::Refused,
        FrameKind::Run,
        FrameKind::Ran,
    ] {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, kind, b"x", b"y").unwrap();
        assert_eq!(read_frame(&mut Cursor::new(bytes)).unwrap().kind, kind);
    }
}

#[test]
fn a_frame_is_said_in_one_write_so_that_a_block_of_sound_is_one_packet_and_not_four() {
    struct CountsWrites(usize, Vec<u8>);
    impl Write for CountsWrites {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0 += 1;
            self.1.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut out = CountsWrites(0, Vec::new());

    write_frame(&mut out, FrameKind::Run, b"header", &[0; 100]).unwrap();

    assert_eq!(out.0, 1);
    assert_eq!(out.1.len(), 4 + 1 + 4 + 6 + 100);
}

#[test]
fn two_frames_one_after_the_other_are_read_one_after_the_other() {
    let mut bytes = Vec::new();
    write_frame(&mut bytes, FrameKind::Made, b"one", b"").unwrap();
    write_frame(&mut bytes, FrameKind::Ran, b"two", b"2").unwrap();
    let mut input = Cursor::new(bytes);

    let first = read_frame(&mut input).unwrap();
    let second = read_frame(&mut input).unwrap();

    assert_eq!((first.kind, first.header), (FrameKind::Made, b"one".to_vec()));
    assert_eq!((second.kind, second.raw), (FrameKind::Ran, b"2".to_vec()));
    assert!(read_frame(&mut input).is_err(), "nothing is left");
}

#[test]
fn a_frame_that_is_cut_short_anywhere_is_a_connection_that_ended_and_never_a_frame() {
    let whole = a_frame_of_every_kind_of_part();

    for kept in 0..whole.len() {
        let error = read_frame(&mut Cursor::new(whole[..kept].to_vec())).unwrap_err();
        assert!(
            matches!(&error, FrameError::Connection(io) if io.kind() == io::ErrorKind::UnexpectedEof),
            "{kept} bytes of {}: {error}",
            whole.len()
        );
    }
}

#[test]
fn a_length_that_cannot_be_is_refused_before_the_memory_for_it_is_made() {
    // (A length of four gigabytes would be made before a byte of it came if the reader trusted it.)
    for length in [0_u32, 4, (MOST_BYTES_IN_A_FRAME + 1) as u32, u32::MAX] {
        let mut bytes = length.to_le_bytes().to_vec();
        bytes.extend_from_slice(&[1, 0, 0, 0, 0]);

        let error = read_frame(&mut Cursor::new(bytes)).unwrap_err();

        assert!(matches!(error, FrameError::Broken(_)), "{length}: {error}");
    }
}

#[test]
fn a_frame_of_a_kind_that_there_is_not_is_refused_and_so_is_a_header_longer_than_the_frame() {
    for kind in [0_u8, 7, 255] {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&5_u32.to_le_bytes());
        bytes.push(kind);
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        assert!(
            matches!(read_frame(&mut Cursor::new(bytes)), Err(FrameError::Broken(_))),
            "{kind}"
        );
    }
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&7_u32.to_le_bytes());
    bytes.push(FrameKind::Run as u8);
    bytes.extend_from_slice(&3_u32.to_le_bytes());
    bytes.extend_from_slice(&[0, 0]);
    assert!(matches!(
        read_frame(&mut Cursor::new(bytes)),
        Err(FrameError::Broken(_))
    ));
}

#[test]
fn a_frame_that_is_longer_than_there_may_be_is_not_said() {
    // (Not made: the check is on the lengths, which a slice of zeros of the size would only slow down.)
    let header_that_is_too_long = vec![0_u8; MOST_BYTES_IN_A_FRAME];

    let error = write_frame(&mut Vec::new(), FrameKind::Make, &header_that_is_too_long, &[]).unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
}

#[test]
fn a_read_that_ran_out_of_time_says_so_and_one_that_ended_does_not() {
    for kind in [io::ErrorKind::TimedOut, io::ErrorKind::WouldBlock] {
        assert!(FrameError::Connection(io::Error::from(kind)).is_a_timeout(), "{kind:?}");
    }
    assert!(!FrameError::Connection(io::Error::from(io::ErrorKind::UnexpectedEof)).is_a_timeout());
    assert!(!FrameError::Broken("x").is_a_timeout());
}

fn every_kind_of_note() -> Vec<NoteEvent> {
    vec![
        NoteEvent {
            time: 0,
            what: NoteEventKind::On {
                channel: 15,
                key: 127,
                velocity: 0.75,
                id: 2_147_483_646,
            },
        },
        NoteEvent {
            time: 17,
            what: NoteEventKind::Off {
                channel: 0,
                key: 1,
                velocity: -0.0,
                id: 0,
            },
        },
        NoteEvent {
            time: 100,
            what: NoteEventKind::Controller {
                channel: 3,
                number: 64,
                value: 127,
            },
        },
        NoteEvent {
            time: 511,
            what: NoteEventKind::PitchBend {
                channel: 9,
                value: 16383,
            },
        },
        NoteEvent {
            time: u32::MAX,
            what: NoteEventKind::PitchBend { channel: 0, value: 0 },
        },
        NoteEvent {
            time: 5,
            what: NoteEventKind::AllOff,
        },
    ]
}

fn a_request() -> RunRequest {
    RunRequest {
        commands: vec![
            Command::SetValue {
                handle: 7,
                value: 0.1,
                at_sample: 0,
            },
            Command::Reset,
            Command::SetValue {
                handle: u32::MAX,
                value: -1.5e-40,
                at_sample: 511,
            },
        ],
        frames: 512,
        inputs: 2,
        outputs: 2,
        notes: Some(every_kind_of_note()),
    }
}

#[test]
fn a_run_request_comes_back_as_it_was_said_with_every_kind_of_command_and_of_note() {
    let request = a_request();

    let back = RunRequest::of_the_header(&request.header()).unwrap();

    assert_eq!(back, request);
    // (`==` on a float says that -0.0 is 0.0: the bits are what the plugin is given, so they are compared too.)
    assert_eq!(back.header(), request.header());
}

#[test]
fn a_run_request_says_apart_notes_that_are_none_and_notes_that_are_nothing() {
    let without = RunRequest {
        notes: None,
        ..a_request()
    };
    let empty = RunRequest {
        notes: Some(Vec::new()),
        ..a_request()
    };

    assert_eq!(RunRequest::of_the_header(&without.header()).unwrap().notes, None);
    assert_eq!(
        RunRequest::of_the_header(&empty.header()).unwrap().notes,
        Some(Vec::new())
    );
    assert_ne!(without.header(), empty.header());
}

#[test]
fn a_float_goes_as_its_bits_whatever_they_are() {
    let odd_values = [
        f32::NAN,
        f32::from_bits(0x7FC0_1234),
        f32::from_bits(0xFFFF_FFFF),
        f32::INFINITY,
        f32::NEG_INFINITY,
        -0.0,
        f32::MIN_POSITIVE / 4.0,
        f32::MAX,
    ];
    for value in odd_values {
        let request = RunRequest {
            commands: vec![Command::SetValue {
                handle: 1,
                value,
                at_sample: 2,
            }],
            frames: 1,
            inputs: 0,
            outputs: 1,
            notes: Some(vec![NoteEvent {
                time: 0,
                what: NoteEventKind::On {
                    channel: 0,
                    key: 60,
                    velocity: value,
                    id: 1,
                },
            }]),
        };

        let back = RunRequest::of_the_header(&request.header()).unwrap();

        let Command::SetValue { value: given, .. } = back.commands[0] else {
            panic!("a value was given")
        };
        assert_eq!(given.to_bits(), value.to_bits());
        let NoteEventKind::On { velocity, .. } = back.notes.unwrap()[0].what else {
            panic!("a note on was given")
        };
        assert_eq!(velocity.to_bits(), value.to_bits());
    }
}

fn an_answer() -> Ran {
    Ran {
        result: Err("it says that processing failed: \u{ab}\u{e9}\u{4e2d}\u{bb}".to_owned()),
        latency: 4096,
        meters: vec![(3, 0.5), (u32::MAX, -2.25)],
        done_between_blocks: WhatWasDoneBetweenBlocks {
            was_restarted: true,
            notes: vec![
                "it asked to be made again".to_owned(),
                String::new(),
                "\u{1f3b9}".to_owned(),
            ],
        },
    }
}

#[test]
fn an_answer_comes_back_as_it_was_said_and_so_does_one_that_went_well_with_nothing_to_say() {
    let answer = an_answer();
    assert_eq!(Ran::of_the_header(&answer.header()).unwrap(), answer);

    let quiet = Ran {
        result: Ok(()),
        latency: 0,
        meters: Vec::new(),
        done_between_blocks: WhatWasDoneBetweenBlocks::default(),
    };
    assert_eq!(Ran::of_the_header(&quiet.header()).unwrap(), quiet);
}

#[test]
fn a_header_that_is_cut_short_anywhere_is_not_a_message() {
    for (name, header, reads) in [
        (
            "a request",
            a_request().header(),
            (|bytes: &[u8]| RunRequest::of_the_header(bytes).is_ok()) as fn(&[u8]) -> bool,
        ),
        ("an answer", an_answer().header(), |bytes| {
            Ran::of_the_header(bytes).is_ok()
        }),
    ] {
        assert!(reads(&header), "{name} is read whole");
        for kept in 0..header.len() {
            assert!(
                !reads(&header[..kept]),
                "{name} cut to {kept} of {} bytes",
                header.len()
            );
        }
    }
}

#[test]
fn a_header_with_bytes_after_its_end_is_not_a_message() {
    let mut request = a_request().header();
    request.push(0);
    assert_eq!(
        RunRequest::of_the_header(&request),
        Err(NotAMessage("it goes on after its end"))
    );
    let mut answer = an_answer().header();
    answer.push(0);
    assert_eq!(
        Ran::of_the_header(&answer),
        Err(NotAMessage("it goes on after its end"))
    );
}

#[test]
fn a_count_that_the_bytes_cannot_hold_is_refused_before_anything_is_made_for_it() {
    // (If the count were trusted, 4 billion commands of twelve bytes would be asked of the memory, and the program would end.)
    let mut request = u32::MAX.to_le_bytes().to_vec();
    request.extend_from_slice(&[0; 20]);
    assert_eq!(
        RunRequest::of_the_header(&request),
        Err(NotAMessage("it counts more than it holds"))
    );
    // The same for the notes: no commands, then a frame count and two channel counts, then notes counted past what is there.
    let mut notes = 0_u32.to_le_bytes().to_vec();
    notes.extend_from_slice(&512_u32.to_le_bytes());
    notes.extend_from_slice(&[2, 2, 1]);
    notes.extend_from_slice(&u32::MAX.to_le_bytes());
    notes.extend_from_slice(&[0; 32]);
    assert_eq!(
        RunRequest::of_the_header(&notes),
        Err(NotAMessage("it counts more than it holds"))
    );
    // And for the meters and the notes of an answer.
    let mut answer = vec![1];
    answer.extend_from_slice(&0_u32.to_le_bytes());
    answer.extend_from_slice(&u32::MAX.to_le_bytes());
    answer.extend_from_slice(&[0; 16]);
    assert_eq!(
        Ran::of_the_header(&answer),
        Err(NotAMessage("it counts more than it holds"))
    );
}

#[test]
fn a_count_of_notes_that_is_more_than_the_bytes_left_can_hold_is_refused_as_that_and_not_as_bytes_that_end_too_soon() {
    // Two notes are counted and there are bytes for one and a half: the count is what is wrong, whatever comes of reading them.
    let mut header = 0_u32.to_le_bytes().to_vec();
    header.extend_from_slice(&512_u32.to_le_bytes());
    header.extend_from_slice(&[0, 2, 1]);
    header.extend_from_slice(&2_u32.to_le_bytes());
    header.extend_from_slice(&[0; BYTES_OF_A_NOTE + BYTES_OF_A_NOTE / 2]);

    assert_eq!(
        RunRequest::of_the_header(&header),
        Err(NotAMessage("it counts more than it holds"))
    );
}

#[test]
fn what_is_neither_yes_nor_no_and_a_kind_that_there_is_not_are_refused() {
    let mut header = a_request().header();
    // The byte after the commands, the frames and the two channel counts says whether there are notes: 3 commands (13 + 1 + 13 bytes after the count), so find it from the end instead.
    let notes_flag = header.len() - (4 + every_kind_of_note().len() * BYTES_OF_A_NOTE) - 1;
    assert_eq!(header[notes_flag], 1);
    header[notes_flag] = 2;
    assert!(RunRequest::of_the_header(&header).is_err());

    let mut header = a_request().header();
    header[4] = 9;
    assert_eq!(
        RunRequest::of_the_header(&header),
        Err(NotAMessage("a command is of a kind that there is not"))
    );

    let mut header = an_answer().header();
    header[0] = 2;
    assert!(Ran::of_the_header(&header).is_err());

    let mut header = RunRequest {
        notes: Some(vec![NoteEvent {
            time: 0,
            what: NoteEventKind::AllOff,
        }]),
        ..a_request()
    }
    .header();
    let tag_of_the_note = header.len() - BYTES_OF_A_NOTE + 4;
    header[tag_of_the_note] = 200;
    assert_eq!(
        RunRequest::of_the_header(&header),
        Err(NotAMessage("a note is of a kind that there is not"))
    );
}

#[test]
fn a_pitch_wheel_that_is_further_than_it_goes_is_refused() {
    let mut header = RunRequest {
        notes: Some(vec![NoteEvent {
            time: 0,
            what: NoteEventKind::PitchBend { channel: 0, value: 1 },
        }]),
        ..a_request()
    }
    .header();
    let id_of_the_note = header.len() - 4;
    header[id_of_the_note..].copy_from_slice(&65_536_u32.to_le_bytes());

    assert_eq!(
        RunRequest::of_the_header(&header),
        Err(NotAMessage("a pitch wheel is further than it goes"))
    );
}

#[test]
fn a_text_that_is_not_utf_8_is_refused() {
    let mut header = vec![0];
    header.extend_from_slice(&2_u32.to_le_bytes());
    header.extend_from_slice(&[0xC3, 0x28]);
    header.extend_from_slice(&[0; 20]);

    assert_eq!(Ran::of_the_header(&header), Err(NotAMessage("a text is not UTF-8")));
}

#[test]
fn samples_go_as_their_bits_and_the_channels_come_back_in_their_places() {
    let left = [1.0_f32, -0.0, f32::NAN, f32::from_bits(0x7FC0_1234)];
    let right = [f32::INFINITY, f32::NEG_INFINITY, f32::MIN_POSITIVE / 8.0, 0.1];
    let mut raw = Vec::new();

    put_samples(&mut raw, [&left[..], &right[..]], 4);
    let (mut back_left, mut back_right) = ([0.0_f32; 4], [0.0_f32; 4]);
    take_samples(&raw, &mut [&mut back_left, &mut back_right], 4).unwrap();

    assert_eq!(raw.len(), 2 * 4 * 4);
    assert_eq!(left.map(f32::to_bits), back_left.map(f32::to_bits));
    assert_eq!(right.map(f32::to_bits), back_right.map(f32::to_bits));
}

#[test]
fn only_the_first_frames_of_a_longer_channel_go_and_are_put_back() {
    let channel = [1.0_f32, 2.0, 3.0, 4.0, 5.0];
    let mut raw = Vec::new();
    put_samples(&mut raw, [&channel[..]], 3);
    let mut back = [9.0_f32; 5];

    take_samples(&raw, &mut [&mut back], 3).unwrap();

    assert_eq!(back, [1.0, 2.0, 3.0, 9.0, 9.0]);
}

#[test]
fn samples_that_are_not_as_many_as_were_said_or_go_into_a_channel_that_is_too_short_are_refused() {
    let mut channel = [0.0_f32; 4];
    for bytes in [0_usize, 15, 17, 32] {
        assert_eq!(
            take_samples(&vec![0; bytes], &mut [&mut channel], 4),
            Err(NotAMessage("the samples are not as many as were said")),
            "{bytes} bytes for 4 samples"
        );
    }
    assert_eq!(
        take_samples(&[0; 20], &mut [&mut channel], 5),
        Err(NotAMessage("a channel is shorter than the samples for it"))
    );
    assert!(take_samples(&[], &mut [], 512).is_ok(), "no channels, no samples");
}

/// A stream of bytes that is the same every time (xorshift).
fn bytes_that_look_like_nothing_in_particular(seed: u64, count: usize) -> Vec<u8> {
    let mut state = seed | 1;
    (0..count)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 32) as u8
        })
        .collect()
}

#[test]
fn nothing_that_is_said_makes_a_reader_panic_cut_flipped_or_made_up() {
    // (A panic is the failure; what each says is not the point.)
    for header in [a_request().header(), an_answer().header()] {
        for place in 0..header.len() {
            for flipped in [0x01_u8, 0x80, 0xFF] {
                let mut damaged = header.clone();
                damaged[place] ^= flipped;
                let _ = RunRequest::of_the_header(&damaged);
                let _ = Ran::of_the_header(&damaged);
            }
        }
    }
    for seed in 0..2000_u64 {
        let junk = bytes_that_look_like_nothing_in_particular(seed, (seed % 97) as usize);
        let _ = RunRequest::of_the_header(&junk);
        let _ = Ran::of_the_header(&junk);
        let _ = read_frame(&mut Cursor::new(junk));
    }
}

#[test]
fn what_the_cold_messages_say_comes_back_as_it_was_said() {
    let make = Make {
        address: Address::Clap {
            id: "org.example.\u{e9}".to_owned(),
        },
        library: PathBuf::from("C:/plugins/a b/plugin.clap"),
        scanned: WhatWasScanned {
            audio_inputs: 0,
            audio_outputs: 2,
            value_handles: vec![1, 4_000_000_000],
            ladspa_label: Some("a_label".to_owned()),
        },
        has_state: true,
    };
    let made = Made {
        latency: 64,
        takes_values_at_samples: true,
        most_samples_at_once: Some(512),
        state_not_taken: Some("no".to_owned()),
        done_between_blocks: WhatWasDoneBetweenBlocks {
            was_restarted: false,
            notes: vec!["a".to_owned()],
        },
    };

    assert_eq!(
        serde_json::from_slice::<Make>(&serde_json::to_vec(&make).unwrap()).unwrap(),
        make
    );
    assert_eq!(
        serde_json::from_slice::<Made>(&serde_json::to_vec(&made).unwrap()).unwrap(),
        made
    );
    for refused in [
        Refused::CouldNotBeLoaded("x".to_owned()),
        Refused::NotWhatWasScanned,
        Refused::CouldNotBeMade("y".to_owned()),
    ] {
        assert_eq!(
            serde_json::from_slice::<Refused>(&serde_json::to_vec(&refused).unwrap()).unwrap(),
            refused
        );
    }
    for address in [
        Address::Ladspa { index: 3 },
        Address::Lv2 {
            uri: "urn:x".to_owned(),
        },
    ] {
        assert_eq!(
            serde_json::from_slice::<Address>(&serde_json::to_vec(&address).unwrap()).unwrap(),
            address
        );
    }
}
