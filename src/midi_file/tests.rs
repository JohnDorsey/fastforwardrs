// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The reader of Standard MIDI Files: files that are built here, byte by byte, so that each case is exactly what is tried.

use super::*;

// --- building files ---

/// A number as the bytes a file keeps it in: 7 bits in each byte, the last without the high bit.
fn variable_length(mut value: u32) -> Vec<u8> {
    let mut bytes = vec![(value & 0x7F) as u8];
    value >>= 7;
    while value > 0 {
        bytes.push((value & 0x7F) as u8 | 0x80);
        value >>= 7;
    }
    bytes.reverse();
    bytes
}

fn chunk(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut bytes = kind.to_vec();
    bytes.extend_from_slice(&(body.len() as u32).to_be_bytes());
    bytes.extend_from_slice(body);
    bytes
}

fn header(format: u16, tracks: u16, division: u16) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&format.to_be_bytes());
    body.extend_from_slice(&tracks.to_be_bytes());
    body.extend_from_slice(&division.to_be_bytes());
    chunk(b"MThd", &body)
}

type Event = (u32, Vec<u8>);

/// A track chunk of these events (each after its number of ticks), with no end of track unless it is one of them.
fn track_of(events: &[Event]) -> Vec<u8> {
    let mut body = Vec::new();
    for (delta, bytes) in events {
        body.extend(variable_length(*delta));
        body.extend_from_slice(bytes);
    }
    chunk(b"MTrk", &body)
}

fn file_of(format: u16, division: u16, tracks: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = header(format, tracks.len() as u16, division);
    for track in tracks {
        bytes.extend_from_slice(track);
    }
    bytes
}

fn on(channel: u8, key: u8, velocity: u8) -> Vec<u8> {
    vec![0x90 | channel, key, velocity]
}

fn off(channel: u8, key: u8) -> Vec<u8> {
    vec![0x80 | channel, key, 64]
}

fn tempo(microseconds_a_quarter: u32) -> Vec<u8> {
    let bytes = microseconds_a_quarter.to_be_bytes();
    vec![0xFF, 0x51, 3, bytes[1], bytes[2], bytes[3]]
}

fn name(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0x03];
    bytes.extend(variable_length(text.len() as u32));
    bytes.extend_from_slice(text.as_bytes());
    bytes
}

fn end() -> Vec<u8> {
    vec![0xFF, 0x2F, 0]
}

fn the_file(bytes: &[u8]) -> MidiFile {
    parse_midi_file(bytes).expect("the file is read")
}

fn note(start_us: u64, end_us: u64, channel: u8, key: u8, velocity: u8) -> MidiNote {
    MidiNote {
        start_us,
        end_us,
        channel,
        key,
        velocity,
    }
}

const QUARTER_NOTE_480: u16 = 480;

// --- time ---

#[test]
fn a_note_lasts_from_its_note_on_to_its_note_off_in_microseconds_at_the_tempo_of_120_a_minute_until_a_tempo_is_said() {
    let bytes = file_of(
        0,
        QUARTER_NOTE_480,
        &[track_of(&[(0, on(0, 60, 100)), (480, off(0, 60)), (0, end())])],
    );

    let file = the_file(&bytes);

    assert_eq!(file.tracks.len(), 1);
    assert_eq!(file.tracks[0].notes, vec![note(0, 500_000, 0, 60, 100)]);
    assert_eq!(file.problems, Vec::<String>::new());
}

#[test]
fn a_change_of_tempo_changes_the_length_of_the_ticks_after_it_and_not_before() {
    // (A quarter note is 500 000 microseconds for the first 480 ticks, and 250 000 after.)
    let bytes = file_of(
        0,
        QUARTER_NOTE_480,
        &[track_of(&[
            (0, on(0, 60, 100)),
            (480, tempo(250_000)),
            (480, off(0, 60)),
            (0, end()),
        ])],
    );

    let file = the_file(&bytes);

    assert_eq!(file.tracks[0].notes, vec![note(0, 750_000, 0, 60, 100)]);
}

