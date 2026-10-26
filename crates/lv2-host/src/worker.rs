// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The worker (`worker:schedule` and `worker:interface`): a plugin that has something to do that takes long (read a file, make a table) does not do it in `run`, which may not
//! wait; it gives the host the work, and the host does it on **a thread of its own** by calling the plugin's `work`, and gives what that answers back to the plugin at the next
//! `run` (as `work_response`, and then `end_run` after `run`). Many plugins require it.
//!
//! One worker for each instance. It is stopped, and its thread has ended, before the plugin is cleaned up.

#![allow(unsafe_code)]

use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

use crate::abi::{Handle, RawWorkerInterface};

/// What the plugin's callbacks and the worker's thread share. (Pointed to by the schedule feature, which is why it is in an `Arc` that the feature set and the thread both hold.)
pub(crate) struct WorkerShared {
    sender: Mutex<Option<Sender<Vec<u8>>>>,
    /// What the work answered, to be given back to the plugin at the next `run`.
    responses: Mutex<VecDeque<Vec<u8>>>,
}

impl WorkerShared {
    pub(crate) fn new() -> (Arc<Self>, Receiver<Vec<u8>>) {
        let (sender, receiver) = mpsc::channel();
        (
            Arc::new(Self {
                sender: Mutex::new(Some(sender)),
                responses: Mutex::new(VecDeque::new()),
            }),
            receiver,
        )
    }

    /// Gives the worker something to do. `false` if there is no worker to do it (it has been stopped).
    pub(crate) fn schedule(&self, work: Vec<u8>) -> bool {
        self.sender
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_some_and(|sender| sender.send(work).is_ok())
    }

    /// What the work has answered since this was last asked, in order.
    pub(crate) fn take_responses(&self) -> Vec<Vec<u8>> {
        self.responses
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain(..)
            .collect()
    }

    fn respond(&self, answer: Vec<u8>) {
        self.responses
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(answer);
    }

    fn stop(&self) {
        // (Dropping the sender ends the thread's wait.)
        self.sender.lock().unwrap_or_else(PoisonError::into_inner).take();
    }
}

/// `LV2_Worker_Respond_Function`: what the plugin's `work` calls to answer.
unsafe extern "C" fn respond_callback(handle: *mut c_void, size: u32, data: *const c_void) -> i32 {
    if handle.is_null() {
        return 1;
    }
    let answer: Vec<u8> = if size == 0 || data.is_null() {
        Vec::new()
    } else {
        // SAFETY: the plugin gives a pointer to `size` bytes that it keeps valid for the length of this call.
        unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size as usize) }.to_vec()
    };
    // SAFETY: `handle` is the pointer to the `WorkerShared` that the thread passed to `work`, which the thread keeps alive (it holds an `Arc`) for as long as it calls it.
    let shared = unsafe { &*(handle as *const WorkerShared) };
    shared.respond(answer);
    0
}

/// What the thread needs to call the plugin: its handle, and its worker interface.
struct Target {
    handle: Handle,
    interface: *const RawWorkerInterface,
}

// SAFETY: the interface says `work` is called from another thread while the plugin runs (it is the point of it), so the plugin's handle and its interface (a table of functions
// that lives as long as the library) may be used from the worker's thread. The worker is stopped and joined before the plugin is cleaned up or the library is let go of.
unsafe impl Send for Target {}

/// The worker of one instance: the thread that does what the plugin schedules.
pub(crate) struct Worker {
    shared: Arc<WorkerShared>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    /// Starts the thread, which calls `interface.work` of the plugin `handle` for each thing that is scheduled.
    ///
    /// # Safety
    /// `handle` has to be a plugin that is alive and `interface` the worker interface that it gave, and both have to stay good until [`Worker::stop`] has returned.
    pub(crate) unsafe fn start(
        shared: Arc<WorkerShared>,
        receiver: Receiver<Vec<u8>>,
        handle: Handle,
        interface: *const RawWorkerInterface,
    ) -> Self {
        let target = Target { handle, interface };
        let for_the_thread = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("lv2-worker".to_owned())
            .spawn(move || {
                let target = target;
                let shared = for_the_thread;
                while let Ok(work) = receiver.recv() {
                    // SAFETY: the caller of `start` promised that the handle and the interface are good until the worker is stopped, and `stop` waits for this thread to end.
                    let interface = unsafe { &*target.interface };
                    if let Some(function) = interface.work {
                        let size = u32::try_from(work.len()).unwrap_or(u32::MAX);
                        // SAFETY: as above for the handle; the data is `size` bytes of `work`, which lives for the length of the call; the respond function and its handle are
                        // ours (`shared` is held by this thread).
                        unsafe {
                            function(
                                target.handle,
                                Some(respond_callback),
                                Arc::as_ptr(&shared) as *mut c_void,
                                size,
                                work.as_ptr().cast(),
                            );
                        }
                    }
                }
            })
            .ok();
        Self { shared, thread }
    }

    pub(crate) fn shared(&self) -> &Arc<WorkerShared> {
        &self.shared
    }

    /// Stops the thread and waits for it to end (what it is doing is finished first). After this the plugin is not called from another thread any more.
    pub(crate) fn stop(&mut self) {
        self.shared.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
    }
}
