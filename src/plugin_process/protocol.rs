// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the editor and the program that runs a plugin say to each other (phase 14, `docs/engine-phase-14-processes-design.md`, section 2).
//!
//! A **frame** is `u32 length` (of what follows, little-endian), `u8 kind`, `u32 header length`, the header and the raw rest. The cold messages (what to make, what was made, why not) are
//! JSON in the header; the two that go once for every block of sound (`Run` and `Ran`) are written by hand, with the samples as raw bytes after the header, **as the 32 bits they are**, so
//! that what a plugin makes in a process is the same bits as in the editor.
//!
//! **Nothing here panics on what it is given**: the editor and the program read each other's bytes, and a plugin that has damaged the memory of its process may say anything. A frame that is
//! too long, a count that is more than the bytes can hold, a kind that is not one: all of them are an [`Err`], and the connection is ended by whoever read it.

use std::io::{self, Read, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::plugin_host::WhatWasScanned;
use crate::sound_engine::{NoteEvent, NoteEventKind};

/// The environment variable that tells the program the token it says first (so that the editor knows that what connected to its port is the program it started).
pub const TOKEN_VARIABLE: &str = "FFRS_PLUGIN_PROCESS_TOKEN";

/// The most bytes there can be in one frame; a longer one is a broken connection. (A plugin's state of 40 MiB is sent whole.)
pub const MOST_BYTES_IN_A_FRAME: usize = 1 << 30;

/// What a frame says it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameKind {
    /// The program says its token.
    Hello = 1,
    /// The editor says which plugin to make.
    Make = 2,
    /// The program says that the plugin is made.
    Made = 3,
    /// The program says that the plugin could not be made, and why.
    Refused = 4,
    /// The editor gives a block of sound to run.
    Run = 5,
    /// The program gives what the plugin made of it.
    Ran = 6,
}

impl FrameKind {
    fn of_the_byte(byte: u8) -> Option<Self> {
        Some(match byte {
            1 => Self::Hello,
            2 => Self::Make,
            3 => Self::Made,
            4 => Self::Refused,
            5 => Self::Run,
            6 => Self::Ran,
            _ => return None,
        })
    }
}

/// One thing said.
#[derive(Debug, PartialEq, Eq)]
pub struct Frame {
    pub kind: FrameKind,
    pub header: Vec<u8>,
    pub raw: Vec<u8>,
}

/// Why a frame could not be read.
#[derive(Debug)]
pub enum FrameError {
    /// The connection said so: it ended, or a limit of time on a read ran out ([`io::ErrorKind::TimedOut`] on Windows, [`io::ErrorKind::WouldBlock`] elsewhere).
    Connection(io::Error),
    /// What came is not a frame.
    Broken(&'static str),
}

impl FrameError {
    /// Whether it is a read that ran out of time.
    pub fn is_a_timeout(&self) -> bool {
        matches!(self, Self::Connection(error) if matches!(error.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock))
    }
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connection(error) => write!(formatter, "{error}"),
            Self::Broken(why) => write!(formatter, "what was said is not a frame ({why})"),
        }
    }
}

/// Says a frame, in one write.
pub fn write_frame(out: &mut impl Write, kind: FrameKind, header: &[u8], raw: &[u8]) -> io::Result<()> {
    let body = 1 + 4 + header.len() + raw.len();
    let body_length = u32::try_from(body)
        .ok()
        .filter(|_| body <= MOST_BYTES_IN_A_FRAME)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "a frame is longer than there may be"))?;
    let mut bytes = Vec::with_capacity(4 + body);
    bytes.extend_from_slice(&body_length.to_le_bytes());
    bytes.push(kind as u8);
    bytes.extend_from_slice(&(header.len() as u32).to_le_bytes());
    bytes.extend_from_slice(header);
    bytes.extend_from_slice(raw);
    out.write_all(&bytes)?;
    out.flush()
}

/// Reads a frame.
pub fn read_frame(input: &mut impl Read) -> Result<Frame, FrameError> {
    let mut length = [0_u8; 4];
    input.read_exact(&mut length).map_err(FrameError::Connection)?;
    let length = u32::from_le_bytes(length) as usize;
    if length < 5 {
        return Err(FrameError::Broken("it is shorter than its own beginning"));
    }
    if length > MOST_BYTES_IN_A_FRAME {
        return Err(FrameError::Broken("it is longer than there may be"));
    }
    // (Not `vec![0; length]`: a length that a broken peer made up must not make a gigabyte before a byte of it has come.)
    let mut body = Vec::new();
    input
        .take(length as u64)
        .read_to_end(&mut body)
        .map_err(FrameError::Connection)?;
    if body.len() < length {
        return Err(FrameError::Connection(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "the connection ended in the middle of a frame",
        )));
    }
    let kind = FrameKind::of_the_byte(body[0]).ok_or(FrameError::Broken("it is of a kind that there is not"))?;
    let header_length = u32::from_le_bytes([body[1], body[2], body[3], body[4]]) as usize;
    if header_length > body.len() - 5 {
        return Err(FrameError::Broken("its header is longer than it is"));
    }
    let raw = body.split_off(5 + header_length);
    body.drain(..5);
    Ok(Frame {
        kind,
        header: body,
        raw,
    })
}

