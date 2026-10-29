// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The reader of Standard MIDI Files (phase 13, piece 13.3a): what the notes of a `.mid` file are, in time that does not depend on the file's tempo, for a MIDI clip to play.
//!
//! **What is read.** Format 0 (one track with every channel) and format 1 (tracks that are played together; the tempo is usually in the first). Format 2 (patterns that are played one
//! after another) is refused: nothing here says what a timeline would do with them. The division is either *ticks to a quarter note* or SMPTE (*ticks to a frame*, at 24, 25, 29.97 or
//! 30 frames a second, where the tempo is not used). **The tempo map is made from the tempo events of every track** and applies to all of them (as format 1 means it). The time of an event is
//! a whole number of **microseconds**, worked out exactly from the ticks (`ticks * tempo / ticks to a quarter note`, added up over the stretches of tempo, rounded down once): the same file
//! gives the same times on every machine, and a microsecond is a good deal less than a sample.
//!
//! **What is kept of a track:** its name; its notes (a note on and the note off that ends it: *of the same channel and key, the one that has been waiting longest*, as most programs do; a note
//! on of no velocity is a note off; **a note that is never ended ends where its track does**); its controllers and its pitch wheel (what a synthesizer is played with besides notes). Program
//! changes, the pressure of keys and of channels, system messages and every meta event but the name, the tempo and the end of the track are read past and not kept.
//!
//! **A file that is wrong is refused when it cannot be read at all** (it is not a MIDI file, it is cut short in its header, its division is nothing, it is format 2, it has too many tracks
//! or events), **and read as far as it goes when only a track is wrong**: a track that is cut short, or has a byte that no event begins with, ends where the fault is, and `problems` says so.
//! (Files in the wild are often cut short, and what comes before the cut is music.)

use std::collections::VecDeque;

use thiserror::Error;

/// The most tracks that a file may have.
pub const MOST_TRACKS: usize = 256;

/// The most events (of any kind) that a file may have: a file of more is refused, as hostile.
pub const MOST_EVENTS_IN_A_FILE: usize = 4_000_000;

/// The most characters of the name of a track that are kept.
pub const MOST_CHARACTERS_IN_THE_NAME_OF_A_TRACK: usize = 200;

/// Why a file cannot be read at all.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum MidiFileProblem {
    #[error("it is not a Standard MIDI File (it does not begin with the header of one)")]
    NotAMidiFile,
    #[error("the header of the file is cut short")]
    HeaderCutShort,
    #[error("the file is of format {0}, and only formats 0 and 1 are read")]
    FormatNotRead(u16),
    #[error("the file says that a quarter note is no ticks long, or that a frame is")]
    NoTicks,
    #[error("the file says it has {0} tracks, and {MOST_TRACKS} are the most that are read")]
    TooManyTracks(usize),
    #[error("the file has more than {MOST_EVENTS_IN_A_FILE} events, which is more than any music has")]
    TooManyEvents,
    #[error("the file has no track in it")]
    NoTracks,
    #[error("the file is so long that its times do not fit in a number")]
    TooLong,
}

/// A note: from `start_us` to `end_us` (microseconds from the start of the file), on a channel (`0..=15`), of a key (`0..=127`), as hard as `velocity` says (`1..=127`; 0 is not a note on).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MidiNote {
    pub start_us: u64,
    pub end_us: u64,
    pub channel: u8,
    pub key: u8,
    pub velocity: u8,
}

/// What a controller or the pitch wheel does at a moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidiControlKind {
    /// A controller (`0..=127`) is given a value (`0..=127`).
    Controller { number: u8, value: u8 },
    /// The pitch wheel, `0..=16383`, 8192 in the middle.
    PitchBend { value: u16 },
}

/// A change of a controller or of the pitch wheel at `time_us`, on a channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MidiControl {
    pub time_us: u64,
    pub channel: u8,
    pub what: MidiControlKind,
}

