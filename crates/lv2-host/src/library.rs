// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! An LV2 library that is loaded into the program. **Loading a library runs code that nobody here wrote**: whoever calls [`Library::open`] has to have been told by a person that
//! this library may be run (the editor's `plugin_trust` module is where that is decided), or be a child process that exists to find out whether it crashes (the scan).
//!
//! Every `unsafe` block says why it is sound: **calling a function of the library** (sound if the library keeps the contract of `lv2.h`, the one thing that cannot be checked,
//! and why a library is only loaded after a person has said yes) and **reading what a descriptor points to** (sound for as long as the library is loaded, which [`Loaded`] makes
//! true by being kept, by an `Arc`, for as long as anything made from it exists).

#![allow(unsafe_code)]

use std::ffi::CStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use thiserror::Error;

use crate::abi::{DescriptorFunction, RawDescriptor};

/// The most plugins that a library may have: it is asked for them one after another until it says there are no more; one that never does is stopped here.
const MOST_PLUGINS: u32 = 1024;

/// Why a library could not be opened.
#[derive(Debug, Error)]
pub enum OpenError {
    /// The system's loader refused: there is no such file, it is not a library of this system, a library that it needs is not there, or its start-up code failed.
    #[error("the library could not be loaded: {0}")]
    CouldNotBeLoaded(String),
    /// It loaded, and it is not an LV2 library: it does not have `lv2_descriptor`.
    #[error("the library has no function called lv2_descriptor, so it is not an LV2 library ({0})")]
    NotAnLv2Library(String),
}

/// A library that is loaded. It stays loaded for as long as anything made from it exists.
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

    /// The descriptor of the plugin whose URI is `uri`, if the library has one.
    pub(crate) fn descriptor_of(&self, uri: &str) -> Option<*const RawDescriptor> {
        for index in 0..MOST_PLUGINS {
            // SAFETY: calling the library's `lv2_descriptor`, which the library has to answer for any index with a descriptor or null (`lv2.h`); it is called on the thread of
            // whoever asks, which is allowed (it has no state that depends on the thread).
            let descriptor = unsafe { (self.descriptor_function)(index) };
            if descriptor.is_null() {
                return None;
            }
            // SAFETY: a descriptor that the library gave is good for as long as the library is loaded, and `self` keeps it loaded; its `uri` is a C string that the library
            // keeps (or null in a library that is broken, which is looked for).
            let said = unsafe {
                let uri_pointer = (*descriptor).uri;
                (!uri_pointer.is_null()).then(|| CStr::from_ptr(uri_pointer).to_string_lossy().into_owned())
            };
            if said.as_deref() == Some(uri) {
                return Some(descriptor);
            }
        }
        None
    }
}

/// An LV2 library that is loaded.
#[derive(Clone)]
pub struct Library {
    pub(crate) loaded: Arc<Loaded>,
}

impl Library {
    /// Loads the library at `path`. **Runs the library's start-up code**: see the head of this module.
    pub fn open(path: &Path) -> Result<Self, OpenError> {
        // SAFETY: loading a library runs its initialisation, which is code of the plugin; that it may be run is for whoever calls this to have been told (see the head of this
        // module). Nothing else here depends on it.
        let library = unsafe { libloading::Library::new(path) }
            .map_err(|error| OpenError::CouldNotBeLoaded(error.to_string()))?;
        // SAFETY: the symbol is looked up by name and read as the function that `lv2.h` says it is; if the library has a symbol of that name that is something else, calling it
        // is not sound, which is the contract that makes a library that is loaded a library that is trusted.
        let function: DescriptorFunction = unsafe {
            *library
                .get::<DescriptorFunction>(b"lv2_descriptor\0")
                .map_err(|error| OpenError::NotAnLv2Library(error.to_string()))?
        };
        Ok(Self {
            loaded: Arc::new(Loaded {
                descriptor_function: function,
                path: path.to_owned(),
                _library: library,
            }),
        })
    }

    pub fn path(&self) -> &Path {
        self.loaded.path()
    }

    /// Whether the library has a plugin whose URI is `uri`.
    pub fn has_plugin(&self, uri: &str) -> bool {
        self.loaded.descriptor_of(uri).is_some()
    }
}
