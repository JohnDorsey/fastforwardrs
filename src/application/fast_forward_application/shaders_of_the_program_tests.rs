// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The shaders that come with the program (`assets/shaders`), in the editor: installed with the ones that are found in folders when the editor is told to (and a person's editor is), left
//! out otherwise, so that every other test finds in a folder all that it finds; listed in the plugin manager; the generators among them in the New menu of a track, each a clip in one step.

use std::time::{Duration, Instant};

use super::sound_plugins_flow::PlacesOfAPersonsPlugins;
use super::test_window::{Window, project_of};
use crate::application::commands::Command;
use crate::application::plugin_manager_window::heading_of_the_shaders_that_can_be_used;
use crate::project::{AnimationTarget, ClipKind, EffectKind, TimelineFrameIndex};
use crate::shader_files::{SHADERS_OF_THE_PROGRAM, scan_of_the_shaders_of_the_program};
use crate::test_support::the_installed_plugins_to_myself;
use egui_kittest::kittest::Queryable;

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

fn names_of(kinds: &[EffectKind]) -> Vec<&'static str> {
    kinds.iter().map(|kind| kind.name()).collect()
}

fn names_of_the_shaders_of_the_program_that(is_wanted: impl Fn(bool) -> bool) -> Vec<&'static str> {
    scan_of_the_shaders_of_the_program()
        .shaders
        .iter()
        .filter(|shader| is_wanted(shader.kind.is_some_and(EffectKind::is_a_generator)))
        .map(|shader| shader.kind.expect("it can be used").name())
        .collect()
}

#[test]
fn the_shaders_of_the_program_are_installed_when_the_editor_is_told_to_and_the_generators_are_not_effects() {
    let _alone = the_installed_plugins_to_myself();
    let mut window = Window::showing(project_of(100));
    window.application_mut().install_the_shaders_of_the_program();
    wait_for_the_scan(&mut window);

    assert!(window.application().plugin_scan_problems().is_empty());
    let all: Vec<&str> = names_of_the_shaders_of_the_program_that(|_| true);
    assert_eq!(all.len(), SHADERS_OF_THE_PROGRAM.len(), "every one of them can be used");
    assert_eq!(
        names_of(&EffectKind::installed_shader_kinds()),
        all,
        "in the order they are listed in"
    );
    assert_eq!(
        names_of(&EffectKind::installed_generators()),
        names_of_the_shaders_of_the_program_that(|is_a_generator| is_a_generator)
    );
    let offered = EffectKind::offered();
    for generator in names_of_the_shaders_of_the_program_that(|is_a_generator| is_a_generator) {
        assert!(
            !names_of(&offered).contains(&generator),
            "{generator} is made into a clip, not put on one"
        );
    }
    for filter in names_of_the_shaders_of_the_program_that(|is_a_generator| !is_a_generator) {
        assert!(names_of(&offered).contains(&filter), "{filter} is offered as an effect");
    }
}

#[test]
fn the_editor_of_a_person_installs_them_with_whatever_else_it_finds_where_it_looks() {
    // (The editor of a person is made with a window, which a test has not: this is what it does after that, with no file kept and no scan program.)
    let _alone = the_installed_plugins_to_myself();
    let mut window = Window::showing(project_of(100));
    window.application_mut().find_plugins_in(PlacesOfAPersonsPlugins {
        settings_file: None,
        bundled_folders: Vec::new(),
        scanner_program: None,
        cache_file: None,
        breadcrumb_file: None,
    });
    wait_for_the_scan(&mut window);

    // (The folders of this computer are looked in as well, so that what a person has fetched may be installed besides: all of the program's own are there.)
    let installed = names_of(&EffectKind::installed_shader_kinds());
    for name in names_of_the_shaders_of_the_program_that(|_| true) {
        assert!(
            installed.contains(&name),
            "{name} is installed for a person: {installed:?}"
        );
    }
}