/// A track of a file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MidiTrack {
    /// What the track says it is called, or nothing.
    pub name: String,
    /// In order of their starts (those that start together, in the order the file has them).
    pub notes: Vec<MidiNote>,
    /// In order of time.
    pub controls: Vec<MidiControl>,
    /// Where the track ends: the time of its last event of any kind, or of the end of its last note, whichever is later.
    pub end_us: u64,
    /// The lowest and the highest key of its notes, found once when it is read (the piano roll on the timeline is drawn from them at every frame); `None` for a track with no notes.
    pub key_range: Option<(u8, u8)>,
}

/// A Standard MIDI File, read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MidiFile {
    /// In the order of the file (a track that is only the tempo is there, with no notes: the number of a track is its place).
    pub tracks: Vec<MidiTrack>,
    /// One line for each thing that was wrong with a track and was read past.
    pub problems: Vec<String>,
}

impl MidiFile {
    /// How long the music is, in microseconds: where the last of its tracks ends.
    pub fn length_us(&self) -> u64 {
        self.tracks.iter().map(|track| track.end_us).max().unwrap_or(0)
    }

    /// The number of tracks that have a note in them.
    pub fn tracks_with_notes(&self) -> usize {
        self.tracks.iter().filter(|track| !track.notes.is_empty()).count()
    }
}

/// How the ticks of the file are counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Division {
    /// This many ticks to a quarter note; how long that is comes from the tempo.
    QuarterNote(u32),
    /// A frame is this many ticks, and there are `frames_a_second_numerator / frames_a_second_denominator` frames in a second.
    Frames {
        ticks_a_frame: u32,
        frames_a_second_numerator: u64,
        frames_a_second_denominator: u64,
    },
}

/// What happened in a track, at a tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Happening {
    NoteOn { channel: u8, key: u8, velocity: u8 },
    NoteOff { channel: u8, key: u8 },
    Control(MidiControlKind, u8),
}

/// A track as it is read, before its ticks are made into time.
#[derive(Default)]
struct RawTrack {
    name: String,
    happenings: Vec<(u64, Happening)>,
    /// The tick of the last event of any kind.
    last_tick: u64,
}

/// The stretches of tempo of a file: from a tick on, a quarter note is so many microseconds, with the time at the start of the stretch (kept as a numerator, `microseconds * ticks to a
/// quarter note`, so that nothing is rounded until a time is asked for).
struct TempoMap {
    division: Division,
    /// `(tick, numerator at the tick, microseconds a quarter note from the tick)`, in order of tick.
    stretches: Vec<(u64, u128, u32)>,
}

impl TempoMap {
    fn new(division: Division, mut changes: Vec<(u64, u32)>) -> Self {
        // (The first change at a tick is overruled by the last: they are sorted by tick and keep the order the tracks gave them in.)
        changes.sort_by_key(|(tick, _)| *tick);
        // (Until a change is said the tempo is 120 quarter notes a minute, which is 500 000 microseconds to a quarter note.)
        let mut stretches: Vec<(u64, u128, u32)> = vec![(0, 0, 500_000)];
        for (tick, tempo) in changes {
            let (last_tick, last_numerator, last_tempo) = *stretches.last().expect("there is a first stretch");
            if tick == last_tick {
                // (A change at the tick that the stretch before it began at overrules it.)
                *stretches.last_mut().expect("there is a first stretch") = (tick, last_numerator, tempo);
            } else {
                let numerator = last_numerator + u128::from(tick - last_tick) * u128::from(last_tempo);
                stretches.push((tick, numerator, tempo));
            }
        }
        Self { division, stretches }
    }

    /// The time of `tick`, in microseconds, rounded down; `None` if it does not fit.
    fn microseconds_at(&self, tick: u64) -> Option<u64> {
        match self.division {
            Division::QuarterNote(ticks_a_quarter) => {
                let place = self.stretches.partition_point(|(from, _, _)| *from <= tick) - 1;
                let (from, numerator, tempo) = self.stretches[place];
                let total = numerator + u128::from(tick - from) * u128::from(tempo);
                u64::try_from(total / u128::from(ticks_a_quarter)).ok()
            }
            Division::Frames {
                ticks_a_frame,
                frames_a_second_numerator,
                frames_a_second_denominator,
            } => {
                let total = u128::from(tick) * 1_000_000 * u128::from(frames_a_second_denominator)
                    / (u128::from(frames_a_second_numerator) * u128::from(ticks_a_frame));
                u64::try_from(total).ok()
            }
        }
    }
}

