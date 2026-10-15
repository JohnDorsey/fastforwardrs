// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the tests that use a graphics device have in common.
//!
//! **They use a software adapter and nothing else** (WARP on Windows, lavapipe on Linux), so that nothing a test does is done on a real graphics
//! card (SF-01), and **one at a time**: two tests drawing at once on one machine have crashed the program before (see the notes on `egui_kittest`).
//! On a computer that has no software adapter (a Mac) they say so and do nothing, unless `FASTFORWARDRS_REQUIRE_SOFTWARE_ADAPTER=1` is set, as it is
//! in the CI of Windows and Ubuntu, where that is a failure: there the adapter is not allowed to vanish unnoticed.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};

use super::device::{AdapterChoice, DeviceError, EngineDevice};

static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// The right to use the graphics device, which one test has at a time, and a device to use.
pub struct SoftwareGraphics {
    _one_at_a_time: MutexGuard<'static, ()>,
    pub device: EngineDevice,
}

fn taking_the_lock() -> MutexGuard<'static, ()> {
    // (A test that failed while it had the lock has not made the device unusable for the next one.)
    ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// What to do when there is no software adapter: say so (and skip the test), or, where there must be one, fail.
fn there_is_no_software_adapter() {
    if std::env::var_os("FASTFORWARDRS_REQUIRE_SOFTWARE_ADAPTER").is_some_and(|value| value == "1") {
        panic!("there is no software adapter, and FASTFORWARDRS_REQUIRE_SOFTWARE_ADAPTER=1 says there must be one");
    }
    eprintln!("SKIPPED: there is no software graphics adapter on this computer");
}

/// A device (the same one for every test that asks, made when the first one does) with the right to use it, or `None` if this computer has no
/// software adapter: the test should then return at once.
pub fn software_graphics() -> Option<SoftwareGraphics> {
    static SHARED_DEVICE: OnceLock<Option<EngineDevice>> = OnceLock::new();
    let guard = taking_the_lock();
    let device = SHARED_DEVICE.get_or_init(|| match EngineDevice::create(&AdapterChoice::SoftwareOnly) {
        Ok(device) => Some(device),
        Err(DeviceError::NoSoftwareAdapter) => None,
        Err(other) => panic!("the software adapter would not give a device: {other}"),
    });
    match device {
        Some(device) => Some(SoftwareGraphics {
            _one_at_a_time: guard,
            device: device.clone(),
        }),
        None => {
            there_is_no_software_adapter();
            None
        }
    }
}

/// Like [`software_graphics`], but with a device of its own that the test may destroy.
pub fn software_graphics_with_a_device_of_its_own() -> Option<SoftwareGraphics> {
    let guard = taking_the_lock();
    match EngineDevice::create(&AdapterChoice::SoftwareOnly) {
        Ok(device) => Some(SoftwareGraphics {
            _one_at_a_time: guard,
            device,
        }),
        Err(DeviceError::NoSoftwareAdapter) => {
            there_is_no_software_adapter();
            None
        }
        Err(other) => panic!("the software adapter would not give a device: {other}"),
    }
}

/// A gate that is closed until a test opens it, which a thread can wait at.
#[derive(Debug, Default)]
pub struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}

impl Gate {
    pub fn closed() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn open(&self) {
        *self.open.lock().unwrap() = true;
        self.changed.notify_all();
    }

    pub fn wait_until_open(&self) {
        let mut open = self.open.lock().unwrap();
        while !*open {
            open = self.changed.wait(open).unwrap();
        }
    }
}

/// Starts the test body only if there is a software adapter. `$graphics` is the [`SoftwareGraphics`].
macro_rules! with_software_graphics {
    ($graphics:ident, $body:block) => {
        let Some($graphics) = $crate::engine::testing::software_graphics() else {
            return;
        };
        $body
    };
}
pub(crate) use with_software_graphics;
