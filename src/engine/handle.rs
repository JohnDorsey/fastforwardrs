// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The engine as the window has it: a handle that asks for frames and takes what comes back, and never waits for the graphics card.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::FrameDescription;
use super::messages::{EngineEvent, EngineRequest, EngineSettings, WakeUp};
use super::worker;

/// How long dropping the handle waits for the engine's thread to finish before it leaves it behind. (A thread that is stuck on the graphics card
/// cannot be stopped from outside: the program leaves it, and it ends with the program.)
const HOW_LONG_TO_WAIT_FOR_THE_THREAD: Duration = Duration::from_millis(1000);

/// The picture engine, running on a thread of its own. See [the module](super).
pub struct Engine {
    requests: Sender<EngineRequest>,
    events: Receiver<EngineEvent>,
    thread: Option<JoinHandle<()>>,
    next_serial: u64,
}

impl Engine {
    /// Starts the engine. This does not wait for it: making the device takes a good fraction of a second, and if that fails the engine says so
    /// as an [`EngineEvent::Stopped`]. `wake_up` is called, from the engine's thread, whenever there is an event to take.
    pub fn start(settings: EngineSettings, wake_up: WakeUp) -> Self {
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
        }
    }

    /// Asks for a frame, and gives the serial number that its [`EngineEvent::Frame`] will have. Serial numbers go up by one for each request.
    pub fn request_frame(&mut self, description: FrameDescription) -> u64 {
        let serial = self.next_serial;
        self.next_serial += 1;
        // (If the engine has stopped, nobody is listening, and the window will have been told so by a `Stopped` event.)
        let _ = self.requests.send(EngineRequest::Render { serial, description });
        serial
    }

    /// The next thing the engine has to say, if it has said something. Never waits.
    pub fn try_next_event(&self) -> Option<EngineEvent> {
        self.events.try_recv().ok()
    }

    /// Whether the engine's thread is still going.
    pub fn is_running(&self) -> bool {
        self.thread.as_ref().is_some_and(|thread| !thread.is_finished())
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
