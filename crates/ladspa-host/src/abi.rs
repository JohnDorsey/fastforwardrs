// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The binary interface of LADSPA (`ladspa.h`, version 1.1), written out as Rust types. **This is the part that is easy to get wrong**, so each fact that could be is stated
//! here and tested (`tests.rs`):
//!
//! * `unsigned long` is **32 bits on Windows (LLP64) and 64 bits on Linux and macOS (LP64)**: the unique id, the number of ports, the number of the port, the sample rate and the
//!   sample count are all `unsigned long`, so they are [`c_ulong`] and never `u64`. (On 64-bit systems the padding that follows the 32-bit field of Windows makes the fields
//!   *start* in the same places, which is why reading a `u64` there would only give the wrong number and not crash, and why it must be tested and not noticed.)
//! * `LADSPA_Data` is `float`, and the port descriptor, the hint descriptor and the properties are `int`.
//! * Every function of a descriptor can be missing in a library that is broken, so each is an `Option` of a function pointer (null is `None`), and the host looks before it
//!   calls.

use std::ffi::{c_char, c_int, c_ulong, c_void};

/// `LADSPA_Handle`: what `instantiate` gives and every other function takes.
pub(crate) type Handle = *mut c_void;

/// `LADSPA_PortRangeHint`.
#[repr(C)]
pub(crate) struct RawPortRangeHint {
    pub hint_descriptor: c_int,
    pub lower_bound: f32,
    pub upper_bound: f32,
}

/// `LADSPA_Descriptor`.
#[repr(C)]
pub(crate) struct RawDescriptor {
    pub unique_id: c_ulong,
    pub label: *const c_char,
    pub properties: c_int,
    pub name: *const c_char,
    pub maker: *const c_char,
    pub copyright: *const c_char,
    pub port_count: c_ulong,
    pub port_descriptors: *const c_int,
    pub port_names: *const *const c_char,
    pub port_range_hints: *const RawPortRangeHint,
    pub implementation_data: *mut c_void,
    pub instantiate: Option<unsafe extern "C" fn(*const RawDescriptor, c_ulong) -> Handle>,
    pub connect_port: Option<unsafe extern "C" fn(Handle, c_ulong, *mut f32)>,
    pub activate: Option<unsafe extern "C" fn(Handle)>,
    pub run: Option<unsafe extern "C" fn(Handle, c_ulong)>,
    pub run_adding: Option<unsafe extern "C" fn(Handle, c_ulong)>,
    pub set_run_adding_gain: Option<unsafe extern "C" fn(Handle, f32)>,
    pub deactivate: Option<unsafe extern "C" fn(Handle)>,
    pub cleanup: Option<unsafe extern "C" fn(Handle)>,
}

/// `const LADSPA_Descriptor * ladspa_descriptor(unsigned long Index)`: the one function a library exports. It gives null for an index that is past the last plugin.
pub(crate) type DescriptorFunction = unsafe extern "C" fn(c_ulong) -> *const RawDescriptor;

/// The name that the function is exported as, with the end that a C string has.
pub(crate) const DESCRIPTOR_FUNCTION_NAME: &[u8] = b"ladspa_descriptor\0";

// The bits of `LADSPA_Properties`.
pub(crate) const PROPERTY_REALTIME: c_int = 0x1;
pub(crate) const PROPERTY_INPLACE_BROKEN: c_int = 0x2;
pub(crate) const PROPERTY_HARD_REALTIME_CAPABLE: c_int = 0x4;

// The bits of `LADSPA_PortDescriptor`.
pub(crate) const PORT_INPUT: c_int = 0x1;
pub(crate) const PORT_OUTPUT: c_int = 0x2;
pub(crate) const PORT_CONTROL: c_int = 0x4;
pub(crate) const PORT_AUDIO: c_int = 0x8;

// The bits of `LADSPA_PortRangeHintDescriptor`.
pub(crate) const HINT_BOUNDED_BELOW: c_int = 0x1;
pub(crate) const HINT_BOUNDED_ABOVE: c_int = 0x2;
pub(crate) const HINT_TOGGLED: c_int = 0x4;
pub(crate) const HINT_SAMPLE_RATE: c_int = 0x8;
pub(crate) const HINT_LOGARITHMIC: c_int = 0x10;
pub(crate) const HINT_INTEGER: c_int = 0x20;
/// The bits that say what the default is (they are a number and not separate flags).
pub(crate) const HINT_DEFAULT_MASK: c_int = 0x3C0;
pub(crate) const HINT_DEFAULT_NONE: c_int = 0x0;
pub(crate) const HINT_DEFAULT_MINIMUM: c_int = 0x40;
pub(crate) const HINT_DEFAULT_LOW: c_int = 0x80;
pub(crate) const HINT_DEFAULT_MIDDLE: c_int = 0xC0;
pub(crate) const HINT_DEFAULT_HIGH: c_int = 0x100;
pub(crate) const HINT_DEFAULT_MAXIMUM: c_int = 0x140;
pub(crate) const HINT_DEFAULT_ZERO: c_int = 0x200;
pub(crate) const HINT_DEFAULT_ONE: c_int = 0x240;
pub(crate) const HINT_DEFAULT_ONE_HUNDRED: c_int = 0x280;
pub(crate) const HINT_DEFAULT_FOUR_HUNDRED_FORTY: c_int = 0x2C0;