#[test]
fn a_tempo_that_is_said_at_the_first_tick_is_the_tempo_from_the_start_and_the_last_of_two_at_one_tick_is_the_one() {
    let bytes = file_of(
        0,
        QUARTER_NOTE_480,
        &[track_of(&[
            (0, tempo(400_000)),
            (0, tempo(1_000_000)),
            (0, on(0, 60, 100)),
            (480, off(0, 60)),
            (0, end()),
        ])],
    );

    assert_eq!(the_file(&bytes).tracks[0].notes, vec![note(0, 1_000_000, 0, 60, 100)]);
}

#[test]
fn the_times_are_worked_out_exactly_and_rounded_down_once_and_not_for_each_stretch_of_tempo() {
    // (Seven ticks to a quarter note, 500 000 microseconds for the first tick and 500 001 after it: tick 2 is 1 000 001 / 7 = 142 857.28, and the stretches rounded one by one would be
    // 71 428 and 71 428, which is 142 856.)
    let bytes = file_of(
        0,
        7,
        &[track_of(&[
            (0, on(0, 60, 100)),
            (1, tempo(500_001)),
            (1, off(0, 60)),
            (0, end()),
        ])],
    );

    assert_eq!(the_file(&bytes).tracks[0].notes, vec![note(0, 142_857, 0, 60, 100)]);
}

#[test]
fn a_tick_that_is_not_a_whole_number_of_microseconds_is_rounded_down() {
    let bytes = file_of(
        0,
        QUARTER_NOTE_480,
        &[track_of(&[(1, on(0, 60, 100)), (1, off(0, 60)), (0, end())])],
    );

    // (One tick is 500 000 / 480 = 1041.67 microseconds, and two are 2083.33.)
    assert_eq!(the_file(&bytes).tracks[0].notes, vec![note(1041, 2083, 0, 60, 100)]);
}

#[test]
fn the_tempo_of_one_track_is_the_tempo_of_all_so_that_a_track_that_is_only_the_tempo_is_the_tempo_of_the_notes_of_the_others()
 {
    let conductor = track_of(&[(0, name("Tempo")), (0, tempo(250_000)), (0, end())]);
    let music = track_of(&[(0, name("Piano")), (0, on(0, 60, 100)), (480, off(0, 60)), (0, end())]);

    let file = the_file(&file_of(1, QUARTER_NOTE_480, &[conductor, music]));

    assert_eq!(
        file.tracks.len(),
        2,
        "the track that is only the tempo is a track, with no notes"
    );
    assert_eq!(file.tracks[0].name, "Tempo");
    assert!(file.tracks[0].notes.is_empty());
    assert_eq!(file.tracks[1].name, "Piano");
    assert_eq!(file.tracks[1].notes, vec![note(0, 250_000, 0, 60, 100)]);
}

#[test]
fn smpte_time_counts_frames_and_ignores_the_tempo() {
    // (-25 frames a second and 40 ticks to a frame: 1000 ticks are a second.)
    let division = (((-25_i8) as u8 as u16) << 8) | 40;
    let bytes = file_of(
        0,
        division,
        &[track_of(&[
            (0, tempo(1)),
            (0, on(0, 60, 100)),
            (1000, off(0, 60)),
            (0, end()),
        ])],
    );

    assert_eq!(the_file(&bytes).tracks[0].notes, vec![note(0, 1_000_000, 0, 60, 100)]);
}

#[test]
fn smpte_time_at_24_29_97_and_30_frames_a_second_has_the_rates_it_says() {
    for (frames, ticks_a_frame, tick, wanted) in [
        (-24_i8, 10_u16, 240_u32, 1_000_000_u64),
        (-30, 10, 300, 1_000_000),
        // (29.97 frames a second is 30 000 / 1001: 2997 ticks of 100 to a frame are 2997 * 1001 / 3 microseconds.)
        (-29, 100, 2997, 999_999),
    ] {
        let division = (((frames) as u8 as u16) << 8) | ticks_a_frame;
        let bytes = file_of(
            0,
            division,
            &[track_of(&[(0, on(0, 60, 100)), (tick, off(0, 60)), (0, end())])],
        );

        assert_eq!(
            the_file(&bytes).tracks[0].notes[0].end_us,
            wanted,
            "{frames} frames a second"
        );
    }
}

