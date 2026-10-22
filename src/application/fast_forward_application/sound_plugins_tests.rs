// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sound plugins as effects (phase 7, piece 7.5): the menu of the plus button offers the plugins that are installed under the heading of their maker, a plugin put on a clip has
//! the controls its ports make, and one that is not installed is kept and says so; and the plugin manager, where a person says where to look.

use egui_kittest::kittest::{NodeT, Queryable};

use super::keyframe_tests::window_with_a_video;
use super::preview_layer_tests::select;
use super::test_window::Window;
use crate::application::commands::Command;
use crate::application::file_chooser::ScriptedFileChooser;
use crate::application::properties_panel::{ADD_EFFECT_LABEL, NOT_RUNNING_NOTE, VALUES_ARE_KEPT_NOTE};

/// What the box of an effect says when the program does not have the effect.
const NOT_INSTALLED_NOTE: &str = "Not installed: it does nothing here, and its values are kept.";
use crate::application::plugin_manager_window::{
    ADD_FOLDER_LABEL, DEPENDENCY_FOLDERS_HEADING, FOLDERS_HEADING, PluginManagerCommand, REMOVE_LABEL,
    SCAN_AGAIN_LABEL, SEARCH_PARENT_FOLDERS_LABEL, TITLE,
};
use crate::plugin_scan::{LibraryOutcome, LibraryReport, ScanFailure};
use crate::project::{AnimationTarget, ClipId, EffectKind, PropertyValue, set_installed_plugins};
use crate::test_support::{an_installed_sound_plugin, the_installed_plugins_to_myself};

fn window_with_the_video_selected() -> (Window, ClipId) {
    let (mut window, clip) = window_with_a_video();
    select(&mut window, clip);
    (window, clip)
}

/// Opens the menu of the plus button and the smaller menu of the plugins that are LADSPA.
fn open_the_menu(window: &mut Window) {
    open_the_menu_of_the_plus_button(window);
    window.open_the_smaller_menu("LADSPA");
}

/// Opens the menu of the plus button alone.
fn open_the_menu_of_the_plus_button(window: &mut Window) {
    let plus = window.rectangle_of(ADD_EFFECT_LABEL).center();
    window.click(plus);
}

pub(super) fn is_shown(window: &Window, label: &str) -> bool {
    window.harness.query_by_label(label).is_some()
}

fn the_effects(window: &Window, clip: ClipId) -> Vec<EffectKind> {
    window
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .map(|effect| effect.kind())
        .collect()
}

fn a_reverb() -> crate::project::InstalledPlugin {
    an_installed_sound_plugin(
        "ladspa:tap_reverb:tap_reverb",
        "Tom Szilagyi",
        &[
            ("decay time", 2500.0, Some((0.0, 10_000.0))),
            ("dry level", -2.0, Some((-70.0, 10.0))),
        ],
    )
}

// --- the menu ---

#[test]
fn the_menu_of_the_plus_button_offers_an_installed_plugin_under_the_heading_of_its_maker_and_not_one_that_is_gone() {
    let _alone = the_installed_plugins_to_myself();
    let mut plugin = a_reverb();
    plugin.descriptor.name = "tap reverb".into();
    set_installed_plugins(vec![plugin]);
    let (mut window, _) = window_with_the_video_selected();

    open_the_menu_of_the_plus_button(&mut window);
    assert!(is_shown(&window, "LADSPA \u{23f5}"), "a smaller menu for the plugins");
    assert!(
        window.rectangle_of("Built-in \u{23f5}").top() < window.rectangle_of("LADSPA \u{23f5}").top(),
        "the one for the effects of the program comes first"
    );
    window.open_the_smaller_menu("LADSPA");
    assert!(is_shown(&window, "Tom Szilagyi"), "the heading is the maker");
    assert!(is_shown(&window, "Tap reverb"));
    assert!(window.rectangle_of("Tom Szilagyi").top() < window.rectangle_of("Tap reverb").top());
    assert!(
        !is_shown(&window, "Hue shift"),
        "the effects of the program are in their own smaller menu"
    );

    // (The plugin is taken out of the list: the menu, opened again, does not have it, and has no smaller menu for plugins.)
    window.press_key(egui::Key::Escape);
    window.settle();
    set_installed_plugins(Vec::new());
    window.settle();
    open_the_menu_of_the_plus_button(&mut window);
    assert!(is_shown(&window, "Built-in \u{23f5}"));
    assert!(!is_shown(&window, "LADSPA \u{23f5}"));
    assert!(!is_shown(&window, "Tap reverb"));
    assert!(!is_shown(&window, "Tom Szilagyi"));
}

