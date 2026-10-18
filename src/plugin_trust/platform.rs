// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What kind of computer this is, as far as a library of native code is concerned.

/// The name of this platform as a build of an embedded library names it: the operating system and the processor, like `windows-x86_64`, `linux-x86_64` or `macos-aarch64`.
pub fn this_platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}
