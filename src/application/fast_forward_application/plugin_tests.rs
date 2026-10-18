// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The questions about plugins in the window (phase 7, piece 7.3): what is asked when a project is opened, the severe question that can only be said yes to by holding a button
//! down for five seconds (with the mouse or with Enter, and never with a click or the Space bar), what is remembered and what is not, and that a plugin that was not said yes to is
//! not loaded. Nothing here loads anything: the loader is one that records what it was asked.
//!
//! (The window is drawn sixty times a second in these tests, so five seconds are three hundred frames: `run_for` says it in seconds.)

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Key, PointerButton};
use egui_kittest::kittest::Queryable;

use super::test_window::Window;
use crate::application::dialogs::{Dialog, PLUGIN_DONT_LOAD_LABEL, PLUGIN_LOAD_LABEL};
use crate::plugin_trust::{LibraryLoader, PluginCache, Question, WhyNot, load_library, this_platform};
use crate::project::{EmbeddedBuild, LinkedLibrary, PluginLibrary, PluginLibraryKind, PluginLibrarySource, Project};

/// Loads nothing, and says what it was asked.
#[derive(Default)]
struct RecordingLoader {
    asked: Vec<PathBuf>,
}

impl LibraryLoader for RecordingLoader {
    type Loaded = PathBuf;

    fn load(&mut self, file: &Path) -> Result<PathBuf, String> {
        self.asked.push(file.to_owned());
        Ok(file.to_owned())
    }
}

fn an_embedded_plugin(name: &str, text: &str) -> PluginLibrary {
    PluginLibrary::new(
        name,
        PluginLibraryKind::Ladspa,
        PluginLibrarySource::Embedded(vec![
            EmbeddedBuild::new(&this_platform(), Arc::from(text.as_bytes())).unwrap(),
            EmbeddedBuild::new("some-other-platform", Arc::from(b"another".as_slice())).unwrap(),
        ]),
    )
    .unwrap()
}

fn a_linked_plugin(name: &str, path: &Path) -> PluginLibrary {
    PluginLibrary::new(
        name,
        PluginLibraryKind::Ladspa,
        PluginLibrarySource::Linked(LinkedLibrary {
            absolute_path: path.to_owned(),
            relative_path: None,
        }),
    )
    .unwrap()
}

fn project_with(libraries: Vec<PluginLibrary>) -> Project {
    let mut project = Project::empty();
    for library in libraries {
        project.add_plugin_library(library).unwrap();
    }
    project
}

fn window_showing(project: Project) -> Window {
    let mut window = Window::showing(Project::empty());
    window.show_another_project(project);
    window
}

/// Draws the window for `seconds` seconds, sixty frames to each.
fn run_for(window: &mut Window, seconds: f64) {
    window.harness.run_steps((seconds * 60.0).round() as usize);
}

fn the_question_shown(window: &Window) -> Option<Question> {
    match &window.application().interface.dialog {
        Some(Dialog::AskAboutAPlugin { question }) => Some(question.clone()),
        _ => None,
    }
}

fn is_shown(window: &Window, label: &str) -> bool {
    window.harness.query_by_label(label).is_some()
}

fn press_on(window: &mut Window, label: &str) {
    let at = window.rectangle_of(label).center();
    window.press(at, PointerButton::Primary);
}

fn let_go(window: &mut Window, label: &str) {
    let at = window.rectangle_of(label).center();
    window.release(at, PointerButton::Primary);
}

/// Whether the plugin would be loaded now: the gate asked with a loader that records.
fn would_be_loaded(window: &Window, library_name: &str) -> Result<(), WhyNot> {
    let application = window.application();
    let library = application
        .project()
        .plugin_libraries()
        .get(library_name)
        .unwrap()
        .clone();
    let cache = PluginCache::in_the_temporary_files().unwrap();
    let trust = application.plugin_trust();
    let mut loader = RecordingLoader::default();
    let result = load_library(
        &library,
        None,
        &this_platform(),
        &trust.approvals,
        &trust.session,
        &cache,
        &mut loader,
    );
    assert_eq!(loader.asked.len(), usize::from(result.is_ok()));
    result.map(|_| ())
}

// --- what is asked ---

