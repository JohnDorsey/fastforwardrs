// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The settings that decide how the program behaves.
//!
//! There are two layers. The **global settings** belong to the program and are the same for every
//! project. Each project can also set any one of them for itself, which is then used instead; a
//! setting a project does not set follows the global one. [`Settings`] holds a value for every
//! setting (the global ones, and also the ones in effect for a project once the two are combined),
//! and [`ProjectSettings`] holds what a project has set for itself.
//!
//! **Adding a setting:** give it a field in [`Settings`] (and its default), a field in
//! [`ProjectSettings`], a line in [`ProjectSettings::resolve`] and a row in [`SETTINGS`]; then add
//! it to the two files that store settings (`project_file` and `settings_file`). If its values are
//! not just on and off, make a type for them that implements [`SettingValue`]. The settings
//! windows are built from the table, and a test that goes through the table makes sure the files
//! keep every setting.

use serde::{Deserialize, Serialize};

/// What the sound of a clip that plays faster than normal is like.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpedUpClipSound {
    /// No sound at all: silence for as long as the clip lasts.
    Mute,
    /// The sound is played faster and keeps its pitch, so that a voice still sounds like the same
    /// voice, just hurried.
    #[default]
    MaintainPitch,
    /// The sound is played faster like a tape that is wound on: the pitch rises with the speed.
    IncreasePitch,
}

impl SpedUpClipSound {
    pub const ALL: [Self; 3] = [Self::Mute, Self::MaintainPitch, Self::IncreasePitch];

    /// What the settings windows call it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Mute => "Mute",
            Self::MaintainPitch => "Maintain pitch",
            Self::IncreasePitch => "Increase pitch",
        }
    }

    /// What the old yes-or-no setting "mute sped-up clips", which files from earlier versions
    /// still have, meant: yes was [`Self::Mute`], and no left the sound on, at its own pitch.
    pub fn from_the_old_mute_setting(muted: bool) -> Self {
        if muted { Self::Mute } else { Self::MaintainPitch }
    }
}

/// A value for every setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Save the project to its file by itself shortly after every change. (Only for a project that
    /// has a file already: the first save always asks where.)
    pub autosave: bool,
    /// What the sound of clips that play faster than normal is like, in the preview and in the
    /// exported video.
    pub sped_up_clip_sound: SpedUpClipSound,
}

impl Settings {
    /// What the global settings are before anyone has changed them.
    pub const DEFAULT: Self = Self {
        autosave: false,
        sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
    };
}

impl Default for Settings {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The settings a project has set for itself. `None` means the project follows the global setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProjectSettings {
    pub autosave: Option<bool>,
    pub sped_up_clip_sound: Option<SpedUpClipSound>,
}

impl ProjectSettings {
    /// A project that follows every global setting, as a new project does.
    pub const FOLLOW_ALL_GLOBAL_SETTINGS: Self = Self {
        autosave: None,
        sped_up_clip_sound: None,
    };

    /// The settings in effect for the project: its own where it has set them, the global ones
    /// everywhere else.
    pub fn resolve(&self, global: Settings) -> Settings {
        Settings {
            autosave: self.autosave.unwrap_or(global.autosave),
            sped_up_clip_sound: self.sped_up_clip_sound.unwrap_or(global.sped_up_clip_sound),
        }
    }
}

/// How a setting is shown in the settings windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingControlKind {
    /// A box that is ticked or not (for a setting with two values: off, then on).
    Checkbox,
    /// A drop-down list of the values.
    Dropdown,
}

/// A kind of value a setting can have.
pub trait SettingValue: Copy + PartialEq + 'static {
    /// Every value, in the order they are listed. The first is what an unticked checkbox means.
    const ALL: &'static [Self];
    const CONTROL: SettingControlKind;

    fn label(self) -> &'static str;
}

impl SettingValue for bool {
    const ALL: &'static [Self] = &[false, true];
    const CONTROL: SettingControlKind = SettingControlKind::Checkbox;

    fn label(self) -> &'static str {
        if self { "On" } else { "Off" }
    }
}

impl SettingValue for SpedUpClipSound {
    const ALL: &'static [Self] = &SpedUpClipSound::ALL;
    const CONTROL: SettingControlKind = SettingControlKind::Dropdown;

    fn label(self) -> &'static str {
        SpedUpClipSound::label(self)
    }
}

/// One setting, with the way to reach it in each of the two layers.
pub struct Setting<Value: SettingValue> {
    /// What the checkbox, or the drop-down list, is called.
    pub label: &'static str,
    pub global: fn(&mut Settings) -> &mut Value,
    pub project: fn(&mut ProjectSettings) -> &mut Option<Value>,
}

