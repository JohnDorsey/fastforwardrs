// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The editor's side of a plugin in a process of its own (phase 14): a [`Launcher`] starts the program for a plugin, and gives a [`RemoteCopy`], which is a
//! [`PluginInstance`] like the copies that run in the editor's process. **Whatever the program does comes out as an error of the call that was
//! waiting for it**: it ends without a word (a crash), it never answers (a hang: the call has a time limit and the program is stopped), it says what is not a message.
//!
//! **Then the program is started again** (a copy made by [`Launcher::start_and_start_again_when_it_ends`], at most [`MOST_RESTARTS_OF_A_PROGRAM`] times): the new one makes the plugin as the
//! first did, is given the values the plugin was last given (`told`), and the block that the other ended in is run again in it with the notes that were held played again from its first sample, so the
//! sound goes on and the copy says between two blocks what happened. A plugin that has the trouble every time, or a program that cannot be started, ends the copy: it answers every call with the
//! same error at once, and the effect that has it leaves it out for the rest of the play and says why (`PluginEffect`).

use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, ExitStatus, Stdio};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::protocol::{
    Command, Frame, FrameError, FrameKind, Hello, Made, Make, Ran, Refused, RunRequest, TOKEN_VARIABLE,
    WhatWasDoneBetweenBlocks, put_samples, read_frame, take_samples, write_frame,
};
use super::told::WhatTheProgramWasTold;
use crate::media::{background_command, collect_error_output_in_background};
use crate::plugin_host::{BetweenBlocks, PluginInstance};
use crate::plugin_scan::child::how_it_ended_badly;
use crate::sound_engine::NoteEvent;

/// The name of the program that runs a plugin, without the ending that programs have on this system.
pub const PROGRAM_NAME: &str = "fastforwardrs-plugin-process";

/// Where the program is, if it is beside the program that is running (as the scan program is), or, when this is a test of the library, in the folder above.
pub fn program_beside_this_program() -> Option<PathBuf> {
    crate::plugin_scan::child::program_beside_this_program(PROGRAM_NAME)
}

/// Whether `program` was built from the code that the editor was (a `cargo run` builds the editor and not the programs beside it), or why not.
pub fn check_that_the_program_is_of_this_build(program: &std::path::Path) -> Result<(), String> {
    use crate::plugin_scan::protocol::BUILD_ID;
    match crate::plugin_scan::child::build_id_of_a_program(program) {
        Ok(Some(id)) if id == BUILD_ID => Ok(()),
        Ok(Some(_) | None) => Err(format!(
            "the program that runs plugins ({PROGRAM_NAME}) was built from other code than the editor: build them together (cargo build --bins)"
        )),
        Err(error) => Err(format!(
            "the program that runs plugins ({PROGRAM_NAME}) could not be run: {error}"
        )),
    }
}

/// How long the editor waits for each thing the program does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// For the program to start and connect.
    pub to_connect: Duration,
    /// For the plugin to be made (a plugin that needs longer is not one to use in a preview: the scan has the same limit).
    pub to_make: Duration,
    /// For one run, which is a block of sound of 512 samples: a plugin that needs this long for one is hung.
    pub to_run: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            to_connect: Duration::from_secs(20),
            to_make: Duration::from_secs(30),
            to_run: Duration::from_secs(5),
        }
    }
}

/// How many times the program of one copy is started again when it ends, at the most: a plugin that ends its program every time it is made would be started again for ever, and the sound
/// would be a gap after a gap.
pub const MOST_RESTARTS_OF_A_PROGRAM: usize = 5;

/// How long the editor waits for a program that has said goodbye (or lost its connection) to end by itself, before it is stopped.
const HOW_LONG_A_PROGRAM_MAY_TAKE_TO_END: Duration = Duration::from_millis(300);

/// How long a connection that has just been made is given to say its first word.
const HOW_LONG_A_CONNECTION_MAY_TAKE_TO_SAY_HELLO: Duration = Duration::from_secs(5);