#[test]
fn a_smaller_menu_that_is_taller_than_the_window_has_a_scroll_bar_and_a_shorter_one_is_not_cut() {
    let _alone = the_installed_plugins_to_myself();
    let many: Vec<_> = (0..60)
        .map(|number| {
            let mut plugin = an_installed_sound_plugin(&format!("ladspa:many:plugin{number:03}"), "Many", &[]);
            plugin.descriptor.name = format!("plugin {number:03}").into();
            plugin
        })
        .collect();
    set_installed_plugins(many);
    let (mut window, _) = window_with_the_video_selected();
    open_the_menu_of_the_plus_button(&mut window);
    window.open_the_smaller_menu("LADSPA");

    let window_bottom = window.harness.ctx.content_rect().bottom();
    let first = window.rectangle_of("Plugin 000");
    assert!(
        first.top() >= 0.0 && first.bottom() <= window_bottom,
        "the first plugin is in the window: {first:?}"
    );
    let last_is_in_the_window = |window: &Window| {
        window
            .harness
            .query_by_label("Plugin 059")
            .is_some_and(|last| last.rect().bottom() <= window_bottom && last.rect().top() >= 0.0)
    };
    assert!(
        !last_is_in_the_window(&window),
        "the last one is out of the window, to be scrolled to"
    );

    for _ in 0..10 {
        window.scroll_the_wheel(first.center(), egui::vec2(0.0, -600.0), egui::Modifiers::NONE);
    }

    assert!(
        last_is_in_the_window(&window),
        "scrolling brings the last plugin into the window"
    );
}

#[test]
fn the_search_of_the_menu_finds_a_plugin_by_its_name_and_by_its_maker() {
    let _alone = the_installed_plugins_to_myself();
    let mut plugin = a_reverb();
    plugin.descriptor.name = "tap reverb".into();
    set_installed_plugins(vec![plugin]);
    for typed in ["reverb", "szilagyi"] {
        let (mut window, _) = window_with_the_video_selected();
        open_the_menu(&mut window);
        window
            .harness
            .query_all_by_role(egui::accesskit::Role::TextInput)
            .find(|field| field.accesskit_node().placeholder() == Some("Search effects"))
            .expect("the search field")
            .type_text(typed);
        window.harness.run();
        assert!(is_shown(&window, "Tap reverb"), "found by {typed:?}");
        assert!(!is_shown(&window, "Blur"), "and the others are not shown");
    }
}

#[test]
fn a_plugin_chosen_in_the_menu_is_put_on_the_clip_with_the_values_its_ports_begin_with_and_its_controls_are_shown() {
    let _alone = the_installed_plugins_to_myself();
    let mut plugin = a_reverb();
    plugin.descriptor.name = "tap reverb".into();
    set_installed_plugins(vec![plugin.clone()]);
    let (mut window, clip) = window_with_the_video_selected();

    open_the_menu(&mut window);
    window.harness.get_by_label("Tap reverb").click();
    window.settle();

    let kinds = the_effects(&window, clip);
    assert_eq!(kinds.len(), 1);
    assert_eq!(kinds[0].key(), "ladspa:tap_reverb:tap_reverb");
    assert!(kinds[0].is_installed());
    let values: Vec<PropertyValue> = window
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .flat_map(|effect| effect.values().to_vec())
        .collect();
    assert_eq!(values, [PropertyValue::Number(2500.0), PropertyValue::Number(-2.0)]);
    assert!(is_shown(&window, "Tap reverb"), "the box of the effect is in the panel");
    assert!(is_shown(&window, "Decay time"), "a control for each value");
    assert!(is_shown(&window, "Dry level"));
    assert!(!is_shown(&window, NOT_INSTALLED_NOTE));
}

/// Whether the window shows a label that has `text` in it.
pub(super) fn shows_containing(window: &Window, text: &str) -> bool {
    window.harness.query_all_by_label_contains(text).next().is_some()
}

/// A window with the reverb on the clip, and the reverb then taken out of the list of what is installed: the effect is a placeholder.
fn window_with_a_reverb_that_is_gone() -> (Window, ClipId, crate::project::InstalledPlugin) {
    let mut plugin = a_reverb();
    plugin.descriptor.name = "tap reverb".into();
    set_installed_plugins(vec![plugin.clone()]);
    let (mut window, clip) = window_with_the_video_selected();
    open_the_menu(&mut window);
    window.harness.get_by_label("Tap reverb").click();
    window.settle();
    set_installed_plugins(Vec::new());
    window.settle();
    (window, clip, plugin)
}

/// Says what the scan found: the libraries and the plugins that cannot be effects, and that there is a scan program and the scan is done.
fn the_scan_found(window: &mut Window, libraries: Vec<LibraryReport>, not_used: Vec<crate::plugin_scan::NotUsed>) {
    window
        .application_mut()
        .use_this_plugin_scanner(std::path::PathBuf::from("a scan program"), None, false);
    let search = &mut window.application_mut().environment.plugin_search;
    search.has_scanned = true;
    search.result.libraries = libraries;
    search.result.not_used = not_used;
    window.settle();
}

