// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The state of a plugin (`state:interface`): a dictionary that the plugin makes when it is asked to **save** (`save` gives each property to a function of the host's: a key, a
//! value, its type and flags), and reads when it is asked to **restore** (`restore` asks a function of the host's for each property it wants, by key).
//!
//! Keys and types are numbers made by the plugin's `urid:map`, which mean nothing outside the instance, so **what is kept is the URIs**: a list of (key URI, type URI, flags,
//! bytes), in bytes of our own (below), which is what goes into an effect's `state` in the project. Only the program that wrote it reads it, and a project that is opened
//! without the plugin keeps it as it was.
//!
//! What is not done: properties that are **paths** (a plugin that keeps files; it needs `state:mapPath`, which this host does not give, so such a plugin is not made if it requires it).

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::ffi::c_void;

use thiserror::Error;

use crate::abi::RawStateInterface;
use crate::features::UridTable;
use crate::instance::Instance;

/// What begins a state, and the version of the layout.
const MAGIC: &[u8; 4] = b"LV2S";
const VERSION: u32 = 1;

/// `LV2_State_Status`.
const STATE_SUCCESS: i32 = 0;
const STATE_ERR_UNKNOWN: i32 = 1;
const STATE_ERR_BAD_TYPE: i32 = 2;
const STATE_ERR_NO_PROPERTY: i32 = 5;

/// Why a state could not be saved or restored.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum StateError {
    #[error("the plugin has no state")]
    NoState,
    #[error("the plugin could not save its state: it said {0}")]
    SaveFailed(i32),
    #[error("the plugin could not take the state it was given: it said {0}")]
    RestoreFailed(i32),
    #[error("what was given is not a state that this program saved: {0}")]
    NotAState(&'static str),
}

/// One property of a state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Property {
    pub key: String,
    pub type_: String,
    pub flags: u32,
    pub value: Vec<u8>,
}

/// The bytes of a state.
pub(crate) fn encode(properties: &[Property]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(properties.len()).unwrap_or(u32::MAX).to_le_bytes());
    for property in properties {
        for text in [&property.key, &property.type_] {
            bytes.extend_from_slice(&u32::try_from(text.len()).unwrap_or(u32::MAX).to_le_bytes());
            bytes.extend_from_slice(text.as_bytes());
        }
        bytes.extend_from_slice(&property.flags.to_le_bytes());
        bytes.extend_from_slice(&(property.value.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&property.value);
    }
    bytes
}

/// A state from its bytes.
pub(crate) fn decode(bytes: &[u8]) -> Result<Vec<Property>, StateError> {
    struct Reader<'bytes> {
        bytes: &'bytes [u8],
        place: usize,
    }
    impl<'bytes> Reader<'bytes> {
        fn take(&mut self, count: usize) -> Result<&'bytes [u8], StateError> {
            let end = self
                .place
                .checked_add(count)
                .filter(|end| *end <= self.bytes.len())
                .ok_or(StateError::NotAState("it ends too soon"))?;
            let taken = &self.bytes[self.place..end];
            self.place = end;
            Ok(taken)
        }
        fn number(&mut self) -> Result<u32, StateError> {
            Ok(u32::from_le_bytes(
                self.take(4)?
                    .try_into()
                    .map_err(|_| StateError::NotAState("a number is cut short"))?,
            ))
        }
        fn text(&mut self) -> Result<String, StateError> {
            let length = self.number()? as usize;
            String::from_utf8(self.take(length)?.to_vec()).map_err(|_| StateError::NotAState("a name is not text"))
        }
    }
    let mut reader = Reader { bytes, place: 0 };
    if reader.take(4)? != MAGIC {
        return Err(StateError::NotAState("it does not begin as a state does"));
    }
    if reader.number()? != VERSION {
        return Err(StateError::NotAState("it is of another version"));
    }
    let count = reader.number()?;
    let mut properties = Vec::new();
    for _ in 0..count {
        let key = reader.text()?;
        let type_ = reader.text()?;
        let flags = reader.number()?;
        let length = u64::from_le_bytes(
            reader
                .take(8)?
                .try_into()
                .map_err(|_| StateError::NotAState("a length is cut short"))?,
        );
        let length = usize::try_from(length).map_err(|_| StateError::NotAState("a value is too long"))?;
        let value = reader.take(length)?.to_vec();
        properties.push(Property {
            key,
            type_,
            flags,
            value,
        });
    }
    if reader.place != bytes.len() {
        return Err(StateError::NotAState("there is more after the end"));
    }
    Ok(properties)
}

/// What `save` collects: the properties the plugin stores, and the table that says what their numbers are.
struct Collector<'table> {
    table: &'table UridTable,
    properties: Vec<Property>,
}