/// What starts the program for a plugin.
#[derive(Clone, Debug)]
pub struct Launcher {
    /// The program (`fastforwardrs-plugin-process`).
    pub program: PathBuf,
    /// What the program's environment is given beside the editor's own: where the libraries that the plugin library needs are looked for (see
    /// [`PluginHost::environment_for_a_program_that_loads`](crate::plugin_host::PluginHost::environment_for_a_program_that_loads)).
    pub environment: Vec<(String, String)>,
    pub limits: Limits,
}

impl Launcher {
    pub fn new(program: PathBuf) -> Self {
        Self {
            program,
            environment: Vec::new(),
            limits: Limits::default(),
        }
    }

    #[must_use]
    pub fn with_environment(mut self, environment: Vec<(String, String)>) -> Self {
        self.environment = environment;
        self
    }

    #[must_use]
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }
}

/// Why a copy could not be started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotStarted {
    /// The program could not be started, did not connect, ended or did not answer: why.
    TheProgram(String),
    /// The program was reached and would not make the copy: why.
    Refused(Refused),
}

impl std::fmt::Display for NotStarted {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TheProgram(why) => write!(formatter, "{why}"),
            Self::Refused(Refused::CouldNotBeLoaded(why)) => {
                write!(formatter, "the library could not be loaded: {why}")
            }
            Self::Refused(Refused::NotWhatWasScanned) => {
                write!(formatter, "the plugin is not what it was when the plugins were scanned")
            }
            Self::Refused(Refused::CouldNotBeMade(why)) => write!(formatter, "the plugin could not be made: {why}"),
        }
    }
}

/// The program, started: its end is its end.
struct Process {
    child: Child,
    /// What it said on its error output (the last of it), collected as it runs.
    errors: Option<JoinHandle<String>>,
}

impl Process {
    fn start(launcher: &Launcher, address: SocketAddr, token: &str) -> Result<Self, String> {
        let mut child = background_command(&launcher.program)
            .envs(launcher.environment.iter().map(|(name, value)| (name, value)))
            .env(TOKEN_VARIABLE, token)
            .arg("--connect")
            .arg(address.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                format!(
                    "the program that runs plugins ({}) could not be started: {error}",
                    launcher.program.display()
                )
            })?;
        let errors = child.stderr.take().map(collect_error_output_in_background);
        Ok(Self { child, errors })
    }

    /// The status the program ended with, if it ends within `how_long`.
    fn status_within(&mut self, how_long: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now() + how_long;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(1)),
                Ok(None) | Err(_) => return None,
            }
        }
    }

    /// The last of what the program said on its error output, on one line, if it said anything (it has ended: what it said is all in).
    fn what_it_said(&mut self) -> Option<String> {
        let errors = self.errors.take()?;
        let deadline = Instant::now() + Duration::from_millis(200);
        while !errors.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        if !errors.is_finished() {
            return None;
        }
        let said = errors.join().ok()?;
        let said: String = said.split_whitespace().collect::<Vec<_>>().join(" ");
        let said = said
            .chars()
            .rev()
            .take(300)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<String>();
        (!said.is_empty()).then_some(said)
    }

    /// Stops the program, if it is running, and waits for it to be gone.
    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// What became of a program whose connection ended, in words.
    fn how_it_ended(&mut self) -> String {
        match self.status_within(Duration::from_millis(1500)) {
            Some(status) => {
                let how = how_it_ended_badly(status);
                match self.what_it_said() {
                    Some(said) => format!("the program that runs the plugin ended ({how}) after saying: {said}"),
                    None => format!("the program that runs the plugin ended ({how})"),
                }
            }
            None => {
                self.stop();
                "the program that runs the plugin closed its connection and did not end, and was stopped".to_owned()
            }
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // (The program ends by itself when its end of the connection is closed; a hung one is stopped.)
        if self.status_within(HOW_LONG_A_PROGRAM_MAY_TAKE_TO_END).is_none() {
            self.stop();
        }
    }
}

