// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the plugin is told of the host, and what it can ask of it (C8): the handlers of `clack-host`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::ThreadId;

use clack_extensions::latency::{HostLatency, HostLatencyImpl};
use clack_extensions::params::{
    HostParams, HostParamsImplMainThread, HostParamsImplShared, ParamClearFlags, ParamRescanFlags,
};
use clack_extensions::thread_check::{HostThreadCheck, HostThreadCheckImpl};
use clack_host::prelude::*;

/// What the plugin has asked for, which the user of an [`crate::Instance`] looks at.
#[derive(Default)]
pub(crate) struct Requests {
    /// `request_restart`, or a change of latency while the plugin is active: the plugin is to be deactivated and activated again.
    pub(crate) restart: AtomicBool,
    /// `request_callback`: `on_main_thread` is to be called on the main thread.
    pub(crate) callback: AtomicBool,
    /// `params.rescan` with a flag that says the parameters are not what they were.
    pub(crate) parameters_changed: AtomicBool,
}

/// Which thread is which, for `thread-check`.
#[derive(Default)]
pub(crate) struct Threads {
    pub(crate) main: Mutex<Option<ThreadId>>,
    pub(crate) audio: Mutex<Option<ThreadId>>,
}

/// The thread-safe part: shared by the main thread and the audio thread.
pub(crate) struct Shared {
    pub(crate) requests: Arc<Requests>,
    pub(crate) threads: Arc<Threads>,
}

impl<'a> SharedHandler<'a> for Shared {
    fn request_restart(&self) {
        self.requests.restart.store(true, Ordering::SeqCst);
    }

    fn request_process(&self) {
        // (Every instance is processed all the time that its clip is heard.)
    }

    fn request_callback(&self) {
        self.requests.callback.store(true, Ordering::SeqCst);
    }
}

impl HostThreadCheckImpl for Shared {
    fn is_main_thread(&self) -> bool {
        *self
            .threads
            .main
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            == Some(std::thread::current().id())
    }

    fn is_audio_thread(&self) -> bool {
        *self
            .threads
            .audio
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            == Some(std::thread::current().id())
    }
}

impl HostParamsImplShared for Shared {
    fn request_flush(&self) {
        // (Parameters are given in `process`, never in a flush.)
    }
}

/// The main-thread part.
pub(crate) struct MainThreadPart<'a> {
    pub(crate) requests: Arc<Requests>,
    pub(crate) _shared: std::marker::PhantomData<&'a ()>,
}

impl<'a> MainThreadHandler<'a> for MainThreadPart<'a> {}

impl HostParamsImplMainThread for MainThreadPart<'_> {
    fn rescan(&self, flags: ParamRescanFlags) {
        if flags.intersects(ParamRescanFlags::ALL | ParamRescanFlags::INFO) {
            self.requests.parameters_changed.store(true, Ordering::SeqCst);
        }
    }

    fn clear(&self, _param_id: ClapId, _flags: ParamClearFlags) {}
}

impl HostLatencyImpl for MainThreadPart<'_> {
    fn changed(&self) {
        // The latency can change only in `activate`; if the plugin is active it has to be restarted for the new one.
        self.requests.restart.store(true, Ordering::SeqCst);
    }
}

/// The handlers of this host.
pub(crate) struct Host;

impl HostHandlers for Host {
    type Shared<'a> = Shared;
    type MainThread<'a> = MainThreadPart<'a>;
    type AudioProcessor<'a> = ();

    fn declare_extensions(builder: &mut HostExtensions<Self>, _shared: &Self::Shared<'_>) {
        builder
            .register::<HostThreadCheck>()
            .register::<HostParams>()
            .register::<HostLatency>();
    }
}
