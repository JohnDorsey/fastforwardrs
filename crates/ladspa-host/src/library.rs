// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A LADSPA library that is loaded into the program, and the reading of what it says about its plugins. **Loading a library runs code that nobody here wrote**: whoever
//! calls [`Library::open`] has to have been told by a person that this library may be run (the editor's `plugin_trust` module is where that is decided), or be a child process
//! that exists to find out whether it crashes (the scan).
//!
//! Every `unsafe` block says why it is sound. Most of them are of two kinds: **calling a function of the library** (sound if the library keeps the contract of `ladspa.h`, which
//! is the one thing that cannot be checked, and why a library is only ever loaded after a person has said yes) and **reading what a descriptor points to** (sound for as long as
//! the library is loaded, which [`Loaded`] makes true by keeping it loaded for as long as anything made from it exists).

#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_ulong};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::abi::{self, DescriptorFunction, RawDescriptor};
use crate::description::{PluginDescription, PluginProperties, PortDescription, PortHint};
use crate::instance::Instance;

/// The most ports that a plugin may say it has. (The ones in the world have a few dozen; a number far above that is a descriptor that is not one.)
const MOST_PORTS: usize = 4096;

/// The most plugins that a library may have. (A library is asked for them one after another until it says there are no more; one that never does is stopped here.)
const MOST_PLUGINS: usize = 1024;

/// Why a library could not be opened.
#[derive(Debug, Error)]
pub enum OpenError {
    /// The system's loader refused: there is no such file, it is not a library of this system, it is damaged, a library that it needs is not there, or its start-up code failed.
    #[error("the library could not be loaded: {0}")]
    CouldNotBeLoaded(String),
    /// It loaded, and it is not a LADSPA library: it does not have `ladspa_descriptor`.
    #[error("the library has no function called ladspa_descriptor, so it is not a LADSPA library ({0})")]
    NotALadspaLibrary(String),
}

/// What a library says of its plugins.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LibraryDescription {
    pub plugins: Vec<PluginDescription>,
    /// One line for each plugin whose descriptor could not be used, and why. (Those plugins are not in `plugins`.)
    pub problems: Vec<String>,
}

/// A library that is loaded. It stays loaded for as long as any instance made from it exists.
pub(crate) struct Loaded {
    descriptor_function: DescriptorFunction,
    path: PathBuf,
    /// (Last: the function above is only good while this is alive, and fields are dropped in order.)
    _library: libloading::Library,
}

impl Loaded {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The descriptor of the plugin number `index`, or null if there is none.
    pub(crate) fn descriptor(&self, index: usize) -> *const RawDescriptor {
        // SAFETY: the function is the library's `ladspa_descriptor`, and the library is loaded (`_library` is a field of this). It may be called with any number: for one
        // that is past its last plugin it gives null.
        unsafe { (self.descriptor_function)(index as c_ulong) }
    }
}

/// A library that is loaded.
#[derive(Clone)]
pub struct Library {
    loaded: Arc<Loaded>,
}

impl Library {
    /// Loads the library at `path`. **This runs the code of the library.**
    pub fn open(path: &Path) -> Result<Self, OpenError> {
        // SAFETY: loading a library runs the initialisation code that it has, which can do anything (this is the reason for everything in `plugin_trust` and for the scan being
        // done by a program of its own). The caller has been allowed to do it, which is the only condition this function has.
        let library = unsafe { libloading::Library::new(path) }
            .map_err(|error| OpenError::CouldNotBeLoaded(whole_message(&error)))?;
        // SAFETY: the type is the one `ladspa.h` gives the function (`abi::DescriptorFunction`). The pointer that is copied out of the symbol is good for as long as the library
        // is loaded, and `Loaded` keeps it loaded for as long as the pointer is kept.
        let descriptor_function = unsafe { library.get::<DescriptorFunction>(abi::DESCRIPTOR_FUNCTION_NAME) }
            .map(|symbol| *symbol)
            .map_err(|error| OpenError::NotALadspaLibrary(whole_message(&error)))?;
        Ok(Self {
            loaded: Arc::new(Loaded {
                descriptor_function,
                path: path.to_owned(),
                _library: library,
            }),
        })
    }

    /// Where the library was loaded from.
    pub fn path(&self) -> &Path {
        self.loaded.path()
    }