/// The connection to a program that is up, and the program if the editor started it.
struct Connection {
    stream: TcpStream,
    process: Option<Process>,
}

impl Drop for Connection {
    fn drop(&mut self) {
        // (Closing the connection is what tells the program that it is done.)
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

/// How many seconds a time is, as a number that is not rounded (20 for twenty seconds, 0.3 for three tenths of one).
fn seconds_in_words(time: Duration) -> String {
    format!("{}", time.as_secs_f64())
}

/// A random word, as hard to guess as the system's random numbers: what the program says first, so that the editor knows that what connected to its port is the program it started.
fn a_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    (0..4)
        .map(|_| {
            let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
            hasher.write_u128(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |since| since.as_nanos()),
            );
            format!("{:016x}", hasher.finish())
        })
        .collect()
}

impl Launcher {
    /// Starts the program, asks it to make the plugin that `make` says (given `state`, which is the plugin's state if the project has one), and gives the copy and what the program says of it.
    /// **The copy is gone for good when its program ends**: see [`Self::start_and_start_again_when_it_ends`] for one that is not.
    pub fn start(&self, make: &Make, state: Option<&[u8]>) -> Result<(RemoteCopy, Made), NotStarted> {
        let (connection, made) = self.connect(make, state)?;
        Ok((RemoteCopy::over(connection, &made, self.limits), made))
    }

    /// The same, and **when the program ends (it crashes, never answers, says nonsense) the program is started again** (at most [`MOST_RESTARTS_OF_A_PROGRAM`] times for the copy): the
    /// new one is made like the first, given the values the plugin was last given and the notes that are held, and the block that the other ended in is run again in it. The copy goes on
    /// as if nothing had happened, and says what happened between two blocks. (A plugin that has the same trouble each time it is made, or that cannot be made again, ends the copy as before.)
    pub fn start_and_start_again_when_it_ends(
        &self,
        make: &Make,
        state: Option<Arc<[u8]>>,
    ) -> Result<(RemoteCopy, Made), NotStarted> {
        let (connection, made) = self.connect(make, state.as_deref())?;
        let (launcher, make) = (self.clone(), make.clone());
        let mut copy = RemoteCopy::over(connection, &made, self.limits);
        copy.restart = Some(Box::new(move || launcher.connect(&make, state.as_deref())));
        Ok((copy, made))
    }

    /// Starts the program and has it make the plugin: the connection to it, and what it says of the plugin.
    fn connect(&self, make: &Make, state: Option<&[u8]>) -> Result<(Connection, Made), NotStarted> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|error| {
            NotStarted::TheProgram(format!(
                "the editor's socket for the program that runs plugins could not be made: {error}"
            ))
        })?;
        listener
            .set_nonblocking(true)
            .map_err(|error| NotStarted::TheProgram(format!("the editor's socket could not be set up: {error}")))?;
        let address = listener
            .local_addr()
            .map_err(|error| NotStarted::TheProgram(format!("the editor's socket has no address: {error}")))?;
        let token = a_token();
        let mut process = Process::start(self, address, &token).map_err(NotStarted::TheProgram)?;
        let stream = accept_the_program(
            &listener,
            Some(&mut process),
            &token,
            self.limits.to_connect,
            HOW_LONG_A_CONNECTION_MAY_TAKE_TO_SAY_HELLO,
        )
        .map_err(NotStarted::TheProgram)?;
        drop(listener);
        let mut connection = Connection {
            stream,
            process: Some(process),
        };
        let header = serde_json::to_vec(make).expect("a request can be written");
        let _ = connection.stream.set_read_timeout(Some(self.limits.to_make));
        let answer = write_frame(
            &mut connection.stream,
            FrameKind::Make,
            &header,
            state.unwrap_or_default(),
        )
        .map_err(FrameError::Connection)
        .and_then(|()| read_frame(&mut connection.stream));
        let frame = match answer {
            Ok(frame) => frame,
            Err(error) => {
                return Err(NotStarted::TheProgram(why_the_program_is_gone(
                    &mut connection,
                    &error,
                    self.limits.to_make,
                )));
            }
        };
        match frame.kind {
            FrameKind::Made => {
                let made: Made = serde_json::from_slice(&frame.header).map_err(|error| {
                    NotStarted::TheProgram(format!(
                        "the program that runs the plugin said what is not a message ({error})"
                    ))
                })?;
                let _ = connection.stream.set_read_timeout(Some(self.limits.to_run));
                Ok((connection, made))
            }
            FrameKind::Refused => match serde_json::from_slice::<Refused>(&frame.header) {
                Ok(refused) => Err(NotStarted::Refused(refused)),
                Err(error) => Err(NotStarted::TheProgram(format!(
                    "the program that runs the plugin said what is not a message ({error})"
                ))),
            },
            other => Err(NotStarted::TheProgram(format!(
                "the program that runs the plugin said {other:?} when it was asked to make the plugin"
            ))),
        }
    }
}