// --- notes ---

#[test]
fn a_note_on_of_no_velocity_is_a_note_off() {
    let bytes = file_of(
        0,
        QUARTER_NOTE_480,
        &[track_of(&[(0, on(0, 60, 100)), (480, on(0, 60, 0)), (0, end())])],
    );

    assert_eq!(the_file(&bytes).tracks[0].notes, vec![note(0, 500_000, 0, 60, 100)]);
}

#[test]
fn a_note_off_ends_the_note_of_the_same_channel_and_key_that_has_waited_longest() {
    let bytes = file_of(
        0,
        QUARTER_NOTE_480,
        &[track_of(&[
            (0, on(0, 60, 90)),
            (480, on(0, 60, 80)),
            (480, off(0, 60)),
            (480, off(0, 60)),
            (0, end()),
        ])],
    );

    let notes = &the_file(&bytes).tracks[0].notes;

    // (The first note on is ended by the first note off, at tick 960, and the second by the second, at 1440; they are in the order of their starts.)
    assert_eq!(
        notes,
        &vec![note(0, 1_000_000, 0, 60, 90), note(500_000, 1_500_000, 0, 60, 80)]
    );
}

#[test]
fn a_note_off_ends_only_a_note_of_its_own_channel_and_its_own_key() {
    let bytes = file_of(
        0,
        QUARTER_NOTE_480,
        &[track_of(&[
            (0, on(0, 60, 100)),
            (0, on(1, 60, 100)),
            (0, on(0, 61, 100)),
            (480, off(1, 60)),
            (480, off(0, 62)),
            (480, end()),
        ])],
    );

    let notes = &the_file(&bytes).tracks[0].notes;

    assert_eq!(
        notes[0],
        note(0, 1_500_000, 0, 60, 100),
        "channel 0, key 60: not ended by a note off of another channel or key, so it lasts until the track does"
    );
    assert_eq!(notes[1], note(0, 500_000, 1, 60, 100));
    assert_eq!(notes[2], note(0, 1_500_000, 0, 61, 100));
}

#[test]
fn a_note_that_is_never_ended_ends_where_its_track_does_and_a_note_off_with_no_note_is_nothing() {
    let bytes = file_of(
        0,
        QUARTER_NOTE_480,
        &[track_of(&[(0, off(0, 60)), (0, on(0, 60, 100)), (960, end())])],
    );

    let track = &the_file(&bytes).tracks[0];

    assert_eq!(track.notes, vec![note(0, 1_000_000, 0, 60, 100)]);
    assert_eq!(track.end_us, 1_000_000);
}

#[test]
fn a_note_that_begins_and_ends_at_one_tick_is_kept_with_no_length() {
    let bytes = file_of(
        0,
        QUARTER_NOTE_480,
        &[track_of(&[(480, on(0, 60, 100)), (0, off(0, 60)), (0, end())])],
    );

    assert_eq!(
        the_file(&bytes).tracks[0].notes,
        vec![note(500_000, 500_000, 0, 60, 100)]
    );
}

#[test]
fn notes_are_in_the_order_of_their_starts_and_those_that_start_together_in_the_order_of_the_file() {
    let bytes = file_of(
        0,
        QUARTER_NOTE_480,
        &[track_of(&[
            (0, on(0, 64, 100)),
            (0, on(0, 60, 100)),
            (240, on(0, 62, 100)),
            (240, off(0, 64)),
            (0, off(0, 60)),
            (0, off(0, 62)),
            (0, end()),
        ])],
    );

    let keys: Vec<u8> = the_file(&bytes).tracks[0].notes.iter().map(|note| note.key).collect();

    assert_eq!(keys, vec![64, 60, 62]);
}

// --- what is read past and what is kept ---

