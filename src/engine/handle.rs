// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The engine as the window has it: a handle that asks for frames and takes what comes back, and never waits for the graphics card.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use std::sync::Arc;

use super::effect_shaders::check_that_it_is_an_effect;
use super::guard::{ShaderProblem, guard_wgsl};
use super::messages::{EngineEvent, EngineRequest, EngineSettings, WakeUp};
use super::worker;
use super::{COLOUR_MATRIX_KEY, FrameDescription, PictureData, PictureId, YuvColour};

/// How long dropping the handle waits for the engine's thread to finish before it leaves it behind. (A thread that is stuck on the graphics card
/// cannot be stopped from outside: the program leaves it, and it ends with the program.)
const HOW_LONG_TO_WAIT_FOR_THE_THREAD: Duration = Duration::from_millis(1000);

/// What the guard did to the shader of an effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectShaderReport {
    /// How many loops of the shader were given a limit on their passes (see `guard`). A loop that is cut short by it makes a picture that is not what the
    /// shader would have made, which the engine does not say when it happens.
    pub loops_capped: usize,
}

/// The picture engine, running on a thread of its own. See [the module](super).
pub struct Engine {
    requests: Sender<EngineRequest>,
    events: Receiver<EngineEvent>,
    thread: Option<JoinHandle<()>>,
    next_serial: u64,
    most_passes_of_a_loop: u32,
}

impl Engine {
    /// Starts the engine. This does not wait for it: making the device takes a good fraction of a second, and if that fails the engine says so
    /// as an [`EngineEvent::Stopped`]. `wake_up` is called, from the engine's thread, whenever there is an event to take.
    pub fn start(settings: EngineSettings, wake_up: WakeUp) -> Self {
        let most_passes_of_a_loop = settings.most_passes_of_a_loop;
        let (request_sender, request_receiver) = mpsc::channel();
        let (event_sender, event_receiver) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("picture engine".to_owned())
            .spawn(move || worker::run(settings, request_receiver, event_sender, wake_up))
            .expect("the operating system gave a thread");
        Self {
            requests: request_sender,
            events: event_receiver,
            thread: Some(thread),
            next_serial: 1,
            most_passes_of_a_loop,
        }
    }

    /// Gives the engine a shader for the effect under `key`, so that frames can ask for it (an `EffectCall` with that key and the numbers the shader is
    /// given): **this is the one way in for a shader that the engine did not write.** The shader is read and checked here, on the caller's thread, and
    /// refused with the place of the mistake if it has one; every loop in it is given a limit on its passes (`EngineSettings::most_passes_of_a_loop`) and it is
    /// sent to the engine, which makes it an effect on the graphics card. If the card's library will not take it (it wants more than an effect is given, or
    /// something of another kind), the engine says so afterwards with an [`EngineEvent::EffectRejected`], and a frame that asks for the effect is made without
    /// it. A shader that was under the key is replaced. Keys that begin with `engine:` are the engine's own. What a shader of an effect has to be is in
    /// `effect_shaders`.
    pub fn compile_effect_shader(&self, key: &str, source: &str) -> Result<EffectShaderReport, ShaderProblem> {
        let refuse = |message: String| ShaderProblem {
            message,
            line: None,
            column: None,
        };
        if key.is_empty() {
            return Err(refuse("an effect has to have a key".to_owned()));
        }
        if key == COLOUR_MATRIX_KEY || key.starts_with("engine:") {
            return Err(refuse(format!(
                "the key {key} belongs to the engine's own effects, which keys that begin with engine: are"
            )));
        }
        let shader = guard_wgsl(source, self.most_passes_of_a_loop)?;
        check_that_it_is_an_effect(&shader).map_err(refuse)?;
        let report = EffectShaderReport {
            loops_capped: shader.loops_capped,
        };
        let _ = self.requests.send(EngineRequest::InstallEffect {
            key: key.to_owned(),
            shader: Arc::new(shader),
        });
        Ok(report)
    }

    /// Sends a picture, which frames can then refer to by `id`. The bytes are not copied.
    pub fn upload_picture(&self, id: PictureId, data: Arc<PictureData>) {
        let _ = self.requests.send(EngineRequest::Upload { id, data });
    }

    /// Tells the engine to let go of these pictures.
    pub fn forget_pictures(&self, ids: Vec<PictureId>) {
        let _ = self.requests.send(EngineRequest::Forget(ids));
    }

    /// Asks for a frame, and gives the serial number that its [`EngineEvent::Frame`] will have. Serial numbers go up by one for each request.
    pub fn request_frame(&mut self, description: FrameDescription) -> u64 {
        let serial = self.next_serial;
        self.next_serial += 1;
        // (If the engine has stopped, nobody is listening, and the window will have been told so by a `Stopped` event.)
        let _ = self.requests.send(EngineRequest::Render { serial, description });
        serial
    }

    /// Asks for a frame of the export, as planes made the way `colour` says, and gives the serial number that its [`EngineEvent::ExportFrame`] will have. Unlike
    /// [`Self::request_frame`] **every one of these is made, in the order it was asked in**.
    pub fn request_export_frame(&mut self, description: FrameDescription, colour: YuvColour) -> u64 {
        let serial = self.next_serial;
        self.next_serial += 1;
        let _ = self.requests.send(EngineRequest::RenderForExport {
            serial,
            description,
            colour,
        });
        serial
    }

    /// The next thing the engine has to say, if it has said something. Never waits.
    pub fn try_next_event(&self) -> Option<EngineEvent> {
        self.events.try_recv().ok()
    }

    /// Tells the engine to stop: it says so (an [`EngineEvent::Stopped`]) and makes no more frames.
    pub fn stop(&self) {
        let _ = self.requests.send(EngineRequest::Stop);
    }

    /// Whether the engine's thread is still going. (An engine that has no thread of its own, which the tests make up, is always going.)
    pub fn is_running(&self) -> bool {
        self.thread.as_ref().is_none_or(|thread| !thread.is_finished())
    }

    /// For the tests of what the window does with what the engine says: an engine that is not one. It has no thread and makes no frame: the test says what it
    /// says (the events it sends are what [`Self::try_next_event`] gives) and reads what it was asked (the requests it gets).
    #[cfg(test)]
    pub(crate) fn scripted_for_tests() -> (Self, Sender<EngineEvent>, Receiver<EngineRequest>) {
        let (request_sender, request_receiver) = mpsc::channel();
        let (event_sender, event_receiver) = mpsc::channel();
        let engine = Self {
            requests: request_sender,
            events: event_receiver,
            thread: None,
            next_serial: 1,
            most_passes_of_a_loop: super::DEFAULT_MOST_PASSES_OF_A_LOOP,
        };
        (engine, event_sender, request_receiver)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.requests.send(EngineRequest::Stop);
        let Some(thread) = self.thread.take() else { return };
        let gave_up_at = Instant::now() + HOW_LONG_TO_WAIT_FOR_THE_THREAD;
        while !thread.is_finished() && Instant::now() < gave_up_at {
            std::thread::sleep(Duration::from_millis(2));
        }
        if thread.is_finished() {
            let _ = thread.join();
        }
        // (Otherwise the thread is left to end with the program.)
    }
}
