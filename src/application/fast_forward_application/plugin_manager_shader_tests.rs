// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugin manager and shaders (phase 6.8): the shaders in the folders it looks in are found by the scan, listed with whether they can be used and why not, installed (the filters in
//! the menu of the plus button under the heading *ISF*, the generators in the New menu of a track), and what is put in a project from them stays there when the folder is gone.

use std::path::Path;
use std::time::{Duration, Instant};

use egui_kittest::kittest::Queryable;

use super::keyframe_tests::window_with_a_video;
use super::preview_layer_tests::select;
use super::test_window::{Window, project_of};
use super::undo_tests::undo;
use crate::application::commands::Command;
use crate::application::plugin_manager_window::{
    PluginManagerCommand, heading_of_the_shaders_that_can_be_used, heading_of_the_shaders_that_cannot_be_used,
};
use crate::application::properties_panel::ADD_EFFECT_LABEL;
use crate::project::{AnimationTarget, ClipId, ClipKind, EffectKind, TimelineFrameIndex};
use crate::test_support::the_installed_plugins_to_myself;

const VIGNETTE: &str = include_str!("../../isf/fixtures/vignette.fs");
const PLASMA: &str = include_str!("../../isf/fixtures/plasma.fs");
const WIPE: &str = include_str!("../../isf/fixtures/wipe_transition.fs");
const BAD: &str = include_str!("../../isf/fixtures/bad_syntax.fs");

fn write(folder: &Path, name: &str, text: &str) {
    std::fs::write(folder.join(name), text).unwrap();
}

/// Waits until the scan that is going on is over and a frame has taken what it found (the scan is a thread of its own).
fn wait_for_the_scan(window: &mut Window) {
    let started = Instant::now();
    loop {
        window.settle();
        if !window.application().is_scanning_plugins() {
            return;
        }
        assert!(started.elapsed() < Duration::from_secs(120), "the scan did not end");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A window that was told of a folder, and has waited for the scan of it.
fn window_that_found_the_shaders_in(folder: &Path) -> Window {
    let mut window = Window::showing(project_of(100));
    window.application_mut().add_plugin_folder(folder.to_owned());
    wait_for_the_scan(&mut window);
    window
}

fn names_of(kinds: &[EffectKind]) -> Vec<&'static str> {
    kinds.iter().map(|kind| kind.name()).collect()
}

fn open_the_plugin_manager(window: &mut Window) {
    window.harness.get_by_label("Edit").click();
    window.settle();
    window.harness.get_by_label("Plugin manager...").click();
    window.settle();
}

fn is_shown(window: &Window, label: &str) -> bool {
    window.harness.query_by_label(label).is_some()
}

fn is_shown_containing(window: &Window, text: &str) -> bool {
    window.harness.query_by_label_contains(text).is_some()
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

// --- what is found and installed ---

#[test]
fn the_shaders_of_a_folder_that_was_added_are_found_and_the_filters_and_generators_that_run_are_installed_and_nothing_else()
 {
    let _alone = the_installed_plugins_to_myself();
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "Vignette.fs", VIGNETTE);
    write(folder.path(), "Plasma.fs", PLASMA);
    write(folder.path(), "Wipe.fs", WIPE);
    write(folder.path(), "Broken.fs", BAD);

    let window = window_that_found_the_shaders_in(folder.path());

    assert!(window.application().plugin_scan_problems().is_empty());
    let offered = EffectKind::offered();
    assert!(
        names_of(&offered).contains(&"vignette"),
        "a filter is an effect that is offered"
    );
    assert!(
        !names_of(&offered).contains(&"plasma"),
        "a generator is not an effect: it is made into a clip"
    );
    assert_eq!(names_of(&EffectKind::installed_generators()), ["plasma"]);
    assert_eq!(
        names_of(&EffectKind::installed_shader_kinds()),
        ["plasma", "vignette"],
        "the two that run, in the order of their file names; the transition (not run yet) and the broken one are not installed"
    );
}

#[test]
fn looking_for_shaders_needs_no_scan_program_and_asks_nothing() {
    let _alone = the_installed_plugins_to_myself();
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "Vignette.fs", VIGNETTE);

    let window = window_that_found_the_shaders_in(folder.path());

    assert_eq!(names_of(&EffectKind::installed_shader_kinds()), ["vignette"]);
    assert!(
        window.application().interface.dialog.is_none(),
        "a shader is data: nothing is asked"
    );
    assert!(window.application().plugin_libraries_found().is_empty());
}

