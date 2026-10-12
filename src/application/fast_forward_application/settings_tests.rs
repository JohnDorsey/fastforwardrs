//! Tests of the Edit menu, the two settings windows, how the settings are kept, and autosave.

use std::time::{Duration, Instant};

use egui_kittest::kittest::{NodeT, Queryable};

use super::test_window::{Window, project_of};
use crate::application::commands::Command;
use crate::application::dialogs::{AfterConfirmation, Dialog};
use crate::application::settings_windows::FRAME_RATE_LABEL;
use crate::project::{
    FrameRate, Project, ProjectSettings, SETTINGS, SettingControlKind, Settings, SourceMedia, SourceMediaIdentifier,
    SourceMediaLink, SourceVideoProperties, SpedUpClipSound, TimelineFrameIndex,
};
use crate::project_file::read_project_file;
use crate::settings_file::read_settings_file;

const AUTOSAVE: &str = "Autosave";
const SPED_UP_CLIP_SOUND: &str = "Sound of sped-up clips";
const USE_GLOBAL_SETTING: &str = "Use global setting";

impl Window {
    /// Opens a window from the Edit menu.
    fn open_from_the_edit_menu(&mut self, item: &str) {
        self.harness.get_by_label("Edit").click();
        self.settle();
        self.harness.get_by_label(item).click();
        self.settle();
    }

    fn is_disabled(&self, label: &str) -> bool {
        self.harness.get_by_label(label).accesskit_node().is_disabled()
    }

    /// Whether a drop-down list that shows `value` is greyed out. (A drop-down list is found by the
    /// value it shows, which is where the screen reader tree puts it.)
    fn dropdown_is_disabled(&self, value: &str) -> bool {
        self.harness.get_by_value(value).accesskit_node().is_disabled()
    }

    /// Clicks the `number`th (from the top, counting from 0) of the checkboxes with this label.
    fn click_checkbox_number(&mut self, label: &str, number: usize) {
        self.harness.get_all_by_label(label).nth(number).unwrap().click();
        self.settle();
    }

    /// Whether the control of a setting is greyed out: the checkbox, or the drop-down list, which
    /// is found by the value it shows.
    fn setting_is_greyed_out(&self, setting: &dyn crate::project::AnySetting, value_shown: &str) -> bool {
        match setting.control() {
            SettingControlKind::Checkbox => self.is_disabled(setting.label()),
            SettingControlKind::Dropdown => self.dropdown_is_disabled(value_shown),
        }
    }

    /// Opens the drop-down list that shows `shown` and picks `option` in it.
    fn choose_from_the_dropdown_showing(&mut self, shown: &str, option: &str) {
        self.harness.get_by_value(shown).click();
        self.settle();
        self.harness.get_by_label(option).click();
        self.settle();
    }
}

fn assert_no_window_is_open(window: &Window) {
    assert!(window.harness.query_by_label("Global settings").is_none());
    assert!(window.harness.query_by_label("Project settings").is_none());
}

// --- the global settings window ---

#[test]
fn the_global_settings_window_has_a_control_for_each_setting_with_the_defaults_to_begin_with() {
    let mut window = Window::showing(project_of(300));
    assert_no_window_is_open(&window);

    window.open_from_the_edit_menu("Global settings");

    assert!(window.harness.query_by_label("Global settings").is_some());
    assert!(!window.is_checked(AUTOSAVE), "autosave starts off");
    assert!(!window.is_disabled(AUTOSAVE));
    assert!(window.harness.query_by_label(SPED_UP_CLIP_SOUND).is_some());
    assert!(
        window.harness.query_by_value("Maintain pitch").is_some(),
        "the drop-down list shows what is chosen: the sound keeps its pitch to begin with"
    );
    assert!(!window.dropdown_is_disabled("Maintain pitch"));
    assert!(
        window.harness.query_by_label(USE_GLOBAL_SETTING).is_none(),
        "that is only in the project's window"
    );
}

#[test]
fn the_dropdown_for_sped_up_clips_offers_mute_maintain_pitch_and_increase_pitch_in_that_order() {
    let mut window = Window::showing(project_of(300));
    window.open_from_the_edit_menu("Global settings");

    window.harness.get_by_value("Maintain pitch").click();
    window.settle();

    // The three choices are listed (the box itself shows the chosen one as its value, not as a label).
    let heights: Vec<f32> = ["Mute", "Maintain pitch", "Increase pitch"]
        .iter()
        .map(|option| window.rectangle_of(option).top())
        .collect();
    assert!(
        heights.windows(2).all(|pair| pair[0] < pair[1]),
        "listed top to bottom in this order: {heights:?}"
    );
}

