// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sound plugins in processes of their own (phase 14; the design is `docs/engine-phase-14-processes-design.md`).
//!
//! A plugin is somebody else's code, and it runs on the sound's thread: when it crashes, whatever process it runs in ends. Run in the editor's process, it takes the editor and the
//! project with it. Here a copy of a plugin runs in **a program of its own**, `fastforwardrs-plugin-process`, and the editor talks to it over a local socket:
//!
//! * [`protocol`] is what they say to each other: frames, with the cold messages as JSON and a block of sound as the 32 bits of each sample.
//! * `told` keeps what a program has been told that it keeps (the last value of each parameter, the notes that are held, the pedal and the wheel), so that a program that is started in the place
//!   of one that ended can be told it again.
//! * [`remote`] is the editor's side: a [`RemoteCopy`](remote::RemoteCopy) is a [`PluginInstance`](crate::plugin_host::PluginInstance), so that the effect that runs a plugin does not know
//!   whether the copy is in its process or in another. **Whatever happens to the program** (it crashes, it never answers, it says nonsense) **comes out as an error of that call**, and the
//!   program is started again, told what it must know, and given the block again; a program that cannot be (it ends every time) leaves the effect out for the rest of the play, which says why,
//!   as it does with a plugin that fails to run.
//! * [`program`] is the inside of the program, which is only this, called from `main`: it makes the copy it is asked for and runs the blocks it is given.
//!
//! Nothing here chooses which plugins go in a process of their own: that is a setting of the [`PluginHost`](crate::plugin_host::PluginHost).

pub mod program;
pub mod protocol;
pub mod remote;
mod told;
