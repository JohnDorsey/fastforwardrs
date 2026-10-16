// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The graphics device that the engine draws with: **its own**, not the window's.
//!
//! The window's renderer (egui-wgpu, in eframe) does nothing about a lost device, so the work of the engine, which may one day run shaders that a
//! stranger wrote, must not be on the window's device (the findings of phase 0.5). A second device from the same adapter works on every
//! adapter that was tried, and takes from 50 to 335 ms to make, which is one more reason for the engine to make it on its own thread.
//!
//! What goes wrong with the device is not reported by a result of the call that went wrong but by two callbacks, which are on other threads:
//! the device was lost, or an error that no error scope took. Both only write down what happened; the engine's thread reads what was written
//! down ([`EngineDevice::take_trouble`]) between frames.

use std::sync::{Arc, Mutex};

use thiserror::Error;

/// Which adapter the engine's device is made from.
#[derive(Clone)]
pub enum AdapterChoice {
    /// The adapter that the window's own device came from (`eframe`'s `wgpu_render_state.adapter`): what the program uses.
    Window(Arc<wgpu::Adapter>),
    /// A software adapter and nothing else (the one of device type `Cpu`: WARP on Windows, lavapipe on Linux), which the tests use so that nothing they
    /// do is done on a real graphics card.
    SoftwareOnly,
    /// The best graphics adapter the computer has (a card of its own before the graphics of the processor, and so on: `how_far_down_the_list_an_adapter_is`): for a
    /// program that has no window to take one from, such as the command-line export. **Not for the tests**, which use a software adapter and nothing else.
    BestAvailable,
    /// For the tests of what happens when the engine cannot start: there is no adapter.
    #[cfg(test)]
    NoAdapterAtAll,
}

impl std::fmt::Debug for AdapterChoice {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Window(adapter) => write!(formatter, "the window's adapter ({})", adapter.get_info().name),
            Self::SoftwareOnly => write!(formatter, "a software adapter"),
            Self::BestAvailable => write!(formatter, "the best graphics adapter the computer has"),
            #[cfg(test)]
            Self::NoAdapterAtAll => write!(formatter, "no adapter at all"),
        }
    }
}

/// What went wrong with a device while it was being used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceTrouble {
    /// The device is gone (the card was reset, the driver was updated, or it was destroyed on purpose): nothing can be done with it.
    Lost { reason: String, message: String },
    /// A mistake that was not caught by an error scope of the engine, in words.
    Error(String),
}

#[derive(Debug, Error)]
pub enum DeviceError {
    #[error("there is no software graphics adapter on this computer")]
    NoSoftwareAdapter,
    #[error("there is no graphics adapter on this computer")]
    NoGraphicsAdapter,
    #[error("the graphics adapter would not give a device: {0}")]
    CouldNotMakeADevice(#[from] wgpu::RequestDeviceError),
}

/// A device, its queue, and what went wrong with it. (Cloning gives another handle to the same device.)
#[derive(Clone)]
pub struct EngineDevice {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter_information: wgpu::AdapterInfo,
    /// Where a slot of uniforms may start in a buffer, in bytes.
    pub uniform_alignment: usize,
    /// The most pixels a texture of the device can be across or down.
    pub largest_texture_side: u32,
    trouble: Arc<Mutex<Vec<DeviceTrouble>>>,
}

impl EngineDevice {
    /// Makes a device of its own from the adapter `choice` says. Takes a good fraction of a second: not for the thread of the window.
    pub fn create(choice: &AdapterChoice) -> Result<Self, DeviceError> {
        let adapter = match choice {
            AdapterChoice::Window(adapter) => adapter.as_ref().clone(),
            AdapterChoice::SoftwareOnly => software_adapter().ok_or(DeviceError::NoSoftwareAdapter)?,
            AdapterChoice::BestAvailable => best_adapter().ok_or(DeviceError::NoGraphicsAdapter)?,
            #[cfg(test)]
            AdapterChoice::NoAdapterAtAll => return Err(DeviceError::NoSoftwareAdapter),
        };
        let adapter_information = adapter.get_info();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("fastforwardrs picture engine"),
            ..Default::default()
        }))?;
        let trouble = Arc::new(Mutex::new(Vec::new()));
        {
            let trouble = Arc::clone(&trouble);
            device.set_device_lost_callback(move |reason, message| {
                if let Ok(mut written_down) = trouble.lock() {
                    written_down.push(DeviceTrouble::Lost {
                        reason: format!("{reason:?}"),
                        message,
                    });
                }
            });
        }
        {
            let trouble = Arc::clone(&trouble);
            device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
                if let Ok(mut written_down) = trouble.lock() {
                    written_down.push(DeviceTrouble::Error(error.to_string()));
                }
            }));
        }
        let limits = device.limits();
        Ok(Self {
            uniform_alignment: limits.min_uniform_buffer_offset_alignment.max(16) as usize,
            largest_texture_side: limits.max_texture_dimension_2d,
            device,
            queue,
            adapter_information,
            trouble,
        })
    }

    /// What went wrong since the last call, oldest first.
    pub fn take_trouble(&self) -> Vec<DeviceTrouble> {
        self.trouble
            .lock()
            .map(|mut written_down| std::mem::take(&mut *written_down))
            .unwrap_or_default()
    }

    /// Ends the device: what is being done with it is let go of (what is already running on a software adapter goes on running: see the findings of
    /// phase 0.5), and every use of it from now on is an error. The device-lost callback is told it was destroyed.
    pub fn destroy(&self) {
        self.device.destroy();
    }

    /// Runs `work`, and says what went wrong in it, if anything: any mistake the card or the library found in what `work` asked of it
    /// (a validation error) or the card running out of memory. The first is kept if there is more than one.
    pub fn catching_errors<T>(&self, work: impl FnOnce() -> T) -> (T, Option<String>) {
        let out_of_memory = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let result = work();
        // (The scopes are taken off in the opposite order to the one they were put on in.)
        let validation_error = pollster::block_on(validation.pop());
        let out_of_memory_error = pollster::block_on(out_of_memory.pop());
        let problem = validation_error.or(out_of_memory_error).map(|error| error.to_string());
        (result, problem)
    }
}