#[test]
fn a_scan_again_finds_a_shader_that_was_added_and_forgets_one_that_was_taken_away() {
    let _alone = the_installed_plugins_to_myself();
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "Vignette.fs", VIGNETTE);
    let mut window = window_that_found_the_shaders_in(folder.path());
    assert_eq!(names_of(&EffectKind::installed_shader_kinds()), ["vignette"]);

    write(folder.path(), "Plasma.fs", PLASMA);
    window.do_this(Command::PluginManager(PluginManagerCommand::ScanAgain));
    wait_for_the_scan(&mut window);
    assert_eq!(names_of(&EffectKind::installed_shader_kinds()), ["plasma", "vignette"]);

    std::fs::remove_file(folder.path().join("Vignette.fs")).unwrap();
    window.do_this(Command::PluginManager(PluginManagerCommand::ScanAgain));
    wait_for_the_scan(&mut window);
    assert_eq!(names_of(&EffectKind::installed_shader_kinds()), ["plasma"]);
    assert!(!names_of(&EffectKind::offered()).contains(&"vignette"));
}

#[test]
fn removing_a_folder_takes_its_shaders_away_and_adding_it_again_brings_them_back() {
    let _alone = the_installed_plugins_to_myself();
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "Vignette.fs", VIGNETTE);
    let mut window = window_that_found_the_shaders_in(folder.path());
    assert_eq!(names_of(&EffectKind::installed_shader_kinds()), ["vignette"]);

    window.do_this(Command::PluginManager(PluginManagerCommand::RemoveFolder(0)));
    wait_for_the_scan(&mut window);
    assert!(EffectKind::installed_shader_kinds().is_empty());

    window.application_mut().add_plugin_folder(folder.path().to_owned());
    wait_for_the_scan(&mut window);
    assert_eq!(names_of(&EffectKind::installed_shader_kinds()), ["vignette"]);
}

#[test]
fn the_same_shader_in_two_folders_is_one_effect_in_the_menu() {
    let _alone = the_installed_plugins_to_myself();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    write(first.path(), "Vignette.fs", VIGNETTE);
    write(second.path(), "Vignette.fs", VIGNETTE);
    let mut window = Window::showing(project_of(100));
    window.application_mut().add_plugin_folder(first.path().to_owned());
    wait_for_the_scan(&mut window);
    window.application_mut().add_plugin_folder(second.path().to_owned());
    wait_for_the_scan(&mut window);

    assert_eq!(names_of(&EffectKind::installed_shader_kinds()), ["vignette"]);
    assert_eq!(
        names_of(&EffectKind::offered())
            .iter()
            .filter(|name| **name == "vignette")
            .count(),
        1
    );
}

// --- the menus ---