/// What the program says first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub token: String,
}

/// Which plugin of a library, in the way the format says.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Address {
    Ladspa { index: usize },
    Clap { id: String },
    Lv2 { uri: String },
}

/// What the editor asks the program to make. The plugin's state, if the project has one, is the raw rest of the frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Make {
    pub address: Address,
    /// A library file, or an LV2 bundle's folder.
    pub library: PathBuf,
    /// What the scan said the plugin is: the copy that is made must be that.
    pub scanned: WhatWasScanned,
    /// Whether the project has a state for the plugin (it is the raw rest of the frame, and may be empty).
    pub has_state: bool,
}

/// What a copy that was asked for and was asked to be made again between blocks has done, as far as the sound of it goes: said by the program with every answer, and gathered by the editor
/// until it is asked.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhatWasDoneBetweenBlocks {
    /// The plugin was made again (it asked to be): it has forgotten the values it was given.
    pub was_restarted: bool,
    /// Words for the person, in the order they came.
    pub notes: Vec<String>,
}

/// What the program says of the copy it made.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Made {
    pub latency: u32,
    pub takes_values_at_samples: bool,
    pub most_samples_at_once: Option<usize>,
    /// Why the plugin did not take the state it was given, if it did not.
    pub state_not_taken: Option<String>,
    pub done_between_blocks: WhatWasDoneBetweenBlocks,
}

/// Why the program did not make the copy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Refused {
    /// The library could not be loaded, or does not have the plugin any more.
    CouldNotBeLoaded(String),
    /// The plugin is not what the scan said it is (the library has changed).
    NotWhatWasScanned,
    /// The plugin said no when it was made.
    CouldNotBeMade(String),
}

/// Something to do to the copy before a run, in the order it was asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    /// Make the value the plugin calls `handle` `value`, from sample `at_sample` of the run (or at once, for a format that cannot say when).
    SetValue { handle: u32, value: f32, at_sample: u32 },
    /// Make the plugin forget what it has heard.
    Reset,
}

/// A block of sound to run, without its samples (which are the raw rest of the frame: `inputs` channels of `frames` samples).
#[derive(Clone, Debug, PartialEq)]
pub struct RunRequest {
    pub commands: Vec<Command>,
    pub frames: usize,
    pub inputs: usize,
    pub outputs: usize,
    /// The notes of the run, if the plugin is given notes (`run_with_notes`); `None` is a plain `run`.
    pub notes: Option<Vec<NoteEvent>>,
}

/// What came of a run, without its samples (`outputs` channels of `frames` samples are the raw rest of the frame, if it went well).
#[derive(Clone, Debug, PartialEq)]
pub struct Ran {
    pub result: Result<(), String>,
    pub latency: u32,
    /// What the plugin's control outputs hold: `(handle, value)`.
    pub meters: Vec<(u32, f32)>,
    pub done_between_blocks: WhatWasDoneBetweenBlocks,
}

/// Why bytes are not a message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotAMessage(pub &'static str);

impl std::fmt::Display for NotAMessage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "what was said is not a message ({})", self.0)
    }
}

/// What reads the numbers of a header, one after another.
struct Reader<'bytes> {
    bytes: &'bytes [u8],
}