fn a_failed_library(name: &str, failure: ScanFailure) -> LibraryReport {
    LibraryReport {
        path: std::path::PathBuf::from("/plugins").join(name),
        outcome: LibraryOutcome::Failed(failure),
        from_cache: false,
    }
}

#[test]
fn an_effect_of_a_plugin_that_is_gone_is_kept_with_its_values_and_is_a_placeholder_that_says_why_until_the_plugin_is_back()
 {
    let _alone = the_installed_plugins_to_myself();
    let mut plugin = a_reverb();
    plugin.descriptor.name = "tap reverb".into();
    set_installed_plugins(vec![plugin.clone()]);
    let (mut window, clip) = window_with_the_video_selected();
    open_the_menu(&mut window);
    window.harness.get_by_label("Tap reverb").click();
    window.settle();
    assert!(!shows_containing(&window, NOT_RUNNING_NOTE) || shows_containing(&window, "trust the folder"));

    set_installed_plugins(Vec::new());
    window.settle();
    assert!(
        shows_containing(&window, NOT_RUNNING_NOTE),
        "the plugin is gone, and the effect says it is not running"
    );
    assert!(shows_containing(&window, VALUES_ARE_KEPT_NOTE));
    assert!(
        shows_containing(&window, "program that scans plugin libraries was not found"),
        "and why: nothing is scanned here"
    );
    assert!(
        is_shown(&window, "Decay time"),
        "the controls are there, and the values"
    );
    assert_eq!(the_effects(&window, clip).len(), 1);

    set_installed_plugins(vec![plugin]);
    window.settle();
    assert!(
        !shows_containing(&window, "The program that scans plugin libraries was not found"),
        "it was found again"
    );
}

#[test]
fn the_placeholder_of_a_plugin_that_is_not_installed_says_the_scan_is_not_done_while_it_is_not() {
    let _alone = the_installed_plugins_to_myself();
    let (mut window, _, _) = window_with_a_reverb_that_is_gone();
    window
        .application_mut()
        .use_this_plugin_scanner(std::path::PathBuf::from("a scan program"), None, false);
    window.settle();

    assert!(shows_containing(&window, "The plugins are still being looked for."));

    the_scan_found(&mut window, Vec::new(), Vec::new());
    assert!(!shows_containing(&window, "still being looked for"));
    assert!(
        shows_containing(
            &window,
            "No library called tap_reverb was found in the folders that are looked in"
        ),
        "the scan is done and found nothing of it"
    );
}

#[test]
fn the_placeholder_says_when_the_library_of_the_plugin_failed_and_why() {
    let _alone = the_installed_plugins_to_myself();
    let (mut window, _, _) = window_with_a_reverb_that_is_gone();

    the_scan_found(
        &mut window,
        vec![a_failed_library(
            "tap_reverb.so",
            ScanFailure::WasInUseWhenTheProgramEndedBadly { others: 0 },
        )],
        Vec::new(),
    );
    assert!(shows_containing(&window, "Its library tap_reverb.so failed"));
    assert!(
        shows_containing(&window, "ended badly"),
        "a plugin that ended the program badly says so"
    );

    the_scan_found(
        &mut window,
        vec![a_failed_library(
            "tap_reverb.so",
            ScanFailure::Crashed {
                how: "signal 11".to_owned(),
                during: Some("making plugin number 0".to_owned()),
            },
        )],
        Vec::new(),
    );
    assert!(shows_containing(&window, "it crashed the scanner (signal 11)"));
    // A library of another name is not this plugin's.
    the_scan_found(
        &mut window,
        vec![a_failed_library(
            "something_else.so",
            ScanFailure::GaveNoAnswer { during: None },
        )],
        Vec::new(),
    );
    assert!(!shows_containing(&window, "Its library"));
    assert!(shows_containing(&window, "No library called tap_reverb"));
}

#[test]
fn the_placeholder_says_when_the_library_is_there_and_has_no_such_plugin_or_the_plugin_cannot_be_an_effect() {
    let _alone = the_installed_plugins_to_myself();
    let (mut window, _, _) = window_with_a_reverb_that_is_gone();
    let there = LibraryReport {
        path: std::path::PathBuf::from("/plugins/tap_reverb.so"),
        outcome: LibraryOutcome::Scanned(crate::plugin_scan::LibraryScan::default()),
        from_cache: false,
    };

    the_scan_found(&mut window, vec![there.clone()], Vec::new());
    assert!(shows_containing(
        &window,
        "The library tap_reverb.so is there, and has no plugin called tap_reverb"
    ));

    the_scan_found(
        &mut window,
        vec![there],
        vec![crate::plugin_scan::NotUsed {
            library: std::path::PathBuf::from("/plugins/tap_reverb.so"),
            label: "tap_reverb".to_owned(),
            name: "Tap reverb".to_owned(),
            reason: "it has no audio input".to_owned(),
        }],
    );
    assert!(shows_containing(
        &window,
        "was found, and cannot be put on a clip: it has no audio input"
    ));
}

