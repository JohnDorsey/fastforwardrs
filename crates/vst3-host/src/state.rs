// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bytes that a project keeps of the state of a plugin: its component's state, and its controller's when it has one. (The two are written by the plugin, as VST3 says; the first
//! bytes and the lengths are ours, so that one piece of bytes holds both.)

use thiserror::Error;

/// The first bytes of a state.
const MAGIC: &[u8; 4] = b"FFV3";

/// The version of the way the two are put together.
const VERSION: u8 = 1;

/// Why a state could not be saved or taken.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum StateError {
    /// The plugin said it has no state, or could not write it.
    #[error("the plugin could not write its state: {0}")]
    CouldNotBeSaved(String),
    /// The bytes are not a state that this host wrote (cut short, or something else).
    #[error("these bytes are not a state of a VST3 plugin that this program wrote")]
    NotOurs,
    /// The plugin did not take the state.
    #[error("the plugin did not take the state it was given: {0}")]
    NotTaken(String),
}

/// The bytes of a state made of the component's and the controller's (which may be empty: a controller that has no state of its own).
pub fn encode_state(component: &[u8], controller: &[u8]) -> Vec<u8> {
    let mut bytes = MAGIC.to_vec();
    bytes.push(VERSION);
    for part in [component, controller] {
        bytes.extend_from_slice(&u32::try_from(part.len()).unwrap_or(u32::MAX).to_le_bytes());
        bytes.extend_from_slice(part);
    }
    bytes
}

/// The component's state and the controller's, out of the bytes that [`encode_state`] made.
pub fn decode_state(bytes: &[u8]) -> Result<(&[u8], &[u8]), StateError> {
    let rest = bytes
        .strip_prefix(MAGIC.as_slice())
        .and_then(|rest| rest.split_first())
        .filter(|(version, _)| **version == VERSION)
        .map(|(_, rest)| rest)
        .ok_or(StateError::NotOurs)?;
    let (component, rest) = take_a_part(rest)?;
    let (controller, rest) = take_a_part(rest)?;
    if !rest.is_empty() {
        return Err(StateError::NotOurs);
    }
    Ok((component, controller))
}

fn take_a_part(bytes: &[u8]) -> Result<(&[u8], &[u8]), StateError> {
    let (length, rest) = bytes.split_at_checked(4).ok_or(StateError::NotOurs)?;
    let length = u32::from_le_bytes(length.try_into().map_err(|_| StateError::NotOurs)?) as usize;
    rest.split_at_checked(length).ok_or(StateError::NotOurs)
}
