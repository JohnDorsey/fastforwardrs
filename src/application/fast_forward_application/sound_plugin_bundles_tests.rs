// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The *Bundles* tab of the window of sound plugins: what it shows, the question before a download, a download with archives made on the spot (nothing comes from the network), what
//! is done when it is over, a failure, and a download that is stopped.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use egui_kittest::kittest::Queryable;

use super::sound_plugins_tests::{is_shown, open_the_window, shows_containing};
use super::test_window::Window;
use crate::application::commands::Command;
use crate::application::sound_plugins_bundles::{
    ASK_NO_LABEL, ASK_YES_LABEL, CANCEL_LABEL, FETCHED_NOTE, NO_BUILD_FOR_THIS_COMPUTER_NOTE, NO_BUILDS_NOTE,
    NOT_FETCHED_NOTE, OK_LABEL,
};
use crate::application::sound_plugins_window::SoundPluginsCommand;
use crate::plugin_bundles::test_support::{FakeFetcher, a_fixture};
use crate::plugin_bundles::{CANCELLED, Fetcher, Manifest};
use crate::test_support::the_installed_plugins_to_myself;

/// A window with its sound plugins window open on the *Bundles* tab, the manifest of `a_fixture` for `windows-x86_64` (whatever this system is), a folder of its own to put plugins in,
/// and `fetcher` in place of the network.
fn window_on_the_bundles_tab(fetcher: Arc<dyn Fetcher>) -> (Window, tempfile::TempDir) {
    let mut window = Window::showing(super::test_window::project_of(100));
    let folder = tempfile::tempdir().unwrap();
    let bundles = &mut window.application_mut().environment.plugin_search.bundles;
    bundles.manifest = Some(a_fixture().manifest);
    bundles.platform = Some("windows-x86_64");
    bundles.folder = Some(folder.path().to_owned());
    bundles.fetcher = Some(fetcher);
    open_the_window(&mut window);
    window.harness.get_by_label("Bundles").click();
    window.settle();
    (window, folder)
}

fn the_fake_fetcher() -> Arc<FakeFetcher> {
    Arc::new(FakeFetcher::new(a_fixture().files))
}