#[test]
fn a_project_with_a_plugin_inside_it_asks_the_severe_question_and_says_which_program_for_which_computer() {
    let window = window_showing(project_with(vec![an_embedded_plugin("amp_1181.dll", "the build")]));

    let question = the_question_shown(&window).expect("the question is asked");
    assert!(
        matches!(&question, Question::Embedded { library, platform, .. } if library == "amp_1181.dll" && *platform == this_platform())
    );
    assert!(is_shown(&window, "This project contains a program"));
    assert!(
        window
            .harness
            .query_by_label_contains("It runs with all your rights and nothing checks what it does.")
            .is_some()
    );
    assert!(window.harness.query_by_label_contains("amp_1181.dll").is_some());
    assert!(is_shown(&window, PLUGIN_DONT_LOAD_LABEL));
    assert!(is_shown(&window, PLUGIN_LOAD_LABEL));
    assert_eq!(
        would_be_loaded(&window, "amp_1181.dll"),
        Err(WhyNot::NotAsked),
        "nothing is loaded while it is asked"
    );
}

#[test]
fn a_project_with_no_plugin_asks_nothing() {
    let window = window_showing(Project::empty());
    assert!(window.application().interface.dialog.is_none());
}

#[test]
fn saying_no_declines_the_plugin_and_it_is_not_loaded_and_the_window_goes_on() {
    let mut window = window_showing(project_with(vec![an_embedded_plugin("amp_1181.dll", "the build")]));

    let at = window.rectangle_of(PLUGIN_DONT_LOAD_LABEL).center();
    window.click(at);

    assert!(window.application().interface.dialog.is_none());
    assert_eq!(would_be_loaded(&window, "amp_1181.dll"), Err(WhyNot::Declined));
    assert!(
        window
            .application()
            .project()
            .plugin_libraries()
            .get("amp_1181.dll")
            .is_some(),
        "the project keeps it, for the next time"
    );
}

#[test]
fn closing_the_window_of_the_question_with_escape_is_saying_no() {
    let mut window = window_showing(project_with(vec![an_embedded_plugin("amp_1181.dll", "the build")]));

    window.press_key(Key::Escape);
    window.settle();

    assert!(window.application().interface.dialog.is_none());
    assert_eq!(would_be_loaded(&window, "amp_1181.dll"), Err(WhyNot::Declined));
}

// --- holding it down ---

#[test]
fn a_click_on_load_it_does_not_load_it() {
    let mut window = window_showing(project_with(vec![an_embedded_plugin("amp_1181.dll", "the build")]));

    let at = window.rectangle_of(PLUGIN_LOAD_LABEL).center();
    window.click(at);
    window.click(at);
    window.double_click(at);

    assert!(the_question_shown(&window).is_some(), "the question is still asked");
    assert_eq!(would_be_loaded(&window, "amp_1181.dll"), Err(WhyNot::NotAsked));
}

#[test]
fn holding_the_mouse_on_load_it_for_five_seconds_loads_it_and_not_before() {
    let mut window = window_showing(project_with(vec![an_embedded_plugin("amp_1181.dll", "the build")]));

    press_on(&mut window, PLUGIN_LOAD_LABEL);
    run_for(&mut window, 4.0);
    assert!(the_question_shown(&window).is_some(), "four seconds are not five");
    run_for(&mut window, 0.9);
    assert!(the_question_shown(&window).is_some(), "four and nine tenths either");
    run_for(&mut window, 0.5);

    assert!(
        window.application().interface.dialog.is_none(),
        "after five seconds the question is answered"
    );
    assert_eq!(would_be_loaded(&window, "amp_1181.dll"), Ok(()));
}

#[test]
fn letting_go_before_the_five_seconds_starts_it_over() {
    let mut window = window_showing(project_with(vec![an_embedded_plugin("amp_1181.dll", "the build")]));

    press_on(&mut window, PLUGIN_LOAD_LABEL);
    run_for(&mut window, 4.5);
    let_go(&mut window, PLUGIN_LOAD_LABEL);
    run_for(&mut window, 0.5);
    press_on(&mut window, PLUGIN_LOAD_LABEL);
    run_for(&mut window, 4.5);

    assert!(
        the_question_shown(&window).is_some(),
        "four and a half seconds twice are not five seconds in a row"
    );
    assert_eq!(would_be_loaded(&window, "amp_1181.dll"), Err(WhyNot::NotAsked));
    run_for(&mut window, 0.8);
    assert_eq!(
        would_be_loaded(&window, "amp_1181.dll"),
        Ok(()),
        "and five in a row are"
    );
}