#[test]
fn the_menu_of_the_plus_button_has_a_smaller_menu_for_isf_and_a_shader_chosen_there_is_put_on_the_clip_with_the_values_of_its_header()
 {
    let _alone = the_installed_plugins_to_myself();
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "Vignette.fs", VIGNETTE);
    write(folder.path(), "Plasma.fs", PLASMA);
    let (mut window, clip) = window_with_a_video();
    window.application_mut().add_plugin_folder(folder.path().to_owned());
    wait_for_the_scan(&mut window);
    select(&mut window, clip);

    let plus = window.rectangle_of(ADD_EFFECT_LABEL).center();
    window.click(plus);
    assert!(is_shown(&window, "ISF \u{23f5}"), "a smaller menu for the shaders");
    assert!(
        window.rectangle_of("Built-in \u{23f5}").top() < window.rectangle_of("ISF \u{23f5}").top(),
        "after the effects of the program"
    );
    window.open_the_smaller_menu("ISF");
    assert!(is_shown(&window, "Vignette"), "the filter is offered");
    assert!(!is_shown(&window, "Plasma"), "the generator is not an effect");
    assert!(
        !is_shown(&window, "Hue shift"),
        "the effects of the program are in their own smaller menu"
    );
    window.harness.get_by_label("Vignette").click();
    window.settle();

    let kinds = the_effects(&window, clip);
    assert_eq!(names_of(&kinds), ["vignette"]);
    assert!(kinds[0].is_installed());
    assert!(
        kinds[0].shader_source().is_some_and(|source| source.text == VIGNETTE),
        "the kind has the text of the file in it, which is what the project keeps"
    );
    assert!(is_shown(&window, "Vignette"), "the box of the effect is in the panel");
}

#[test]
fn a_shader_that_was_put_on_a_clip_goes_on_working_when_its_folder_is_gone() {
    let _alone = the_installed_plugins_to_myself();
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "Vignette.fs", VIGNETTE);
    let (mut window, clip) = window_with_a_video();
    window.application_mut().add_plugin_folder(folder.path().to_owned());
    wait_for_the_scan(&mut window);
    select(&mut window, clip);
    let plus = window.rectangle_of(ADD_EFFECT_LABEL).center();
    window.click(plus);
    window.open_the_smaller_menu("ISF");
    window.harness.get_by_label("Vignette").click();
    window.settle();
    assert_eq!(the_effects(&window, clip).len(), 1);

    window.do_this(Command::PluginManager(PluginManagerCommand::RemoveFolder(0)));
    wait_for_the_scan(&mut window);

    assert!(
        EffectKind::installed_shader_kinds().is_empty(),
        "nothing is installed any more"
    );
    let kinds = the_effects(&window, clip);
    assert_eq!(kinds.len(), 1, "the effect is still on the clip");
    assert!(
        kinds[0].is_installed(),
        "and runs: the project has the text of the shader, not the folder"
    );
}

#[test]
fn the_new_menu_of_a_track_lists_the_generators_that_were_found_and_one_chosen_is_a_clip_in_one_step() {
    let _alone = the_installed_plugins_to_myself();
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "Plasma.fs", PLASMA);
    write(folder.path(), "Vignette.fs", VIGNETTE);
    let mut window = Window::showing(project_of(90));
    window.application_mut().add_plugin_folder(folder.path().to_owned());
    wait_for_the_scan(&mut window);

    let point = window.middle_of_the_strip_at(0, 100);
    window.right_click(point);
    window.harness.get_by_label_contains("New").click();
    window.settle();
    assert!(
        is_shown(&window, "Generator from file..."),
        "that item is there as it was"
    );
    window.harness.get_by_label("Generator \u{23f5}").click();
    window.settle();
    assert!(is_shown(&window, "Plasma"));
    assert!(!is_shown(&window, "Vignette"), "a filter is not a generator");
    window.harness.get_by_label("Plasma").click();
    window.settle();

    let clip = window.clip(0, 1);
    let placement = window.application().project().placement_of(clip).unwrap();
    assert_eq!(placement.clip.kind(), ClipKind::Generator);
    assert!(
        placement.first_timeline_frame.value().abs_diff(100) <= 1,
        "where the menu was opened"
    );
    let kinds = the_effects(&window, clip);
    assert_eq!(
        names_of(&kinds),
        ["plasma"],
        "what the clip makes is the shader, the first of its chain"
    );
    assert!(kinds[0].is_a_generator());

    undo(&mut window);
    assert!(
        window.application().project().placement_of(clip).is_none(),
        "one step, taken back whole"
    );
}

