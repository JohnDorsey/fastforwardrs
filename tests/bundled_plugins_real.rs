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
//! What it checks is what a person is promised: every plugin of the manifest (a CLAP file, or an LV2 bundle) is found by the scan program, loads, can be an effect, and no CLAP plugin is
//! unreliable at its start.

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
    let platform = fastforwardrs::plugin_bundles::this_platform().expect("builds for this system");
    // (The sets that have a build for this system: Wolf Shaper has none for macOS, and the data of Surge XT has none for macOS either.)
    let sets_here: Vec<&serde_json::Value> = manifest["sets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|set| set["platforms"][platform].is_object())
        .collect();
    // (The plugins among them: a set of data is a folder of files that nothing runs.)
    let wanted_files: Vec<String> = sets_here
        .iter()
        .filter(|set| set["kind"] != "data")
        .flat_map(|set| {
            // (The files the script takes for this system: the names are the same on every system.)
            set["platforms"][platform]["members"]
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
            LibraryOutcome::ScannedLv2(lv2) => {
                assert!(file.ends_with(".lv2"), "{file} was scanned as an LV2 bundle");
                assert_eq!(
                    lv2.problems,
                    Vec::<String>::new(),
                    "{file}: what could not be understood of the bundle"
                );
                assert!(!lv2.plugins.is_empty(), "{file} has no plugins");
                for plugin in &lv2.plugins {
                    assert!(plugin.problem.is_none(), "{file}: {:?}", plugin.problem);
                }
            }
            other => panic!("{file} was not scanned as a plugin library or bundle: {other:?}"),
        }
    }
    let found = plugins_found_in(&scan.libraries);
    assert_eq!(found.effects.len(), wanted_files.len(), "{:?}", found.not_used);
    // Every folder of the set has its licence text and its notice.
    for set in sets_here {
        // (A set of data is in the folder that it says, and not in one called after its id.)
        let set_folder = folder.join(set["folder"].as_str().unwrap_or_else(|| set["id"].as_str().unwrap()));
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

#[test]
#[ignore = "needs the folder of bundled plugins that tools/bundled_plugins/fetch_bundled_plugins.py makes (FFRS_BUNDLED_PLUGINS)"]
fn the_data_of_surge_xt_is_beside_the_plugins_whole_and_without_what_may_not_ship() {
    let folder = bundled_plugins_folder().expect("FFRS_BUNDLED_PLUGINS names the folder that the script made");
    let platform = fastforwardrs::plugin_bundles::this_platform().expect("builds for this system");
    if platform == "macos-universal" {
        eprintln!("SKIPPED: the data of Surge XT has no build for macOS");
        return;
    }
    let data = folder.join("SurgeXTData");

    let count = |sub: &str, extension: &str| -> usize {
        fn walk(folder: &std::path::Path, extension: &str) -> usize {
            std::fs::read_dir(folder)
                .map(|entries| {
                    entries
                        .flatten()
                        .map(|entry| {
                            let path = entry.path();
                            if path.is_dir() {
                                walk(&path, extension)
                            } else {
                                usize::from(
                                    path.extension()
                                        .is_some_and(|found| found.eq_ignore_ascii_case(extension)),
                                )
                            }
                        })
                        .sum()
                })
                .unwrap_or(0)
        }
        walk(&data.join(sub), extension)
    };
    // (The numbers of release 1.3.4 of Surge XT: 4549 files in all, 3008 of them patches; the Waldorf wavetables, which are left out, were 51 files.)
    assert_eq!(count("patches_factory", "fxp"), 637, "the factory patches");
    assert_eq!(count("patches_3rdparty", "fxp"), 2_371, "the third-party patches");
    assert_eq!(count("fx_presets", "srgfx"), 291, "the presets of the effects");
    assert_eq!(
        count("modulator_presets", "modpreset"),
        100,
        "the presets of the modulators"
    );
    // (The wavetables of the folder `wavetables` are 179 `.wt` files in release 1.3.4, 51 of them the Waldorf ones that are left out; the third-party ones that are `.wav` files are 382.)
    assert_eq!(
        count("wavetables", "wt"),
        179 - 51,
        "the wavetables, without the Waldorf ones"
    );
    assert_eq!(
        count("wavetables_3rdparty", "wav"),
        382,
        "the third-party wavetables that are .wav files"
    );
    assert!(data.join("tuning_library").is_dir() && data.join("skins").is_dir());
    // (What may not ship is not there; what a download writes with the data is.)
    assert!(!data.join("wavetables").join("Waldorf").exists());
    assert!(
        !data
            .join("skins")
            .join("Tutorials")
            .join("10 Adding Fonts.surge-skin")
            .exists()
    );
    for name in ["LICENSE.txt", "AUTHORS.txt", "NOTICE.txt"] {
        assert!(data.join(name).is_file(), "{name}");
    }
    // (Names that are not ASCII were written by the program that took the folder out of the archive.)
    assert!(
        data.join("patches_factory")
            .join("Leads")
            .join("\u{b5}computer.fxp")
            .is_file()
    );
}

fn manifest_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("bundled-plugins")
        .join("manifest.json")
}

/// What an LV2 plugin made of a steady test sound: whether every sample is finite, how loud it was, and how much it changed the sound.
struct WhatItMade {
    all_finite: bool,
    peak: f32,
    rms_out: f64,
    rms_of_the_change: f64,
    latency: u32,
    outputs_of_control: Vec<(u32, f32)>,
}

/// Runs `plugin` for `blocks` blocks of 512 frames at 48 kHz with the sound of two tones and a little noise in each channel, with every control at its default, and says what came out of the last half.
fn run_an_lv2_plugin(plugin: &lv2_host::PluginDescription, blocks: usize) -> Result<WhatItMade, String> {
    const FRAMES: usize = 512;
    let library = lv2_host::Library::open(&plugin.binary).map_err(|error| error.to_string())?;
    let mut instance =
        lv2_host::Instance::new(&library, plugin, 48_000.0, FRAMES as u32).map_err(|error| error.to_string())?;
    let (inputs, outputs) = instance.audio_port_counts();
    let mut seed = 0x9E37_79B9_u32;
    let mut noise = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed as f32 / u32::MAX as f32 - 0.5) * 0.05
    };
    let mut made = WhatItMade {
        all_finite: true,
        peak: 0.0,
        rms_out: 0.0,
        rms_of_the_change: 0.0,
        latency: 0,
        outputs_of_control: Vec::new(),
    };
    let (mut summed_out, mut summed_change, mut counted) = (0.0_f64, 0.0_f64, 0_u64);
    for block in 0..blocks {
        let mut ins: Vec<Vec<f32>> = (0..inputs)
            .map(|channel| {
                (0..FRAMES)
                    .map(|frame| {
                        let at = (block * FRAMES + frame) as f32 / 48_000.0;
                        let tone = if channel == 0 { 220.0 } else { 330.0 };
                        0.25 * (std::f32::consts::TAU * tone * at).sin() + noise()
                    })
                    .collect()
            })
            .collect();
        let heard = ins.clone();
        let mut outs: Vec<Vec<f32>> = (0..outputs).map(|_| vec![0.0; FRAMES]).collect();
        {
            let mut in_slices: Vec<&mut [f32]> = ins.iter_mut().map(Vec::as_mut_slice).collect();
            let mut out_slices: Vec<&mut [f32]> = outs.iter_mut().map(Vec::as_mut_slice).collect();
            instance
                .run(&mut in_slices, &mut out_slices, FRAMES)
                .map_err(|error| error.to_string())?;
        }
        for (channel, out) in outs.iter().enumerate() {
            for (frame, sample) in out.iter().enumerate() {
                made.all_finite &= sample.is_finite();
                made.peak = made.peak.max(sample.abs());
                if block >= blocks / 2 {
                    let same_place = heard.get(channel).map_or(0.0, |input| input[frame]);
                    summed_out += f64::from(*sample).powi(2);
                    summed_change += f64::from(sample - same_place).powi(2);
                    counted += 1;
                }
            }
        }
    }
    made.rms_out = (summed_out / counted.max(1) as f64).sqrt();
    made.rms_of_the_change = (summed_change / counted.max(1) as f64).sqrt();
    made.latency = instance.latency();
    made.outputs_of_control = instance.control_outputs();
    Ok(made)
}