#[test]
fn controllers_and_the_pitch_wheel_are_kept_with_the_wheel_of_fourteen_bits_least_byte_first() {
    let bytes = file_of(
        0,
        QUARTER_NOTE_480,
        &[track_of(&[
            (480, vec![0xB2, 64, 127]),
            (480, vec![0xE1, 0x00, 0x40]),
            (0, vec![0xE1, 0x7F, 0x7F]),
            (0, vec![0xE1, 0x01, 0x02]),
            (0, end()),
        ])],
    );

    let controls = &the_file(&bytes).tracks[0].controls;

    assert_eq!(
        controls,
        &vec![
            MidiControl {
                time_us: 500_000,
                channel: 2,
                what: MidiControlKind::Controller { number: 64, value: 127 }
            },
            MidiControl {
                time_us: 1_000_000,
                channel: 1,
                what: MidiControlKind::PitchBend { value: 8192 }
            },
            MidiControl {
                time_us: 1_000_000,
                channel: 1,
                what: MidiControlKind::PitchBend { value: 16383 }
            },
            MidiControl {
                time_us: 1_000_000,
                channel: 1,
                what: MidiControlKind::PitchBend { value: 1 | (2 << 7) }
            },
        ]
    );
}

#[test]
fn the_pressure_of_keys_and_channels_and_program_changes_are_read_past_with_the_right_number_of_bytes() {
    let bytes = file_of(
        0,
        QUARTER_NOTE_480,
        &[track_of(&[
            (0, vec![0xC3, 5]),
            (0, vec![0xA0, 60, 30]),
            (0, vec![0xD0, 20]),
            (0, on(0, 60, 100)),
            (0, vec![0xC3, 6]),
            (480, off(0, 60)),
            (0, end()),
        ])],
    );

    let file = the_file(&bytes);

    assert_eq!(file.tracks[0].notes, vec![note(0, 500_000, 0, 60, 100)]);
    assert_eq!(file.tracks[0].controls, Vec::new());
    assert_eq!(file.problems, Vec::<String>::new());
}

#[test]
fn meta_events_and_system_messages_that_are_not_kept_are_read_past() {
    let text = {
        let mut bytes = vec![0xFF, 0x01, 5];
        bytes.extend_from_slice(b"hello");
        bytes
    };
    let bytes = file_of(
        0,
        QUARTER_NOTE_480,
        &[track_of(&[
            (0, text),
            (0, vec![0xFF, 0x58, 4, 4, 2, 24, 8]),
            (0, vec![0xFF, 0x59, 2, 0, 0]),
            (0, vec![0xF0, 3, 0x43, 0x12, 0xF7]),
            (0, vec![0xF7, 2, 1, 2]),
            (0, on(0, 60, 100)),
            (480, off(0, 60)),
            (0, end()),
        ])],
    );

    let file = the_file(&bytes);

    assert_eq!(file.tracks[0].notes, vec![note(0, 500_000, 0, 60, 100)]);
    assert_eq!(file.problems, Vec::<String>::new());
}

#[test]
fn running_status_gives_the_next_events_the_status_of_the_one_before() {
    // (0x90 60 100, then 62 100 and 64 100 with no status, then 0x80 and its key and velocity, then the same key with no status.)
    let bytes = file_of(
        0,
        QUARTER_NOTE_480,
        &[track_of(&[
            (0, vec![0x90, 60, 100]),
            (0, vec![62, 100]),
            (0, vec![64, 100]),
            (480, vec![0x80, 60, 64]),
            (0, vec![62, 64]),
            (0, vec![64, 64]),
            (0, end()),
        ])],
    );

    let notes = &the_file(&bytes).tracks[0].notes;

    assert_eq!(
        notes,
        &vec![
            note(0, 500_000, 0, 60, 100),
            note(0, 500_000, 0, 62, 100),
            note(0, 500_000, 0, 64, 100)
        ]
    );
}

