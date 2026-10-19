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

/// Puts `folders` before the others in the places where the system looks for the libraries that a library needs, for the libraries that are loaded from now on, **where that
/// can be done after the program has started: on Windows**, where it is the `PATH` of the program (`original` is what the search path was before; this is given it each time, so
/// that it does not grow). Gives whether it was done. The other systems read their search path when the program starts: a library that needs one that is not beside it
/// has to be loaded by a program that was started with the folder in `LD_LIBRARY_PATH`.
#[cfg(windows)]
pub fn put_folders_before_the_search_path(folders: &[std::path::PathBuf], original: &std::ffi::OsStr) -> bool {
    let all = folders.iter().cloned().chain(std::env::split_paths(original));
    let Ok(value) = std::env::join_paths(all) else {
        return false;
    };
    // SAFETY: `set_var` is unsafe because on some systems another thread that reads the environment at the same time can see it half changed. On Windows the environment of the
    // program is kept by the system, and this is a call of the system that is made one variable at a time, from any thread.
    unsafe { std::env::set_var("PATH", value) };
    true
}

/// (See above: the search path of this system is read when the program starts.)
#[cfg(not(windows))]
pub fn put_folders_before_the_search_path(_folders: &[std::path::PathBuf], _original: &std::ffi::OsStr) -> bool {
    false
}