/// A [`Setting`] of any kind of value, which is what lets the settings windows (and the tests of
/// the files) go through all the settings in one list. A value is named by its number in the list
/// of values the setting can have.
pub trait AnySetting: Sync {
    fn label(&self) -> &'static str;
    fn control(&self) -> SettingControlKind;
    /// The label of each value, in order.
    fn option_labels(&self) -> Vec<&'static str>;
    /// The number of the value the global setting has.
    fn global_option(&self, settings: Settings) -> usize;
    fn set_global_option(&self, settings: &mut Settings, option: usize);
    /// The number of the value the project has set for itself, or `None` if it follows the global one.
    fn project_option(&self, settings: ProjectSettings) -> Option<usize>;
    fn set_project_option(&self, settings: &mut ProjectSettings, option: Option<usize>);
}

impl<Value: SettingValue> Setting<Value> {
    fn option_of(value: Value) -> usize {
        Value::ALL
            .iter()
            .position(|candidate| *candidate == value)
            .expect("every value is in the list of all values")
    }
}

impl<Value: SettingValue> AnySetting for Setting<Value> {
    fn label(&self) -> &'static str {
        self.label
    }

    fn control(&self) -> SettingControlKind {
        Value::CONTROL
    }

    fn option_labels(&self) -> Vec<&'static str> {
        Value::ALL.iter().map(|value| value.label()).collect()
    }

    fn global_option(&self, mut settings: Settings) -> usize {
        Self::option_of(*(self.global)(&mut settings))
    }

    fn set_global_option(&self, settings: &mut Settings, option: usize) {
        if let Some(value) = Value::ALL.get(option) {
            *(self.global)(settings) = *value;
        }
    }

    fn project_option(&self, mut settings: ProjectSettings) -> Option<usize> {
        (*(self.project)(&mut settings)).map(Self::option_of)
    }

    fn set_project_option(&self, settings: &mut ProjectSettings, option: Option<usize>) {
        *(self.project)(settings) = option.and_then(|number| Value::ALL.get(number).copied());
    }
}

const AUTOSAVE: Setting<bool> = Setting {
    label: "Autosave",
    global: |settings| &mut settings.autosave,
    project: |settings| &mut settings.autosave,
};

const SPED_UP_CLIP_SOUND: Setting<SpedUpClipSound> = Setting {
    label: "Sound of sped-up clips",
    global: |settings| &mut settings.sped_up_clip_sound,
    project: |settings| &mut settings.sped_up_clip_sound,
};