#[test]
fn choosing_a_sound_for_sped_up_clips_in_the_global_window_changes_the_global_setting_only() {
    let mut window = Window::showing(project_of(300));
    window.open_from_the_edit_menu("Global settings");

    window.choose_from_the_dropdown_showing("Maintain pitch", "Increase pitch");
    assert_eq!(
        window.application().global_settings().sped_up_clip_sound,
        SpedUpClipSound::IncreasePitch
    );
    assert!(
        window.harness.query_by_value("Increase pitch").is_some(),
        "and the box shows it"
    );

    window.choose_from_the_dropdown_showing("Increase pitch", "Mute");
    assert_eq!(
        window.application().global_settings().sped_up_clip_sound,
        SpedUpClipSound::Mute
    );

    window.choose_from_the_dropdown_showing("Mute", "Maintain pitch");
    assert_eq!(window.application().global_settings(), Settings::DEFAULT);
    assert!(
        !window.application().has_unsaved_changes(),
        "a global setting is not a change to the project"
    );
}

#[test]
fn ticking_a_global_setting_changes_it_and_unticking_changes_it_back() {
    let mut window = Window::showing(project_of(300));
    window.open_from_the_edit_menu("Global settings");

    window.harness.get_by_label(AUTOSAVE).click();
    window.settle();
    assert!(window.application().global_settings().autosave);
    assert!(window.is_checked(AUTOSAVE));
    assert_eq!(
        window.application().global_settings().sped_up_clip_sound,
        SpedUpClipSound::MaintainPitch,
        "the other is untouched"
    );

    window.harness.get_by_label(AUTOSAVE).click();
    window.settle();
    assert_eq!(window.application().global_settings(), Settings::DEFAULT);
}

#[test]
fn changing_a_global_setting_is_not_a_change_to_the_project() {
    let mut window = Window::showing(project_of(300));
    window.open_from_the_edit_menu("Global settings");
    window.choose_from_the_dropdown_showing("Maintain pitch", "Mute");
    assert_eq!(
        window.application().global_settings().sped_up_clip_sound,
        SpedUpClipSound::Mute
    );
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn a_settings_window_closes_with_its_close_button_and_can_be_opened_again() {
    let mut window = Window::showing(project_of(300));
    window.open_from_the_edit_menu("Global settings");
    window.harness.get_by_label("Close window").click();
    window.settle();
    assert_no_window_is_open(&window);

    window.open_from_the_edit_menu("Global settings");
    assert!(window.harness.query_by_label("Global settings").is_some());
}

// --- the project settings window ---

#[test]
fn a_new_project_follows_every_global_setting_so_the_controls_are_greyed_out() {
    let mut window = Window::showing(project_of(300));

    window.open_from_the_edit_menu("Project settings");

    assert!(window.harness.query_by_label("Project settings").is_some());
    let follow_checkboxes = window.harness.get_all_by_label(USE_GLOBAL_SETTING).count();
    assert_eq!(follow_checkboxes, SETTINGS.len(), "one next to each control");
    for number in 0..follow_checkboxes {
        let checkbox = window.harness.get_all_by_label(USE_GLOBAL_SETTING).nth(number).unwrap();
        assert_eq!(
            checkbox.accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::True)
        );
    }
    for setting in SETTINGS {
        let shown_value = setting.option_labels()[setting.global_option(Settings::DEFAULT)];
        assert!(
            window.setting_is_greyed_out(setting, shown_value),
            "{} is greyed out",
            setting.label()
        );
    }
    assert_eq!(
        window.application().project.settings(),
        ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS
    );
}

#[test]
fn the_greyed_out_control_shows_the_global_value_that_is_being_used() {
    let mut window = Window::showing(project_of(300));
    window.application_mut().global_settings.sped_up_clip_sound = SpedUpClipSound::IncreasePitch;
    window.application_mut().global_settings.autosave = true;

    window.open_from_the_edit_menu("Project settings");

    assert!(
        window.harness.query_by_value("Increase pitch").is_some(),
        "the global choice is shown"
    );
    assert!(window.dropdown_is_disabled("Increase pitch"));
    assert!(window.is_checked(AUTOSAVE), "and the global 'on'");
    assert!(window.is_disabled(AUTOSAVE));
}