    /// What the library says of each of its plugins. A plugin whose descriptor cannot be used is left out and said in `problems`.
    pub fn describe(&self) -> LibraryDescription {
        let mut description = LibraryDescription::default();
        for index in 0..MOST_PLUGINS {
            let raw = self.loaded.descriptor(index);
            if raw.is_null() {
                return description;
            }
            // SAFETY: `raw` is not null and is the descriptor that the library gave for `index`, which stays good while the library is loaded (it is, as `self` has it).
            match unsafe { read_descriptor(raw, index) } {
                Ok(plugin) => description.plugins.push(plugin),
                Err(problem) => description.problems.push(format!("plugin number {index}: {problem}")),
            }
        }
        description.problems.push(format!(
            "the library has more than {MOST_PLUGINS} plugins, and the rest are not looked at"
        ));
        description
    }

    /// Makes an instance of the plugin number `plugin_index` that works at `sample_rate`. **This runs the code of the plugin.**
    pub fn instantiate(
        &self,
        plugin_index: usize,
        sample_rate: u32,
    ) -> Result<Instance, crate::instance::InstantiateError> {
        Instance::new(&self.loaded, plugin_index, sample_rate)
    }
}

/// What an error says, and what it says was the cause, and so on: the loader's own words are in the cause (`LoadLibraryExW failed` is all that the error says by itself, and the
/// system's "The specified module could not be found" is what it was caused by).
fn whole_message(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut cause = error.source();
    while let Some(next) = cause {
        message.push_str(": ");
        message.push_str(&next.to_string());
        cause = next.source();
    }
    message
}

/// Reads a descriptor.
///
/// # Safety
///
/// `raw` must point to a descriptor that stays valid for the call: not null, and (as `ladspa.h` has it) with `port_count` entries in each of the three arrays that it points
/// to, and C strings that end, behind the pointers to text.
pub(crate) unsafe fn read_descriptor(raw: *const RawDescriptor, index: usize) -> Result<PluginDescription, String> {
    // SAFETY: the caller promises that `raw` is a valid descriptor.
    let descriptor = unsafe { &*raw };
    let port_count =
        usize::try_from(descriptor.port_count).map_err(|_| "it has more ports than a program can count".to_owned())?;
    if port_count > MOST_PORTS {
        return Err(format!(
            "it says it has {port_count} ports, which is more than the {MOST_PORTS} that a plugin is allowed"
        ));
    }
    let mut ports = Vec::with_capacity(port_count);
    if port_count > 0 {
        if descriptor.port_descriptors.is_null()
            || descriptor.port_names.is_null()
            || descriptor.port_range_hints.is_null()
        {
            return Err(format!(
                "it says it has {port_count} ports and does not say what they are (a list of them is missing)"
            ));
        }
        // SAFETY: the three arrays are not null, and have `port_count` entries each, which is the contract of the descriptor (the caller promises it).
        let (bits, names, hints) = unsafe {
            (
                std::slice::from_raw_parts(descriptor.port_descriptors, port_count),
                std::slice::from_raw_parts(descriptor.port_names, port_count),
                std::slice::from_raw_parts(descriptor.port_range_hints, port_count),
            )
        };
        for number in 0..port_count {
            // SAFETY: the name is a C string that ends, or null, which `read_text` allows for.
            let name = unsafe { read_text(names[number]) };
            let hint = PortHint::from_bits(
                hints[number].hint_descriptor,
                hints[number].lower_bound,
                hints[number].upper_bound,
            );
            ports.push(
                PortDescription::from_bits(name, bits[number], hint)
                    .map_err(|problem| format!("port number {number}: {problem}"))?,
            );
        }
    }
    // SAFETY: these are C strings that end, or null.
    let (label, name, maker, copyright) = unsafe {
        (
            read_text(descriptor.label),
            read_text(descriptor.name),
            read_text(descriptor.maker),
            read_text(descriptor.copyright),
        )
    };
    // (`unsigned long` is 32 bits on Windows and 64 on the others: the conversion is a real one on the first, and is to the same type on the second.)
    #[allow(clippy::useless_conversion)]
    let unique_id = u64::from(descriptor.unique_id);
    Ok(PluginDescription {
        index,
        unique_id,
        label,
        name,
        maker,
        copyright,
        properties: PluginProperties::from_bits(descriptor.properties),
        ports,
    })
}

/// The text a plugin gives. Null is no text, and what is not valid UTF-8 is read as well as it can be (a plugin is free to use another encoding).
///
/// # Safety
///
/// `pointer` must be null or point to a C string that ends.
unsafe fn read_text(pointer: *const c_char) -> String {
    if pointer.is_null() {
        return String::new();
    }
    // SAFETY: not null, and a C string that ends: the caller promises it.
    unsafe { CStr::from_ptr(pointer) }.to_string_lossy().into_owned()
}
