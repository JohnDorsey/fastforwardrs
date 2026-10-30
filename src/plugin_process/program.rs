// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The inside of the program that runs a plugin (`fastforwardrs-plugin-process`, which is only this, called from `main`). **It loads the library it is asked to**, makes one copy of one plugin,
//! and runs the blocks of sound it is given, until the editor is gone. It is a program of its own so that a plugin that crashes it, or never returns from it, costs the editor one copy
//! of one plugin and not the editor and the project (design: `docs/engine-phase-14-processes-design.md`).
//!
//! The program is started by the editor with the address of the editor's socket (`--connect`) and, in the environment, the token it says first. **The thread that reads the socket ends the
//! process the moment the editor's end is closed**, even if the plugin is in the middle of a call: a program that is left behind with a plugin that never returns would be there for ever.

use std::ffi::OsString;
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::protocol::{
    Address, Command, Frame, FrameKind, Hello, Made, Make, Ran, Refused, RunRequest, WhatWasDoneBetweenBlocks,
    put_samples, read_frame, take_samples, write_frame,
};
use crate::plugin_host::{
    ClapCopy, ClapMaker, LadspaCopy, Lives, Lv2Copy, MOST_FRAMES_OF_AN_LV2_PLUGIN, PluginInstance,
};
use crate::sound_engine::SAMPLE_RATE;

/// What the program says when it is used wrongly.
pub const USAGE: &str = "Usage: fastforwardrs-plugin-process --connect <address>

Runs one copy of a sound plugin for the editor. It is started by the editor, which gives it the
address of its socket and, in the environment, the token to say first (see plugin_process::protocol);
it ends when the editor's end of the socket is closed.
";

/// How long the program waits to connect to the editor, which is listening already.
const HOW_LONG_TO_WAIT_TO_CONNECT: Duration = Duration::from_secs(10);

/// The most frames of one run that the program makes room for. (A run is a block of the sound engine, 512 samples; a request for more than this is a broken request, and room for it would be
/// the memory of the computer.)
pub const MOST_FRAMES_OF_A_RUN: usize = 1 << 20;

/// The most channels of one side of a run that the program makes room for.
pub const MOST_CHANNELS_OF_A_RUN: usize = 64;

/// How the program ends when something other than the editor's going away ends it.
pub mod exit {
    pub const EDITOR_GONE: i32 = 0;
    pub const NOT_CONNECTED: i32 = 3;
    pub const NOT_ASKED_TO_MAKE_A_COPY: i32 = 4;
    pub const SAID_SOMETHING_BROKEN: i32 = 5;
}

/// Reads what the program was started with (not its name): where the editor's socket is.
pub fn parse_arguments(arguments: impl IntoIterator<Item = OsString>) -> Result<SocketAddr, String> {
    let mut arguments = arguments.into_iter();
    let mut address = None;
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--connect" => {
                let value = arguments.next().ok_or("--connect needs an address after it")?;
                address = Some(
                    value
                        .to_string_lossy()
                        .parse::<SocketAddr>()
                        .map_err(|error| format!("{:?} is not an address ({error})", value.to_string_lossy()))?,
                );
            }
            other => return Err(format!("{other:?} is not something this program is told")),
        }
    }
    address.ok_or_else(|| "--connect is needed".to_owned())
}

/// Runs the program: connects to the editor, says the token, makes the copy that is asked for and runs what it is given. Returns how the program ends (see [`exit`]); **it returns only
/// when the editor did not ask for what the program is for** (otherwise it ends the process itself, when the editor goes).
pub fn run(address: SocketAddr, token: &str) -> i32 {
    let Ok(mut stream) = TcpStream::connect_timeout(&address, HOW_LONG_TO_WAIT_TO_CONNECT) else {
        return exit::NOT_CONNECTED;
    };
    let _ = stream.set_nodelay(true);
    let hello = serde_json::to_vec(&Hello {
        token: token.to_owned(),
    })
    .expect("a hello can be written");
    if write_frame(&mut stream, FrameKind::Hello, &hello, &[]).is_err() {
        return exit::NOT_CONNECTED;
    }
    let Ok(to_read_from) = stream.try_clone() else {
        return exit::NOT_CONNECTED;
    };
    let requests = read_requests_on_a_thread_of_their_own(to_read_from);

    let Ok(first) = requests.recv() else {
        return exit::EDITOR_GONE;
    };
    let Some((make, state)) = what_to_make(&first) else {
        return exit::NOT_ASKED_TO_MAKE_A_COPY;
    };
    let mut copy = match make_the_copy(&make, state) {
        Ok((copy, made)) => {
            say(
                &mut stream,
                FrameKind::Made,
                &serde_json::to_vec(&made).expect("a message can be written"),
            );
            copy
        }
        Err(refused) => {
            say(
                &mut stream,
                FrameKind::Refused,
                &serde_json::to_vec(&refused).expect("a message can be written"),
            );
            return exit::EDITOR_GONE;
        }
    };
    // (A request that is not a run, or is not a good one, is a broken editor: this ends, and so does the copy with it.)
    while let Ok(frame) = requests.recv() {
        let Some((answer, raw)) = serve(copy.as_mut(), &frame) else {
            return exit::SAID_SOMETHING_BROKEN;
        };
        if write_frame(&mut stream, FrameKind::Ran, &answer.header(), &raw).is_err() {
            return exit::EDITOR_GONE;
        }
    }
    exit::EDITOR_GONE
}