/// `LV2_State_Store_Function`.
unsafe extern "C" fn store_callback(
    handle: *mut c_void,
    key: u32,
    value: *const c_void,
    size: usize,
    type_: u32,
    flags: u32,
) -> i32 {
    if handle.is_null() || value.is_null() || size == 0 {
        return STATE_ERR_UNKNOWN;
    }
    // SAFETY: `handle` is the pointer to the `Collector` that `save_state` made and keeps alive for the length of the call to `save`, which is the only time the plugin has it.
    let collector = unsafe { &mut *(handle as *mut Collector<'_>) };
    let (Some(key), Some(type_)) = (collector.table.uri_of(key), collector.table.uri_of(type_)) else {
        return STATE_ERR_BAD_TYPE;
    };
    // SAFETY: the plugin gives a pointer to `size` bytes that are good for the length of this call (the interface says so; the host copies them).
    let value = unsafe { std::slice::from_raw_parts(value.cast::<u8>(), size) }.to_vec();
    collector.properties.push(Property {
        key,
        type_,
        flags,
        value,
    });
    STATE_SUCCESS
}

/// The properties that `restore` asks for, by the number of their keys.
struct Restorer {
    by_key: HashMap<u32, (u32, u32, Vec<u8>)>,
}

/// `LV2_State_Retrieve_Function`.
unsafe extern "C" fn retrieve_callback(
    handle: *mut c_void,
    key: u32,
    size: *mut usize,
    type_: *mut u32,
    flags: *mut u32,
) -> *const c_void {
    if handle.is_null() {
        return std::ptr::null();
    }
    // SAFETY: `handle` is the pointer to the `Restorer` that `restore_state` made and keeps alive for the length of the call to `restore`, which is the only time the plugin has it.
    let restorer = unsafe { &*(handle as *const Restorer) };
    let Some((type_of_it, flags_of_it, value)) = restorer.by_key.get(&key) else {
        return std::ptr::null();
    };
    // SAFETY: the plugin gives pointers that it may leave null (the interface says they are optional) or that point to a place of the right size to write to.
    unsafe {
        if !size.is_null() {
            *size = value.len();
        }
        if !type_.is_null() {
            *type_ = *type_of_it;
        }
        if !flags.is_null() {
            *flags = *flags_of_it;
        }
    }
    value.as_ptr().cast()
}

impl Instance {
    /// Whether the plugin has a state to save (it has the `state:interface`).
    pub fn has_state(&self) -> bool {
        !self.state_interface.is_null()
    }

    /// The state of the plugin, as bytes that [`Self::restore_state`] takes (of this program's own making: they say the URIs of the keys and the types, and the plugin's bytes).
    pub fn save_state(&mut self) -> Result<Vec<u8>, StateError> {
        if self.state_interface.is_null() {
            return Err(StateError::NoState);
        }
        // SAFETY: the interface is the plugin's own table, good for as long as the library is loaded.
        let save = unsafe { (*self.state_interface.cast::<RawStateInterface>()).save }.ok_or(StateError::NoState)?;
        let mut collector = Collector {
            table: &self.table,
            properties: Vec::new(),
        };
        // SAFETY: calling the plugin's `save` with its handle, our store function, the pointer to the collector that it hands back to that function (the collector lives for the
        // call, and `self` is borrowed mutably so nothing else is using the plugin), no flags, and the list of features that the instance keeps alive.
        let status = unsafe {
            save(
                self.handle,
                Some(store_callback),
                (&raw mut collector).cast(),
                0,
                self.features.as_pointer(),
            )
        };
        if status != STATE_SUCCESS {
            return Err(StateError::SaveFailed(status));
        }
        Ok(encode(&collector.properties))
    }

    /// Gives the plugin a state that [`Self::save_state`] made (of this instance or of another of the same plugin).
    pub fn restore_state(&mut self, bytes: &[u8]) -> Result<(), StateError> {
        if self.state_interface.is_null() {
            return Err(StateError::NoState);
        }
        let properties = decode(bytes)?;
        // SAFETY: the interface is the plugin's own table, good for as long as the library is loaded.
        let restore =
            unsafe { (*self.state_interface.cast::<RawStateInterface>()).restore }.ok_or(StateError::NoState)?;
        let restorer = Restorer {
            by_key: properties
                .into_iter()
                .map(|property| {
                    (
                        self.table.map(&property.key),
                        (self.table.map(&property.type_), property.flags, property.value),
                    )
                })
                .collect(),
        };
        // SAFETY: calling the plugin's `restore` with its handle, our retrieve function, the pointer to the restorer that it hands back to that function (it lives for the call;
        // the plugin may only read what it gives while the call lasts), no flags, and the list of features that the instance keeps alive.
        let status = unsafe {
            restore(
                self.handle,
                Some(retrieve_callback),
                (&raw const restorer).cast_mut().cast(),
                0,
                self.features.as_pointer(),
            )
        };
        // (A plugin that finds a property missing says so and goes on with a default: that is not a failure of the state.)
        if status != STATE_SUCCESS && status != STATE_ERR_NO_PROPERTY {
            return Err(StateError::RestoreFailed(status));
        }
        Ok(())
    }
}