impl<'bytes> Reader<'bytes> {
    fn take(&mut self, count: usize) -> Result<&'bytes [u8], NotAMessage> {
        if count > self.bytes.len() {
            return Err(NotAMessage("it ends too soon"));
        }
        let (taken, rest) = self.bytes.split_at(count);
        self.bytes = rest;
        Ok(taken)
    }

    fn byte(&mut self) -> Result<u8, NotAMessage> {
        Ok(self.take(1)?[0])
    }

    fn number(&mut self) -> Result<u32, NotAMessage> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn float(&mut self) -> Result<f32, NotAMessage> {
        Ok(f32::from_bits(self.number()?))
    }

    fn text(&mut self) -> Result<String, NotAMessage> {
        let length = self.number()? as usize;
        String::from_utf8(self.take(length)?.to_vec()).map_err(|_| NotAMessage("a text is not UTF-8"))
    }

    /// A count of things that are each at least `least_bytes` long: one that the rest of the bytes cannot hold is refused before anything is made for it.
    fn count(&mut self, least_bytes: usize) -> Result<usize, NotAMessage> {
        let count = self.number()? as usize;
        if count > self.bytes.len() / least_bytes.max(1) {
            return Err(NotAMessage("it counts more than it holds"));
        }
        Ok(count)
    }

    fn finish(self) -> Result<(), NotAMessage> {
        if self.bytes.is_empty() {
            Ok(())
        } else {
            Err(NotAMessage("it goes on after its end"))
        }
    }
}

fn put_number(into: &mut Vec<u8>, number: u32) {
    into.extend_from_slice(&number.to_le_bytes());
}

fn put_text(into: &mut Vec<u8>, text: &str) {
    put_number(into, text.len() as u32);
    into.extend_from_slice(text.as_bytes());
}

const NOTE_ON: u8 = 0;
const NOTE_OFF: u8 = 1;
const CONTROLLER: u8 = 2;
const PITCH_BEND: u8 = 3;
const ALL_OFF: u8 = 4;

/// How many bytes a note takes: the time, a tag, three bytes, and two numbers.
const BYTES_OF_A_NOTE: usize = 16;

fn put_note(into: &mut Vec<u8>, note: &NoteEvent) {
    put_number(into, note.time);
    let (tag, channel, key, value, velocity, id) = match note.what {
        NoteEventKind::On {
            channel,
            key,
            velocity,
            id,
        } => (NOTE_ON, channel, key, 0, velocity, id),
        NoteEventKind::Off {
            channel,
            key,
            velocity,
            id,
        } => (NOTE_OFF, channel, key, 0, velocity, id),
        NoteEventKind::Controller { channel, number, value } => (CONTROLLER, channel, number, value, 0.0, 0),
        NoteEventKind::PitchBend { channel, value } => (PITCH_BEND, channel, 0, 0, 0.0, u32::from(value)),
        NoteEventKind::AllOff => (ALL_OFF, 0, 0, 0, 0.0, 0),
    };
    into.extend_from_slice(&[tag, channel, key, value]);
    put_number(into, velocity.to_bits());
    put_number(into, id);
}

fn note_in(reader: &mut Reader<'_>) -> Result<NoteEvent, NotAMessage> {
    let time = reader.number()?;
    let [tag, channel, key, value] = [reader.byte()?, reader.byte()?, reader.byte()?, reader.byte()?];
    let velocity = reader.float()?;
    let id = reader.number()?;
    let what = match tag {
        NOTE_ON => NoteEventKind::On {
            channel,
            key,
            velocity,
            id,
        },
        NOTE_OFF => NoteEventKind::Off {
            channel,
            key,
            velocity,
            id,
        },
        CONTROLLER => NoteEventKind::Controller {
            channel,
            number: key,
            value,
        },
        PITCH_BEND => NoteEventKind::PitchBend {
            channel,
            value: u16::try_from(id).map_err(|_| NotAMessage("a pitch wheel is further than it goes"))?,
        },
        ALL_OFF => NoteEventKind::AllOff,
        _ => return Err(NotAMessage("a note is of a kind that there is not")),
    };
    Ok(NoteEvent { time, what })
}

const SET_VALUE: u8 = 0;
const RESET: u8 = 1;

fn put_what_was_done(into: &mut Vec<u8>, done: &WhatWasDoneBetweenBlocks) {
    into.push(u8::from(done.was_restarted));
    put_number(into, done.notes.len() as u32);
    for note in &done.notes {
        put_text(into, note);
    }
}

fn what_was_done_in(reader: &mut Reader<'_>) -> Result<WhatWasDoneBetweenBlocks, NotAMessage> {
    let was_restarted = reader.byte()? != 0;
    // (Each note is at least the four bytes of its length.)
    let count = reader.count(4)?;
    let notes = (0..count).map(|_| reader.text()).collect::<Result<_, _>>()?;
    Ok(WhatWasDoneBetweenBlocks { was_restarted, notes })
}