/// A reader of the bytes of a track.
struct Bytes<'a> {
    data: &'a [u8],
    at: usize,
}

impl Bytes<'_> {
    fn byte(&mut self) -> Option<u8> {
        let byte = *self.data.get(self.at)?;
        self.at += 1;
        Some(byte)
    }

    fn skip(&mut self, count: usize) -> Option<&[u8]> {
        let end = self.at.checked_add(count)?;
        let taken = self.data.get(self.at..end)?;
        self.at = end;
        Some(taken)
    }

    /// A number that is 7 bits in each byte, the last without the high bit (at most four bytes).
    fn variable_length(&mut self) -> Option<u32> {
        let mut value = 0_u32;
        for _ in 0..4 {
            let byte = self.byte()?;
            value = (value << 7) | u32::from(byte & 0x7F);
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }
}

fn two_bytes(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*data.get(at)?, *data.get(at + 1)?]))
}

/// Reads a Standard MIDI File.
pub fn parse_midi_file(bytes: &[u8]) -> Result<MidiFile, MidiFileProblem> {
    parse_with_limits(bytes, MOST_EVENTS_IN_A_FILE)
}

fn parse_with_limits(bytes: &[u8], most_events: usize) -> Result<MidiFile, MidiFileProblem> {
    if bytes.len() < 4 || &bytes[..4] != b"MThd" {
        return Err(MidiFileProblem::NotAMidiFile);
    }
    let length = bytes
        .get(4..8)
        .and_then(|four| four.try_into().ok())
        .map(u32::from_be_bytes)
        .ok_or(MidiFileProblem::HeaderCutShort)? as usize;
    if length < 6 || bytes.len() < 8 + 6 {
        return Err(MidiFileProblem::HeaderCutShort);
    }
    let format = two_bytes(bytes, 8).ok_or(MidiFileProblem::HeaderCutShort)?;
    let declared_tracks = two_bytes(bytes, 10).ok_or(MidiFileProblem::HeaderCutShort)? as usize;
    let division_word = two_bytes(bytes, 12).ok_or(MidiFileProblem::HeaderCutShort)?;
    if format > 1 {
        return Err(MidiFileProblem::FormatNotRead(format));
    }
    if declared_tracks > MOST_TRACKS {
        return Err(MidiFileProblem::TooManyTracks(declared_tracks));
    }
    let division = if division_word & 0x8000 == 0 {
        if division_word == 0 {
            return Err(MidiFileProblem::NoTicks);
        }
        Division::QuarterNote(u32::from(division_word))
    } else {
        let ticks_a_frame = u32::from(division_word & 0xFF);
        if ticks_a_frame == 0 {
            return Err(MidiFileProblem::NoTicks);
        }
        // (The high byte is the frames a second as a negative number: -24, -25, -29 for 29.97, -30.)
        let (numerator, denominator) = match (division_word >> 8) as u8 as i8 {
            -24 => (24, 1),
            -25 => (25, 1),
            -29 => (30_000, 1001),
            -30 => (30, 1),
            // (Any other is not a rate that SMPTE has; the nearest whole one is what a program that reads it would use.)
            other => (u64::from(other.unsigned_abs().max(1)), 1),
        };
        Division::Frames {
            ticks_a_frame,
            frames_a_second_numerator: numerator,
            frames_a_second_denominator: denominator,
        }
    };

    // The chunks after the header: tracks are read, anything else is skipped by its length.
    let mut problems = Vec::new();
    let mut raw_tracks: Vec<RawTrack> = Vec::new();
    let mut tempo_changes: Vec<(u64, u32)> = Vec::new();
    let mut events_so_far = 0_usize;
    let mut at = 8 + length;
    while at + 8 <= bytes.len() {
        let kind = &bytes[at..at + 4];
        let chunk_length = u32::from_be_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]]) as usize;
        let body_start = at + 8;
        let body_end = body_start.saturating_add(chunk_length).min(bytes.len());
        if kind == b"MTrk" {
            if raw_tracks.len() == MOST_TRACKS {
                return Err(MidiFileProblem::TooManyTracks(MOST_TRACKS + 1));
            }
            let number = raw_tracks.len();
            let track = read_a_track(
                &bytes[body_start..body_end],
                number,
                &mut tempo_changes,
                &mut events_so_far,
                most_events,
                &mut problems,
            )?;
            if body_start.saturating_add(chunk_length) > bytes.len() {
                problems.push(format!("track {}: the file ends before the track does", number + 1));
            }
            raw_tracks.push(track);
        }
        at = match body_start.checked_add(chunk_length) {
            Some(next) => next,
            None => break,
        };
    }
    if raw_tracks.is_empty() {
        return Err(MidiFileProblem::NoTracks);
    }

    let map = TempoMap::new(division, tempo_changes);
    let mut tracks = Vec::with_capacity(raw_tracks.len());
    for raw in raw_tracks {
        tracks.push(make_the_track(&raw, &map)?);
    }
    Ok(MidiFile { tracks, problems })
}