/// Reads the frames of `stream` on a thread of its own and hands them over; **ends the process when the stream does** (the editor is gone, or has closed it on purpose): whatever the
/// plugin is doing, this program has nothing left to do.
fn read_requests_on_a_thread_of_their_own(mut stream: TcpStream) -> Receiver<Frame> {
    let (sender, requests) = mpsc::channel();
    std::thread::spawn(move || {
        loop {
            match read_frame(&mut stream) {
                Ok(frame) => {
                    if sender.send(frame).is_err() {
                        return;
                    }
                }
                Err(_) => std::process::exit(exit::EDITOR_GONE),
            }
        }
    });
    requests
}

/// Says a frame; an editor that is gone cannot be told, and the reading thread ends the process.
fn say(stream: &mut TcpStream, kind: FrameKind, header: &[u8]) {
    let _ = write_frame(stream, kind, header, &[]);
}

/// What the first frame asks to be made, and the plugin's state if there is one.
fn what_to_make(frame: &Frame) -> Option<(Make, Option<Arc<[u8]>>)> {
    if frame.kind != FrameKind::Make {
        return None;
    }
    let make: Make = serde_json::from_slice(&frame.header).ok()?;
    let state = make.has_state.then(|| Arc::<[u8]>::from(frame.raw.as_slice()));
    Some((make, state))
}

/// Makes the copy that `make` asks for, given its state if it has one, and says what it is like. **The same steps, with the same checks, as the editor takes for a copy in its own
/// process** (`PluginHost`), once for each format.
pub fn make_the_copy(make: &Make, state: Option<Arc<[u8]>>) -> Result<(Box<dyn PluginInstance>, Made), Refused> {
    let (mut copy, state_not_taken): (Box<dyn PluginInstance>, Option<String>) = match &make.address {
        Address::Clap { id } => make_a_clap_copy(make, id, state)?,
        Address::Lv2 { uri } => make_an_lv2_copy(make, uri, state.as_deref())?,
        Address::Ladspa { index } => (make_a_ladspa_copy(make, *index)?, None),
    };
    // (What the plugin asked for while it was made is done before anyone is told: the editor's first question is answered with it.)
    let done_between_blocks = what_was_done_by(copy.as_mut());
    let made = Made {
        latency: u32::try_from(copy.latency()).unwrap_or(u32::MAX),
        takes_values_at_samples: copy.takes_values_at_samples(),
        most_samples_at_once: copy.most_samples_at_once(),
        state_not_taken,
        done_between_blocks,
    };
    Ok((copy, made))
}

fn make_a_clap_copy(
    make: &Make,
    id: &str,
    state: Option<Arc<[u8]>>,
) -> Result<(Box<dyn PluginInstance>, Option<String>), Refused> {
    let library =
        clap_host::Library::open(&make.library).map_err(|error| Refused::CouldNotBeLoaded(error.to_string()))?;
    // (The copy that is made again when it asks to be needs a place to say that it is alive, as every copy does; here nothing waits for it.)
    let lives = Arc::<Mutex<Lives>>::default();
    let alive = Lives::is_alive(&lives, &make.library, 0);
    let maker = ClapMaker {
        worker: clap_host::MainThread::start(),
        library,
        id: id.to_owned(),
        state,
    };
    let (instance, state_not_taken) = maker
        .make_and_say_if_the_state_was_not_taken()
        .map_err(|error| Refused::CouldNotBeMade(error.to_string()))?;
    if !make.scanned.is_this_clap_plugin(&instance) {
        return Err(Refused::NotWhatWasScanned);
    }
    Ok((Box::new(ClapCopy::new(instance, maker, alive)), state_not_taken))
}

