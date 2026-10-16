// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The exact shape of the settings file on disk. As with the project file, these types are kept
//! apart from the model ([`crate::project::Settings`]) so that the file format stays readable by
//! future versions while the model is free to change.
//!
//! Unlike a project file, nothing here refuses what it does not know: unknown fields are ignored
//! and missing ones are left to get their defaults.

use serde::{Deserialize, Serialize};

use crate::project::{DisplayTransform, OutputColourSpace, SpedUpClipSound, WorkingColourSpace};

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct SettingsDocument {
    /// Always [`super::FILE_FORMAT_NAME`]; lets us recognise (and refuse) other JSON files.
    pub file_format: String,
    pub file_format_version: u32,
    #[serde(default)]
    pub settings: SettingsValuesDocument,
}

/// A value for each setting, or none for one that the file does not have.
#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct SettingsValuesDocument {
    #[serde(default)]
    pub autosave: Option<bool>,
    #[serde(default)]
    pub sped_up_clip_sound: Option<SpedUpClipSound>,
    /// What versions that could only mute or not mute wrote instead of `sped_up_clip_sound`. It is
    /// read (when `sped_up_clip_sound` is not there) and never written.
    #[serde(default, skip_serializing)]
    pub mute_sped_up_clips: Option<bool>,
    /// The colour settings (phase 3): not there in the files of earlier versions, which get the defaults.
    #[serde(default)]
    pub display_transform: Option<DisplayTransform>,
    #[serde(default)]
    pub working_colour_space: Option<WorkingColourSpace>,
    #[serde(default)]
    pub output_colour_space: Option<OutputColourSpace>,
}
