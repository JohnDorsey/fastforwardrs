// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Who may run a plugin (phase 7, piece 7.3; the design is `docs/engine-phase-7-design.md`, G14; the decisions are SF-04 to SF-10 of the features list).
//!
//! **A plugin is a program that runs with all the rights of the person who runs the editor, and nothing checks what it does.** A project file can name one or carry one, so
//! a project from somebody else could, if nothing stood in the way, run anything. Everything that stands in the way is here, and **none of it loads anything**: it decides, and a
//! [`LibraryLoader`] (which phase 7.4 makes; the tests have a fake that records what it was asked) does what was decided.
//!
//! * A **linked** plugin (a file somewhere) is asked about **the first time one from its folder is needed, and once for the folder** ([`ApprovalStore`] remembers folders: the
//!   maintainer does not want a question for every locally installed plugin). A yes to a folder is a yes to what is in it, and to what is put in it.
//! * An **embedded** plugin (inside the project file) is asked about **every time the project is opened**, and never remembered ([`TrustSession`] lasts as long as the project
//!   is open). The question is the severe one: *Don't load* is the default, and *Load it* can only be said by holding it down for five seconds ([`HoldToConfirm`]).
//! * An embedded plugin has **one build for each kind of computer**; the one for this computer is the one that is asked about, and a project that has none says which it has.
//! * An embedded plugin is **written out to a folder of its own** only after the person has said yes, **checked against the hash the project recorded for it**, and not bigger
//!   than 256 MiB ([`PluginCache`]); the folder is removed when the program ends.
//! * **Declining is safe**: a plugin that is not loaded is a plugin that is not there, and what uses it passes the sound through unchanged, as any effect that is missing does.
//! * The command line cannot ask: it refuses, and lists them, unless it is told `--allow-plugins` ([`what_the_command_line_refuses`]).
//!
//! No `unsafe` is used here, and the editor's crate forbids it: the one place that will need it is the crate that loads libraries (phase 7.4).

mod approvals;
mod extraction;
mod gate;
mod hold;
mod platform;

pub use approvals::{ApprovalStore, folder_for_a_person, folder_key, folder_of};
pub use extraction::{ExtractionError, MOST_BYTES_OF_A_PLUGIN, PluginCache};
pub use gate::{
    Answer, FolderStanding, LibraryLoader, Question, TrustSession, Unavailable, WhatIsNeeded, WhyNot, load_library,
    what_is_needed, what_the_command_line_refuses,
};
pub use hold::{HoldToConfirm, SECONDS_TO_HOLD};
pub use platform::this_platform;

#[cfg(test)]
mod tests;
