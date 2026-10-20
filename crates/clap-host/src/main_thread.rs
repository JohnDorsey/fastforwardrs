// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The thread that does what CLAP asks to be done on the main thread (C8).

use std::collections::HashMap;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use clack_host::prelude::*;

use crate::host::Host;

/// The plugin instances that a main thread has made: they never leave it (`PluginInstance` is not `Send`).
#[derive(Default)]
pub(crate) struct Instances {
    pub(crate) map: HashMap<u64, PluginInstance<Host>>,
    pub(crate) next: u64,
}

type Job = Box<dyn FnOnce(&mut Instances) + Send>;

/// A thread of its own that makes, activates, deactivates and destroys plugin instances, and calls `on_main_thread` for them.
pub struct MainThread {
    jobs: Mutex<Option<Sender<Job>>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl MainThread {
    /// Starts the thread.
    pub fn start() -> Arc<Self> {
        let (sender, receiver) = mpsc::channel::<Job>();
        let thread = std::thread::Builder::new()
            .name("clap main thread".to_owned())
            .spawn(move || {
                let mut instances = Instances::default();
                while let Ok(job) = receiver.recv() {
                    job(&mut instances);
                }
                // (The instances that are left are dropped here, on the thread that made them.)
            })
            .expect("a thread can be started");
        Arc::new(Self {
            jobs: Mutex::new(Some(sender)),
            thread: Mutex::new(Some(thread)),
        })
    }

    /// Does `job` on the main thread and gives what it gives. `None` if the thread has ended (it never does while a `MainThread` is in use).
    pub(crate) fn call<R: Send + 'static>(&self, job: impl FnOnce(&mut Instances) -> R + Send + 'static) -> Option<R> {
        let (answer_sender, answer) = mpsc::channel();
        let sender = self
            .jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()?;
        sender
            .send(Box::new(move |instances: &mut Instances| {
                let _ = answer_sender.send(job(instances));
            }))
            .ok()?;
        answer.recv().ok()
    }
}

impl Drop for MainThread {
    fn drop(&mut self) {
        // (Closing the channel ends the loop of the thread, which drops what is left on it.)
        self.jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(thread) = self
            .thread
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = thread.join();
        }
    }
}