#[test]
fn a_system_message_ends_running_status_and_data_with_no_status_after_it_is_a_fault_in_the_track() {
    let bytes = file_of(
        0,
        QUARTER_NOTE_480,
        &[track_of(&[
            (0, on(0, 60, 100)),
            (480, vec![0xF0, 1, 0xF7]),
            (0, vec![62, 100]),
            (0, end()),
        ])],
    );

    let file = the_file(&bytes);

    assert_eq!(
        file.tracks[0].notes,
        vec![note(0, 500_000, 0, 60, 100)],
        "what comes before the fault is read, and the note that was sounding ends where the track was read to"
    );
    assert_eq!(file.problems.len(), 1, "{:?}", file.problems);
    assert!(
        file.problems[0].contains("track 1") && file.problems[0].contains("no event begins with"),
        "{:?}",
        file.problems
    );
}

#[test]
fn the_name_of_a_track_is_the_first_that_it_says_without_control_characters_and_not_too_long() {
    let long = "x".repeat(300);
    let tracks = [
        track_of(&[(0, name("  Lead\u{7}\n synth ")), (0, name("Second")), (0, end())]),
        track_of(&[(0, name(&long)), (0, end())]),
        track_of(&[(0, end())]),
    ];

    let file = the_file(&file_of(1, QUARTER_NOTE_480, &tracks));

    assert_eq!(file.tracks[0].name, "Lead synth");
    assert_eq!(
        file.tracks[1].name.chars().count(),
        MOST_CHARACTERS_IN_THE_NAME_OF_A_TRACK
    );
    assert_eq!(file.tracks[2].name, "");
}

#[test]
fn the_end_of_a_track_is_the_time_of_its_end_of_track_event_or_of_its_last_event_if_it_has_none() {
    let with_end = track_of(&[(0, on(0, 60, 100)), (480, off(0, 60)), (480, end())]);
    let without = track_of(&[(0, on(0, 60, 100)), (480, off(0, 60)), (480, vec![0xB0, 1, 1])]);

    let file = the_file(&file_of(1, QUARTER_NOTE_480, &[with_end, without]));

    assert_eq!(file.tracks[0].end_us, 1_000_000);
    assert_eq!(file.tracks[1].end_us, 1_000_000);
    assert_eq!(file.length_us(), 1_000_000);
}

#[test]
fn the_length_of_the_music_is_where_the_last_track_ends_and_the_tracks_with_notes_are_counted() {
    let short = track_of(&[(0, on(0, 60, 100)), (480, off(0, 60)), (0, end())]);
    let long = track_of(&[(0, on(0, 60, 100)), (1920, off(0, 60)), (0, end())]);
    let empty = track_of(&[(960, end())]);

    let file = the_file(&file_of(1, QUARTER_NOTE_480, &[short, long, empty]));

    assert_eq!(file.length_us(), 2_000_000);
    assert_eq!(file.tracks_with_notes(), 2);
    assert_eq!(MidiFile::default().length_us(), 0);
}

#[test]
fn chunks_that_are_not_tracks_are_skipped_and_a_header_that_is_longer_than_it_needs_to_be_is_read() {
    let mut bytes = Vec::new();
    // (A header of eight bytes: the two that follow are not read.)
    bytes.extend_from_slice(b"MThd");
    bytes.extend_from_slice(&8_u32.to_be_bytes());
    bytes.extend_from_slice(&0_u16.to_be_bytes());
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.extend_from_slice(&QUARTER_NOTE_480.to_be_bytes());
    bytes.extend_from_slice(&[0xAA, 0xBB]);
    bytes.extend(chunk(b"XFIH", &[1, 2, 3]));
    bytes.extend(track_of(&[(0, on(0, 60, 100)), (480, off(0, 60)), (0, end())]));
    bytes.extend(chunk(b"junk", &[9; 11]));

    let file = the_file(&bytes);

    assert_eq!(file.tracks.len(), 1);
    assert_eq!(file.tracks[0].notes, vec![note(0, 500_000, 0, 60, 100)]);
}

// --- a file that is wrong ---

#[test]
fn a_file_that_does_not_begin_with_the_header_of_a_midi_file_is_not_one() {
    for bytes in [
        &b""[..],
        b"MTh",
        b"RIFF\0\0\0\0RMIDdata",
        b"not a midi file at all",
        b"MTrk\0\0\0\0",
    ] {
        assert_eq!(parse_midi_file(bytes), Err(MidiFileProblem::NotAMidiFile), "{bytes:?}");
    }
}