#[test]
fn the_new_menu_has_no_list_of_generators_when_none_was_found() {
    let _alone = the_installed_plugins_to_myself();
    let mut window = Window::showing(project_of(90));
    let point = window.middle_of_the_strip_at(0, 100);
    window.right_click(point);
    window.harness.get_by_label_contains("New").click();
    window.settle();
    assert!(is_shown(&window, "Generator from file..."));
    assert!(window.harness.query_by_label("Generator \u{23f5}").is_none());
}

#[test]
fn a_command_to_make_a_clip_of_a_kind_that_is_not_a_generator_makes_none() {
    let _alone = the_installed_plugins_to_myself();
    let mut window = Window::showing(project_of(90));
    let before = window.application().project().clone();
    let track = window.application().project().tracks().first().expect("a track").id();
    // (At a frame where there is room: the video of the project is 90 frames long. It is only the kind that is wrong.)
    window.do_this(Command::NewGeneratorClipOfKind {
        track,
        frame: TimelineFrameIndex::new(100),
        kind: EffectKind::HUE_SHIFT,
    });
    assert_eq!(*window.application().project(), before, "a hue shift is no generator");
    let generator = tempfile::tempdir().unwrap();
    write(generator.path(), "Plasma.fs", PLASMA);
    window.application_mut().add_plugin_folder(generator.path().to_owned());
    wait_for_the_scan(&mut window);
    window.do_this(Command::NewGeneratorClipOfKind {
        track,
        frame: TimelineFrameIndex::new(100),
        kind: EffectKind::installed_generators()[0],
    });
    assert_ne!(
        *window.application().project(),
        before,
        "and with a kind that is a generator, there was room: the clip is made"
    );
}

// --- what the window says ---

#[test]
fn the_window_says_how_many_shaders_can_be_used_lists_them_and_lists_those_that_cannot_with_the_reason() {
    let _alone = the_installed_plugins_to_myself();
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "Vignette.fs", VIGNETTE);
    write(folder.path(), "Plasma.fs", PLASMA);
    write(folder.path(), "Wipe.fs", WIPE);
    write(folder.path(), "Broken.fs", BAD);
    let mut window = window_that_found_the_shaders_in(folder.path());

    open_the_plugin_manager(&mut window);

    assert!(
        is_shown_containing(&window, "2 shaders can be used, 2 cannot"),
        "the line under the button that scans again"
    );
    assert!(is_shown(&window, &heading_of_the_shaders_that_can_be_used(2)));
    assert!(is_shown(&window, &heading_of_the_shaders_that_cannot_be_used(2)));
    // The list of those that cannot be used is open: each says why.
    assert!(is_shown_containing(&window, "Broken.fs: "), "the file and why");
    assert!(is_shown_containing(&window, "Wipe.fs: this shader is a transition"));
    // The list of those that can is closed, and opened by its heading.
    assert!(!is_shown(&window, "Vignette (filter): Vignette.fs"));
    window
        .harness
        .get_by_label(&heading_of_the_shaders_that_can_be_used(2))
        .click();
    window.settle();
    assert!(is_shown(&window, "Vignette (filter): Vignette.fs"));
    assert!(is_shown(&window, "Plasma (generator): Plasma.fs"));
}

#[test]
fn the_window_has_no_list_of_shaders_where_none_was_found_and_no_word_about_them_in_its_summary() {
    let _alone = the_installed_plugins_to_myself();
    let mut window = Window::showing(project_of(100));
    open_the_plugin_manager(&mut window);
    assert!(!is_shown_containing(&window, "Shaders that"));
    assert!(
        !is_shown_containing(&window, "can be used"),
        "no word of shaders in the line under the button"
    );
}

