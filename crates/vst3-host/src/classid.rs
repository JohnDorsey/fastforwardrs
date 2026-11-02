// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The id of a class of a VST3 module, in a form that is the same on every system.
//!
//! A class id is 16 bytes (a `TUID`), which Steinberg writes as four 32-bit numbers (`INLINE_UID(a, b, c, d)`). **The bytes that a plugin gives are not the same on Windows as
//! on the other systems** (on Windows they are the layout of a COM `GUID`: the first number and the two halves of the second are little-endian), so the bytes cannot be what a
//! project file names a plugin by: a project made on one system would not find its plugin on another. The four numbers are the same everywhere, and are what this type keeps.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vst3::Steinberg::TUID;

/// The id of a class: four numbers.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClassId([u32; 4]);

impl ClassId {
    /// The id that `INLINE_UID(a, b, c, d)` writes.
    pub const fn from_numbers(a: u32, b: u32, c: u32, d: u32) -> Self {
        Self([a, b, c, d])
    }

    /// The four numbers.
    pub const fn numbers(self) -> [u32; 4] {
        self.0
    }

    /// The id that the 16 bytes of this system mean (what a factory gives).
    pub(crate) fn from_tuid(tuid: &TUID) -> Self {
        let byte = |place: usize| u32::from(tuid[place] as u8);
        let big_endian =
            |start: usize| (byte(start) << 24) | (byte(start + 1) << 16) | (byte(start + 2) << 8) | byte(start + 3);
        if cfg!(target_os = "windows") {
            // (The inverse of `vst3::uid` on Windows: `a` is little-endian, `b` is its two halves each little-endian and the high half first.)
            let a = (byte(3) << 24) | (byte(2) << 16) | (byte(1) << 8) | byte(0);
            let b = (byte(5) << 24) | (byte(4) << 16) | (byte(7) << 8) | byte(6);
            Self([a, b, big_endian(8), big_endian(12)])
        } else {
            Self([big_endian(0), big_endian(4), big_endian(8), big_endian(12)])
        }
    }

    /// The 16 bytes of this system that mean this id (what is given to a factory to make the class).
    pub(crate) fn to_tuid(self) -> TUID {
        vst3::uid(self.0[0], self.0[1], self.0[2], self.0[3])
    }

    /// The id as 32 hexadecimal digits (the four numbers, each as 8 digits, in capitals): how Steinberg's own tools write it, and what a project file keeps.
    pub fn text(self) -> String {
        format!("{:08X}{:08X}{:08X}{:08X}", self.0[0], self.0[1], self.0[2], self.0[3])
    }

    /// The id that [`Self::text`] wrote (`None` if the text is not 32 hexadecimal digits).
    pub fn from_text(text: &str) -> Option<Self> {
        if text.len() != 32 || !text.is_ascii() {
            return None;
        }
        let mut numbers = [0_u32; 4];
        for (place, number) in numbers.iter_mut().enumerate() {
            *number = u32::from_str_radix(&text[place * 8..place * 8 + 8], 16).ok()?;
        }
        Some(Self(numbers))
    }
}

impl fmt::Debug for ClassId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ClassId({})", self.text())
    }
}

impl fmt::Display for ClassId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.text())
    }
}

impl Serialize for ClassId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text())
    }
}

impl<'de> Deserialize<'de> for ClassId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::from_text(&text).ok_or_else(|| serde::de::Error::custom("a class id is 32 hexadecimal digits"))
    }
}
