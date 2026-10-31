// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The shaders in the *Bundles* tab of the plugin manager (phase 6.9): a section of its own with its own button, the question before the download, a download with files made on the
//! spot (nothing comes from the network), the shaders being found and offered once they are fetched, a failure, a download that is stopped, and that the two sections do not wait for
//! each other.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use egui_kittest::kittest::Queryable;

use super::sound_plugins_tests::{is_shown, open_the_window, show_the_bundle_tab, shows_containing};
use super::test_window::Window;
use crate::application::commands::Command;
use crate::application::plugin_manager_bundles::{
    ASK_NO_LABEL, ASK_YES_LABEL, BundleKind, CANCEL_LABEL, FETCHED_NOTE, NOT_FETCHED_NOTE, OK_LABEL,
};
use crate::application::plugin_manager_window::PluginManagerCommand;
use crate::plugin_bundles::test_support::{FakeFetcher, ShaderFixture, a_fixture, a_shader_fixture};
use crate::plugin_bundles::{CANCELLED, Fetcher, ShaderManifest};
use crate::project::EffectKind;
use crate::test_support::the_installed_plugins_to_myself;

/// A window with its plugin manager open on the *Bundles* tab, the manifest of `a_shader_fixture`, and the one of `a_fixture` cut down to its first set (the window is not tall, and
/// the section of the shaders is the second: with the four sets of plugins it would be below the part of the window that is shown, where a click does not reach), a folder of its own
/// for each, and `fetcher` in place of the network.
fn window_on_the_bundles_tab(fetcher: Arc<dyn Fetcher>) -> (Window, tempfile::TempDir, tempfile::TempDir) {
    let mut window = Window::showing(super::test_window::project_of(100));
    let plugins = tempfile::tempdir().unwrap();
    let shaders = tempfile::tempdir().unwrap();
    let bundles = &mut window.application_mut().environment.plugin_search.bundles;
    let mut plugin_manifest = a_fixture().manifest;
    plugin_manifest.sets.truncate(1);
    bundles.manifest = Some(plugin_manifest);
    bundles.platform = Some("windows-x86_64");
    bundles.folder = Some(plugins.path().to_owned());
    bundles.shader_manifest = Some(a_shader_fixture().manifest);
    bundles.shader_folder = Some(shaders.path().to_owned());
    bundles.fetcher = Some(fetcher);
    open_the_window(&mut window);
    window.harness.get_by_label("Bundles").click();
    window.settle();
    (window, plugins, shaders)
}

fn the_fake_fetcher() -> Arc<FakeFetcher> {
    let ShaderFixture { files, .. } = a_shader_fixture();
    Arc::new(FakeFetcher::new(files))
}