#[test]
fn unticking_use_global_setting_frees_the_control_starting_at_the_global_value() {
    let mut window = Window::showing(project_of(300));
    window.application_mut().global_settings.sped_up_clip_sound = SpedUpClipSound::Mute;
    window.open_from_the_edit_menu("Project settings");
    assert!(!window.application().has_unsaved_changes());

    // The second row is the sound of sped-up clips.
    window.click_checkbox_number(USE_GLOBAL_SETTING, 1);

    assert_eq!(
        window.application().project.settings(),
        ProjectSettings {
            autosave: None,
            sped_up_clip_sound: Some(SpedUpClipSound::Mute),
        },
        "the project now has its own setting, which starts as what the global one was"
    );
    assert!(!window.dropdown_is_disabled("Mute"), "the control is free");
    assert!(window.is_disabled(AUTOSAVE), "the other one still follows");
    assert!(
        window.application().has_unsaved_changes(),
        "a project setting is saved with the project"
    );
}

#[test]
fn a_project_can_set_its_own_value_against_the_global_one_in_either_direction() {
    let mut window = Window::showing(project_of(300));
    window.application_mut().global_settings.autosave = true;
    window.open_from_the_edit_menu("Project settings");

    window.click_checkbox_number(USE_GLOBAL_SETTING, 0); // let go of the global autosave
    window.harness.get_by_label(AUTOSAVE).click(); // and turn it off for this project
    window.settle();

    assert_eq!(window.application().project.settings().autosave, Some(false));
    assert!(
        window.application().global_settings().autosave,
        "the global setting is untouched"
    );
    assert!(
        !window.application().settings_in_effect().autosave,
        "the project's own wins"
    );
}

#[test]
fn ticking_use_global_setting_again_goes_back_to_following_and_forgets_the_projects_own_value() {
    let mut window = Window::showing(project_of(300));
    window.open_from_the_edit_menu("Project settings");
    window.click_checkbox_number(USE_GLOBAL_SETTING, 0);
    window.harness.get_by_label(AUTOSAVE).click();
    window.settle();
    assert_eq!(window.application().project.settings().autosave, Some(true));

    window.click_checkbox_number(USE_GLOBAL_SETTING, 0);

    assert_eq!(
        window.application().project.settings(),
        ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS
    );
    assert!(window.is_disabled(AUTOSAVE));
}

#[test]
fn a_global_change_reaches_a_project_that_follows_and_not_one_that_has_its_own_value() {
    let mut window = Window::showing(project_of(300));
    window.application_mut().project.set_settings(ProjectSettings {
        autosave: Some(false),
        sped_up_clip_sound: None,
    });
    window.open_from_the_edit_menu("Global settings");
    window.harness.get_by_label(AUTOSAVE).click();
    window.choose_from_the_dropdown_showing("Maintain pitch", "Mute");

    let in_effect = window.application().settings_in_effect();
    assert!(!in_effect.autosave, "the project has its own 'off'");
    assert_eq!(
        in_effect.sped_up_clip_sound,
        SpedUpClipSound::Mute,
        "the project follows this one"
    );
}

#[test]
fn a_new_project_follows_the_global_settings_again_whatever_the_old_one_had_set() {
    let mut window = Window::showing(project_of(300));
    window.application_mut().project.set_settings(ProjectSettings {
        autosave: Some(true),
        sped_up_clip_sound: Some(SpedUpClipSound::Mute),
    });

    window.application_mut().start_empty_project();

    assert_eq!(
        window.application().project.settings(),
        ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS
    );
}

#[test]
fn both_windows_can_be_open_together() {
    let mut window = Window::showing(project_of(300));
    window.open_from_the_edit_menu("Global settings");
    window.open_from_the_edit_menu("Project settings");
    assert!(window.harness.query_by_label("Global settings").is_some());
    assert!(window.harness.query_by_label("Project settings").is_some());
}

// --- keeping the global settings ---

#[test]
fn the_global_settings_are_saved_to_their_file_as_soon_as_they_change() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("fastforwardrs").join("settings.json");
    let mut window = Window::showing(project_of(300));
    window.application_mut().keep_global_settings_in(path.clone());
    assert!(!path.exists(), "nothing is written until there is something to write");

    window.open_from_the_edit_menu("Global settings");
    window.choose_from_the_dropdown_showing("Maintain pitch", "Increase pitch");

    assert_eq!(
        read_settings_file(&path).unwrap(),
        Settings {
            autosave: false,
            sped_up_clip_sound: SpedUpClipSound::IncreasePitch
        }
    );
}