/// Waits for the program to connect to `listener` and say `token`; a connection that says another word, or none within `how_long_to_say_hello`, is not the program, and is let go. The
/// wait ends early if `process` (the program, if the editor started it) has ended.
fn accept_the_program(
    listener: &TcpListener,
    mut process: Option<&mut Process>,
    token: &str,
    how_long: Duration,
    how_long_to_say_hello: Duration,
) -> Result<TcpStream, String> {
    let deadline = Instant::now() + how_long;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                if stream.set_nonblocking(false).is_ok()
                    && stream.set_nodelay(true).is_ok()
                    && stream.set_read_timeout(Some(how_long_to_say_hello)).is_ok()
                    && says_the_token(&stream, token)
                {
                    return Ok(stream);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => {
                return Err(format!(
                    "the editor's socket failed while it waited for the program: {error}"
                ));
            }
        }
        if let Some(Ok(Some(status))) = process.as_mut().map(|process| process.child.try_wait()) {
            return Err(format!(
                "the program that runs the plugin ended before it connected ({})",
                how_it_ended_badly(status)
            ));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "the program that runs the plugin did not connect within {} seconds",
                seconds_in_words(how_long)
            ));
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn says_the_token(mut stream: &TcpStream, token: &str) -> bool {
    matches!(
        read_frame(&mut stream),
        Ok(Frame { kind: FrameKind::Hello, header, .. })
            if serde_json::from_slice::<Hello>(&header).is_ok_and(|hello| hello.token == token)
    )
}

/// What a connection that gave an error, or a read that ran out of time, comes to: the program is stopped, or its end is waited for, and the words say which.
fn why_the_program_is_gone(connection: &mut Connection, error: &FrameError, limit: Duration) -> String {
    let words = if error.is_a_timeout() {
        if let Some(process) = &mut connection.process {
            process.stop();
        }
        format!(
            "the program that runs the plugin did not answer within {} seconds, and was stopped",
            seconds_in_words(limit)
        )
    } else if let FrameError::Broken(why) = error {
        if let Some(process) = &mut connection.process {
            process.stop();
        }
        format!("the program that runs the plugin said what is not a frame ({why}), and was stopped")
    } else {
        match &mut connection.process {
            Some(process) => process.how_it_ended(),
            None => format!("the connection to the program that runs the plugin ended ({error})"),
        }
    };
    let _ = connection.stream.shutdown(Shutdown::Both);
    words
}

