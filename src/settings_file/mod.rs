// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Saving and loading the global settings as pretty, human-readable JSON, in a file in the
//! operating system's folder for programs' settings (for example `%APPDATA%` on Windows).
//!
//! The file is more forgiving than a project file: a setting that is missing gets its default, and
//! one this version does not know is ignored, so that settings written by a newer or older version
//! never stop the program from starting.

mod document;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::atomic_file::write_file_atomically;
use crate::project::{Settings, SpedUpClipSound};
use document::{SettingsDocument, SettingsValuesDocument};

/// The value of the `file_format` field that marks a JSON file as fastforwardrs settings.
pub const FILE_FORMAT_NAME: &str = "fastforwardrs-settings";

/// The version of the file layout this build reads and writes.
pub const FILE_FORMAT_VERSION: u32 = 1;

/// The name of the folder, inside the operating system's settings folder, that holds the file.
const SETTINGS_FOLDER_NAME: &str = "fastforwardrs";

const SETTINGS_FILE_NAME: &str = "settings.json";

#[derive(Debug, Error)]
pub enum SettingsFileError {
    #[error("the settings file {path} could not be read: {source}")]
    CouldNotRead { path: PathBuf, source: io::Error },
    #[error("the settings file {path} could not be written: {source}")]
    CouldNotWrite { path: PathBuf, source: io::Error },
    #[error("the settings file {path} is not readable: {reason}")]
    NotASettingsFile { path: PathBuf, reason: String },
}

/// Where the global settings are kept, or `None` if this system has no settings folder (in which
/// case the settings are only remembered while the program runs).
pub fn default_settings_file_path() -> Option<PathBuf> {
    dirs::config_dir().map(|folder| folder.join(SETTINGS_FOLDER_NAME).join(SETTINGS_FILE_NAME))
}

/// Loads the settings from a file. A file that does not exist is not an error: it means the
/// settings have never been changed, so they are the defaults.
pub fn read_settings_file(path: &Path) -> Result<Settings, SettingsFileError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Settings::DEFAULT),
        Err(source) => {
            return Err(SettingsFileError::CouldNotRead {
                path: path.to_owned(),
                source,
            });
        }
    };
    settings_from_json_text(&text).map_err(|reason| SettingsFileError::NotASettingsFile {
        path: path.to_owned(),
        reason,
    })
}

/// Saves the settings to a file (and the folder it is in, if that does not exist yet), replacing
/// any file already there.
pub fn write_settings_file(settings: Settings, path: &Path) -> Result<(), SettingsFileError> {
    let could_not_write = |source: io::Error| SettingsFileError::CouldNotWrite {
        path: path.to_owned(),
        source,
    };
    if let Some(folder) = path.parent().filter(|folder| !folder.as_os_str().is_empty()) {
        fs::create_dir_all(folder).map_err(could_not_write)?;
    }
    write_file_atomically(path, &settings_to_json_text(settings)).map_err(could_not_write)
}

/// Turns JSON text into settings. The reason is given if it is not a settings file.
pub fn settings_from_json_text(text: &str) -> Result<Settings, String> {
    let document: SettingsDocument = serde_json::from_str(text).map_err(|error| error.to_string())?;
    if document.file_format != FILE_FORMAT_NAME {
        return Err(format!(
            "its file_format is \"{}\", not \"{FILE_FORMAT_NAME}\"",
            document.file_format
        ));
    }
    // A newer version's file is still read: what this version does not know about is ignored.
    let values = document.settings;
    Ok(Settings {
        autosave: values.autosave.unwrap_or(Settings::DEFAULT.autosave),
        sped_up_clip_sound: values
            .sped_up_clip_sound
            .or(values
                .mute_sped_up_clips
                .map(SpedUpClipSound::from_the_old_mute_setting))
            .unwrap_or(Settings::DEFAULT.sped_up_clip_sound),
    })
}