#[test]
#[ignore = "needs the folder of bundled plugins that tools/bundled_plugins/fetch_bundled_plugins.py makes (FFRS_BUNDLED_PLUGINS)"]
fn every_real_lv2_plugin_makes_finite_sound_that_is_not_silent_and_is_changed_by_it() {
    let folder = bundled_plugins_folder().expect("FFRS_BUNDLED_PLUGINS names the folder that the script made");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(manifest_file()).expect("the manifest is read"))
            .expect("the manifest is JSON");
    let platform = fastforwardrs::plugin_bundles::this_platform().expect("builds for this system");
    // (How many bundles the manifest says this system gets: a system with none, macOS until its builds are known, has nothing to try.)
    let expected: usize = manifest["sets"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|set| set["platforms"][platform]["members"].as_array())
        .flatten()
        .filter(|member| member["to"].as_str().is_some_and(|to| to.ends_with(".lv2")))
        .count();
    if expected == 0 {
        eprintln!("SKIPPED: the manifest has no LV2 bundles for {platform}");
        return;
    }
    let bundles = fastforwardrs::plugin_scan::lv2_bundles_in(&folder).unwrap();
    assert_eq!(bundles.len(), expected, "the LV2 bundles of {folder:?}: {bundles:?}");
    let mut table = Vec::new();
    let mut tried = 0;
    for bundle in &bundles {
        let description = lv2_host::describe_bundle(bundle).expect("the bundle is described");
        for plugin in &description.plugins {
            let count = |direction: lv2_host::PortDirection| {
                plugin
                    .ports
                    .iter()
                    .filter(|port| port.kind == lv2_host::PortKind::Audio && port.direction == direction)
                    .count()
            };
            let (inputs, outputs) = (
                count(lv2_host::PortDirection::Input),
                count(lv2_host::PortDirection::Output),
            );
            if !matches!((inputs, outputs), (1, 1) | (2, 2) | (1, 2) | (2, 1)) {
                println!(
                    "{}: not an effect of the shapes the program uses ({inputs} in, {outputs} out)",
                    plugin.name
                );
                continue;
            }
            tried += 1;
            let made = run_an_lv2_plugin(plugin, 200).unwrap_or_else(|error| panic!("{}: {error}", plugin.name));
            println!(
                "{:<28} latency {:>5}  peak {:>7.4}  rms {:>8.5}  change {:>8.5}  control outputs {:?}",
                plugin.name, made.latency, made.peak, made.rms_out, made.rms_of_the_change, made.outputs_of_control
            );
            table.push((plugin.name.clone(), made));
        }
    }
    assert!(tried >= 10, "only {tried} plugins were tried");
    for (name, made) in &table {
        assert!(made.all_finite, "{name} made a sample that is not a number");
        assert!(made.peak < 8.0, "{name} made a peak of {}", made.peak);
        assert!(made.rms_out > 1e-5, "{name} made silence from a steady sound");
        assert!(
            made.outputs_of_control.iter().all(|(_, value)| value.is_finite()),
            "{name}: {:?}",
            made.outputs_of_control
        );
    }
}