/// A copy of a plugin that runs in a process of its own.
pub struct RemoteCopy {
    /// `None` when the program is gone.
    connection: Option<Connection>,
    /// Why the program is gone, once it is.
    gone_because: Option<String>,
    limits: Limits,
    /// What is to be done to the copy before the next run, in the order it was asked for.
    commands: Vec<Command>,
    latency: usize,
    takes_values_at_samples: bool,
    most_samples_at_once: Option<usize>,
    /// What the plugin's control outputs held after the last run.
    meters: Vec<(u32, f32)>,
    /// What the plugin asked for since it was last asked.
    done_between_blocks: WhatWasDoneBetweenBlocks,
    /// What starts the program again when it ends, if the copy is to go on (see [`Launcher::start_and_start_again_when_it_ends`]).
    restart: Option<Restart>,
    /// How many times the program was started again.
    restarts: usize,
    /// What the program has been told that it keeps: what a program that is started again is told.
    told: WhatTheProgramWasTold,
}

/// What starts the program again, and has it make the plugin.
type Restart = Box<dyn FnMut() -> Result<(Connection, Made), NotStarted> + Send>;

/// Why a run did not go well.
enum Failure {
    /// The plugin said that it could not run: the program and the copy go on.
    ThePlugin(String),
    /// The program ended, or is not to be trusted any more: it is gone, and why.
    TheProgram(String),
}

impl RemoteCopy {
    /// The copy that a connection to a program that has made it is. (The program is a part of the connection when the editor started it; a test that plays the program's part
    /// from a thread gives none.)
    fn over(connection: Connection, made: &Made, limits: Limits) -> Self {
        Self {
            connection: Some(connection),
            gone_because: None,
            limits,
            commands: Vec::new(),
            latency: made.latency as usize,
            takes_values_at_samples: made.takes_values_at_samples,
            most_samples_at_once: made.most_samples_at_once,
            meters: Vec::new(),
            done_between_blocks: made.done_between_blocks.clone(),
            restart: None,
            restarts: 0,
            told: WhatTheProgramWasTold::default(),
        }
    }

    /// A copy over a stream whose other end is somebody that plays the program's part and has made a plugin like `made` (**for the tests**, which play the program from a thread to try
    /// what the copy does about the things a program can do).
    pub fn over_a_stream_that_has_made_a_plugin_like(stream: TcpStream, made: &Made, limits: Limits) -> Self {
        let _ = stream.set_nodelay(true);
        let _ = stream.set_read_timeout(Some(limits.to_run));
        Self::over(Connection { stream, process: None }, made, limits)
    }

    /// The same, with a way to start the program again when it ends: `start` gives the stream to a program that is played from a thread, and what it says of the plugin it made (**for the
    /// tests**).
    #[must_use]
    pub fn with_a_way_to_start_again(
        mut self,
        mut start: impl FnMut() -> Result<(TcpStream, Made), NotStarted> + Send + 'static,
    ) -> Self {
        let limits = self.limits;
        self.restart = Some(Box::new(move || {
            start().map(|(stream, made)| {
                let _ = stream.set_nodelay(true);
                let _ = stream.set_read_timeout(Some(limits.to_run));
                (Connection { stream, process: None }, made)
            })
        }));
        self
    }

    /// How many times the program was started again after it ended.
    pub fn times_started_again(&self) -> usize {
        self.restarts
    }

    /// The number the system gave the program, while it is there and the editor started it.
    pub fn process_id(&self) -> Option<u32> {
        self.connection
            .as_ref()?
            .process
            .as_ref()
            .map(|process| process.child.id())
    }

    /// Why the program is gone, if it is.
    pub fn gone_because(&self) -> Option<&str> {
        self.gone_because.as_deref()
    }

    /// The program is gone, or has to be: the copy answers every call with `why` from now on.
    fn go(&mut self, why: String) -> String {
        self.connection = None;
        self.gone_because = Some(why.clone());
        why
    }