#[test]
fn the_placeholder_of_a_clap_plugin_that_is_not_installed_names_the_libraries_that_failed() {
    let _alone = the_installed_plugins_to_myself();
    let mut plugin = a_reverb();
    plugin.descriptor.key = "clap:com.example.reverb".into();
    plugin.descriptor.name = "example reverb".into();
    set_installed_plugins(vec![plugin]);
    let (mut window, _) = window_with_the_video_selected();
    open_the_menu_of_the_plus_button(&mut window);
    window.open_the_smaller_menu("CLAP");
    window.harness.get_by_label("Example reverb").click();
    window.settle();
    set_installed_plugins(Vec::new());
    window.settle();

    the_scan_found(&mut window, Vec::new(), Vec::new());
    assert!(shows_containing(
        &window,
        "No plugin with this id was found in the folders that are looked in"
    ));

    the_scan_found(
        &mut window,
        vec![
            a_failed_library(
                "one.clap",
                ScanFailure::TookTooLong {
                    seconds: 5,
                    during: None,
                },
            ),
            a_failed_library("two.clap", ScanFailure::NotAClapLibrary("no factory".to_owned())),
            a_failed_library("not_a_clap_one.so", ScanFailure::GaveNoAnswer { during: None }),
        ],
        Vec::new(),
    );
    assert!(shows_containing(&window, "some libraries failed in the last scan"));
    assert!(shows_containing(&window, "one.clap (it did not finish in 5 seconds"));
    assert!(
        !shows_containing(&window, "not_a_clap_one.so"),
        "only libraries of the kind of the plugin"
    );
}

#[test]
fn the_placeholder_of_an_installed_plugin_says_when_its_folder_is_not_trusted_or_it_went_wrong() {
    let _alone = the_installed_plugins_to_myself();
    let mut plugin = a_reverb();
    plugin.descriptor.name = "tap reverb".into();
    set_installed_plugins(vec![plugin.clone()]);
    let (mut window, _) = window_with_the_video_selected();
    open_the_menu(&mut window);
    window.harness.get_by_label("Tap reverb").click();
    window.settle();
    let folder = crate::plugin_trust::folder_of(&plugin.library);

    assert!(
        shows_containing(&window, "You have not said whether to trust the folder"),
        "nobody has answered for the folder"
    );

    window.application().environment.plugin_host.decline_folder(&folder);
    window.settle();
    assert!(shows_containing(&window, "You did not trust the folder"));
    assert!(!shows_containing(&window, "have not said"));

    window.application().environment.plugin_host.approve_folder(&folder);
    window.settle();
    assert!(
        !shows_containing(&window, NOT_RUNNING_NOTE),
        "trusted, and nothing went wrong"
    );

    window.application().environment.plugin_host.note_a_problem_with(
        "ladspa:tap_reverb:tap_reverb",
        "The sound plugin \u{ab}tap reverb\u{bb} could not be run (it said no).",
    );
    window.settle();
    assert!(
        shows_containing(&window, "could not be run (it said no)"),
        "what went wrong when it was wanted"
    );
    assert!(shows_containing(&window, NOT_RUNNING_NOTE));
}

// --- the window ---

pub(super) fn open_the_window(window: &mut Window) {
    window.harness.get_by_label("Edit").click();
    window.settle();
    window.harness.get_by_label("Plugin manager...").click();
    window.settle();
}

#[test]
fn the_plugin_manager_is_opened_from_the_edit_menu_and_shows_where_plugins_are_looked_for() {
    let mut window = Window::showing(super::test_window::project_of(100));
    assert!(!is_shown(&window, TITLE));
    open_the_window(&mut window);
    assert!(window.application().plugin_manager_is_open());
    assert!(is_shown(&window, TITLE));
    assert!(is_shown(&window, FOLDERS_HEADING));
    assert!(is_shown(&window, DEPENDENCY_FOLDERS_HEADING));
    assert!(is_shown(&window, SCAN_AGAIN_LABEL));
    assert!(is_shown(&window, SEARCH_PARENT_FOLDERS_LABEL));
    assert!(
        window.is_checked(SEARCH_PARENT_FOLDERS_LABEL),
        "the folders above a plugin library are looked in unless a person says not"
    );
    assert_eq!(
        window.how_many_are_called(ADD_FOLDER_LABEL),
        2,
        "one for the plugins, one for the libraries they need"
    );
}