#[test]
fn the_global_settings_are_loaded_from_their_file_when_the_program_starts() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("settings.json");
    crate::settings_file::write_settings_file(
        Settings {
            autosave: true,
            sped_up_clip_sound: SpedUpClipSound::Mute,
        },
        &path,
    )
    .unwrap();
    let mut window = Window::showing(project_of(300));

    window.application_mut().keep_global_settings_in(path);

    assert!(window.application().global_settings().autosave);
    assert_eq!(
        window.application().global_settings().sped_up_clip_sound,
        SpedUpClipSound::Mute
    );
    assert!(window.application().message_showing().is_none());
}

#[test]
fn a_damaged_settings_file_gives_a_message_and_the_defaults_and_is_not_overwritten_until_a_change() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("settings.json");
    std::fs::write(&path, "{ not json").unwrap();
    let mut window = Window::showing(project_of(300));

    window.application_mut().keep_global_settings_in(path.clone());
    window.settle();

    assert_eq!(window.application().global_settings(), Settings::DEFAULT);
    let (title, text) = window
        .application()
        .message_showing()
        .expect("a message says what happened");
    assert_eq!(title, "The settings could not be loaded");
    assert!(text.contains("settings.json"), "{text}");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "{ not json",
        "the damaged file is left as it was"
    );
}

#[test]
fn a_settings_file_that_cannot_be_written_gives_a_message_and_the_setting_still_applies() {
    let folder = tempfile::tempdir().unwrap();
    // The "file" is a folder, so it cannot be written.
    let path = folder.path().join("settings.json");
    std::fs::create_dir(&path).unwrap();
    let mut window = Window::showing(project_of(300));
    window.application_mut().global_settings_path = Some(path);

    window.open_from_the_edit_menu("Global settings");
    window.harness.get_by_label(AUTOSAVE).click();
    window.settle();

    let (title, _) = window
        .application()
        .message_showing()
        .expect("a message says it was not saved");
    assert_eq!(title, "The settings could not be saved");
    assert!(
        window.application().global_settings().autosave,
        "it works for now anyway"
    );
}

// --- autosave ---

/// A window with a project that has a file (so that it can be autosaved), saved and unchanged.
fn window_with_a_saved_project(folder: &tempfile::TempDir) -> (Window, std::path::PathBuf) {
    let mut window = Window::showing(project_of(300));
    let path = folder.path().join("holiday.fastforwardrs.json");
    window.application_mut().save_project_to(path.clone());
    assert!(!window.application().has_unsaved_changes());
    (window, path)
}

fn clip_count_in_file(path: &std::path::Path) -> usize {
    read_project_file(path).unwrap().tracks()[0].entries().len()
}

#[test]
fn with_autosave_off_nothing_is_saved_by_itself_however_long_it_takes() {
    let folder = tempfile::tempdir().unwrap();
    let (mut window, path) = window_with_a_saved_project(&folder);

    window.cut_at_frame(100);
    let much_later = Instant::now() + Duration::from_secs(3600);
    window
        .application_mut()
        .autosave_when_quiet_for_long_enough(&egui::Context::default(), much_later);

    assert!(window.application().has_unsaved_changes());
    assert_eq!(clip_count_in_file(&path), 1, "the file is as it was saved");
}