/// A value of an effect as the number that the plugin's port holds.
fn number_of(value: &fastforwardrs::project::PropertyValue) -> f32 {
    use fastforwardrs::project::PropertyValue;
    match value {
        PropertyValue::Number(number) => *number,
        PropertyValue::Flag(on) => f32::from(u8::from(*on)),
        PropertyValue::Words(id) => id.parse().expect("the id of an option of an LV2 choice is its number"),
        other => panic!("not a value of an LV2 port: {other:?}"),
    }
}

#[test]
#[ignore = "needs the folder of bundled plugins that tools/bundled_plugins/fetch_bundled_plugins.py makes (FFRS_BUNDLED_PLUGINS)"]
fn the_presets_that_the_real_bundles_say_are_found_and_every_one_of_them_runs_in_its_plugin_with_finite_sound() {
    let folder = bundled_plugins_folder().expect("FFRS_BUNDLED_PLUGINS names the folder that the script made");
    let mut scanner = PluginScanner::new(PathBuf::from(SCANNER), ScanCache::in_memory())
        .with_time_limit(std::time::Duration::from_secs(30));
    let scan = scanner.scan_folders(std::slice::from_ref(&folder));
    let found = plugins_found_in(&scan.libraries);
    let mut tried = Vec::new();
    for effect in &found.effects {
        let fastforwardrs::project::PluginAddress::Lv2 { uri } = &effect.address else {
            continue;
        };
        if effect.presets.is_empty() {
            continue;
        }
        let description = lv2_host::describe_bundle(&effect.library).expect("the bundle is described");
        let plugin = description
            .plugins
            .iter()
            .find(|plugin| plugin.uri == *uri)
            .expect("the plugin is in its bundle");
        let library = lv2_host::Library::open(&plugin.binary).expect("the library loads");
        for preset in &effect.presets {
            let mut instance = lv2_host::Instance::new(&library, plugin, 48_000.0, 512).expect("the plugin is made");
            for (slot, value) in &preset.values {
                assert!(
                    instance.set_control(effect.value_handles[*slot], number_of(value)),
                    "{}: {}: the port of slot {slot} is not a control input",
                    effect.descriptor.name,
                    preset.label
                );
            }
            let (inputs, outputs) = instance.audio_port_counts();
            let (mut peak, mut finite) = (0.0_f32, true);
            for block in 0..50 {
                let mut ins: Vec<Vec<f32>> = (0..inputs)
                    .map(|channel| {
                        (0..512)
                            .map(|frame| {
                                let at = (block * 512 + frame) as f32 / 48_000.0;
                                0.25 * (std::f32::consts::TAU * (220.0 + 110.0 * channel as f32) * at).sin()
                            })
                            .collect()
                    })
                    .collect();
                let mut outs: Vec<Vec<f32>> = (0..outputs).map(|_| vec![0.0; 512]).collect();
                let mut in_slices: Vec<&mut [f32]> = ins.iter_mut().map(Vec::as_mut_slice).collect();
                let mut out_slices: Vec<&mut [f32]> = outs.iter_mut().map(Vec::as_mut_slice).collect();
                instance
                    .run(&mut in_slices, &mut out_slices, 512)
                    .expect("the plugin runs");
                for sample in outs.iter().flatten() {
                    finite &= sample.is_finite();
                    peak = peak.max(sample.abs());
                }
            }
            println!(
                "{:<14} {:<22} {} values ({} left out)  peak {peak:.3}",
                effect.descriptor.name,
                preset.label,
                preset.values.len(),
                preset.left_out
            );
            assert!(
                finite,
                "{}: {}: a sample that is not a number",
                effect.descriptor.name, preset.label
            );
            assert!(
                peak < 16.0,
                "{}: {}: a peak of {peak}",
                effect.descriptor.name,
                preset.label
            );
            tried.push((effect.descriptor.name.to_string(), preset.label.clone()));
        }
    }
    let names: std::collections::BTreeSet<&str> = tried.iter().map(|(name, _)| name.as_str()).collect();
    for wanted in ["mverb", "3 band eq", "ping pong pan", "cycle shifter"] {
        assert!(names.contains(wanted), "no preset of {wanted}: {names:?}");
    }
}