#[test]
fn a_folder_that_is_chosen_is_added_and_kept_in_the_file_and_removing_it_takes_it_out_again() {
    // (Adding a folder looks in it for shaders too, which installs what it finds: the list of what is installed is held.)
    let _alone = the_installed_plugins_to_myself();
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("plugin_search.json");
    let chooser = ScriptedFileChooser::default();
    let mut window = Window::showing(super::test_window::project_of(100));
    window.use_file_chooser(&chooser);
    window.application_mut().keep_plugin_search_settings_in(file.clone());
    open_the_window(&mut window);

    chooser.will_answer(Some(folder.path().join("plugins")));
    window
        .harness
        .get_all_by_label(ADD_FOLDER_LABEL)
        .next()
        .unwrap()
        .click();
    window.settle();
    assert_eq!(chooser.questions_asked().len(), 1);
    assert_eq!(
        chooser.questions_asked()[0].title,
        "Choose a folder with plugins or shaders"
    );
    assert_eq!(
        window.application().plugin_search_settings().folders,
        [folder.path().join("plugins")]
    );
    assert!(is_shown(&window, &folder.path().join("plugins").display().to_string()));
    let (read_back, problem) = crate::plugin_scan::PluginSearchSettings::read(&file);
    assert_eq!(problem, None);
    assert_eq!(read_back.folders, [folder.path().join("plugins")], "kept in the file");

    chooser.will_answer(Some(folder.path().join("libraries")));
    window
        .harness
        .get_all_by_label(ADD_FOLDER_LABEL)
        .nth(1)
        .unwrap()
        .click();
    window.settle();
    assert_eq!(
        chooser.questions_asked()[1].title,
        "Choose a folder with libraries that plugins need"
    );
    assert_eq!(
        window.application().plugin_search_settings().dependency_folders,
        [folder.path().join("libraries")]
    );

    // A person who changes their mind at the file chooser changes nothing.
    chooser.will_answer(None);
    window
        .harness
        .get_all_by_label(ADD_FOLDER_LABEL)
        .next()
        .unwrap()
        .click();
    window.settle();
    assert_eq!(window.application().plugin_search_settings().folders.len(), 1);

    window.harness.get_all_by_label(REMOVE_LABEL).next().unwrap().click();
    window.settle();
    assert!(window.application().plugin_search_settings().folders.is_empty());
    assert_eq!(
        window.application().plugin_search_settings().dependency_folders.len(),
        1
    );
    assert_eq!(
        crate::plugin_scan::PluginSearchSettings::read(&file).0.folders,
        Vec::<std::path::PathBuf>::new()
    );
    window.harness.get_all_by_label(REMOVE_LABEL).next().unwrap().click();
    window.settle();
    assert!(
        window
            .application()
            .plugin_search_settings()
            .dependency_folders
            .is_empty()
    );
}

#[test]
fn the_choice_to_look_in_the_folders_above_a_plugin_library_is_kept_and_can_be_turned_off_and_on() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("plugin_search.json");
    let mut window = Window::showing(super::test_window::project_of(100));
    window.application_mut().keep_plugin_search_settings_in(file.clone());
    open_the_window(&mut window);
    window.harness.get_by_label(SEARCH_PARENT_FOLDERS_LABEL).click();
    window.settle();
    assert!(!window.application().plugin_search_settings().search_parent_folders);
    assert!(!window.is_checked(SEARCH_PARENT_FOLDERS_LABEL));
    assert!(
        !crate::plugin_scan::PluginSearchSettings::read(&file)
            .0
            .search_parent_folders
    );
    window.harness.get_by_label(SEARCH_PARENT_FOLDERS_LABEL).click();
    window.settle();
    assert!(window.application().plugin_search_settings().search_parent_folders);
    assert!(
        crate::plugin_scan::PluginSearchSettings::read(&file)
            .0
            .search_parent_folders
    );
}

#[test]
fn a_file_of_places_that_cannot_be_read_is_said_and_the_usual_places_are_used() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("plugin_search.json");
    std::fs::write(&file, "this is not json").unwrap();
    let mut window = Window::showing(super::test_window::project_of(100));
    window.application_mut().keep_plugin_search_settings_in(file);
    window.settle();
    let (title, _) = window.application().message_showing().expect("a message");
    assert_eq!(title, "Where to look for sound plugins could not be read");
    assert_eq!(
        *window.application().plugin_search_settings(),
        crate::plugin_scan::PluginSearchSettings::default()
    );
}

