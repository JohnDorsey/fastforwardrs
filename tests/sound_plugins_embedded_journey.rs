// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A plugin that is **inside the project file** and is not installed, in the real editor, with the real scan program and the test libraries (AU-13): the project is opened, the severe
//! question is asked, and when *Load it* has been held down the plugin is written out, scanned in the background, installed, and **heard in the sound of an export**; a *no* leaves
//! the effect whole and silent about nothing; and another project that is opened takes it all back (it is asked about again, every time).
//!
//! The list of the plugins that are installed is for the whole program, so the tests of this file do not run at the same time.

mod journey;
mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use egui::Key;
use egui_kittest::kittest::Queryable;
use fastforwardrs::plugin_trust::this_platform;
use fastforwardrs::project::{
    AnimationTarget, EffectKind, EmbeddedBuild, PluginLibrary, PluginLibraryKind, PluginLibrarySource,
    set_installed_plugins,
};
use fastforwardrs::project_file::{read_project_file, write_project_file};
use journey::Journey;
use ladspa_test_plugins::library_file_for_tests;
use support::{counter_video, scratch_directory};

const SCANNER: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-scan");

/// The label of the button of the severe question (the editor's own constant is not public).
const PLUGIN_LOAD_LABEL: &str = "Load it";

fn the_installed_plugins_to_myself() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    set_installed_plugins(Vec::new());
    guard
}

fn library_name() -> String {
    format!("good.{}", fastforwardrs::plugin_scan::library_file_extension())
}

fn effects_of_the_first_clip(journey: &Journey) -> Vec<EffectKind> {
    let clip = journey.application().project().tracks()[0]
        .placement_at_index(0)
        .unwrap()
        .id;
    journey
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .map(|effect| effect.kind())
        .collect()
}

/// A project file with the multiplier plugin of the test library on the clip of the video, **and the library inside the file** (and the plugin is not installed where this is
/// opened: the list is emptied). Made in an editor that has the plugin installed, as a person would make it, and then given the library.
fn a_project_file_with_the_plugin_inside(
    toolchain: &fastforwardrs::media::FfmpegToolchain,
    video: &Path,
    scratch: &Path,
) -> PathBuf {
    let plugins = scratch.join("plugins");
    fs::create_dir_all(&plugins).unwrap();
    let installed_library = plugins.join(library_name());
    fs::copy(library_file_for_tests(), &installed_library).unwrap();
    let mut maker = Journey::importing(toolchain, video);
    maker
        .application_mut()
        .use_this_plugin_scanner(PathBuf::from(SCANNER), None, false);
    maker.application_mut().add_plugin_folder(plugins);
    let began = Instant::now();
    while maker.application().is_scanning_plugins() {
        assert!(began.elapsed() < Duration::from_secs(60), "the scan did not end");
        maker.keep_drawing_for(Duration::from_millis(50));
    }
    maker.select_the_clip_at(0);
    let plus = maker.rectangle_of("Add an effect").center();
    maker.click(plus);
    maker.settle();
    maker.open_the_smaller_menu("LADSPA");
    maker.harness.get_by_label("Test plugin ffrs_test_switches").click();
    maker.settle();
    let without_the_library = scratch.join("without_the_library.ffrs");
    assert!(maker.application_mut().save_project_to(without_the_library.clone()));

    let mut project = read_project_file(&without_the_library).unwrap();
    let build = EmbeddedBuild::new(&this_platform(), Arc::from(fs::read(&installed_library).unwrap())).unwrap();
    project
        .add_plugin_library(
            PluginLibrary::new(
                &library_name(),
                PluginLibraryKind::Ladspa,
                PluginLibrarySource::Embedded(vec![build]),
            )
            .unwrap(),
        )
        .unwrap();
    let with_the_library = scratch.join("with_the_library.ffrs");
    write_project_file(&project, &with_the_library).unwrap();
    set_installed_plugins(Vec::new());
    with_the_library
}

/// An editor that has no folder to look in for plugins, that has opened `file`, and is showing the severe question.
fn an_editor_asking_about_the_plugin_inside(toolchain: &fastforwardrs::media::FfmpegToolchain, file: &Path) -> Journey {
    let mut editor = Journey::start(toolchain);
    editor
        .application_mut()
        .use_this_plugin_scanner(PathBuf::from(SCANNER), None, false);
    editor.application_mut().load_project_from(file.to_owned());
    editor.wait_until_idle();
    assert!(
        editor.harness.query_by_label(PLUGIN_LOAD_LABEL).is_some(),
        "the question about the plugin inside the project is asked"
    );
    editor
}

/// Holds *Enter* down for five and a half seconds of the window's own time, and lets go.
fn hold_load_it(editor: &mut Journey) {
    editor.harness.key_down(Key::Enter);
    for _ in 0..24 {
        editor.harness.step();
    }
    editor.harness.key_up(Key::Enter);
    editor.settle();
}

fn wait_until_the_plugin_is_installed(editor: &mut Journey) {
    let began = Instant::now();
    while !effects_of_the_first_clip(editor)[0].is_installed() {
        assert!(
            began.elapsed() < Duration::from_secs(60),
            "the plugin inside the project was not installed: {:?}",
            editor.application().message_showing()
        );
        editor.keep_drawing_for(Duration::from_millis(50));
    }
}

