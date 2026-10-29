// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The section of the *Bundles* tab for the **data that plugins use** (Surge XT's patches and wavetables): it has its own button, size and question, and a download of it does not wait for
//! the plugins' (a person who wants the plugins is not made to fetch the data, which is bigger), puts the data in a folder beside the plugins, and begins no scan.

use std::sync::Arc;
use std::time::Duration;

use egui_kittest::kittest::Queryable;

use super::sound_plugins_tests::{is_shown, open_the_window, shows_containing};
use super::test_window::Window;
use crate::application::commands::Command;
use crate::application::plugin_manager_bundles::{
    ASK_NO_LABEL, ASK_YES_LABEL, BundleKind, FETCHED_NOTE, NO_BUILDS_NOTE, NOT_FETCHED_NOTE, OK_LABEL,
};
use crate::application::plugin_manager_window::PluginManagerCommand;
use crate::plugin_bundles::test_support::{FakeFetcher, a_fixture, a_fixture_with_data};
use crate::test_support::the_installed_plugins_to_myself;

/// A window with its plugin manager open on the *Bundles* tab, the manifest of `a_fixture_with_data` for `windows-x86_64` (whatever this system is) with **one set of plugins and one set of
/// data** (a window shows only so much, and a click on a button that is below what it shows does not reach it: the tests that click have the section of the data in view), a folder of its
/// own to put what is fetched in, and `fetcher` in place of the network.
fn window_on_the_bundles_tab(fetcher: Arc<FakeFetcher>) -> (Window, tempfile::TempDir) {
    window_on_the_bundles_tab_with(fetcher, |manifest| {
        manifest.sets.retain(|set| set.id == "data" || set.id == "two")
    })
}

/// The same with the manifest changed by `change`.
fn window_on_the_bundles_tab_with(
    fetcher: Arc<FakeFetcher>,
    change: impl FnOnce(&mut crate::plugin_bundles::Manifest),
) -> (Window, tempfile::TempDir) {
    let mut window = Window::showing(super::test_window::project_of(100));
    let folder = tempfile::tempdir().unwrap();
    let bundles = &mut window.application_mut().environment.plugin_search.bundles;
    bundles.shader_manifest = Some(crate::plugin_bundles::ShaderManifest {
        format: 1,
        sets: Vec::new(),
    });
    let mut manifest = a_fixture_with_data().manifest;
    change(&mut manifest);
    bundles.manifest = Some(manifest);
    bundles.platform = Some("windows-x86_64");
    bundles.folder = Some(folder.path().to_owned());
    bundles.fetcher = Some(fetcher);
    open_the_window(&mut window);
    window.harness.get_by_label("Bundles").click();
    window.settle();
    (window, folder)
}

fn the_fake_fetcher() -> Arc<FakeFetcher> {
    Arc::new(FakeFetcher::new(a_fixture_with_data().files))
}