fn wait_until_the_download_is_over(window: &mut Window) {
    for _ in 0..600 {
        window.settle();
        if !window.application().shader_download_is_going_on() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the download did not end");
}

fn wait_until_it_shows(window: &mut Window, words: &str) {
    for _ in 0..600 {
        window.settle();
        if shows_containing(window, words) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the window never showed {words:?}");
}

fn wait_for_the_scan(window: &mut Window) {
    for _ in 0..1000 {
        window.settle();
        if !window.application().is_scanning_plugins() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the scan did not end");
}

/// A fetcher that goes on until it is told to stop.
struct NeverEnding;

impl Fetcher for NeverEnding {
    fn fetch(&self, _: &str, _: &Path, _: &mut dyn FnMut(u64), cancelled: &dyn Fn() -> bool) -> Result<(), String> {
        for _ in 0..2000 {
            if cancelled() {
                return Err(CANCELLED.to_owned());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Err("it was never told to stop".to_owned())
    }
}

#[test]
fn the_bundles_tab_has_a_section_for_the_shaders_beside_the_one_for_the_sound_plugins_each_with_a_button_of_its_own() {
    let _alone = the_installed_plugins_to_myself();

    let (mut window, _plugins, _shaders) = window_on_the_bundles_tab(the_fake_fetcher());

    // (A tab for each kind, with how many of the sets are fetched out of how many there are; the sound plugins are shown first.)
    assert!(is_shown(&window, "Sound plugins (0/1)"));
    assert!(is_shown(&window, "Shaders (0/2)"));
    assert_eq!(
        window.harness.query_all_by_label_contains("Download all (").count(),
        1,
        "the button of the plugins is in the tab that is shown"
    );
    assert!(
        !shows_containing(&window, "Download all shaders ("),
        "and the one of the shaders is not"
    );
    assert!(!shows_containing(&window, "PACK abc1234"));
    show_the_bundle_tab(&mut window, "Shaders");
    assert!(
        !shows_containing(&window, "Download all ("),
        "and now it is the other way round"
    );
    assert!(shows_containing(&window, "Shaders (ISF) that other people made"));
    assert!(is_shown(&window, "PACK abc1234") && is_shown(&window, "SOLO abc1234"));
    assert!(shows_containing(
        &window,
        "2 shaders: 1 filter, 1 generator (MIT AND BSD-3-Clause)"
    ));
    assert!(shows_containing(&window, "1 shaders: 1 transition (MIT)"));
    assert_eq!(
        window
            .harness
            .query_all_by_label_contains("Download all shaders (")
            .count(),
        1,
        "one button for the shaders"
    );
    assert_eq!(
        window.harness.query_all_by_label_contains("Download all (").count(),
        0,
        "and none for the plugins, which are in the other tab"
    );
    assert_eq!(
        window.harness.query_all_by_label(NOT_FETCHED_NOTE).count(),
        2,
        "the two sets of shaders"
    );
    show_the_bundle_tab(&mut window, "Sound plugins");
    assert_eq!(
        window.harness.query_all_by_label(NOT_FETCHED_NOTE).count(),
        1,
        "and the set of plugins in the other"
    );
}

#[test]
fn the_section_of_the_shaders_is_not_there_when_the_program_has_none_to_fetch() {
    let _alone = the_installed_plugins_to_myself();
    let (mut window, _plugins, _shaders) = window_on_the_bundles_tab(the_fake_fetcher());
    window
        .application_mut()
        .environment
        .plugin_search
        .bundles
        .shader_manifest = Some(ShaderManifest {
        format: 1,
        sets: Vec::new(),
    });
    window.settle();
    window.settle();

    assert!(shows_containing(&window, "Sound plugins ("));
    assert!(
        !shows_containing(&window, "Shaders ("),
        "there is no tab for what there is nothing of"
    );
    assert!(!shows_containing(&window, "Download all shaders"));
}

#[test]
fn the_button_of_the_shaders_asks_first_what_how_many_from_where_and_to_what_folder_and_nothing_is_downloaded_until_yes()
 {
    let _alone = the_installed_plugins_to_myself();
    let fetcher = the_fake_fetcher();
    let (mut window, _plugins, folder) = window_on_the_bundles_tab(fetcher.clone());

    show_the_bundle_tab(&mut window, "Shaders");
    window.harness.get_by_label_contains("Download all shaders (").click();
    window.settle();

    assert!(
        shows_containing(&window, "Download 3 shaders ("),
        "all the shaders of all the sets"
    );
    assert!(
        shows_containing(&window, "from example.org and keep them in"),
        "each site once"
    );
    assert!(shows_containing(&window, &folder.path().display().to_string()));
    assert!(shows_containing(&window, "is not used if it is different"));
    assert!(is_shown(&window, ASK_YES_LABEL) && is_shown(&window, ASK_NO_LABEL));
    show_the_bundle_tab(&mut window, "Sound plugins");
    assert!(
        shows_containing(&window, "Download all ("),
        "the button of the plugins is still there, with no question of its own"
    );
    assert!(fetcher.asked().is_empty(), "nothing is downloaded before the answer");

    show_the_bundle_tab(&mut window, "Shaders");
    window.harness.get_by_label(ASK_NO_LABEL).click();
    window.settle();

    assert!(
        shows_containing(&window, "Download all shaders ("),
        "the button is back"
    );
    assert!(fetcher.asked().is_empty());
    assert!(!is_shown(&window, ASK_YES_LABEL));
}

#[test]
fn a_download_of_shaders_that_nobody_asked_about_is_never_begun() {
    let _alone = the_installed_plugins_to_myself();
    let fetcher = the_fake_fetcher();
    let (mut window, _plugins, folder) = window_on_the_bundles_tab(fetcher.clone());

    window.do_this(Command::PluginManager(PluginManagerCommand::DownloadBundles(
        BundleKind::Shaders,
    )));
    window.settle();
    std::thread::sleep(Duration::from_millis(100));
    window.settle();

    assert!(fetcher.asked().is_empty());
    assert!(!window.application().shader_download_is_going_on());
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
}

#[test]
fn yes_fetches_the_shaders_into_a_folder_for_each_set_says_what_was_fetched_and_they_are_found_and_offered_when_it_is_over()
 {
    let _alone = the_installed_plugins_to_myself();
    let fetcher = the_fake_fetcher();
    let (mut window, plugins, folder) = window_on_the_bundles_tab(fetcher.clone());
    show_the_bundle_tab(&mut window, "Shaders");
    window.harness.get_by_label_contains("Download all shaders (").click();
    window.settle();

    window.harness.get_by_label(ASK_YES_LABEL).click();
    wait_until_the_download_is_over(&mut window);

    let fixture = a_shader_fixture();
    for set in &fixture.manifest.sets {
        for shader in &set.shaders {
            assert_eq!(
                std::fs::read(folder.path().join(&set.id).join(&shader.file)).unwrap(),
                fixture.files[&shader.url],
                "{}",
                shader.file
            );
        }
        assert!(folder.path().join(&set.id).join("NOTICE.txt").is_file());
        assert!(folder.path().join(&set.id).join("LICENSE.txt").is_file());
    }
    assert!(
        shows_containing(&window, "Fetched 3 shaders"),
        "what was fetched is said"
    );
    assert!(shows_containing(&window, "PACK, SOLO"));
    assert_eq!(
        std::fs::read_dir(plugins.path()).unwrap().count(),
        0,
        "nothing of the plugins was fetched: the two downloads are not one"
    );
    // The shaders are looked for when the download is over, and are offered when they are found.
    wait_for_the_scan(&mut window);
    let names: Vec<&str> = EffectKind::installed_shader_kinds()
        .iter()
        .map(|kind| kind.name())
        .collect();
    assert_eq!(names.len(), 3, "{names:?}");
    for wanted in ["swirl thing", "glow", "wipe"] {
        assert!(names.contains(&wanted), "{wanted}: {names:?}");
    }
    // No question of trust was asked: a shader is data, and the folder of the plugins is a folder of code. (And what was fetched is not made trusted as a folder of plugins is.)
    assert!(window.application().interface.dialog.is_none());
    assert_eq!(
        window
            .application()
            .environment
            .plugin_host
            .standing_of(&crate::plugin_trust::folder_of(
                &folder.path().join("pack").join("Glow.fs")
            )),
        crate::plugin_trust::FolderStanding::NotAsked,
        "the folder of the shaders is not a folder of plugins that is approved"
    );

    window.harness.get_by_label(OK_LABEL).click();
    window.settle();
    assert_eq!(
        window.harness.query_all_by_label(FETCHED_NOTE).count(),
        2,
        "each set of shaders is fetched now (and none of the plugins)"
    );
    assert!(
        shows_containing(&window, "Download all shaders ("),
        "it can be done again"
    );
}

#[test]
fn a_download_of_shaders_that_fails_says_why_and_leaves_nothing_and_ok_takes_the_message_away() {
    let _alone = the_installed_plugins_to_myself();
    let fixture = a_shader_fixture();
    let mut broken = fixture.files.clone();
    let missing = fixture.manifest.sets[1].shaders[0].url.clone();
    broken.remove(&missing);
    let (mut window, _plugins, folder) = window_on_the_bundles_tab(Arc::new(FakeFetcher::new(broken)));
    show_the_bundle_tab(&mut window, "Shaders");
    window.harness.get_by_label_contains("Download all shaders (").click();
    window.settle();

    window.harness.get_by_label(ASK_YES_LABEL).click();
    wait_until_the_download_is_over(&mut window);

    assert!(
        shows_containing(&window, &format!("{missing} could not be downloaded")),
        "it says which address and why"
    );
    assert!(!folder.path().join("solo").exists(), "nothing of the set that failed");
    assert!(
        folder.path().join("pack").join("Glow.fs").is_file(),
        "the set before it was made whole and is left, as a download of plugins leaves the sets before the one that failed"
    );
    window.harness.get_by_label(OK_LABEL).click();
    window.settle();
    assert!(!shows_containing(&window, "could not be downloaded"));
    assert!(shows_containing(&window, "Download all shaders ("));
}

#[test]
fn cancel_stops_a_download_of_shaders_that_is_going_on_and_the_button_comes_back() {
    let _alone = the_installed_plugins_to_myself();
    let (mut window, _plugins, folder) = window_on_the_bundles_tab(Arc::new(NeverEnding));
    show_the_bundle_tab(&mut window, "Shaders");
    window.harness.get_by_label_contains("Download all shaders (").click();
    window.settle();
    window.harness.get_by_label(ASK_YES_LABEL).click();
    window.settle();
    assert!(window.application().shader_download_is_going_on());
    assert!(
        !window.application().bundle_download_is_going_on(),
        "the plugins are not being fetched"
    );
    assert!(is_shown(&window, CANCEL_LABEL), "there is a way to stop it");
    wait_until_it_shows(&mut window, "Downloading PACK (1 of 2)");

    window.harness.get_by_label(CANCEL_LABEL).click();
    wait_until_the_download_is_over(&mut window);

    assert!(
        shows_containing(&window, "Download all shaders ("),
        "stopped on purpose is not an error"
    );
    assert!(!shows_containing(&window, "cancelled"));
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0, "nothing is left");
}

#[test]
fn the_two_downloads_are_independent_asking_for_one_leaves_the_other_alone() {
    let _alone = the_installed_plugins_to_myself();
    let (mut window, _plugins, _shaders) = window_on_the_bundles_tab(the_fake_fetcher());

    window.harness.get_by_label_contains("Download all (").click();
    window.settle();

    assert!(shows_containing(&window, "Download 1 plugin ("));
    assert!(
        !shows_containing(&window, "Download 3 shaders ("),
        "the shaders were not asked about"
    );
    show_the_bundle_tab(&mut window, "Shaders");
    assert!(
        shows_containing(&window, "Download all shaders ("),
        "their button is as it was"
    );
    show_the_bundle_tab(&mut window, "Sound plugins");
    window.harness.get_by_label(ASK_NO_LABEL).click();
    window.settle();

    show_the_bundle_tab(&mut window, "Shaders");
    window.harness.get_by_label_contains("Download all shaders (").click();
    window.settle();
    assert!(shows_containing(&window, "Download 3 shaders ("));
    assert!(!shows_containing(&window, "Download 1 plugin ("));
}

#[test]
fn the_manifest_of_shaders_in_the_program_is_what_the_section_lists_when_nothing_is_put_in_its_place() {
    let _alone = the_installed_plugins_to_myself();
    let mut window = Window::showing(super::test_window::project_of(100));
    open_the_window(&mut window);
    window.harness.get_by_label("Bundles").click();
    window.settle();

    show_the_bundle_tab(&mut window, "Shaders");
    for set in ShaderManifest::embedded().sets {
        assert!(
            is_shown(&window, &format!("{} {}", set.name, set.version)),
            "{}",
            set.name
        );
    }
}

/// The sizes and the colours of the text that is drawn in the window now, as `(size, colour)`; text that has no colour of its own is the colour of text.
fn the_text_drawn(window: &Window) -> Vec<(f32, egui::Color32)> {
    let visuals = window.harness.ctx.global_style().visuals.clone();
    let mut found = Vec::new();
    // (The plugin manager is drawn over the rest of the program, after it: what follows its title is what the window draws.)
    let shapes = window.shapes_drawn();
    let title = shapes
        .iter()
        .position(|shape| matches!(shape, egui::Shape::Text(text) if text.galley.text() == "Plugin manager"))
        .expect("the title of the plugin manager is drawn");
    for shape in &shapes[title + 1..] {
        if let egui::Shape::Text(text) = shape {
            for section in &text.galley.job.sections {
                let colour = if section.format.color == egui::Color32::PLACEHOLDER {
                    if text.override_text_color.is_some() || text.fallback_color == egui::Color32::PLACEHOLDER {
                        visuals.text_color()
                    } else {
                        text.fallback_color
                    }
                } else {
                    section.format.color
                };
                found.push((section.format.font_id.size, colour));
            }
        }
    }
    found
}

#[test]
fn the_text_of_the_bundles_tab_is_all_of_one_size_and_none_of_it_is_hard_to_read_against_the_window() {
    let _alone = the_installed_plugins_to_myself();
    let (mut window, _plugins, _shaders) = window_on_the_bundles_tab(the_fake_fetcher());
    let background = window.harness.ctx.global_style().visuals.window_fill;

    for title in ["Sound plugins", "Shaders"] {
        show_the_bundle_tab(&mut window, title);
        let text = the_text_drawn(&window);

        assert!(text.len() > 8, "{title}: there is text");
        let mut sizes: Vec<f32> = text.iter().map(|(size, _)| *size).collect();
        sizes.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        sizes.sort_by(f32::total_cmp);
        sizes.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        assert_eq!(sizes.len(), 1, "{title}: one size of text and not {sizes:?}");
        for (_, colour) in &text {
            assert!(
                crate::application::plugin_manager_text::contrast(*colour, background)
                    >= crate::application::plugin_manager_text::LEAST_CONTRAST - 0.1,
                "{title}: text of {colour:?} on {background:?} is hard to read ({})",
                crate::application::plugin_manager_text::contrast(*colour, background)
            );
        }
    }
}

#[test]
fn a_tab_says_how_many_of_its_sets_are_fetched_out_of_how_many_there_are_and_the_number_follows_a_download() {
    let _alone = the_installed_plugins_to_myself();
    let (mut window, _plugins, _shaders) = window_on_the_bundles_tab(the_fake_fetcher());
    assert!(is_shown(&window, "Shaders (0/2)") && is_shown(&window, "Sound plugins (0/1)"));
    show_the_bundle_tab(&mut window, "Shaders");

    window.harness.get_by_label_contains("Download all shaders (").click();
    window.settle();
    window.harness.get_by_label(ASK_YES_LABEL).click();
    wait_until_the_download_is_over(&mut window);
    window.settle();

    assert!(
        is_shown(&window, "Shaders (2/2)"),
        "the sets that are fetched are counted"
    );
    assert!(is_shown(&window, "Sound plugins (0/1)"), "and the others are not");
}

#[test]
fn the_tab_that_was_chosen_stays_chosen_and_the_buttons_are_at_the_top_of_it_above_the_list() {
    let _alone = the_installed_plugins_to_myself();
    let (mut window, _plugins, _shaders) = window_on_the_bundles_tab(the_fake_fetcher());

    window.do_this(Command::PluginManager(PluginManagerCommand::SelectBundleKind(
        BundleKind::Shaders,
    )));
    window.settle();
    window.settle();

    assert!(shows_containing(&window, "Download all shaders ("));
    assert!(!shows_containing(&window, "Download all ("));
    let button = window.harness.get_by_label_contains("Download all shaders (").rect();
    let first_set = window.harness.get_by_label_contains("PACK abc1234").rect();
    assert!(button.bottom() <= first_set.top(), "the button is above the list");
}