#[test]
fn a_shader_that_is_in_a_folder_too_is_one_kind_of_effect_and_the_one_that_comes_with_the_program_is_listed_first() {
    let _alone = the_installed_plugins_to_myself();
    let folder = tempfile::tempdir().unwrap();
    let plasma = SHADERS_OF_THE_PROGRAM
        .iter()
        .find(|shader| shader.file_name == "Plasma Storm.fs")
        .expect("the program has a plasma");
    std::fs::write(folder.path().join("Plasma Storm.fs"), plasma.text).unwrap();
    let mut window = Window::showing(project_of(100));
    window.application_mut().install_the_shaders_of_the_program();
    // (Asked for while the scan of that goes on: another follows it, which has the folder.)
    window.application_mut().add_plugin_folder(folder.path().to_owned());
    wait_for_the_scan(&mut window);

    let found = &window.application().environment.plugin_search.result.shaders.shaders;
    let copies: Vec<bool> = found
        .iter()
        .filter(|shader| shader.file_name == "Plasma Storm.fs")
        .map(|shader| shader.built_into_the_program)
        .collect();
    assert_eq!(
        copies,
        [true, false],
        "both are found, and the one that comes with the program is first"
    );
    assert_eq!(
        names_of(&EffectKind::installed_shader_kinds())
            .iter()
            .filter(|name| **name == "plasma storm")
            .count(),
        1,
        "the same text is the same kind of effect: it is installed once"
    );
}

#[test]
fn an_editor_that_is_not_told_to_has_none_of_them_so_that_what_a_test_finds_in_a_folder_is_all_it_finds() {
    let _alone = the_installed_plugins_to_myself();
    let mut window = Window::showing(project_of(100));
    window.application_mut().start_scanning_plugins();
    wait_for_the_scan(&mut window);
    assert!(EffectKind::installed_shader_kinds().is_empty());
    assert!(EffectKind::installed_generators().is_empty());
}

#[test]
fn the_plugin_manager_counts_them_among_the_shaders_that_can_be_used() {
    let _alone = the_installed_plugins_to_myself();
    let mut window = Window::showing(project_of(100));
    window.application_mut().install_the_shaders_of_the_program();
    wait_for_the_scan(&mut window);
    window.harness.get_by_label("Edit").click();
    window.settle();
    window.harness.get_by_label("Plugin manager...").click();
    window.settle();
    assert!(
        window
            .harness
            .query_by_label(&heading_of_the_shaders_that_can_be_used(SHADERS_OF_THE_PROGRAM.len()))
            .is_some(),
        "the heading of the list says how many there are"
    );
}

#[test]
fn a_generator_of_the_program_is_made_into_a_clip_whose_first_effect_it_is_with_its_text_in_the_project() {
    let _alone = the_installed_plugins_to_myself();
    let mut window = Window::showing(project_of(90));
    window.application_mut().install_the_shaders_of_the_program();
    wait_for_the_scan(&mut window);
    let kind = EffectKind::installed_generators()
        .into_iter()
        .find(|kind| kind.name() == "mandelbrot explorer")
        .expect("the Mandelbrot explorer is installed");
    let track = window.application().project().tracks().first().expect("a track").id();
    window.do_this(Command::NewGeneratorClipOfKind {
        track,
        frame: TimelineFrameIndex::new(100),
        kind,
    });

    // (The video of the project is the first clip of the track, and the generator was put after it.)
    let clip = window.clip(0, 1);
    let placement = window.application().project().placement_of(clip).unwrap();
    assert_eq!(placement.clip.kind(), ClipKind::Generator);
    let effects: Vec<EffectKind> = window
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .map(|effect| effect.kind())
        .collect();
    assert_eq!(
        effects,
        [kind],
        "what the clip makes is the first effect of its chain, and the only one"
    );
    assert_eq!(
        kind.shader_source().map(|source| source.text.as_ref()),
        SHADERS_OF_THE_PROGRAM
            .iter()
            .find(|shader| shader.file_name == "Mandelbrot Explorer.fs")
            .map(|shader| shader.text),
        "the project has the text of the shader: it runs where the program has another version of it"
    );
}