fn wait_until_the_data_download_is_over(window: &mut Window) {
    for _ in 0..600 {
        window.settle();
        if !window.application().data_download_is_going_on() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the download did not end");
}

fn wait_until_the_plugin_download_is_over(window: &mut Window) {
    for _ in 0..600 {
        window.settle();
        if !window.application().bundle_download_is_going_on() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the download did not end");
}

#[test]
fn the_tab_has_a_section_of_its_own_for_data_with_its_own_button_and_the_plugins_section_does_not_count_it() {
    let _alone = the_installed_plugins_to_myself();

    let (window, _folder) = window_on_the_bundles_tab_with(the_fake_fetcher(), |_| {});

    assert!(is_shown(&window, "Data for plugins"));
    assert!(shows_containing(
        &window,
        "Patches, wavetables and presets that plugins use"
    ));
    assert!(is_shown(&window, "THE DATA 1.0"));
    assert!(shows_containing(
        &window,
        "patches, wavetables, skins (GPL-3.0-or-later)"
    ));
    assert!(shows_containing(&window, "Download all data ("), "its own button");
    assert!(
        shows_containing(&window, "Download all ("),
        "and the plugins' button, not in its place"
    );
    let fixture = a_fixture_with_data();
    let data_size = fixture.manifest.sets[0].platforms["windows-x86_64"].size;
    let application = window.application();
    assert_eq!(
        application.what_the_bundles_tab_shows(BundleKind::PluginData).2,
        data_size
    );
    let plugin_rows = application.what_the_bundles_tab_shows(BundleKind::SoundPlugins).0;
    assert_eq!(
        plugin_rows.iter().map(|row| row.name.as_str()).collect::<Vec<_>>(),
        ["TWO", "THREE", "ELSEWHERE"],
        "the data is not in the list of plugins"
    );
    let data_rows = application.what_the_bundles_tab_shows(BundleKind::PluginData).0;
    assert_eq!(data_rows.len(), 1);
    assert_eq!(
        (data_rows[0].count, data_rows[0].has_build, data_rows[0].is_fetched),
        (1, true, false)
    );
    assert_eq!(
        window.harness.query_all_by_label(NOT_FETCHED_NOTE).count(),
        3,
        "two sets of plugins that have a build here, and the data"
    );
}

#[test]
fn a_manifest_with_no_data_has_no_section_for_it() {
    let _alone = the_installed_plugins_to_myself();
    let mut window = Window::showing(super::test_window::project_of(100));
    let bundles = &mut window.application_mut().environment.plugin_search.bundles;
    bundles.shader_manifest = Some(crate::plugin_bundles::ShaderManifest {
        format: 1,
        sets: Vec::new(),
    });
    bundles.manifest = Some(a_fixture().manifest);
    bundles.platform = Some("windows-x86_64");
    open_the_window(&mut window);
    window.harness.get_by_label("Bundles").click();
    window.settle();

    assert!(!shows_containing(&window, "Data for plugins"));
    assert!(!shows_containing(&window, "Download all data ("));
    assert!(shows_containing(&window, "Download all ("));
}

#[test]
fn the_button_of_the_data_asks_first_in_its_own_words_and_nothing_is_downloaded_until_yes() {
    let _alone = the_installed_plugins_to_myself();
    let fetcher = the_fake_fetcher();
    let (mut window, folder) = window_on_the_bundles_tab(fetcher.clone());

    window.harness.get_by_label_contains("Download all data (").click();
    window.settle();

    assert!(shows_containing(&window, "Download 1 set of data ("));
    assert!(shows_containing(&window, "from example.org and keep it in"));
    assert!(shows_containing(&window, &folder.path().display().to_string()));
    assert!(is_shown(&window, ASK_YES_LABEL));
    assert!(
        shows_containing(&window, "Download all ("),
        "the plugins' button is still there: the questions do not wait for each other"
    );
    assert!(fetcher.asked().is_empty());

    window.harness.get_by_label(ASK_NO_LABEL).click();
    window.settle();

    assert!(shows_containing(&window, "Download all data ("), "the button is back");
    assert!(fetcher.asked().is_empty());
}

#[test]
fn a_data_download_that_nobody_asked_about_is_never_begun() {
    let _alone = the_installed_plugins_to_myself();
    let fetcher = the_fake_fetcher();
    let (mut window, folder) = window_on_the_bundles_tab(fetcher.clone());

    window.do_this(Command::PluginManager(PluginManagerCommand::DownloadBundles(
        BundleKind::PluginData,
    )));
    window.settle();
    std::thread::sleep(Duration::from_millis(100));
    window.settle();

    assert!(fetcher.asked().is_empty());
    assert!(!window.application().data_download_is_going_on());
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
}

#[test]
fn yes_fetches_only_the_data_into_a_folder_beside_the_plugins_and_begins_no_scan_and_the_plugins_are_then_fetched_without_it()
 {
    let _alone = the_installed_plugins_to_myself();
    let fetcher = the_fake_fetcher();
    let (mut window, folder) = window_on_the_bundles_tab(fetcher.clone());
    window.application_mut().use_this_plugin_scanner(
        std::path::PathBuf::from("/nowhere/fastforwardrs-plugin-scan"),
        None,
        false,
    );
    window.harness.get_by_label_contains("Download all data (").click();
    window.settle();

    window.harness.get_by_label(ASK_YES_LABEL).click();
    wait_until_the_data_download_is_over(&mut window);

    assert_eq!(
        std::fs::read(folder.path().join("TheData").join("patches").join("a.fxp")).unwrap(),
        b"a patch"
    );
    assert!(!folder.path().join("TheData").join("wavetables").join("Maybe").exists());
    assert!(folder.path().join("TheData").join("NOTICE.txt").is_file());
    assert!(!folder.path().join("two").exists(), "no plugin was fetched with it");
    assert_eq!(
        fetcher.asked(),
        ["https://example.org/data.zip", "https://example.org/LICENSE"],
        "the release file of the data and its licence text, and nothing of the plugins"
    );
    assert!(shows_containing(&window, "Fetched "), "what was fetched is said");
    assert!(shows_containing(&window, "of data: THE DATA"));
    assert!(shows_containing(&window, "find it when they are started"));
    std::thread::sleep(Duration::from_millis(100));
    window.settle();
    assert!(
        !window.application().environment.plugin_search.has_scanned,
        "data is not looked for as plugins are: no scan is begun"
    );
    window.harness.get_by_label(OK_LABEL).click();
    window.settle();
    assert_eq!(
        window.harness.query_all_by_label(FETCHED_NOTE).count(),
        1,
        "the data is fetched, and the plugins are not"
    );

    window.harness.get_by_label_contains("Download all (").click();
    window.settle();
    window.harness.get_by_label(ASK_YES_LABEL).click();
    wait_until_the_plugin_download_is_over(&mut window);

    assert!(folder.path().join("two").join("Second.clap").is_file());
    let asked = fetcher.asked();
    assert_eq!(
        asked
            .iter()
            .filter(|url| url.as_str() == "https://example.org/data.zip")
            .count(),
        1,
        "the data was not fetched again with the plugins: {asked:?}"
    );
    assert!(
        folder.path().join("TheData").join("NOTICE.txt").is_file(),
        "and is still there"
    );
}

#[test]
fn a_data_download_that_fails_says_why_and_leaves_nothing() {
    let _alone = the_installed_plugins_to_myself();
    let mut broken = a_fixture_with_data().files;
    broken.remove("https://example.org/data.zip");
    let (mut window, folder) = window_on_the_bundles_tab(Arc::new(FakeFetcher::new(broken)));
    window.harness.get_by_label_contains("Download all data (").click();
    window.settle();

    window.harness.get_by_label(ASK_YES_LABEL).click();
    wait_until_the_data_download_is_over(&mut window);

    assert!(shows_containing(&window, "data.zip could not be downloaded"));
    assert!(!folder.path().join("TheData").exists());
    window.harness.get_by_label(OK_LABEL).click();
    window.settle();
    assert!(shows_containing(&window, "Download all data ("));
}

#[test]
fn a_computer_with_no_builds_of_the_data_is_told_so_in_its_section_too() {
    let _alone = the_installed_plugins_to_myself();
    let (mut window, _folder) = window_on_the_bundles_tab(the_fake_fetcher());
    window.application_mut().environment.plugin_search.bundles.platform = Some("amiga");
    window.settle();
    window.settle();

    assert_eq!(
        window.harness.query_all_by_label(NO_BUILDS_NOTE).count(),
        2,
        "plugins and data"
    );
    assert!(!shows_containing(&window, "Download all data ("));
}