/// Reads the events of one track (`number` counts from 0) into what happened at which tick, and puts the tempo changes it has in `tempo_changes`.
fn read_a_track(
    data: &[u8],
    number: usize,
    tempo_changes: &mut Vec<(u64, u32)>,
    events_so_far: &mut usize,
    most_events: usize,
    problems: &mut Vec<String>,
) -> Result<RawTrack, MidiFileProblem> {
    let mut track = RawTrack::default();
    let mut bytes = Bytes { data, at: 0 };
    let mut tick = 0_u64;
    let mut running_status: Option<u8> = None;
    let fault = |problems: &mut Vec<String>, what: &str| {
        problems.push(format!("track {}: {what}; what comes before it is read", number + 1));
    };
    while bytes.at < data.len() {
        let Some(delta) = bytes.variable_length() else {
            fault(problems, "an event is cut short or its time is too long");
            break;
        };
        // (No more than 4 million events, each at most 2^28 ticks after the one before: far from the end of a `u64`.)
        tick += u64::from(delta);
        *events_so_far += 1;
        if *events_so_far > most_events {
            return Err(MidiFileProblem::TooManyEvents);
        }
        let Some(first) = bytes.byte() else {
            fault(problems, "an event is cut short");
            break;
        };
        let status = if first & 0x80 != 0 {
            first
        } else {
            match running_status {
                Some(status) => {
                    // (The byte was the first of the data, and is read again.)
                    bytes.at -= 1;
                    status
                }
                None => {
                    fault(problems, "a byte that no event begins with");
                    break;
                }
            }
        };
        match status {
            0xFF => {
                running_status = None;
                let (Some(kind), Some(length)) = (bytes.byte(), bytes.variable_length()) else {
                    fault(problems, "a meta event is cut short");
                    break;
                };
                let Some(body) = bytes.skip(length as usize) else {
                    fault(problems, "a meta event is cut short");
                    break;
                };
                match kind {
                    0x2F => {
                        // (The end of the track is where the track ends, whether or not a note is sounding then.)
                        track.last_tick = tick;
                        break;
                    }
                    0x51 if body.len() == 3 => {
                        let tempo = u32::from_be_bytes([0, body[0], body[1], body[2]]);
                        if tempo > 0 {
                            tempo_changes.push((tick, tempo));
                        }
                    }
                    0x03 if track.name.is_empty() => {
                        track.name = String::from_utf8_lossy(body)
                            .chars()
                            .filter(|character| !character.is_control())
                            .take(MOST_CHARACTERS_IN_THE_NAME_OF_A_TRACK)
                            .collect::<String>()
                            .trim()
                            .to_owned();
                    }
                    _ => {}
                }
            }
            0xF0 | 0xF7 => {
                running_status = None;
                let Some(length) = bytes.variable_length() else {
                    fault(problems, "a system message is cut short");
                    break;
                };
                if bytes.skip(length as usize).is_none() {
                    fault(problems, "a system message is cut short");
                    break;
                }
            }
            0x80..=0xEF => {
                running_status = Some(status);
                let channel = status & 15;
                let wanted = if matches!(status & 0xF0, 0xC0 | 0xD0) { 1 } else { 2 };
                let Some(data_bytes) = bytes.skip(wanted) else {
                    fault(problems, "an event is cut short");
                    break;
                };
                let (one, two) = (data_bytes[0], data_bytes.get(1).copied().unwrap_or(0));
                if (one | two) & 0x80 != 0 {
                    fault(problems, "an event has a byte of data that has the high bit set");
                    break;
                }
                let happening = match status & 0xF0 {
                    0x80 => Some(Happening::NoteOff { channel, key: one }),
                    0x90 if two == 0 => Some(Happening::NoteOff { channel, key: one }),
                    0x90 => Some(Happening::NoteOn {
                        channel,
                        key: one,
                        velocity: two,
                    }),
                    0xB0 => Some(Happening::Control(
                        MidiControlKind::Controller {
                            number: one,
                            value: two,
                        },
                        channel,
                    )),
                    0xE0 => Some(Happening::Control(
                        MidiControlKind::PitchBend {
                            value: u16::from(one) | (u16::from(two) << 7),
                        },
                        channel,
                    )),
                    // (The pressure of a key, a program change, the pressure of a channel: read past.)
                    _ => None,
                };
                if let Some(happening) = happening {
                    track.happenings.push((tick, happening));
                }
            }
            _ => {
                // (0xF1 to 0xFE are not in a file.)
                fault(problems, "a byte that no event begins with");
                break;
            }
        }
        track.last_tick = tick;
    }
    Ok(track)
}