impl RunRequest {
    /// The header of the frame that asks for the run.
    pub fn header(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(
            32 + self.commands.len() * 13 + self.notes.as_ref().map_or(0, Vec::len) * BYTES_OF_A_NOTE,
        );
        put_number(&mut bytes, self.commands.len() as u32);
        for command in &self.commands {
            match *command {
                Command::SetValue {
                    handle,
                    value,
                    at_sample,
                } => {
                    bytes.push(SET_VALUE);
                    put_number(&mut bytes, handle);
                    put_number(&mut bytes, value.to_bits());
                    put_number(&mut bytes, at_sample);
                }
                Command::Reset => bytes.push(RESET),
            }
        }
        put_number(&mut bytes, self.frames as u32);
        bytes.push(self.inputs as u8);
        bytes.push(self.outputs as u8);
        match &self.notes {
            None => bytes.push(0),
            Some(notes) => {
                bytes.push(1);
                put_number(&mut bytes, notes.len() as u32);
                for note in notes {
                    put_note(&mut bytes, note);
                }
            }
        }
        bytes
    }

    /// The request that a header is.
    pub fn of_the_header(header: &[u8]) -> Result<Self, NotAMessage> {
        let mut reader = Reader { bytes: header };
        // (A command is a byte at least.)
        let count = reader.count(1)?;
        let mut commands = Vec::with_capacity(count);
        for _ in 0..count {
            commands.push(match reader.byte()? {
                SET_VALUE => Command::SetValue {
                    handle: reader.number()?,
                    value: reader.float()?,
                    at_sample: reader.number()?,
                },
                RESET => Command::Reset,
                _ => return Err(NotAMessage("a command is of a kind that there is not")),
            });
        }
        let frames = reader.number()? as usize;
        let inputs = usize::from(reader.byte()?);
        let outputs = usize::from(reader.byte()?);
        let notes = match reader.byte()? {
            0 => None,
            1 => {
                let count = reader.count(BYTES_OF_A_NOTE)?;
                Some((0..count).map(|_| note_in(&mut reader)).collect::<Result<_, _>>()?)
            }
            _ => return Err(NotAMessage("whether there are notes is neither yes nor no")),
        };
        reader.finish()?;
        Ok(Self {
            commands,
            frames,
            inputs,
            outputs,
            notes,
        })
    }
}

impl Ran {
    /// The header of the frame that answers a run.
    pub fn header(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(32 + self.meters.len() * 8);
        match &self.result {
            Ok(()) => bytes.push(1),
            Err(why) => {
                bytes.push(0);
                put_text(&mut bytes, why);
            }
        }
        put_number(&mut bytes, self.latency);
        put_number(&mut bytes, self.meters.len() as u32);
        for (handle, value) in &self.meters {
            put_number(&mut bytes, *handle);
            put_number(&mut bytes, value.to_bits());
        }
        put_what_was_done(&mut bytes, &self.done_between_blocks);
        bytes
    }

    /// The answer that a header is.
    pub fn of_the_header(header: &[u8]) -> Result<Self, NotAMessage> {
        let mut reader = Reader { bytes: header };
        let result = match reader.byte()? {
            1 => Ok(()),
            0 => Err(reader.text()?),
            _ => return Err(NotAMessage("whether it went well is neither yes nor no")),
        };
        let latency = reader.number()?;
        let count = reader.count(8)?;
        let meters = (0..count)
            .map(|_| Ok((reader.number()?, reader.float()?)))
            .collect::<Result<_, NotAMessage>>()?;
        let done_between_blocks = what_was_done_in(&mut reader)?;
        reader.finish()?;
        Ok(Self {
            result,
            latency,
            meters,
            done_between_blocks,
        })
    }
}

/// The samples of `channels` (the first `frames` of each, one channel after another) as the raw bytes of a frame, appended to `into`.
pub fn put_samples<'channel>(into: &mut Vec<u8>, channels: impl IntoIterator<Item = &'channel [f32]>, frames: usize) {
    for channel in channels {
        for sample in &channel[..frames] {
            into.extend_from_slice(&sample.to_bits().to_le_bytes());
        }
    }
}

/// Puts the samples of the raw bytes of a frame in `channels` (the first `frames` of each); the bytes must be exactly those of that many samples.
pub fn take_samples(raw: &[u8], channels: &mut [&mut [f32]], frames: usize) -> Result<(), NotAMessage> {
    if raw.len() != channels.len() * frames * 4 {
        return Err(NotAMessage("the samples are not as many as were said"));
    }
    if channels.iter().any(|channel| channel.len() < frames) {
        return Err(NotAMessage("a channel is shorter than the samples for it"));
    }
    let mut numbers = raw.as_chunks::<4>().0.iter();
    for channel in channels {
        for sample in &mut channel[..frames] {
            let bytes = numbers.next().expect("there are as many as were counted");
            *sample = f32::from_bits(u32::from_le_bytes(*bytes));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