#[test]
fn the_window_says_what_the_scan_found_the_libraries_that_failed_with_the_reason_and_the_plugins_that_cannot_be_effects()
 {
    let _alone = the_installed_plugins_to_myself();
    let mut window = Window::showing(super::test_window::project_of(100));
    open_the_window(&mut window);
    let library = |name: &str, outcome: LibraryOutcome| LibraryReport {
        path: std::path::PathBuf::from(format!("/plugins/{name}")),
        outcome,
        from_cache: false,
    };
    window.application_mut().pretend_the_scan_found(vec![
        library(
            "good.so",
            LibraryOutcome::Scanned(crate::plugin_scan::LibraryScan::default()),
        ),
        library(
            "bad.so",
            LibraryOutcome::Failed(ScanFailure::CouldNotBeLoaded(
                "The specified module could not be found".to_owned(),
            )),
        ),
    ]);
    window.settle();
    assert!(is_shown(&window, "good.so: 0 plugins"));
    assert!(is_shown(
        &window,
        "bad.so: it could not be loaded: The specified module could not be found"
    ));
    assert!(is_shown(
        &window,
        "2 libraries found, 1 of them failed; 0 plugins can be put on a clip."
    ));
}

#[test]
fn asking_to_scan_again_with_no_scan_program_does_nothing_and_does_not_fail() {
    let mut window = Window::showing(super::test_window::project_of(100));
    open_the_window(&mut window);
    window.do_this(Command::PluginManager(PluginManagerCommand::ScanAgain));
    assert!(!window.application().is_scanning_plugins());
}

// --- the controls that the ports make ---

fn a_filter_with_a_frequency_and_a_whole_number() -> crate::project::InstalledPlugin {
    use crate::project::{ParameterControl, ValueScale};
    let mut plugin = an_installed_sound_plugin(
        "ladspa:filters:low_pass",
        "Maker",
        &[
            ("cutoff", 1000.0, Some((20.0, 20_000.0))),
            ("poles", 2.0, Some((1.0, 8.0))),
        ],
    );
    let values = plugin.descriptor.parameters.to_mut();
    values[0].scale = ValueScale::Logarithmic;
    values[1].control = ParameterControl::Integer;
    plugin.descriptor.name = "low pass".into();
    plugin
}

#[test]
fn a_frequency_that_the_plugin_says_is_logarithmic_has_a_slider_whose_middle_is_the_geometric_middle_of_its_range() {
    let _alone = the_installed_plugins_to_myself();
    set_installed_plugins(vec![a_filter_with_a_frequency_and_a_whole_number()]);
    let (mut window, clip) = window_with_the_video_selected();
    open_the_menu(&mut window);
    window.harness.get_by_label("Low pass").click();
    window.settle();

    let slider = window
        .harness
        .query_all_by_role(egui::accesskit::Role::Slider)
        .find(|slider| slider.accesskit_node().label().as_deref() == Some("Cutoff"))
        .expect("a slider for the frequency")
        .rect();
    window.click(slider.center());
    window.settle();
    let value = |window: &Window, slot: usize| {
        window
            .application()
            .project()
            .effects_of(AnimationTarget::Clip(clip))
            .unwrap()
            .iter()
            .next()
            .unwrap()
            .values()[slot]
            .clone()
    };
    let PropertyValue::Number(middle) = value(&window, 0) else {
        panic!("a number");
    };
    // The middle of a slider that is logarithmic from 20 to 20000 is 632 (the middle of one that is straight would be 10010); the knob has a width, so it is near.
    assert!(middle > 400.0 && middle < 1000.0, "{middle}");
}

#[test]
fn a_whole_number_that_the_plugin_says_is_one_is_set_as_a_whole_number() {
    let _alone = the_installed_plugins_to_myself();
    set_installed_plugins(vec![a_filter_with_a_frequency_and_a_whole_number()]);
    let (mut window, clip) = window_with_the_video_selected();
    open_the_menu(&mut window);
    window.harness.get_by_label("Low pass").click();
    window.settle();
    let slider = window
        .harness
        .query_all_by_role(egui::accesskit::Role::Slider)
        .find(|slider| slider.accesskit_node().label().as_deref() == Some("Poles"))
        .expect("a slider for the number of poles")
        .rect();
    window.click(slider.center());
    window.settle();
    let poles = window
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .values()[1]
        .clone();
    let PropertyValue::Number(poles) = poles else {
        panic!("a number");
    };
    assert_eq!(poles, poles.round(), "{poles} is a whole number");
    assert!((1.0..=8.0).contains(&poles));
}

// --- the folders: what is added twice, and which one is removed ---