fn make_an_lv2_copy(
    make: &Make,
    uri: &str,
    state: Option<&[u8]>,
) -> Result<(Box<dyn PluginInstance>, Option<String>), Refused> {
    let bundle =
        lv2_host::describe_bundle(&make.library).map_err(|error| Refused::CouldNotBeLoaded(error.to_string()))?;
    let Some(description) = bundle.plugins.iter().find(|candidate| candidate.uri == uri) else {
        return Err(Refused::CouldNotBeLoaded(format!(
            "the bundle does not describe a plugin called {uri} any more"
        )));
    };
    let library =
        lv2_host::Library::open(&description.binary).map_err(|error| Refused::CouldNotBeLoaded(error.to_string()))?;
    if !make.scanned.is_this_lv2_plugin(description) {
        return Err(Refused::NotWhatWasScanned);
    }
    let mut instance = lv2_host::Instance::new(
        &library,
        description,
        f64::from(SAMPLE_RATE),
        MOST_FRAMES_OF_AN_LV2_PLUGIN as u32,
    )
    .map_err(|error| Refused::CouldNotBeMade(error.to_string()))?;
    // The state before the values: what the project says the values are is what they are, whatever the state says.
    let state_not_taken = state
        .filter(|_| instance.has_state())
        .and_then(|bytes| instance.restore_state(bytes).err())
        .map(|error| error.to_string());
    Ok((Box::new(Lv2Copy(instance)), state_not_taken))
}

fn make_a_ladspa_copy(make: &Make, index: usize) -> Result<Box<dyn PluginInstance>, Refused> {
    let library =
        ladspa_host::Library::open(&make.library).map_err(|error| Refused::CouldNotBeLoaded(error.to_string()))?;
    let instance = library
        .instantiate(index, SAMPLE_RATE)
        .map_err(|error| Refused::CouldNotBeMade(error.to_string()))?;
    if !make.scanned.is_this_ladspa_plugin(&instance) {
        return Err(Refused::NotWhatWasScanned);
    }
    Ok(Box::new(LadspaCopy(instance)))
}

/// What the copy has asked of its host since it was last asked, done.
fn what_was_done_by(copy: &mut dyn PluginInstance) -> WhatWasDoneBetweenBlocks {
    let done = copy.between_blocks();
    WhatWasDoneBetweenBlocks {
        was_restarted: done.was_restarted,
        notes: done.note.into_iter().collect(),
    }
}

/// Does what a request says and answers it: the commands, then the run, then what the copy asked for while it ran. `None` if the frame is not a run, or is not a good one.
fn serve(copy: &mut dyn PluginInstance, frame: &Frame) -> Option<(Ran, Vec<u8>)> {
    if frame.kind != FrameKind::Run {
        return None;
    }
    let request = RunRequest::of_the_header(&frame.header).ok()?;
    if request.frames > MOST_FRAMES_OF_A_RUN
        || request.inputs > MOST_CHANNELS_OF_A_RUN
        || request.outputs > MOST_CHANNELS_OF_A_RUN
    {
        return None;
    }
    for command in &request.commands {
        match *command {
            Command::SetValue {
                handle,
                value,
                at_sample,
            } => copy.set_value(handle, value, at_sample as usize),
            Command::Reset => copy.reset(),
        }
    }
    let mut inputs: Vec<Vec<f32>> = vec![vec![0.0; request.frames]; request.inputs];
    let mut outputs: Vec<Vec<f32>> = vec![vec![0.0; request.frames]; request.outputs];
    {
        let mut input_slices: Vec<&mut [f32]> = inputs.iter_mut().map(Vec::as_mut_slice).collect();
        take_samples(&frame.raw, &mut input_slices, request.frames).ok()?;
    }
    let result = {
        let mut input_slices: Vec<&mut [f32]> = inputs.iter_mut().map(Vec::as_mut_slice).collect();
        let mut output_slices: Vec<&mut [f32]> = outputs.iter_mut().map(Vec::as_mut_slice).collect();
        match &request.notes {
            Some(notes) => copy.run_with_notes(&mut input_slices, &mut output_slices, request.frames, notes),
            None => copy.run(&mut input_slices, &mut output_slices, request.frames),
        }
    };
    // (What the plugin asked for during the run is done now, before the answer: the editor finds out at the start of its next block, as it would have in its own process.)
    let done_between_blocks = what_was_done_by(copy);
    let mut raw = Vec::new();
    if result.is_ok() {
        put_samples(&mut raw, outputs.iter().map(Vec::as_slice), request.frames);
    }
    Some((
        Ran {
            result,
            latency: u32::try_from(copy.latency()).unwrap_or(u32::MAX),
            meters: copy.outputs(),
            done_between_blocks,
        },
        raw,
    ))
}