#[test]
fn a_shader_that_is_found_once_is_said_to_be_usable_in_the_singular_and_the_folder_question_says_plugins_or_shaders() {
    let _alone = the_installed_plugins_to_myself();
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "Vignette.fs", VIGNETTE);
    let mut window = window_that_found_the_shaders_in(folder.path());
    open_the_plugin_manager(&mut window);
    assert!(is_shown_containing(&window, "1 shader can be used."));
    assert!(!is_shown_containing(&window, "cannot"));
    assert!(is_shown(&window, "Folders to look for plugins and shaders in"));
}

#[test]
fn a_shader_that_is_in_two_folders_is_listed_once_among_those_that_can_be_used() {
    let _alone = the_installed_plugins_to_myself();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    write(first.path(), "Vignette.fs", VIGNETTE);
    write(second.path(), "Vignette.fs", VIGNETTE);
    let mut window = Window::showing(project_of(100));
    window.application_mut().add_plugin_folder(first.path().to_owned());
    wait_for_the_scan(&mut window);
    window.application_mut().add_plugin_folder(second.path().to_owned());
    wait_for_the_scan(&mut window);

    open_the_plugin_manager(&mut window);

    assert!(is_shown(&window, &heading_of_the_shaders_that_can_be_used(1)));
    assert!(is_shown_containing(&window, "1 shader can be used."));
}

#[test]
fn the_generators_of_the_new_menu_are_listed_by_name_whatever_the_order_of_the_folders_they_were_found_in() {
    let _alone = the_installed_plugins_to_myself();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    write(
        first.path(),
        "c.fs",
        "/*{ \"INPUTS\": [] }*/\nvoid main() { gl_FragColor = vec4(0.1, 0.2, 0.3, 1.0); }\n",
    );
    write(
        second.path(),
        "b.fs",
        "/*{ \"INPUTS\": [] }*/\nvoid main() { gl_FragColor = vec4(0.3, 0.2, 0.1, 1.0); }\n",
    );
    write(
        second.path(),
        "a.fs",
        "/*{ \"INPUTS\": [] }*/\nvoid main() { gl_FragColor = vec4(0.2, 0.2, 0.2, 1.0); }\n",
    );
    let mut window = Window::showing(project_of(90));
    window.application_mut().add_plugin_folder(first.path().to_owned());
    window.application_mut().add_plugin_folder(second.path().to_owned());
    wait_for_the_scan(&mut window);
    wait_for_the_scan(&mut window);
    assert_eq!(
        names_of(&EffectKind::installed_generators()),
        ["c", "a", "b"],
        "found in the order of the folders"
    );

    let point = window.middle_of_the_strip_at(0, 100);
    window.right_click(point);
    window.harness.get_by_label_contains("New").click();
    window.settle();
    window.harness.get_by_label("Generator \u{23f5}").click();
    window.settle();
    let tops: Vec<f32> = ["A", "B", "C"]
        .iter()
        .map(|name| window.rectangle_of(name).top())
        .collect();
    assert!(tops.windows(2).all(|pair| pair[0] < pair[1]), "by name: {tops:?}");
}

#[test]
fn where_the_scan_program_is_not_found_the_window_says_so_and_the_shaders_are_found_all_the_same() {
    let _alone = the_installed_plugins_to_myself();
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "Vignette.fs", VIGNETTE);
    let mut window = Window::showing(project_of(100));
    window.application_mut().pretend_the_scan_program_is_missing();
    window.application_mut().add_plugin_folder(folder.path().to_owned());
    wait_for_the_scan(&mut window);

    open_the_plugin_manager(&mut window);

    assert!(is_shown_containing(
        &window,
        "was not found beside the editor, so no plugins are looked for (shaders are)."
    ));
    assert_eq!(names_of(&EffectKind::installed_shader_kinds()), ["vignette"]);
    assert!(is_shown_containing(&window, "1 shader can be used."));
    assert!(
        window.application().plugin_scan_problems().is_empty(),
        "the note is the window's, and the scans said nothing"
    );
}