/// Makes the ticks of a track into time, and pairs its notes.
fn make_the_track(raw: &RawTrack, map: &TempoMap) -> Result<MidiTrack, MidiFileProblem> {
    let microseconds = |tick: u64| map.microseconds_at(tick).ok_or(MidiFileProblem::TooLong);
    let end_us = microseconds(raw.last_tick)?;
    let mut notes: Vec<MidiNote> = Vec::new();
    let mut controls = Vec::new();
    // Where the notes that have begun and not ended are, by channel and key, the one that has waited longest first.
    let mut waiting: Vec<VecDeque<usize>> = vec![VecDeque::new(); 16 * 128];
    let mut ended: Vec<bool> = Vec::new();
    for &(tick, happening) in &raw.happenings {
        let time_us = microseconds(tick)?;
        match happening {
            Happening::NoteOn { channel, key, velocity } => {
                waiting[usize::from(channel) * 128 + usize::from(key)].push_back(notes.len());
                notes.push(MidiNote {
                    start_us: time_us,
                    end_us: time_us,
                    channel,
                    key,
                    velocity,
                });
                ended.push(false);
            }
            Happening::NoteOff { channel, key } => {
                if let Some(place) = waiting[usize::from(channel) * 128 + usize::from(key)].pop_front() {
                    notes[place].end_us = time_us;
                    ended[place] = true;
                }
            }
            Happening::Control(what, channel) => controls.push(MidiControl { time_us, channel, what }),
        }
    }
    // A note that was never ended lasts until its track does.
    for (note, was_ended) in notes.iter_mut().zip(&ended) {
        if !was_ended {
            note.end_us = end_us.max(note.start_us);
        }
    }
    let last_note_end = notes.iter().map(|note| note.end_us).max().unwrap_or(0);
    let key_range = notes
        .iter()
        .map(|note| note.key)
        .fold(None, |range: Option<(u8, u8)>, key| {
            Some(range.map_or((key, key), |(lowest, highest)| (lowest.min(key), highest.max(key))))
        });
    Ok(MidiTrack {
        name: raw.name.clone(),
        notes,
        controls,
        end_us: end_us.max(last_note_end),
        key_range,
    })
}

#[cfg(test)]
mod tests;