/// Whether this computer has a software adapter, for the tests that must not run on a real graphics card (see [`AdapterChoice::SoftwareOnly`]) to say that they are
/// skipped where there is none.
pub fn there_is_a_software_adapter() -> bool {
    software_adapter().is_some()
}

/// The software adapter, if there is one: of device type `Cpu`, and of the backend that is liked best if several are (Vulkan before Direct3D
/// before Metal). OpenGL is not looked at, so that no OpenGL context is made on a computer that has no screen (a machine that runs tests).
fn software_adapter() -> Option<wgpu::Adapter> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut software: Vec<wgpu::Adapter> = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::PRIMARY))
        .into_iter()
        .filter(|adapter| adapter.get_info().device_type == wgpu::DeviceType::Cpu)
        .collect();
    software.sort_by_key(|adapter| how_far_down_the_list_a_backend_is(adapter.get_info().backend));
    software.into_iter().next()
}

/// The best graphics adapter there is: of those the primary backends find, the one that is liked best by [`how_far_down_the_list_an_adapter_is`]. (A software
/// adapter is the last choice, and is the one that is used if there is no other.)
fn best_adapter() -> Option<wgpu::Adapter> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut adapters: Vec<wgpu::Adapter> = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::PRIMARY));
    adapters.sort_by_key(|adapter| {
        let information = adapter.get_info();
        how_far_down_the_list_an_adapter_is(information.device_type, information.backend)
    });
    adapters.into_iter().next()
}

/// Where an adapter is in the list of the ones that are liked, the lower the better: a card of its own before the graphics of the processor, before a virtual card,
/// before whatever else there is, before a software adapter; and of those of one kind the backend that is liked best
/// ([`how_far_down_the_list_a_backend_is`]).
pub(super) fn how_far_down_the_list_an_adapter_is(device_type: wgpu::DeviceType, backend: wgpu::Backend) -> (u8, u8) {
    let kind = match device_type {
        wgpu::DeviceType::DiscreteGpu => 0,
        wgpu::DeviceType::IntegratedGpu => 1,
        wgpu::DeviceType::VirtualGpu => 2,
        wgpu::DeviceType::Other => 3,
        wgpu::DeviceType::Cpu => 4,
    };
    (kind, how_far_down_the_list_a_backend_is(backend))
}

/// Where a backend is in the list of the ones that are liked, when there is a software adapter of more than one: the lower the number, the better liked.
pub(super) fn how_far_down_the_list_a_backend_is(backend: wgpu::Backend) -> u8 {
    match backend {
        wgpu::Backend::Vulkan => 0,
        wgpu::Backend::Dx12 => 1,
        _ => 2,
    }
}