#[test]
fn a_header_that_is_cut_short_is_refused() {
    let whole = header(0, 1, QUARTER_NOTE_480);
    for length in 4..whole.len() {
        assert_eq!(
            parse_midi_file(&whole[..length]),
            Err(MidiFileProblem::HeaderCutShort),
            "{length} bytes"
        );
    }
    // (A header that says it is shorter than the six bytes that it must have.)
    let mut short = b"MThd".to_vec();
    short.extend_from_slice(&4_u32.to_be_bytes());
    short.extend_from_slice(&[0; 8]);
    assert_eq!(parse_midi_file(&short), Err(MidiFileProblem::HeaderCutShort));
}

#[test]
fn format_2_is_refused_and_so_is_a_division_of_no_ticks() {
    let track = track_of(&[(0, end())]);

    assert_eq!(
        parse_midi_file(&file_of(2, QUARTER_NOTE_480, std::slice::from_ref(&track))),
        Err(MidiFileProblem::FormatNotRead(2))
    );
    assert_eq!(
        parse_midi_file(&file_of(7, QUARTER_NOTE_480, std::slice::from_ref(&track))),
        Err(MidiFileProblem::FormatNotRead(7))
    );
    assert_eq!(
        parse_midi_file(&file_of(0, 0, std::slice::from_ref(&track))),
        Err(MidiFileProblem::NoTicks)
    );
    let smpte_with_no_ticks = ((-25_i8) as u8 as u16) << 8;
    assert_eq!(
        parse_midi_file(&file_of(0, smpte_with_no_ticks, std::slice::from_ref(&track))),
        Err(MidiFileProblem::NoTicks)
    );
}

#[test]
fn a_file_with_no_track_is_refused_and_one_that_says_it_has_too_many_is_refused_before_they_are_read() {
    assert_eq!(
        parse_midi_file(&header(1, 0, QUARTER_NOTE_480)),
        Err(MidiFileProblem::NoTracks)
    );
    assert_eq!(
        parse_midi_file(&header(1, 1, QUARTER_NOTE_480)),
        Err(MidiFileProblem::NoTracks),
        "it says it has one and has none"
    );
    assert_eq!(
        parse_midi_file(&header(1, MOST_TRACKS as u16 + 1, QUARTER_NOTE_480)),
        Err(MidiFileProblem::TooManyTracks(MOST_TRACKS + 1))
    );
}

#[test]
fn a_file_that_has_more_tracks_than_it_said_and_more_than_there_may_be_is_refused() {
    let track = track_of(&[(0, end())]);
    let mut bytes = header(1, 1, QUARTER_NOTE_480);
    for _ in 0..=MOST_TRACKS {
        bytes.extend_from_slice(&track);
    }

    assert_eq!(
        parse_midi_file(&bytes),
        Err(MidiFileProblem::TooManyTracks(MOST_TRACKS + 1))
    );

    // (As many as there may be are read.)
    let mut bytes = header(1, MOST_TRACKS as u16, QUARTER_NOTE_480);
    for _ in 0..MOST_TRACKS {
        bytes.extend_from_slice(&track);
    }
    assert_eq!(the_file(&bytes).tracks.len(), MOST_TRACKS);
}

#[test]
fn a_file_with_more_events_than_the_limit_is_refused_and_one_with_as_many_is_read() {
    let events: Vec<Event> = (0..5).map(|_| (1, vec![0xB0, 1, 1])).collect();
    let bytes = file_of(0, QUARTER_NOTE_480, &[track_of(&events)]);

    assert_eq!(parse_with_limits(&bytes, 4), Err(MidiFileProblem::TooManyEvents));
    assert_eq!(parse_with_limits(&bytes, 5).unwrap().tracks[0].controls.len(), 5);
    let two_tracks = file_of(1, QUARTER_NOTE_480, &[track_of(&events), track_of(&events)]);
    assert_eq!(
        parse_with_limits(&two_tracks, 9),
        Err(MidiFileProblem::TooManyEvents),
        "the events of all the tracks are counted"
    );
    assert!(parse_with_limits(&two_tracks, 10).is_ok());
}

