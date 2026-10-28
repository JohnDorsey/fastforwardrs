// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Running sound plugins in the sound of the preview and of the export (phase 7, piece 7.6; G4, G5, G10, G11, G12, G18 of `docs/engine-phase-7-design.md`).
//!
//! * [`PluginHost`] is what the sound engine asks for the effect of a plugin ([`PluginRunner`](crate::sound_engine::PluginRunner)): it checks that the folder the plugin is
//!   in has been said yes to (**one question for a folder**, see `plugin_trust`: when it has not, the sound goes through the plugin unchanged and the editor asks), loads the
//!   library once, writes the breadcrumb before it does, makes the plugin, and checks that it is the plugin that was scanned.
//! * [`PluginEffect`] is the effect: the channel layouts, the values that change with the keyframes, the plugin that is too slow and is left out, the sound that is not a number.
//!
//! Plugins run **in the editor's own process** (G4). A plugin that crashes takes the editor with it; the breadcrumb says, the next time, which library it was.

mod effect;
mod host;
mod instance;
mod needs;

pub use effect::{
    Layout, PluginEffect, SAMPLES_BETWEEN_LOOKS_WHEN_ANIMATED, SECONDS_TOO_SLOW, SlownessGuard, frame_of_a_sample,
};
pub use host::{PluginHost, Wanted};
pub use instance::{
    BetweenBlocks, ClapCopy, ClapMaker, LadspaCopy, LiveGuard, Lives, Lv2Copy, MOST_FRAMES_OF_A_CLAP_PLUGIN,
    PluginInstance, places_to_look_in,
};
pub use needs::folders_the_sound_of_a_project_needs;

#[cfg(test)]
mod synthesizer_tests;
#[cfg(test)]
mod tests;