/// Turns settings into pretty JSON text.
pub fn settings_to_json_text(settings: Settings) -> String {
    let document = SettingsDocument {
        file_format: FILE_FORMAT_NAME.to_owned(),
        file_format_version: FILE_FORMAT_VERSION,
        settings: SettingsValuesDocument {
            autosave: Some(settings.autosave),
            sped_up_clip_sound: Some(settings.sped_up_clip_sound),
            mute_sped_up_clips: None,
        },
    };
    let mut text = serde_json::to_string_pretty(&document).expect("settings can always be written as JSON");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{SETTINGS, SpedUpClipSound};

    fn everything_at_its_last_value() -> Settings {
        Settings {
            autosave: true,
            sped_up_clip_sound: SpedUpClipSound::IncreasePitch,
        }
    }

    #[test]
    fn the_json_has_the_documented_pretty_human_readable_shape() {
        let text = settings_to_json_text(Settings {
            autosave: true,
            sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
        });
        assert_eq!(
            text,
            r#"{
  "file_format": "fastforwardrs-settings",
  "file_format_version": 1,
  "settings": {
    "autosave": true,
    "sped_up_clip_sound": "maintain_pitch"
  }
}
"#
        );
    }

    #[test]
    fn every_setting_in_the_table_survives_a_trip_through_text_in_every_state() {
        for setting in SETTINGS {
            for option in 0..setting.option_labels().len() {
                let mut settings = Settings::DEFAULT;
                setting.set_global_option(&mut settings, option);
                let text = settings_to_json_text(settings);
                assert_eq!(
                    settings_from_json_text(&text).unwrap(),
                    settings,
                    "{} set to {}
{text}",
                    setting.label(),
                    setting.option_labels()[option]
                );
            }
        }
        // And all of them together.
        assert_eq!(
            settings_from_json_text(&settings_to_json_text(everything_at_its_last_value())).unwrap(),
            everything_at_its_last_value()
        );
    }

    #[test]
    fn a_setting_missing_from_the_file_gets_its_default_and_the_rest_are_kept() {
        let text = r#"{ "file_format": "fastforwardrs-settings", "file_format_version": 1,
                        "settings": { "sped_up_clip_sound": "mute" } }"#;
        assert_eq!(
            settings_from_json_text(text).unwrap(),
            Settings {
                autosave: Settings::DEFAULT.autosave,
                sped_up_clip_sound: SpedUpClipSound::Mute
            }
        );
        let no_settings_at_all = r#"{ "file_format": "fastforwardrs-settings", "file_format_version": 1 }"#;
        assert_eq!(settings_from_json_text(no_settings_at_all).unwrap(), Settings::DEFAULT);
    }

    /// Regression test for the change from a yes-or-no setting to a choice of three: a settings file
    /// written by the earlier version keeps meaning what it meant.
    #[test]
    fn the_old_yes_or_no_mute_setting_still_means_mute_or_the_sound_at_its_own_pitch() {
        for (old_value, expected) in [
            ("true", SpedUpClipSound::Mute),
            ("false", SpedUpClipSound::MaintainPitch),
        ] {
            let text = format!(
                r#"{{ "file_format": "fastforwardrs-settings", "file_format_version": 1,
                    "settings": {{ "autosave": true, "mute_sped_up_clips": {old_value} }} }}"#
            );
            assert_eq!(
                settings_from_json_text(&text).unwrap(),
                Settings {
                    autosave: true,
                    sped_up_clip_sound: expected
                },
                "mute_sped_up_clips: {old_value}"
            );
        }
    }

    #[test]
    fn the_new_setting_wins_when_a_file_somehow_has_both_and_the_old_one_is_never_written_again() {
        let text = r#"{ "file_format": "fastforwardrs-settings", "file_format_version": 1,
                        "settings": { "sped_up_clip_sound": "increase_pitch", "mute_sped_up_clips": true } }"#;
        let settings = settings_from_json_text(text).unwrap();
        assert_eq!(settings.sped_up_clip_sound, SpedUpClipSound::IncreasePitch);
        assert!(!settings_to_json_text(settings).contains("mute_sped_up_clips"));
    }

    #[test]
    fn a_setting_this_version_does_not_know_is_ignored_rather_than_refused() {
        let text = r#"{ "file_format": "fastforwardrs-settings", "file_format_version": 7,
                        "something_new": [1, 2, 3],
                        "settings": { "autosave": true, "dark_mode": "very" } }"#;
        assert_eq!(
            settings_from_json_text(text).unwrap(),
            Settings {
                autosave: true,
                sped_up_clip_sound: Settings::DEFAULT.sped_up_clip_sound
            }
        );
    }

    #[test]
    fn text_that_is_not_a_settings_file_is_refused_with_a_reason() {
        for bad_text in [
            "",
            "not json",
            "[]",
            "{}",
            r#"{ "file_format": "fastforwardrs-project" }"#,
        ] {
            assert!(settings_from_json_text(bad_text).is_err(), "{bad_text:?}");
        }
        let reason = settings_from_json_text(r#"{ "file_format": "fastforwardrs-project", "file_format_version": 2 }"#)
            .unwrap_err();
        assert!(reason.contains("fastforwardrs-project"), "{reason}");
    }

    #[test]
    fn a_file_that_does_not_exist_means_the_settings_were_never_changed() {
        let folder = tempfile::tempdir().unwrap();
        assert_eq!(
            read_settings_file(&folder.path().join("settings.json")).unwrap(),
            Settings::DEFAULT
        );
    }

    #[test]
    fn settings_survive_a_trip_through_a_real_file_and_the_folder_is_made_when_needed() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a new folder").join("settings.json");

        write_settings_file(everything_at_its_last_value(), &path).unwrap();

        assert_eq!(read_settings_file(&path).unwrap(), everything_at_its_last_value());
        write_settings_file(Settings::DEFAULT, &path).unwrap();
        assert_eq!(
            read_settings_file(&path).unwrap(),
            Settings::DEFAULT,
            "and can be changed again"
        );
    }

    #[test]
    fn a_damaged_file_is_reported_with_its_path_so_the_program_can_say_what_it_ignored() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.json");
        fs::write(&path, "{ this is not json").unwrap();
        let error = read_settings_file(&path).unwrap_err();
        assert!(matches!(error, SettingsFileError::NotASettingsFile { .. }), "{error:?}");
        assert!(error.to_string().contains("settings.json"), "{error}");
    }

    #[test]
    fn the_default_place_is_a_file_in_a_folder_of_the_programs_own() {
        // (Where the settings folder is depends on the system, and a system may have none.)
        if let Some(path) = default_settings_file_path() {
            assert!(
                path.ends_with(Path::new("fastforwardrs").join("settings.json")),
                "{path:?}"
            );
        }
    }
}