fn export_and_take_the_words(journey: &mut Journey, destination: &Path) -> String {
    journey.application_mut().export_video_to(destination.to_owned());
    journey.wait_until_idle();
    let (title, text) = journey
        .application()
        .message_showing()
        .expect("a message says how the export ended");
    assert_eq!(title, "Export finished", "{text}");
    text.to_owned()
}

#[test]
fn a_plugin_inside_the_project_that_is_not_installed_is_installed_when_load_it_is_held_down_and_is_heard_in_the_export_and_is_taken_back_when_another_project_is_opened()
 {
    let toolchain = toolchain_and_graphics_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let _one_export_at_a_time = support::one_export_at_a_time();
    let scratch = scratch_directory();
    let video = counter_video(&toolchain);
    let project_file = a_project_file_with_the_plugin_inside(&toolchain, &video.path, scratch.path());

    let mut plain = Journey::importing(&toolchain, &video.path);
    export_and_take_the_words(&mut plain, &scratch.path().join("plain.mp4"));
    let plain_loudness = support::loudness(&support::decoded_sound(
        &toolchain,
        &scratch.path().join("plain.mp4"),
        2,
        48_000,
    ));
    assert!(plain_loudness > 0.02, "there is a tone to be changed: {plain_loudness}");

    // Opened: the plugin is asked about, and it is not installed, and nothing has been written out or scanned while nobody has said yes.
    let mut editor = an_editor_asking_about_the_plugin_inside(&toolchain, &project_file);
    assert_eq!(effects_of_the_first_clip(&editor).len(), 1);
    assert!(!effects_of_the_first_clip(&editor)[0].is_installed());
    editor.keep_drawing_for(Duration::from_millis(500));
    assert!(
        !effects_of_the_first_clip(&editor)[0].is_installed(),
        "not installed while the question is not answered"
    );

    // A click is not a yes. Holding it down is: the plugin comes to be installed, from a file that is not in any folder that was added.
    let at = editor.rectangle_of(PLUGIN_LOAD_LABEL).center();
    editor.click(at);
    assert!(
        editor.harness.query_by_label(PLUGIN_LOAD_LABEL).is_some(),
        "a click does not load it"
    );
    hold_load_it(&mut editor);
    wait_until_the_plugin_is_installed(&mut editor);
    let installed = effects_of_the_first_clip(&editor)[0].installed_plugin().unwrap();
    assert_eq!(
        installed.library.file_name().unwrap().to_string_lossy(),
        library_name(),
        "the file keeps its name, which is what the key of the plugin is made from"
    );
    assert!(
        installed
            .library
            .starts_with(std::env::temp_dir().canonicalize().unwrap())
            || installed.library.starts_with(std::env::temp_dir()),
        "it was written out to the temporary files: {:?}",
        installed.library
    );

    // The sound of the export has the plugin in it: the multiplier makes it louder.
    let words = export_and_take_the_words(&mut editor, &scratch.path().join("with_the_plugin.mp4"));
    assert!(!words.contains("were not run"), "everything was run: {words}");
    let louder = support::loudness(&support::decoded_sound(
        &toolchain,
        &scratch.path().join("with_the_plugin.mp4"),
        2,
        48_000,
    ));
    assert!(
        louder / plain_loudness >= 1.9,
        "the plugin that was inside the project makes the sound louder: {louder} against {plain_loudness}"
    );

    // Another project is opened: what was said yes to is taken back, the plugin is not installed, and the first project asks again when it is opened again.
    editor.press_button("OK");
    editor.application_mut().start_empty_project();
    editor.settle();
    assert!(
        EffectKind::installed_plugin_kinds().is_empty(),
        "nothing is installed: the plugin was only for the project that had it"
    );
    editor.application_mut().load_project_from(project_file.clone());
    editor.wait_until_idle();
    assert!(
        editor.harness.query_by_label(PLUGIN_LOAD_LABEL).is_some(),
        "asked again, as every time the project is opened"
    );
    assert!(!effects_of_the_first_clip(&editor)[0].is_installed());
}

#[test]
fn a_no_to_a_plugin_inside_the_project_leaves_the_effect_whole_and_not_installed_and_the_sound_as_it_was() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let scratch = scratch_directory();
    let video = counter_video(&toolchain);
    let project_file = a_project_file_with_the_plugin_inside(&toolchain, &video.path, scratch.path());
    let mut editor = an_editor_asking_about_the_plugin_inside(&toolchain, &project_file);

    editor.press_key(Key::Escape);
    editor.settle();
    editor.keep_drawing_for(Duration::from_millis(500));

    assert!(
        editor.harness.query_by_label(PLUGIN_LOAD_LABEL).is_none(),
        "the question is gone"
    );
    assert_eq!(effects_of_the_first_clip(&editor).len(), 1, "the effect is kept");
    assert!(!effects_of_the_first_clip(&editor)[0].is_installed());
    assert!(EffectKind::installed_plugin_kinds().is_empty());
    let resaved = scratch.path().join("resaved.ffrs");
    assert!(editor.application_mut().save_project_to(resaved.clone()));
    assert!(
        read_project_file(&resaved)
            .unwrap()
            .plugin_libraries()
            .get(&library_name())
            .is_some_and(PluginLibrary::is_embedded),
        "the project is saved with the library still inside it"
    );
}