#[test]
fn a_track_that_is_cut_short_is_read_as_far_as_it_goes_and_the_file_says_so() {
    let whole = file_of(
        0,
        QUARTER_NOTE_480,
        &[track_of(&[
            (0, on(0, 60, 100)),
            (480, off(0, 60)),
            (0, on(0, 62, 100)),
            (480, off(0, 62)),
            (0, end()),
        ])],
    );

    // (Cut in the middle of the last note off: the chunk says it is longer than the file is.)
    let cut = &whole[..whole.len() - 6];
    let file = the_file(cut);

    assert!(file.tracks[0].notes.contains(&note(0, 500_000, 0, 60, 100)));
    assert!(
        file.problems.iter().any(|problem| problem.contains("track 1")),
        "{:?}",
        file.problems
    );
    assert!(
        file.problems
            .iter()
            .any(|problem| problem.contains("the file ends before the track does")),
        "{:?}",
        file.problems
    );
    // (The note that was sounding where it was cut ends where what was read ends.)
    let second = file.tracks[0]
        .notes
        .iter()
        .find(|note| note.key == 62)
        .expect("the second note is there");
    assert_eq!(second.start_us, 500_000);
    assert!(second.end_us >= second.start_us);
}

#[test]
fn an_event_that_is_cut_short_a_meta_event_longer_than_the_track_and_a_time_that_is_too_long_each_end_the_track_with_a_problem()
 {
    let cases: [(&str, Vec<Event>, Vec<u8>); 4] = [
        (
            "a note on with no velocity",
            vec![(0, on(0, 60, 100)), (480, vec![0x90, 60])],
            Vec::new(),
        ),
        (
            "a meta event longer than the track",
            vec![(0, on(0, 60, 100)), (480, vec![0xFF, 0x01, 100, b'a'])],
            Vec::new(),
        ),
        (
            "a data byte with the high bit set",
            vec![(0, on(0, 60, 100)), (480, vec![0x90, 60, 0x90])],
            Vec::new(),
        ),
        (
            "a byte that is no event",
            vec![(0, on(0, 60, 100)), (480, vec![0xF1])],
            Vec::new(),
        ),
    ];
    for (what, events, _) in cases {
        let file = the_file(&file_of(0, QUARTER_NOTE_480, &[track_of(&events)]));

        assert_eq!(file.tracks[0].notes.len(), 1, "{what}: what came before is read");
        assert_eq!(file.problems.len(), 1, "{what}: {:?}", file.problems);
        assert!(file.problems[0].starts_with("track 1:"), "{what}: {:?}", file.problems);
    }
    // (A time of five bytes: the number is no more than four.)
    let mut body = variable_length(0);
    body.extend(on(0, 60, 100));
    body.extend([0x81, 0x81, 0x81, 0x81, 0x01]);
    body.extend(off(0, 60));
    let mut bytes = header(0, 1, QUARTER_NOTE_480);
    bytes.extend(chunk(b"MTrk", &body));
    let file = the_file(&bytes);
    assert_eq!(file.tracks[0].notes.len(), 1);
    assert_eq!(file.problems.len(), 1, "{:?}", file.problems);
}

#[test]
fn data_with_no_status_at_the_start_of_a_track_is_a_fault_in_it() {
    let file = the_file(&file_of(0, QUARTER_NOTE_480, &[track_of(&[(0, vec![60, 100])])]));

    assert!(file.tracks[0].notes.is_empty());
    assert_eq!(file.problems.len(), 1, "{:?}", file.problems);
}

#[test]
fn a_file_whose_times_do_not_fit_in_a_number_is_refused() {
    // (A quarter note of 16 777 215 microseconds and one tick to it, and 70 000 events each 268 million ticks after the one before.)
    let mut events: Vec<Event> = vec![(0, tempo(0xFF_FFFF))];
    events.extend((0..70_000).map(|_| (0x0FFF_FFFF, vec![0xD0, 0])));
    let bytes = file_of(0, 1, &[track_of(&events)]);

    assert_eq!(parse_midi_file(&bytes), Err(MidiFileProblem::TooLong));
}

