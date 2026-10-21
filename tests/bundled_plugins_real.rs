// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugins that ship with the program, as a release has them: **run by hand on a folder that `tools/bundled_plugins/fetch_bundled_plugins.py` made** (they are downloaded, so the
//! test is ignored unless asked for), with the folder named by the environment variable `FFRS_BUNDLED_PLUGINS`:
//!
//! ```text
//! py tools/bundled_plugins/fetch_bundled_plugins.py --out target/bundled/plugins
//! FFRS_BUNDLED_PLUGINS=target/bundled/plugins cargo test --release --test bundled_plugins_real -- --ignored
//! ```
//!
//! What it checks is what a person is promised: every plugin of the manifest is found by the scan program, loads, can be an effect, and none of them is unreliable at its start.

use std::path::PathBuf;

use fastforwardrs::plugin_scan::{LibraryOutcome, PluginScanner, ScanCache, bundled_plugins_folder, plugins_found_in};

const SCANNER: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-scan");

#[test]
#[ignore = "needs the folder of bundled plugins that tools/bundled_plugins/fetch_bundled_plugins.py makes (FFRS_BUNDLED_PLUGINS)"]
fn every_plugin_of_the_manifest_is_found_loads_can_be_an_effect_and_starts_steady() {
    let folder = bundled_plugins_folder().expect("FFRS_BUNDLED_PLUGINS names the folder that the script made");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(manifest_file()).expect("the manifest is read"))
            .expect("the manifest is JSON");
    let wanted_files: Vec<String> = manifest["sets"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|set| {
            // (The files the script takes for this system: the names are the same on every system.)
            set["platforms"][fastforwardrs::plugin_bundles::this_platform().expect("builds for this system")]["members"]
                .as_array()
                .unwrap()
                .iter()
                .map(|member| member["to"].as_str().unwrap().to_owned())
        })
        .collect();

    let mut scanner = PluginScanner::new(PathBuf::from(SCANNER), ScanCache::in_memory())
        .with_time_limit(std::time::Duration::from_secs(30));
    let scan = scanner.scan_folders(std::slice::from_ref(&folder));

    assert_eq!(scan.problems, Vec::<String>::new());
    for file in &wanted_files {
        let report = scan
            .libraries
            .iter()
            .find(|report| {
                report
                    .path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy() == file.as_str())
            })
            .unwrap_or_else(|| panic!("{file} was not found in {folder:?}"));
        match &report.outcome {
            LibraryOutcome::ScannedClap(clap) => {
                assert!(!clap.plugins.is_empty(), "{file} has no plugins");
                for plugin in &clap.plugins {
                    assert!(plugin.problem.is_none(), "{file}: {:?}", plugin.problem);
                    assert!(
                        !matches!(plugin.start, clap_host::start_check::StartCheck::Unreliable { .. }),
                        "{file}: {} is unreliable at its start",
                        plugin.description.name
                    );
                }
            }
            other => panic!("{file} was not scanned as a CLAP library: {other:?}"),
        }
    }
    let found = plugins_found_in(&scan.libraries);
    assert_eq!(found.effects.len(), wanted_files.len(), "{:?}", found.not_used);
    // Every folder of the set has its licence text and its notice.
    for set in manifest["sets"].as_array().unwrap() {
        let set_folder = folder.join(set["id"].as_str().unwrap());
        assert!(
            set_folder.join("NOTICE.txt").is_file(),
            "{set_folder:?} has no NOTICE.txt"
        );
        for text in set["licence_texts"].as_array().unwrap() {
            let name = text["name"].as_str().unwrap();
            assert!(set_folder.join(name).is_file(), "{set_folder:?} has no {name}");
        }
    }
}

fn manifest_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("bundled-plugins")
        .join("manifest.json")
}