fn wait_until_the_download_is_over(window: &mut Window) {
    for _ in 0..600 {
        window.settle();
        if !window.application().bundle_download_is_going_on() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the download did not end");
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
fn the_window_opens_on_the_plugins_tab_and_the_bundles_tab_lists_each_set_with_its_licence_and_source_and_a_button() {
    let _alone = the_installed_plugins_to_myself();
    let mut window = Window::showing(super::test_window::project_of(100));
    open_the_window(&mut window);
    assert!(
        is_shown(&window, "Scan again"),
        "the first tab is the one of the plugins"
    );
    assert!(!shows_containing(&window, "nothing is downloaded until you ask"));

    let (window, _folder) = window_on_the_bundles_tab(the_fake_fetcher());

    assert!(shows_containing(&window, "nothing is downloaded until you ask"));
    assert!(
        !is_shown(&window, "Scan again"),
        "the other tab's controls are not there"
    );
    for wanted in ["ONE 1.0", "TWO 1.0", "THREE 1.0"] {
        assert!(is_shown(&window, wanted), "{wanted}");
    }
    assert!(shows_containing(&window, "(GPL-3.0-or-later)"));
    assert!(shows_containing(
        &window,
        "Source: https://example.org/one-source.tar.gz"
    ));
    assert_eq!(
        window.harness.query_all_by_label(NOT_FETCHED_NOTE).count(),
        3,
        "the sets that have a build for this system"
    );
    assert!(
        is_shown(&window, NO_BUILD_FOR_THIS_COMPUTER_NOTE),
        "the one that has none is listed and says so"
    );
    assert!(
        shows_containing(&window, "Download all ("),
        "the button says how big the download is"
    );
}

#[test]
fn the_button_asks_first_with_what_where_from_how_big_and_where_to_and_nothing_is_downloaded_until_yes() {
    let _alone = the_installed_plugins_to_myself();
    let fetcher = the_fake_fetcher();
    let (mut window, folder) = window_on_the_bundles_tab(fetcher.clone());

    window.harness.get_by_label_contains("Download all (").click();
    window.settle();

    assert!(
        shows_containing(&window, "Download 4 plugins ("),
        "one, two (two plugins) and three: the set that has no build here is not counted"
    );
    assert!(
        shows_containing(&window, "from example.org and keep them in"),
        "each site once"
    );
    assert!(shows_containing(&window, &folder.path().display().to_string()));
    assert!(shows_containing(&window, "is not used if it is different"));
    assert!(is_shown(&window, ASK_YES_LABEL) && is_shown(&window, ASK_NO_LABEL));
    assert!(fetcher.asked().is_empty(), "nothing is downloaded before the answer");

    window.harness.get_by_label(ASK_NO_LABEL).click();
    window.settle();

    assert!(shows_containing(&window, "Download all ("), "the button is back");
    assert!(fetcher.asked().is_empty());
    assert!(!is_shown(&window, ASK_YES_LABEL));
}

#[test]
fn a_download_that_nobody_asked_about_is_never_begun() {
    let _alone = the_installed_plugins_to_myself();
    let fetcher = the_fake_fetcher();
    let (mut window, folder) = window_on_the_bundles_tab(fetcher.clone());

    window.do_this(Command::SoundPlugins(SoundPluginsCommand::DownloadBundles));
    window.settle();
    std::thread::sleep(Duration::from_millis(100));
    window.settle();

    assert!(fetcher.asked().is_empty());
    assert!(!window.application().bundle_download_is_going_on());
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
}

#[test]
fn yes_fetches_the_plugins_trusts_the_folder_says_what_was_fetched_and_the_sets_are_fetched_after_it() {
    let _alone = the_installed_plugins_to_myself();
    let fetcher = the_fake_fetcher();
    let (mut window, folder) = window_on_the_bundles_tab(fetcher.clone());
    // (A scan program that is not there: what matters is that a scan is begun, which makes `has_scanned` true when it is over.)
    window.application_mut().use_this_plugin_scanner(
        std::path::PathBuf::from("/nowhere/fastforwardrs-plugin-scan"),
        None,
        false,
    );
    assert!(!window.application().environment.plugin_search.has_scanned);
    window.harness.get_by_label_contains("Download all (").click();
    window.settle();

    window.harness.get_by_label(ASK_YES_LABEL).click();
    wait_until_the_download_is_over(&mut window);

    let plugin = folder.path().join("two").join("Second.clap");
    assert_eq!(std::fs::read(&plugin).unwrap(), b"second");
    assert!(folder.path().join("one").join("NOTICE.txt").is_file());
    assert!(folder.path().join("one").join("LICENSE.txt").is_file());
    assert!(
        shows_containing(&window, "Fetched 4 plugins"),
        "what was fetched is said"
    );
    assert!(shows_containing(&window, "ONE, TWO, THREE"));
    for _ in 0..200 {
        window.settle();
        if window.application().environment.plugin_search.has_scanned {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        window.application().environment.plugin_search.has_scanned,
        "the plugins are looked for when the download is over"
    );
    assert_eq!(
        window
            .application()
            .environment
            .plugin_host
            .standing_of(&crate::plugin_trust::folder_of(&plugin)),
        crate::plugin_trust::FolderStanding::Approved,
        "what the program fetched is trusted"
    );

    window.harness.get_by_label(OK_LABEL).click();
    window.settle();

    assert_eq!(
        window.harness.query_all_by_label(FETCHED_NOTE).count(),
        3,
        "each set that has a build is fetched now"
    );
    assert!(shows_containing(&window, "Download all ("), "it can be done again");
}

#[test]
fn a_download_that_fails_says_why_and_leaves_nothing_and_ok_takes_the_message_away() {
    let _alone = the_installed_plugins_to_myself();
    let mut broken = a_fixture().files;
    broken.remove("https://example.org/two.zip");
    let (mut window, folder) = window_on_the_bundles_tab(Arc::new(FakeFetcher::new(broken)));
    window.harness.get_by_label_contains("Download all (").click();
    window.settle();

    window.harness.get_by_label(ASK_YES_LABEL).click();
    wait_until_the_download_is_over(&mut window);

    assert!(
        shows_containing(&window, "two.zip could not be downloaded"),
        "it says which address and why"
    );
    assert!(!folder.path().join("two").exists());
    window.harness.get_by_label(OK_LABEL).click();
    window.settle();
    assert!(!shows_containing(&window, "could not be downloaded"));
    assert!(shows_containing(&window, "Download all ("));
}

#[test]
fn cancel_stops_a_download_that_is_going_on_and_the_button_comes_back() {
    let _alone = the_installed_plugins_to_myself();
    let (mut window, folder) = window_on_the_bundles_tab(Arc::new(NeverEnding));
    window.harness.get_by_label_contains("Download all (").click();
    window.settle();
    window.harness.get_by_label(ASK_YES_LABEL).click();
    window.settle();
    assert!(window.application().bundle_download_is_going_on());
    assert!(is_shown(&window, CANCEL_LABEL), "there is a way to stop it");
    assert!(
        shows_containing(&window, "Downloading ONE (1 of 3)"),
        "and it says what it is doing"
    );

    window.harness.get_by_label(CANCEL_LABEL).click();
    wait_until_the_download_is_over(&mut window);

    assert!(
        shows_containing(&window, "Download all ("),
        "stopped on purpose is not an error: the button is back"
    );
    assert!(!shows_containing(&window, "cancelled"));
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0, "nothing is left");
}

#[test]
fn a_computer_with_no_builds_is_told_so_and_has_no_button() {
    let _alone = the_installed_plugins_to_myself();
    let (mut window, _folder) = window_on_the_bundles_tab(the_fake_fetcher());
    window.application_mut().environment.plugin_search.bundles.platform = Some("amiga");
    window.settle();
    window.settle();

    assert!(is_shown(&window, NO_BUILDS_NOTE));
    assert!(!shows_containing(&window, "Download all ("));
    assert_eq!(window.harness.query_all_by_label(FETCHED_NOTE).count(), 0);
}

#[test]
fn the_manifest_in_the_program_is_what_the_tab_lists_when_nothing_is_put_in_its_place() {
    let _alone = the_installed_plugins_to_myself();
    let mut window = Window::showing(super::test_window::project_of(100));
    open_the_window(&mut window);
    window.harness.get_by_label("Bundles").click();
    window.settle();

    for set in Manifest::embedded().sets {
        assert!(
            is_shown(&window, &format!("{} {}", set.name, set.version)),
            "{}",
            set.name
        );
    }
}