#[test]
fn a_press_that_began_somewhere_else_and_moved_onto_load_it_does_not_count() {
    let mut window = window_showing(project_with(vec![an_embedded_plugin("amp_1181.dll", "the build")]));

    // The mouse goes down on the other button and is dragged over Load it, and held there for much longer than it takes.
    press_on(&mut window, PLUGIN_DONT_LOAD_LABEL);
    let onto = window.rectangle_of(PLUGIN_LOAD_LABEL).center();
    window.move_pointer_to(onto);
    run_for(&mut window, 8.0);

    assert!(the_question_shown(&window).is_some());
    assert_eq!(would_be_loaded(&window, "amp_1181.dll"), Err(WhyNot::NotAsked));
}

#[test]
fn holding_enter_for_five_seconds_loads_it_and_a_tap_of_enter_does_not() {
    let mut window = window_showing(project_with(vec![an_embedded_plugin("amp_1181.dll", "the build")]));

    window.harness.key_press(Key::Enter);
    run_for(&mut window, 1.0);
    assert!(the_question_shown(&window).is_some(), "a tap");

    window.harness.key_down(Key::Enter);
    run_for(&mut window, 4.5);
    assert!(the_question_shown(&window).is_some(), "four and a half seconds");
    window.harness.key_up(Key::Enter);
    run_for(&mut window, 0.5);
    window.harness.key_down(Key::Enter);
    run_for(&mut window, 4.5);
    assert!(the_question_shown(&window).is_some(), "letting go started it over");
    run_for(&mut window, 0.8);

    assert!(window.application().interface.dialog.is_none());
    assert_eq!(would_be_loaded(&window, "amp_1181.dll"), Ok(()));
    window.harness.key_up(Key::Enter);
}

#[test]
fn the_space_bar_held_for_as_long_as_you_like_does_nothing() {
    let mut window = window_showing(project_with(vec![an_embedded_plugin("amp_1181.dll", "the build")]));

    window.harness.key_down(Key::Space);
    run_for(&mut window, 12.0);
    window.harness.key_up(Key::Space);
    window.harness.key_press(Key::Space);
    run_for(&mut window, 1.0);

    assert!(the_question_shown(&window).is_some());
    assert_eq!(would_be_loaded(&window, "amp_1181.dll"), Err(WhyNot::NotAsked));
}

// --- what is remembered ---

#[test]
fn a_yes_to_a_plugin_inside_the_project_is_for_this_time_only_and_it_is_asked_again_when_the_project_is_opened_again() {
    let project = project_with(vec![an_embedded_plugin("amp_1181.dll", "the build")]);
    let mut window = window_showing(project.clone());
    press_on(&mut window, PLUGIN_LOAD_LABEL);
    run_for(&mut window, 5.5);
    assert_eq!(would_be_loaded(&window, "amp_1181.dll"), Ok(()));
    assert!(
        window.application().plugin_trust().approvals.is_empty(),
        "nothing is written down"
    );

    window.show_another_project(project);

    assert!(the_question_shown(&window).is_some(), "asked again, as every time");
    assert_eq!(would_be_loaded(&window, "amp_1181.dll"), Err(WhyNot::NotAsked));
}