#[test]
fn a_folder_that_is_chosen_twice_is_in_the_list_once() {
    // (Adding a folder looks in it for shaders too, which installs what it finds: the list of what is installed is held.)
    let _alone = the_installed_plugins_to_myself();
    let chooser = ScriptedFileChooser::default();
    let mut window = Window::showing(super::test_window::project_of(100));
    window.use_file_chooser(&chooser);
    open_the_window(&mut window);
    for _ in 0..2 {
        chooser.will_answer(Some(std::path::PathBuf::from("/same/folder")));
        window
            .harness
            .get_all_by_label(ADD_FOLDER_LABEL)
            .next()
            .unwrap()
            .click();
        window.settle();
    }
    assert_eq!(window.application().plugin_search_settings().folders.len(), 1);
    for _ in 0..2 {
        chooser.will_answer(Some(std::path::PathBuf::from("/same/libraries")));
        window
            .harness
            .get_all_by_label(ADD_FOLDER_LABEL)
            .nth(1)
            .unwrap()
            .click();
        window.settle();
    }
    assert_eq!(
        window.application().plugin_search_settings().dependency_folders.len(),
        1
    );
}

#[test]
fn the_folder_whose_button_is_pressed_is_the_one_that_is_removed() {
    // (Adding a folder looks in it for shaders too, which installs what it finds: the list of what is installed is held.)
    let _alone = the_installed_plugins_to_myself();
    let mut window = Window::showing(super::test_window::project_of(100));
    for name in ["/first", "/second", "/third"] {
        window
            .application_mut()
            .add_plugin_folder(std::path::PathBuf::from(name));
    }
    for name in ["/libraries/one", "/libraries/two"] {
        window
            .application_mut()
            .add_dependency_folder(std::path::PathBuf::from(name));
    }
    open_the_window(&mut window);
    // The buttons are in the order of the folders, those of the folders for plugins first.
    window.harness.get_all_by_label(REMOVE_LABEL).nth(1).unwrap().click();
    window.settle();
    assert_eq!(
        window.application().plugin_search_settings().folders,
        [std::path::PathBuf::from("/first"), std::path::PathBuf::from("/third")]
    );
    window.harness.get_all_by_label(REMOVE_LABEL).nth(3).unwrap().click();
    window.settle();
    assert_eq!(
        window.application().plugin_search_settings().dependency_folders,
        [std::path::PathBuf::from("/libraries/one")],
        "the second folder with libraries was removed, and not the first"
    );
    assert_eq!(window.application().plugin_search_settings().folders.len(), 2);
}

// --- the window by itself ---

/// A scan that found no shader.
static NO_SHADERS_FOUND: crate::shader_files::ShaderScan = crate::shader_files::ShaderScan {
    shaders: Vec::new(),
    problems: Vec::new(),
};

fn the_view<'a>(
    settings: &'a crate::plugin_scan::PluginSearchSettings,
    is_scanning: bool,
    has_scanned: bool,
) -> crate::application::plugin_manager_window::PluginManagerView<'a> {
    crate::application::plugin_manager_window::PluginManagerView {
        settings,
        other_folders: &[],
        bundled_folder: None,
        fetched_folder: None,
        fetched_shaders_folder: None,
        tab: crate::application::plugin_manager_window::PluginManagerTab::Plugins,
        bundles: crate::application::plugin_manager_bundles::BundlesView {
            kind: crate::application::plugin_manager_bundles::BundleKind::SoundPlugins,
            rows: &[],
            has_builds: true,
            total_bytes: 0,
            hosts: "",
            folder: None,
            state: crate::application::plugin_manager_bundles::BundleState::Idle,
        },
        shader_bundles: crate::application::plugin_manager_bundles::BundlesView {
            kind: crate::application::plugin_manager_bundles::BundleKind::Shaders,
            rows: &[],
            has_builds: false,
            total_bytes: 0,
            hosts: "",
            folder: None,
            state: crate::application::plugin_manager_bundles::BundleState::Idle,
        },
        is_scanning,
        has_scanned,
        libraries: &[],
        not_used: &[],
        effect_count: 1,
        problems: &[],
        shaders: &NO_SHADERS_FOUND,
    }
}

fn what_the_window_asks_for_when_the_button_is_pressed(is_scanning: bool) -> (Vec<Command>, bool, bool) {
    let settings = crate::plugin_scan::PluginSearchSettings::default();
    let mut open = true;
    let commands = std::cell::RefCell::new(Vec::new());
    let mut harness = egui_kittest::Harness::new_ui(|ui| {
        crate::application::plugin_manager_window::show(
            ui.ctx(),
            &mut open,
            &the_view(&settings, is_scanning, !is_scanning),
            &mut commands.borrow_mut(),
        );
    });
    harness.run();
    let disabled = harness.get_by_label(SCAN_AGAIN_LABEL).accesskit_node().is_disabled();
    let says_scanning = harness.query_by_label("Scanning...").is_some();
    harness.get_by_label(SCAN_AGAIN_LABEL).click();
    harness.run();
    let asked = commands.borrow().clone();
    (asked, disabled, says_scanning)
}