#[test]
fn with_autosave_on_a_change_is_saved_once_the_project_has_been_quiet_for_a_while_and_not_before() {
    let folder = tempfile::tempdir().unwrap();
    let (mut window, path) = window_with_a_saved_project(&folder);
    window.application_mut().global_settings.autosave = true;
    let context = egui::Context::default();

    window.cut_at_frame(100);
    let changed_at = Instant::now();

    window
        .application_mut()
        .autosave_when_quiet_for_long_enough(&context, changed_at + Duration::from_millis(500));
    assert_eq!(clip_count_in_file(&path), 1, "too soon");
    assert!(window.application().has_unsaved_changes());

    window
        .application_mut()
        .autosave_when_quiet_for_long_enough(&context, changed_at + Duration::from_secs(5));
    assert_eq!(clip_count_in_file(&path), 2, "saved");
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn a_project_that_sets_autosave_for_itself_is_autosaved_even_though_the_global_one_is_off() {
    let folder = tempfile::tempdir().unwrap();
    let (mut window, path) = window_with_a_saved_project(&folder);
    window.application_mut().project.set_settings(ProjectSettings {
        autosave: Some(true),
        sped_up_clip_sound: None,
    });

    window.cut_at_frame(100);
    window
        .application_mut()
        .autosave_when_quiet_for_long_enough(&egui::Context::default(), Instant::now() + Duration::from_secs(5));

    assert_eq!(clip_count_in_file(&path), 2);
}

#[test]
fn a_project_that_never_had_a_file_is_not_autosaved_because_that_would_need_a_question() {
    let mut window = Window::showing(project_of(300));
    window.application_mut().global_settings.autosave = true;
    window.cut_at_frame(100);
    window
        .application_mut()
        .autosave_when_quiet_for_long_enough(&egui::Context::default(), Instant::now() + Duration::from_secs(5));
    assert!(window.application().has_unsaved_changes());
    assert!(window.application().message_showing().is_none());
    assert_eq!(window.application().project_file_path(), None);
}

#[test]
fn autosave_asks_for_another_look_when_it_will_be_time() {
    let folder = tempfile::tempdir().unwrap();
    let (mut window, _) = window_with_a_saved_project(&folder);
    window.application_mut().global_settings.autosave = true;
    window.cut_at_frame(100);
    let context = egui::Context::default();

    window
        .application_mut()
        .autosave_when_quiet_for_long_enough(&context, Instant::now());

    assert!(
        context.has_requested_repaint(),
        "the window must be drawn again after the quiet time to do the saving, though nobody touches it"
    );
}

#[test]
fn with_autosave_on_closing_or_starting_a_new_project_does_not_ask_about_unsaved_changes() {
    let folder = tempfile::tempdir().unwrap();
    let (mut window, path) = window_with_a_saved_project(&folder);
    window.application_mut().global_settings.autosave = true;
    window.cut_at_frame(100);
    assert!(
        window.application().has_unsaved_changes(),
        "just changed, not yet autosaved"
    );

    // (Closing the window: the other answers would go on to open a real file dialog, and wait.)
    window
        .application_mut()
        .ask_before_discarding(AfterConfirmation::CloseWindow);

    assert!(
        !matches!(window.application().dialog, Some(Dialog::ConfirmDiscardChanges { .. })),
        "nothing to ask: the changes were saved first"
    );
    assert_eq!(clip_count_in_file(&path), 2);
}

#[test]
fn without_autosave_the_question_is_still_asked() {
    let folder = tempfile::tempdir().unwrap();
    let (mut window, path) = window_with_a_saved_project(&folder);
    window.cut_at_frame(100);

    window
        .application_mut()
        .ask_before_discarding(AfterConfirmation::NewProject);

    assert!(matches!(
        window.application().dialog,
        Some(Dialog::ConfirmDiscardChanges { .. })
    ));
    assert_eq!(
        clip_count_in_file(&path),
        1,
        "and nothing was saved behind the user's back"
    );
}

#[test]
fn a_failed_autosave_says_so_once_and_does_not_try_again_until_the_project_changes() {
    let folder = tempfile::tempdir().unwrap();
    let (mut window, original_path) = window_with_a_saved_project(&folder);
    window.application_mut().global_settings.autosave = true;
    // The file's place becomes impossible to write to: its folder is replaced by a file.
    let folder_of_project = folder.path().join("blocked");
    std::fs::write(&folder_of_project, "in the way").unwrap();
    window.application_mut().project_file_path = Some(folder_of_project.join("holiday.fastforwardrs.json"));
    window.cut_at_frame(100);
    let context = egui::Context::default();
    let later = Instant::now() + Duration::from_secs(5);

    window
        .application_mut()
        .autosave_when_quiet_for_long_enough(&context, later);
    let (title, _) = window.application().message_showing().expect("the failure is reported");
    assert_eq!(title, "The project could not be saved");
    assert!(window.application().has_unsaved_changes());

    window.application_mut().dialog = None;
    window
        .application_mut()
        .autosave_when_quiet_for_long_enough(&context, later + Duration::from_secs(5));
    assert!(
        window.application().message_showing().is_none(),
        "no second attempt, so no second message"
    );

    // A new change is a new reason to try.
    window.cut_at_frame(200);
    window
        .application_mut()
        .autosave_when_quiet_for_long_enough(&context, Instant::now() + Duration::from_secs(5));
    assert!(
        window.application().message_showing().is_some(),
        "it tried again after the change"
    );
    assert_eq!(
        clip_count_in_file(&original_path),
        1,
        "and the file that was there is untouched"
    );
}

// --- the frame rate of the project ---

fn frame_rate(numerator: u32, denominator: u32) -> FrameRate {
    FrameRate::new(numerator, denominator).unwrap()
}

#[test]
fn the_project_settings_window_has_a_drop_down_list_of_frame_rates_that_shows_the_one_of_the_project() {
    let mut window = Window::showing(project_of(300));
    window.open_from_the_edit_menu("Project settings");
    assert!(window.harness.query_by_label(FRAME_RATE_LABEL).is_some());
    assert!(
        window.harness.query_by_value("30 fps").is_some(),
        "the video runs at 30"
    );
    assert!(
        window.harness.query_by_label(AUTOSAVE).is_some(),
        "next to the settings that follow the global ones"
    );

    window.harness.get_by_value("30 fps").click();
    window.settle();
    for usual in [
        "12 fps",
        "24 fps",
        "23.976 fps",
        "25 fps",
        "29.97 fps",
        "50 fps",
        "59.94 fps",
        "60 fps",
    ] {
        assert!(window.harness.query_by_label(usual).is_some(), "{usual} is offered");
    }
}

#[test]
fn the_global_settings_window_has_no_frame_rate_because_a_project_has_its_own() {
    let mut window = Window::showing(project_of(300));
    window.open_from_the_edit_menu("Global settings");
    assert!(window.harness.query_by_label(FRAME_RATE_LABEL).is_none());
}

#[test]
fn choosing_another_frame_rate_counts_everything_again_and_undoing_it_takes_it_back() {
    let mut window = Window::showing(project_of(300));
    let before = window.application().project().clone();
    window.application_mut().note_everything_is_saved();
    window.open_from_the_edit_menu("Project settings");

    window.choose_from_the_dropdown_showing("30 fps", "24 fps");

    let project = window.application().project();
    assert_eq!(project.frame_rate(), frame_rate(24, 1));
    assert_eq!(project.chosen_frame_rate(), Some(frame_rate(24, 1)));
    assert_eq!(project.total_frame_count(), 240, "ten seconds, at 24 frames a second");
    assert!(
        window.harness.query_by_value("24 fps").is_some(),
        "the list shows what is chosen"
    );
    assert!(window.application().has_unsaved_changes());

    window.do_this(Command::Undo);
    assert_eq!(window.application().project(), &before);
    assert!(window.harness.query_by_value("30 fps").is_some());
    window.do_this(Command::Redo);
    assert_eq!(window.application().project().frame_rate(), frame_rate(24, 1));
}

#[test]
fn the_current_frame_stays_at_the_same_moment_when_the_frame_rate_changes_and_when_that_is_undone() {
    let mut window = Window::showing(project_of(300));
    window.move_current_frame_to(150); // five seconds
    window.open_from_the_edit_menu("Project settings");

    window.choose_from_the_dropdown_showing("30 fps", "15 fps");
    assert_eq!(window.application().project().frame_rate(), frame_rate(15, 1));
    assert_eq!(window.current_frame(), 75, "five seconds at 15 frames a second");

    window.do_this(Command::Undo);
    assert_eq!(window.current_frame(), 150);

    // (The fastest of the usual ones can be reached too: the list is not made to scroll.)
    window.choose_from_the_dropdown_showing("30 fps", "60 fps");
    assert_eq!(window.application().project().frame_rate(), frame_rate(60, 1));
    assert_eq!(window.current_frame(), 300);
}

#[test]
fn a_frame_rate_that_is_not_a_usual_one_is_shown_and_listed_in_its_place_among_the_others() {
    let mut project = Project::empty();
    project
        .import_source(
            SourceMedia::video(
                SourceMediaIdentifier::new(1),
                SourceMediaLink::from_absolute_path(std::path::PathBuf::from("holiday.mp4")),
                SourceVideoProperties {
                    frame_rate: frame_rate(17, 1),
                    total_frame_count: 170,
                    display_width_in_pixels: 640,
                    display_height_in_pixels: 360,
                    has_audio: false,
                },
            ),
            TimelineFrameIndex::new(0),
        )
        .unwrap();
    let mut window = Window::showing(project);
    window.open_from_the_edit_menu("Project settings");
    assert!(window.harness.query_by_value("17 fps").is_some());

    window.harness.get_by_value("17 fps").click();
    window.settle();
    let top_of = |label: &str| window.rectangle_of(label).top();
    assert!(
        top_of("15 fps") < top_of("24 fps") && top_of("12 fps") < top_of("15 fps"),
        "the usual ones, from the slowest"
    );
    let all_items = window.harness.get_all_by_label("17 fps").count();
    assert!(all_items >= 1);
}