#[test]
fn a_yes_to_a_plugin_that_is_a_file_is_remembered_for_that_file_and_asked_again_when_the_file_changes() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("tap_reverb.so");
    fs::write(&file, "version one").unwrap();
    let project = project_with(vec![a_linked_plugin("tap_reverb.so", &file)]);
    let mut window = window_showing(project.clone());
    let approvals_file = folder.path().join("plugin_approvals.json");
    window
        .application_mut()
        .keep_plugin_approvals_in(approvals_file.clone());

    let question = the_question_shown(&window);
    assert!(matches!(question, Some(Question::Linked { .. })), "{question:?}");
    assert!(is_shown(&window, "Run this plugin?"));
    let at = window.rectangle_of(PLUGIN_LOAD_LABEL).center();
    window.click(at);
    assert!(
        window.application().interface.dialog.is_none(),
        "the warning is said yes to with a click"
    );
    assert_eq!(would_be_loaded(&window, "tap_reverb.so"), Ok(()));
    assert!(approvals_file.exists(), "and written down");

    window.show_another_project(project.clone());
    assert!(
        window.application().interface.dialog.is_none(),
        "not asked again: the same file"
    );
    assert_eq!(would_be_loaded(&window, "tap_reverb.so"), Ok(()));

    fs::write(&file, "version two, which somebody changed").unwrap();
    window.show_another_project(project);
    assert!(
        the_question_shown(&window).is_some(),
        "the file has changed: asked again"
    );
    assert_eq!(would_be_loaded(&window, "tap_reverb.so"), Err(WhyNot::NotAsked));
}

#[test]
fn saying_no_to_a_plugin_that_is_a_file_remembers_nothing() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("tap_reverb.so");
    fs::write(&file, "bytes").unwrap();
    let mut window = window_showing(project_with(vec![a_linked_plugin("tap_reverb.so", &file)]));

    let at = window.rectangle_of(PLUGIN_DONT_LOAD_LABEL).center();
    window.click(at);

    assert!(window.application().plugin_trust().approvals.is_empty());
    assert_eq!(would_be_loaded(&window, "tap_reverb.so"), Err(WhyNot::Declined));
}

// --- several, and the ones that cannot be used ---

#[test]
fn several_plugins_are_asked_about_one_after_another_in_the_order_of_the_libraries() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("tap_reverb.so");
    fs::write(&file, "bytes").unwrap();
    let mut window = window_showing(project_with(vec![
        an_embedded_plugin("first.dll", "first"),
        a_linked_plugin("second.so", &file),
        an_embedded_plugin("third.dll", "third"),
    ]));

    let mut asked = Vec::new();
    while let Some(question) = the_question_shown(&window) {
        asked.push(question.library().to_owned());
        let at = window.rectangle_of(PLUGIN_DONT_LOAD_LABEL).center();
        window.click(at);
        assert!(asked.len() <= 3);
    }

    assert_eq!(asked, ["first.dll", "second.so", "third.dll"]);
    for name in ["first.dll", "second.so", "third.dll"] {
        assert_eq!(would_be_loaded(&window, name), Err(WhyNot::Declined), "{name}");
    }
}

#[test]
fn a_plugin_with_no_build_for_this_computer_is_said_to_be_unusable_and_with_the_computers_it_has_and_is_not_asked_about()
 {
    let library = PluginLibrary::new(
        "only_elsewhere.dll",
        PluginLibraryKind::Ladspa,
        PluginLibrarySource::Embedded(vec![
            EmbeddedBuild::new("some-other-platform", Arc::from(b"a".as_slice())).unwrap(),
            EmbeddedBuild::new("yet-another", Arc::from(b"b".as_slice())).unwrap(),
        ]),
    )
    .unwrap();
    let window = window_showing(project_with(vec![library]));

    assert!(the_question_shown(&window).is_none());
    match &window.application().interface.dialog {
        Some(Dialog::Message { title, text }) => {
            assert_eq!(title, "A plugin cannot be used");
            assert!(text.contains("only_elsewhere.dll"), "{text}");
            assert!(text.contains("some-other-platform and yet-another"), "{text}");
            assert!(text.contains(&this_platform()), "{text}");
            assert!(text.contains("passes the sound through"), "{text}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_plugin_file_that_is_not_where_it_was_is_said_to_be_unusable_and_not_asked_about() {
    let folder = tempfile::tempdir().unwrap();
    let window = window_showing(project_with(vec![a_linked_plugin(
        "gone.so",
        &folder.path().join("gone.so"),
    )]));

    assert!(the_question_shown(&window).is_none());
    assert!(matches!(
        &window.application().interface.dialog,
        Some(Dialog::Message { title, text }) if title == "A plugin cannot be used" && text.contains("gone.so") && text.contains("is not where the project says it is")
    ));
}