#[test]
fn scan_again_can_be_pressed_when_no_scan_is_going_on_and_not_while_one_is() {
    let (asked, disabled, says_scanning) = what_the_window_asks_for_when_the_button_is_pressed(false);
    assert_eq!(asked, [Command::PluginManager(PluginManagerCommand::ScanAgain)]);
    assert!(!disabled && !says_scanning);
    let (asked, disabled, says_scanning) = what_the_window_asks_for_when_the_button_is_pressed(true);
    assert_eq!(
        asked,
        Vec::<Command>::new(),
        "a button that is greyed out asks for nothing"
    );
    assert!(disabled && says_scanning);
}

#[test]
fn the_window_tells_of_a_clap_plugin_whose_start_depends_on_memory_it_did_not_write_and_of_no_other() {
    use clap_host::start_check::StartCheck;
    let _alone = the_installed_plugins_to_myself();
    let mut window = Window::showing(super::test_window::project_of(100));
    open_the_window(&mut window);
    let plugin = |name: &str, start: StartCheck| clap_host::scan::ScannedPlugin {
        description: clap_host::PluginDescription {
            id: format!("com.example.{name}"),
            name: name.to_owned(),
            ..clap_host::PluginDescription::default()
        },
        audio_inputs: Vec::new(),
        audio_outputs: Vec::new(),
        parameters: Vec::new(),
        latency: 0,
        tail: None,
        has_state: false,
        start,
        problem: None,
    };
    window.application_mut().pretend_the_scan_found(vec![LibraryReport {
        path: std::path::PathBuf::from("/plugins/reverbs.clap"),
        outcome: LibraryOutcome::ScannedClap(clap_host::scan::LibraryScan {
            plugins: vec![
                plugin(
                    "Roarer",
                    StartCheck::Unreliable {
                        filled_with: 0xFF,
                        relative_difference: 1.5,
                    },
                ),
                plugin("Steady One", StartCheck::Steady),
            ],
            problems: Vec::new(),
        }),
        from_cache: false,
    }]);
    window.settle();

    assert!(is_shown(&window, "reverbs.clap: 2 plugins"));
    let warning = StartCheck::Unreliable {
        filled_with: 0xFF,
        relative_difference: 1.5,
    }
    .warning()
    .unwrap();
    assert!(is_shown(
        &window,
        &format!("Roarer is unreliable at its start: {warning}.")
    ));
    assert!(!is_shown(&window, "Steady One is unreliable"));
}

#[test]
fn the_placeholder_of_a_plugin_in_the_bundled_folder_does_not_ask_for_trust_and_one_elsewhere_does() {
    let _alone = the_installed_plugins_to_myself();
    let mut plugin = a_reverb();
    plugin.descriptor.name = "tap reverb".into();
    set_installed_plugins(vec![plugin.clone()]);
    let (mut window, _) = window_with_the_video_selected();
    open_the_menu(&mut window);
    window.harness.get_by_label("Tap reverb").click();
    window.settle();
    assert!(shows_containing(
        &window,
        "You have not said whether to trust the folder"
    ));

    window
        .application_mut()
        .trust_the_plugins_bundled_in(&crate::plugin_trust::folder_of(&plugin.library));
    window.settle();

    assert!(
        !shows_containing(&window, "You have not said whether to trust the folder"),
        "the plugins that came with the program are trusted"
    );
    assert!(!shows_containing(&window, NOT_RUNNING_NOTE));
}

#[test]
fn the_window_calls_the_bundled_folder_bundled_and_the_others_usual() {
    let settings = crate::plugin_scan::PluginSearchSettings::default();
    let mut open = true;
    let others = [
        std::path::PathBuf::from("/usr/lib/clap"),
        std::path::PathBuf::from("/the/program/plugins"),
    ];
    let mut commands = Vec::new();
    let mut harness = egui_kittest::Harness::new_ui(|ui| {
        let mut view = the_view(&settings, false, true);
        view.other_folders = &others;
        view.bundled_folder = Some(std::path::Path::new("/the/program/plugins"));
        crate::application::plugin_manager_window::show(ui.ctx(), &mut open, &view, &mut commands);
    });
    harness.run();

    let shown = |text: String| harness.query_by_label(&text).is_some();
    assert!(shown(format!(
        "{} (usual)",
        std::path::Path::new("/usr/lib/clap").display()
    )));
    assert!(shown(format!(
        "{} ({})",
        std::path::Path::new("/the/program/plugins").display(),
        crate::application::plugin_manager_window::BUNDLED_LABEL
    )));
    assert!(!shown(format!(
        "{} (usual)",
        std::path::Path::new("/the/program/plugins").display()
    )));
}