/// Every setting, in the order the settings windows show them.
pub static SETTINGS: [&dyn AnySetting; 2] = [&AUTOSAVE, &SPED_UP_CLIP_SOUND];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every setting at the last of its values, which for these is not the default.
    fn everything_at_its_last_value() -> Settings {
        Settings {
            autosave: true,
            sped_up_clip_sound: SpedUpClipSound::IncreasePitch,
        }
    }

    #[test]
    fn nothing_is_on_until_someone_turns_it_on_and_sped_up_sound_keeps_its_pitch() {
        assert_eq!(
            Settings::DEFAULT,
            Settings {
                autosave: false,
                sped_up_clip_sound: SpedUpClipSound::MaintainPitch
            },
            "in particular, no autosave, and the sound of fast clips is not muted"
        );
        assert_eq!(Settings::default(), Settings::DEFAULT);
        assert_eq!(SpedUpClipSound::default(), SpedUpClipSound::MaintainPitch);
    }

    #[test]
    fn the_three_kinds_of_sped_up_sound_are_listed_in_the_order_mute_maintain_increase() {
        let labels: Vec<&str> = SpedUpClipSound::ALL.iter().map(|sound| sound.label()).collect();
        assert_eq!(labels, ["Mute", "Maintain pitch", "Increase pitch"]);
    }

    #[test]
    fn a_project_that_follows_everything_gets_the_global_settings() {
        for global in [Settings::DEFAULT, everything_at_its_last_value()] {
            assert_eq!(ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS.resolve(global), global);
        }
        assert_eq!(ProjectSettings::default(), ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS);
    }

    #[test]
    fn a_setting_the_project_has_set_wins_over_the_global_one_whichever_way_it_differs() {
        let project = ProjectSettings {
            autosave: Some(true),
            sped_up_clip_sound: None,
        };
        assert_eq!(
            project.resolve(Settings::DEFAULT),
            Settings {
                autosave: true,
                sped_up_clip_sound: SpedUpClipSound::MaintainPitch
            }
        );

        let project = ProjectSettings {
            autosave: Some(false),
            sped_up_clip_sound: Some(SpedUpClipSound::Mute),
        };
        assert_eq!(
            project.resolve(everything_at_its_last_value()),
            Settings {
                autosave: false,
                sped_up_clip_sound: SpedUpClipSound::Mute
            },
            "the project's own choices beat the global ones"
        );

        let project = ProjectSettings {
            autosave: Some(false),
            sped_up_clip_sound: None,
        };
        assert_eq!(
            project.resolve(everything_at_its_last_value()).sped_up_clip_sound,
            SpedUpClipSound::IncreasePitch,
            "the setting it does not set follows the global one"
        );
    }

    #[test]
    fn the_table_reaches_every_setting_in_both_layers_and_each_row_reaches_a_different_one() {
        // Giving one row at a time its last value in each layer must change exactly that much, so
        // that two rows can never point at the same setting, and no setting is missing a row.
        let mut changed_globally = Settings::DEFAULT;
        let mut changed_in_the_project = ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS;
        for setting in SETTINGS {
            let last_option = setting.option_labels().len() - 1;

            let before = changed_globally;
            setting.set_global_option(&mut changed_globally, last_option);
            assert_ne!(changed_globally, before, "{} reaches a global setting", setting.label());
            assert_eq!(
                setting.global_option(changed_globally),
                last_option,
                "{}",
                setting.label()
            );

            let before = changed_in_the_project;
            setting.set_project_option(&mut changed_in_the_project, Some(last_option));
            assert_ne!(
                changed_in_the_project,
                before,
                "{} reaches a project setting",
                setting.label()
            );
            assert_eq!(
                setting.project_option(changed_in_the_project),
                Some(last_option),
                "{}",
                setting.label()
            );
        }
        assert_eq!(
            changed_globally,
            everything_at_its_last_value(),
            "every global setting has a row"
        );
        assert_eq!(
            changed_in_the_project.resolve(Settings::DEFAULT),
            everything_at_its_last_value(),
            "every project setting has a row"
        );
    }

    #[test]
    fn letting_go_of_a_project_setting_makes_it_follow_the_global_one_again() {
        for setting in SETTINGS {
            let mut project = ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS;
            setting.set_project_option(&mut project, Some(1));
            assert_eq!(setting.project_option(project), Some(1), "{}", setting.label());
            setting.set_project_option(&mut project, None);
            assert_eq!(
                project,
                ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS,
                "{}",
                setting.label()
            );
        }
    }

    #[test]
    fn a_number_that_is_not_a_value_of_the_setting_changes_nothing() {
        for setting in SETTINGS {
            let mut global = Settings::DEFAULT;
            setting.set_global_option(&mut global, 99);
            assert_eq!(global, Settings::DEFAULT, "{}", setting.label());
            let mut project = ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS;
            setting.set_project_option(&mut project, Some(99));
            assert_eq!(
                project,
                ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS,
                "{}",
                setting.label()
            );
        }
    }

    #[test]
    fn every_setting_has_its_own_label_and_at_least_two_values_each_with_its_own_label() {
        let mut labels: Vec<&str> = SETTINGS.iter().map(|setting| setting.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), SETTINGS.len());
        for setting in SETTINGS {
            let mut option_labels = setting.option_labels();
            assert!(option_labels.len() >= 2, "{}", setting.label());
            option_labels.sort_unstable();
            option_labels.dedup();
            assert_eq!(
                option_labels.len(),
                setting.option_labels().len(),
                "{}",
                setting.label()
            );
        }
    }

    #[test]
    fn checkboxes_are_for_the_settings_with_two_values_and_dropdowns_for_the_others() {
        let controls: Vec<(&str, SettingControlKind)> = SETTINGS
            .iter()
            .map(|setting| (setting.label(), setting.control()))
            .collect();
        assert_eq!(
            controls,
            [
                ("Autosave", SettingControlKind::Checkbox),
                ("Sound of sped-up clips", SettingControlKind::Dropdown)
            ]
        );
    }

    #[test]
    fn the_values_of_a_setting_are_serialised_as_plain_snake_case_words() {
        let words: Vec<String> = SpedUpClipSound::ALL
            .iter()
            .map(|sound| serde_json::to_string(sound).unwrap())
            .collect();
        assert_eq!(words, ["\"mute\"", "\"maintain_pitch\"", "\"increase_pitch\""]);
        assert!(serde_json::from_str::<SpedUpClipSound>("\"loud\"").is_err());
    }
}