    /// Sends a frame and reads the answer.
    fn ask(&mut self, kind: FrameKind, header: &[u8], raw: &[u8]) -> Result<Frame, String> {
        let limit = self.limits.to_run;
        let Some(connection) = &mut self.connection else {
            return Err(self.gone_because.clone().unwrap_or_default());
        };
        let answer = write_frame(&mut connection.stream, kind, header, raw)
            .map_err(FrameError::Connection)
            .and_then(|()| read_frame(&mut connection.stream));
        match answer {
            Ok(frame) => Ok(frame),
            Err(error) => {
                let why = why_the_program_is_gone(connection, &error, limit);
                Err(self.go(why))
            }
        }
    }

    fn run_and_say_how_it_went(
        &mut self,
        inputs: &mut [&mut [f32]],
        outputs: &mut [&mut [f32]],
        frames: usize,
        notes: Option<&[NoteEvent]>,
    ) -> Result<(), String> {
        if let Some(why) = &self.gone_because {
            return Err(why.clone());
        }
        if inputs
            .iter()
            .chain(outputs.iter())
            .any(|channel| channel.len() < frames)
            || inputs.len() > usize::from(u8::MAX)
            || outputs.len() > usize::from(u8::MAX)
        {
            return Err("the sound is not as long as the run, or has too many channels".to_owned());
        }
        let commands = std::mem::take(&mut self.commands);
        let mut to_give = (commands.clone(), notes.map(<[NoteEvent]>::to_vec));
        loop {
            match self.run_once(&to_give.0, to_give.1.as_deref(), inputs, outputs, frames) {
                Ok(()) => {
                    self.told.after_commands(&commands);
                    if let Some(notes) = notes {
                        self.told.after_notes(notes);
                    }
                    return Ok(());
                }
                Err(Failure::ThePlugin(why)) => return Err(why),
                Err(Failure::TheProgram(why)) => {
                    self.start_again_after(&why)?;
                    to_give = self.what_to_give_a_program_that_is_started_again(&commands, notes);
                }
            }
        }
    }

    /// Gives the program one run, and puts what it made in `outputs`.
    fn run_once(
        &mut self,
        commands: &[Command],
        notes: Option<&[NoteEvent]>,
        inputs: &[&mut [f32]],
        outputs: &mut [&mut [f32]],
        frames: usize,
    ) -> Result<(), Failure> {
        let request = RunRequest {
            commands: commands.to_vec(),
            frames,
            inputs: inputs.len(),
            outputs: outputs.len(),
            notes: notes.map(<[NoteEvent]>::to_vec),
        };
        let mut raw = Vec::with_capacity(inputs.len() * frames * 4);
        put_samples(&mut raw, inputs.iter().map(|channel| &**channel), frames);
        let frame = self
            .ask(FrameKind::Run, &request.header(), &raw)
            .map_err(Failure::TheProgram)?;
        if frame.kind != FrameKind::Ran {
            return Err(Failure::TheProgram(self.go(format!(
                "the program that runs the plugin said {:?} when it was given sound to run",
                frame.kind
            ))));
        }
        let ran = match Ran::of_the_header(&frame.header) {
            Ok(ran) => ran,
            Err(error) => {
                return Err(Failure::TheProgram(self.go(format!(
                    "the program that runs the plugin said what is not a message ({error})"
                ))));
            }
        };
        self.latency = ran.latency as usize;
        self.meters = ran.meters;
        self.done_between_blocks.was_restarted |= ran.done_between_blocks.was_restarted;
        self.done_between_blocks.notes.extend(ran.done_between_blocks.notes);
        // (A plugin that says that it could not run is not a program that is gone: the copy goes on, and so does the program.)
        ran.result.map_err(Failure::ThePlugin)?;
        take_samples(&frame.raw, outputs, frames).map_err(|error| {
            Failure::TheProgram(self.go(format!(
                "the program that runs the plugin gave sound that is not what was asked for ({error})"
            )))
        })
    }

