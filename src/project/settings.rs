//! The settings that decide how the program behaves.
//!
//! There are two layers. The **global settings** belong to the program and are the same for every
//! project. Each project can also set any one of them for itself, which is then used instead; a
//! setting a project does not set follows the global one. [`Settings`] holds a value for every
//! setting (the global ones, and also the ones in effect for a project once the two are combined),
//! and [`ProjectSettings`] holds what a project has set for itself.
//!
//! **Adding a setting:** give it a field in [`Settings`] (and its default), a field in
//! [`ProjectSettings`], a line in [`ProjectSettings::resolve`] and a row in [`BOOLEAN_SETTINGS`];
//! then add it to the two files that store settings (`project_file` and `settings_file`). The
//! settings windows are built from the table, and a test that goes through the table makes sure the
//! files keep every setting.

/// A value for every setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Save the project to its file by itself shortly after every change. (Only for a project that
    /// has a file already: the first save always asks where.)
    pub autosave: bool,
    /// Leave the sound of sped-up clips out, in the preview and in the exported video.
    pub mute_sped_up_clips: bool,
}

impl Settings {
    /// What the global settings are before anyone has changed them.
    pub const DEFAULT: Self = Self {
        autosave: false,
        mute_sped_up_clips: false,
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
    pub mute_sped_up_clips: Option<bool>,
}

impl ProjectSettings {
    /// A project that follows every global setting, as a new project does.
    pub const FOLLOW_ALL_GLOBAL_SETTINGS: Self = Self {
        autosave: None,
        mute_sped_up_clips: None,
    };

    /// The settings in effect for the project: its own where it has set them, the global ones
    /// everywhere else.
    pub fn resolve(&self, global: Settings) -> Settings {
        Settings {
            autosave: self.autosave.unwrap_or(global.autosave),
            mute_sped_up_clips: self.mute_sped_up_clips.unwrap_or(global.mute_sped_up_clips),
        }
    }
}

/// One setting that is a checkbox, with the way to reach it in each of the two layers. The
/// settings windows are made from a list of these.
#[derive(Clone, Copy)]
pub struct BooleanSetting {
    /// What the checkbox says.
    pub label: &'static str,
    pub global: fn(&mut Settings) -> &mut bool,
    pub project: fn(&mut ProjectSettings) -> &mut Option<bool>,
}

/// Every setting, in the order the settings windows show them.
pub const BOOLEAN_SETTINGS: [BooleanSetting; 2] = [
    BooleanSetting {
        label: "Autosave",
        global: |settings| &mut settings.autosave,
        project: |settings| &mut settings.autosave,
    },
    BooleanSetting {
        label: "Mute sped-up clips",
        global: |settings| &mut settings.mute_sped_up_clips,
        project: |settings| &mut settings.mute_sped_up_clips,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn everything_on() -> Settings {
        Settings {
            autosave: true,
            mute_sped_up_clips: true,
        }
    }

    #[test]
    fn nothing_is_on_until_someone_turns_it_on() {
        assert_eq!(
            Settings::DEFAULT,
            Settings {
                autosave: false,
                mute_sped_up_clips: false
            },
            "in particular, no autosave"
        );
        assert_eq!(Settings::default(), Settings::DEFAULT);
    }

    #[test]
    fn a_project_that_follows_everything_gets_the_global_settings() {
        for global in [Settings::DEFAULT, everything_on()] {
            assert_eq!(ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS.resolve(global), global);
        }
        assert_eq!(ProjectSettings::default(), ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS);
    }

    #[test]
    fn a_setting_the_project_has_set_wins_over_the_global_one_whichever_way_it_differs() {
        let project = ProjectSettings {
            autosave: Some(true),
            mute_sped_up_clips: None,
        };
        assert_eq!(
            project.resolve(Settings::DEFAULT),
            Settings {
                autosave: true,
                mute_sped_up_clips: false
            }
        );

        let project = ProjectSettings {
            autosave: Some(false),
            mute_sped_up_clips: None,
        };
        assert_eq!(
            project.resolve(everything_on()),
            Settings {
                autosave: false,
                mute_sped_up_clips: true
            },
            "the project's own 'off' beats a global 'on', and the setting it does not set follows"
        );
    }

    #[test]
    fn the_table_reaches_every_setting_in_both_layers_and_each_row_reaches_a_different_one() {
        // Turning on one row at a time in each layer must change exactly that much, so that two
        // rows can never point at the same setting, and no setting is missing a row.
        let mut turned_on_globally = Settings::DEFAULT;
        let mut turned_on_in_the_project = ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS;
        for setting in BOOLEAN_SETTINGS {
            let before = turned_on_globally;
            *(setting.global)(&mut turned_on_globally) = true;
            assert_ne!(turned_on_globally, before, "{} reaches a global setting", setting.label);
            let before = turned_on_in_the_project;
            *(setting.project)(&mut turned_on_in_the_project) = Some(true);
            assert_ne!(
                turned_on_in_the_project, before,
                "{} reaches a project setting",
                setting.label
            );
        }
        assert_eq!(turned_on_globally, everything_on(), "every global setting has a row");
        assert_eq!(
            turned_on_in_the_project.resolve(Settings::DEFAULT),
            everything_on(),
            "every project setting has a row"
        );
    }

    #[test]
    fn every_setting_has_its_own_label() {
        let mut labels: Vec<&str> = BOOLEAN_SETTINGS.iter().map(|setting| setting.label).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), BOOLEAN_SETTINGS.len());
    }
}
