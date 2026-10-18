// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a program that loads libraries of other people does so that a library that crashes it does not put a window on the screen or leave a file of its memory behind.

#![allow(unsafe_code)]

/// Tells the system that a crash of this program is not to be shown to anybody (on Windows: no "has stopped working" window, which would keep the program alive until
/// someone clicked it, and no window about a library that cannot be found). The program ends, with a code that says how.
#[cfg(windows)]
pub fn prepare_to_crash_quietly() {
    // `SetErrorMode` of kernel32 takes a set of bits and gives the old ones.
    const NO_CRITICAL_ERROR_WINDOWS: u32 = 0x0001;
    const NO_CRASH_WINDOWS: u32 = 0x0002;
    const NO_OPEN_FILE_ERROR_WINDOWS: u32 = 0x8000;
    // SAFETY: the function exists in kernel32 (every Windows has it) and has this signature; it only sets a flag of the process, and takes no pointers.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        safe fn SetErrorMode(mode: u32) -> u32;
    }
    SetErrorMode(NO_CRITICAL_ERROR_WINDOWS | NO_CRASH_WINDOWS | NO_OPEN_FILE_ERROR_WINDOWS);
}

/// Tells the system that a crash of this program is not to leave a file of its memory behind (a "core file", which can be hundreds of megabytes, in whatever folder the program
/// was started from): the limit on the size of one is made zero.
#[cfg(all(unix, target_pointer_width = "64"))]
pub fn prepare_to_crash_quietly() {
    /// `struct rlimit` on the 64-bit systems of the Unix family: two 64-bit numbers.
    #[repr(C)]
    struct ResourceLimit {
        soft: u64,
        hard: u64,
    }
    /// `RLIMIT_CORE`: the same number on Linux and on macOS.
    const LIMIT_ON_CORE_FILES: i32 = 4;
    // SAFETY: `setrlimit` is in the C library of every system of the Unix family and has this signature (the resource is an `int` there too).
    unsafe extern "C" {
        fn setrlimit(resource: i32, limit: *const ResourceLimit) -> i32;
    }
    let none = ResourceLimit { soft: 0, hard: 0 };
    // SAFETY: the pointer is to a value on this stack that lives until the call is over, which only reads it. (If the call fails, core files are not prevented, which is all.)
    let _ = unsafe { setrlimit(LIMIT_ON_CORE_FILES, &none) };
}

/// (See above: there is nothing to do on this system.)
#[cfg(not(any(windows, all(unix, target_pointer_width = "64"))))]
pub fn prepare_to_crash_quietly() {}