    /// What a program that is started in the place of the one that ended is given with the run that the other ended in: the values the plugin was last given, then the commands of the run,
    /// and, if the plugin is given notes, the controllers and the notes that are held where the commands of the run leave them, then the notes of the run.
    fn what_to_give_a_program_that_is_started_again(
        &self,
        commands: &[Command],
        notes: Option<&[NoteEvent]>,
    ) -> (Vec<Command>, Option<Vec<NoteEvent>>) {
        let mut after_the_commands = self.told.clone();
        after_the_commands.after_commands(commands);
        let mut to_do = self.told.values_to_set_again();
        to_do.extend_from_slice(commands);
        let notes = notes.map(|notes| {
            let mut all = after_the_commands.events_to_give_again();
            all.extend_from_slice(notes);
            all
        });
        (to_do, notes)
    }

    /// The program ended, for the reason `why`: starts it again if the copy may be and has not been started again too often, and says so between the blocks. If it cannot be, the copy is gone
    /// for good, and the error says why and how many times it was started again.
    fn start_again_after(&mut self, why: &str) -> Result<(), String> {
        let Some(restart) = self.restart.as_mut() else {
            return Err(why.to_owned());
        };
        if self.restarts >= MOST_RESTARTS_OF_A_PROGRAM {
            let times = self.restarts;
            return Err(self.go(format!(
                "{why} (it was started again {times} times, and is not any more)"
            )));
        }
        self.restarts += 1;
        match restart() {
            Ok((connection, made)) => {
                let latency_changed = made.latency as usize != self.latency;
                self.connection = Some(connection);
                self.gone_because = None;
                self.latency = made.latency as usize;
                self.takes_values_at_samples = made.takes_values_at_samples;
                self.most_samples_at_once = made.most_samples_at_once;
                self.meters.clear();
                self.done_between_blocks.was_restarted |= made.done_between_blocks.was_restarted;
                self.done_between_blocks.notes.extend(made.done_between_blocks.notes);
                self.done_between_blocks.notes.push(format!(
                    "{why}, and was started again; the notes that were held are played again from here{}",
                    if latency_changed {
                        " (its latency changed, and the sound is not made up for the new latency until the next play)"
                    } else {
                        ""
                    }
                ));
                Ok(())
            }
            Err(not_started) => Err(self.go(format!("{why}; it could not be started again: {not_started}"))),
        }
    }
}

impl PluginInstance for RemoteCopy {
    fn set_value(&mut self, handle: u32, value: f32, at_sample: usize) {
        self.commands.push(Command::SetValue {
            handle,
            value,
            at_sample: u32::try_from(at_sample).unwrap_or(u32::MAX),
        });
    }

    fn takes_values_at_samples(&self) -> bool {
        self.takes_values_at_samples
    }

    fn most_samples_at_once(&self) -> Option<usize> {
        self.most_samples_at_once
    }

    fn run(&mut self, inputs: &mut [&mut [f32]], outputs: &mut [&mut [f32]], frames: usize) -> Result<(), String> {
        self.run_and_say_how_it_went(inputs, outputs, frames, None)
    }

    fn run_with_notes(
        &mut self,
        inputs: &mut [&mut [f32]],
        outputs: &mut [&mut [f32]],
        frames: usize,
        notes: &[NoteEvent],
    ) -> Result<(), String> {
        self.run_and_say_how_it_went(inputs, outputs, frames, Some(notes))
    }

    fn reset(&mut self) {
        self.commands.push(Command::Reset);
    }

    fn latency(&self) -> usize {
        self.latency
    }

    fn outputs(&self) -> Vec<(u32, f32)> {
        self.meters.clone()
    }

    fn between_blocks(&mut self) -> BetweenBlocks {
        let done = std::mem::take(&mut self.done_between_blocks);
        BetweenBlocks {
            was_restarted: done.was_restarted,
            note: (!done.notes.is_empty()).then(|| done.notes.join("; ")),
        }
    }
}

#[cfg(test)]
mod tests;
