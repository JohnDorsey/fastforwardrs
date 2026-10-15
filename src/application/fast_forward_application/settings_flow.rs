// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The settings: which are in effect, changing them, and keeping the global ones in their file.

use std::path::PathBuf;

use super::FastForwardApplication;
use crate::project::{ProjectSettings, Settings};
use crate::settings_file::{read_settings_file, write_settings_file};

impl FastForwardApplication {
    /// The settings in effect for the project being edited: its own where it has set them, the
    /// global ones everywhere else.
    pub(super) fn effective_settings(&self) -> Settings {
        self.project.settings().resolve(self.global_settings)
    }

    /// Uses `path` as the file the global settings are kept in: the settings are read from it now
    /// (if it exists), and written to it whenever they are changed. If the file cannot be read,
    /// the default settings are used and a message says so; the file is left alone until the
    /// settings are changed.
    pub fn keep_global_settings_in(&mut self, path: PathBuf) {
        match read_settings_file(&path) {
            Ok(settings) => self.global_settings = settings,
            Err(error) => {
                self.global_settings = Settings::DEFAULT;
                self.add_to_message(
                    "The settings could not be loaded",
                    format!("{error}\n\nThe default settings are being used until you change them."),
                );
            }
        }
        self.global_settings_path = Some(path);
    }

    pub(super) fn set_global_settings(&mut self, settings: Settings) {
        if settings == self.global_settings {
            return;
        }
        self.global_settings = settings;
        if let Some(path) = self.global_settings_path.clone()
            && let Err(error) = write_settings_file(settings, &path)
        {
            self.show_message("The settings could not be saved", error.to_string());
        }
    }

    pub(super) fn set_project_settings(&mut self, settings: ProjectSettings) {
        if settings == self.project.settings() {
            return;
        }
        self.project.set_settings(settings);
        self.note_unsaved_change();
    }
}