#[test]
fn a_tempo_of_no_microseconds_is_not_a_tempo() {
    let bytes = file_of(
        0,
        QUARTER_NOTE_480,
        &[track_of(&[
            (0, tempo(0)),
            (0, on(0, 60, 100)),
            (480, off(0, 60)),
            (0, end()),
        ])],
    );

    assert_eq!(the_file(&bytes).tracks[0].notes, vec![note(0, 500_000, 0, 60, 100)]);
}

#[test]
fn a_file_that_a_program_wrote_with_two_tracks_of_notes_on_several_channels_is_read_whole() {
    let conductor = track_of(&[(0, name("Conductor")), (0, tempo(600_000)), (0, end())]);
    let melody = track_of(&[
        (0, name("Melody")),
        (0, vec![0xC0, 40]),
        (0, on(0, 72, 96)),
        (480, off(0, 72)),
        (0, on(0, 74, 80)),
        (480, off(0, 74)),
        (0, end()),
    ]);
    let drums = track_of(&[
        (0, name("Drums")),
        (0, on(9, 36, 127)),
        (240, off(9, 36)),
        (240, on(9, 38, 100)),
        (240, off(9, 38)),
        (0, end()),
    ]);

    let file = the_file(&file_of(1, QUARTER_NOTE_480, &[conductor, melody, drums]));

    assert_eq!(file.tracks.len(), 3);
    assert_eq!(
        file.tracks[1].notes,
        vec![note(0, 600_000, 0, 72, 96), note(600_000, 1_200_000, 0, 74, 80)]
    );
    assert_eq!(
        file.tracks[2].notes,
        vec![note(0, 300_000, 9, 36, 127), note(600_000, 900_000, 9, 38, 100)]
    );
    assert_eq!(file.length_us(), 1_200_000);
    assert_eq!(file.problems, Vec::<String>::new());
}

#[test]
fn the_tempo_changes_of_several_tracks_are_put_in_order_of_their_ticks_whichever_track_has_them() {
    // (Track 0 changes the tempo at tick 960, track 1 at tick 480: the one at 480 is the first, whichever track is first in the file.)
    let late = track_of(&[(960, tempo(1_000_000)), (0, end())]);
    let early = track_of(&[(480, tempo(250_000)), (0, end())]);
    let music = track_of(&[(0, on(0, 60, 100)), (1440, off(0, 60)), (0, end())]);

    let file = the_file(&file_of(1, QUARTER_NOTE_480, &[late, early, music]));

    // (500 000 for the first 480 ticks, 250 000 for the next 480, and 1 000 000 for the last 480.)
    assert_eq!(
        file.tracks[2].notes,
        vec![note(0, 500_000 + 250_000 + 1_000_000, 0, 60, 100)]
    );
}

#[test]
fn a_meta_event_ends_running_status_as_a_system_message_does() {
    let bytes = file_of(
        0,
        QUARTER_NOTE_480,
        &[track_of(&[
            (0, on(0, 60, 100)),
            (480, vec![0xFF, 0x01, 1, b'x']),
            (0, vec![62, 100]),
            (0, end()),
        ])],
    );

    let file = the_file(&bytes);

    assert_eq!(file.problems.len(), 1, "{:?}", file.problems);
    assert_eq!(file.tracks[0].notes.len(), 1);
}

#[test]
fn a_track_knows_its_lowest_and_highest_key_and_one_with_no_notes_has_none() {
    let bytes = crate::test_support::a_midi_file_with_tracks(&[
        &[(0, 100, 64), (100, 200, 72), (200, 300, 60), (300, 400, 72)],
        &[],
        &[(0, 50, 36)],
    ]);

    let file = parse_midi_file(&bytes).unwrap();

    // (The conductor's track is the first, with the tempo only.)
    let ranges: Vec<Option<(u8, u8)>> = file.tracks.iter().map(|track| track.key_range).collect();
    assert_eq!(ranges, vec![None, Some((60, 72)), None, Some((36, 36))]);
}
