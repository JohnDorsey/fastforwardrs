// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugins that were found, as types of effect: the key, the values and the channel layouts (G10), and where the libraries that plugin libraries need are looked for.

use std::fs;
use std::path::{Path, PathBuf};

use ladspa_host::{
    DefaultHint, LibraryScan, PluginDescription, PluginProperties, PortDescription, PortDirection, PortHint, PortKind,
    ScannedPlugin,
};

use super::cache::{CachedOutcome, FileFingerprint, ScanCache};
use super::effects::{effect_of_a_plugin, id_of_a_value, key_of, plugins_found_in};
use super::failure::ScanFailure;
use super::scanner::{LibraryOutcome, LibraryReport, PluginScanner};
use super::*;
use crate::project::{EffectDomain, ParameterControl, PropertyValue, ValueScale};

fn hint() -> PortHint {
    PortHint {
        lower: None,
        upper: None,
        toggled: false,
        relative_to_sample_rate: false,
        logarithmic: false,
        integer: false,
        default: DefaultHint::None,
    }
}

fn port(name: &str, direction: PortDirection, kind: PortKind, hint: PortHint) -> PortDescription {
    PortDescription {
        name: name.to_owned(),
        direction,
        kind,
        hint,
    }
}

fn audio_in(name: &str) -> PortDescription {
    port(name, PortDirection::Input, PortKind::Audio, hint())
}

fn audio_out(name: &str) -> PortDescription {
    port(name, PortDirection::Output, PortKind::Audio, hint())
}

fn control(name: &str, hint: PortHint) -> PortDescription {
    port(name, PortDirection::Input, PortKind::Control, hint)
}

fn plugin(label: &str, name: &str, maker: &str, ports: Vec<PortDescription>) -> ScannedPlugin {
    ScannedPlugin {
        description: PluginDescription {
            index: 3,
            unique_id: 1234,
            label: label.to_owned(),
            name: name.to_owned(),
            maker: maker.to_owned(),
            copyright: String::new(),
            properties: PluginProperties::default(),
            ports,
        },
        problem: None,
    }
}

fn a_simple_amp() -> ScannedPlugin {
    plugin(
        "amp",
        "Simple Amplifier",
        "Somebody",
        vec![
            control(
                "Amps gain (dB)",
                PortHint {
                    lower: Some(-70.0),
                    upper: Some(70.0),
                    default: DefaultHint::Zero,
                    ..hint()
                },
            ),
            audio_in("Input"),
            audio_out("Output"),
        ],
    )
}

// --- the key and the ids ---

#[test]
fn the_key_is_the_library_file_without_its_ending_and_the_label() {
    assert_eq!(
        key_of(Path::new("/plugins/amp_1181.so"), "amp_1181"),
        "ladspa:amp_1181:amp_1181"
    );
    assert_eq!(key_of(Path::new("C:\\ladspa\\caps.dll"), "Plate"), "ladspa:caps:Plate");
    assert_eq!(
        key_of(Path::new("/a/b:c.so"), "x:y"),
        "ladspa:b_c:x_y",
        "a colon would make the key something else"
    );
}

#[test]
fn the_id_of_a_value_is_its_port_and_its_name_in_lower_case_letters_and_digits() {
    assert_eq!(id_of_a_value(0, "Gain"), "port0_gain");
    assert_eq!(id_of_a_value(7, "Cutoff frequency (Hz)"), "port7_cutoff_frequency_hz");
    assert_eq!(id_of_a_value(2, "--Q!!"), "port2_q");
    assert_eq!(id_of_a_value(4, "***"), "port4_");
}

// --- what a plugin is as an effect ---

#[test]
fn a_plugin_with_a_control_an_input_and_an_output_is_a_sound_effect_in_the_category_of_its_maker() {
    let effect = effect_of_a_plugin(Path::new("/plugins/amp_1181.so"), &a_simple_amp()).expect("it is an effect");
    let descriptor = &effect.descriptor;
    assert_eq!(descriptor.key, "ladspa:amp_1181:amp");
    assert_eq!(descriptor.version, (1, 0));
    assert_eq!(descriptor.name, "simple amplifier");
    assert_eq!(descriptor.category, "Somebody");
    assert_eq!(descriptor.description, "amp, in amp_1181.so");
    assert_eq!(descriptor.domain, EffectDomain::Sound);
    assert!(!descriptor.changes_each_pixel_by_itself);
    assert_eq!(descriptor.problem(), None);
    assert_eq!((effect.plugin_index, effect.unique_id), (3, 1234));
    assert_eq!((effect.audio_inputs, effect.audio_outputs), (1, 1));
    assert_eq!(effect.library, PathBuf::from("/plugins/amp_1181.so"));
    assert_eq!(descriptor.parameters.len(), 1);
    let gain = &descriptor.parameters[0];
    assert_eq!(gain.id, "port0_amps_gain_db");
    assert_eq!(gain.name, "amps gain (db)");
    assert_eq!(gain.label, "Amps gain (dB)");
    assert_eq!(gain.default, PropertyValue::Number(0.0));
    assert_eq!(gain.range, Some((-70.0, 70.0)));
    assert_eq!(gain.scale, ValueScale::Linear);
    assert_eq!(gain.control, ParameterControl::Automatic);
    assert_eq!(effect.value_ports, [0]);
}

#[test]
fn a_plugin_that_has_no_name_is_called_by_its_label_and_one_with_no_maker_is_in_a_category_for_plugins() {
    let mut nameless = a_simple_amp();
    nameless.description.name = "  ".to_owned();
    nameless.description.label = "Plate_Reverb".to_owned();
    nameless.description.maker = String::new();
    let effect = effect_of_a_plugin(Path::new("/x/plate.so"), &nameless).unwrap();
    assert_eq!(effect.descriptor.name, "plate_reverb");
    assert_eq!(effect.descriptor.category, "Plugins");
}

#[test]
fn only_the_control_inputs_are_values_and_the_value_ports_are_their_ports_in_order() {
    let ports = vec![
        audio_in("Input"),
        control("Cutoff", hint()),
        audio_out("Output"),
        port("Level meter", PortDirection::Output, PortKind::Control, hint()),
        control("Resonance", hint()),
    ];
    let effect = effect_of_a_plugin(Path::new("/x/filter.so"), &plugin("filter", "Filter", "M", ports)).unwrap();
    let ids: Vec<&str> = effect
        .descriptor
        .parameters
        .iter()
        .map(|value| value.id.as_ref())
        .collect();
    assert_eq!(
        ids,
        ["port1_cutoff", "port4_resonance"],
        "a control output is not a value"
    );
    assert_eq!(effect.value_ports, [1, 4]);
}

#[test]
fn the_hints_make_a_toggle_a_whole_number_a_logarithmic_scale_and_a_range_at_the_rate_of_the_sound_engine() {
    let ports = vec![
        audio_in("Input"),
        audio_out("Output"),
        control(
            "Invert",
            PortHint {
                toggled: true,
                default: DefaultHint::One,
                ..hint()
            },
        ),
        control(
            "Steps",
            PortHint {
                lower: Some(1.0),
                upper: Some(8.0),
                integer: true,
                default: DefaultHint::Middle,
                ..hint()
            },
        ),
        control(
            "Cutoff",
            PortHint {
                lower: Some(0.001),
                upper: Some(0.45),
                relative_to_sample_rate: true,
                logarithmic: true,
                default: DefaultHint::Low,
                ..hint()
            },
        ),
        control(
            "Through zero",
            PortHint {
                lower: Some(0.0),
                upper: Some(10.0),
                logarithmic: true,
                ..hint()
            },
        ),
        control(
            "Only a floor",
            PortHint {
                lower: Some(0.0),
                ..hint()
            },
        ),
    ];
    let effect = effect_of_a_plugin(Path::new("/x/mixed.so"), &plugin("mixed", "Mixed", "M", ports)).unwrap();
    let values = &effect.descriptor.parameters;
    assert_eq!(values[0].control, ParameterControl::Toggle);
    assert_eq!(values[0].default, PropertyValue::Flag(true));
    assert_eq!(values[0].range, None);
    let off_to_begin_with = effect_of_a_plugin(
        Path::new("/x/off.so"),
        &plugin(
            "off",
            "Off",
            "M",
            vec![
                audio_in("In"),
                audio_out("Out"),
                control(
                    "Invert",
                    PortHint {
                        toggled: true,
                        default: DefaultHint::Zero,
                        ..hint()
                    },
                ),
                control(
                    "Nothing said",
                    PortHint {
                        toggled: true,
                        ..hint()
                    },
                ),
            ],
        ),
    )
    .unwrap();
    assert_eq!(
        off_to_begin_with.descriptor.parameters[0].default,
        PropertyValue::Flag(false),
        "a toggle that begins at zero is off"
    );
    assert_eq!(
        off_to_begin_with.descriptor.parameters[1].default,
        PropertyValue::Flag(false),
        "and one that says nothing is off"
    );
    assert_eq!(values[1].control, ParameterControl::Integer);
    assert_eq!(values[1].default, PropertyValue::Number(5.0));
    assert_eq!(values[1].range, Some((1.0, 8.0)));
    assert_eq!(values[2].scale, ValueScale::Logarithmic);
    let (low, high) = values[2].range.unwrap();
    assert!(
        (low - 48.0).abs() < 1e-3 && (high - 21_600.0).abs() < 1e-2,
        "{low} to {high}: Hz at 48000 a second"
    );
    assert!(matches!(values[2].default, PropertyValue::Number(hertz) if hertz > low && hertz < high));
    assert_eq!(
        values[3].scale,
        ValueScale::Linear,
        "a logarithmic range that reaches zero has no logarithm"
    );
    assert_eq!(
        values[4].range, None,
        "a range with one end is no range: the value is a field to drag"
    );
    assert_eq!(effect.descriptor.problem(), None);
}

#[test]
fn the_channel_layouts_that_can_be_effects_are_one_to_one_two_to_two_one_to_two_and_two_to_one() {
    let layout = |inputs: usize, outputs: usize| {
        let mut ports = vec![control("Value", hint())];
        ports.extend((0..inputs).map(|number| audio_in(&format!("In {number}"))));
        ports.extend((0..outputs).map(|number| audio_out(&format!("Out {number}"))));
        effect_of_a_plugin(Path::new("/x/layout.so"), &plugin("layout", "Layout", "M", ports))
    };
    for (inputs, outputs) in [(1, 1), (2, 2), (1, 2), (2, 1)] {
        let effect = layout(inputs, outputs).unwrap_or_else(|why| panic!("{inputs} in, {outputs} out: {why}"));
        assert_eq!((effect.audio_inputs, effect.audio_outputs), (inputs, outputs));
    }
    assert!(layout(0, 1).unwrap_err().contains("generator"), "no audio input");
    assert!(layout(0, 0).unwrap_err().contains("no audio input"));
    assert!(layout(1, 0).unwrap_err().contains("no audio output"));
    let odd = layout(3, 3).unwrap_err();
    assert!(odd.contains("3 audio inputs and 3 audio outputs"), "{odd}");
    assert!(layout(4, 1).is_err() && layout(1, 3).is_err() && layout(2, 3).is_err() && layout(3, 1).is_err());
}

#[test]
fn a_plugin_that_could_not_be_made_or_whose_hints_make_no_sense_is_not_an_effect_and_says_why() {
    let mut broken = a_simple_amp();
    broken.problem = Some("its instantiate gave nothing".to_owned());
    assert!(
        effect_of_a_plugin(Path::new("/x/a.so"), &broken)
            .unwrap_err()
            .contains("its instantiate gave nothing")
    );
    let backwards = plugin(
        "backwards",
        "Backwards",
        "M",
        vec![
            control(
                "Value",
                PortHint {
                    lower: Some(5.0),
                    upper: Some(1.0),
                    ..hint()
                },
            ),
            audio_in("In"),
            audio_out("Out"),
        ],
    );
    let why = effect_of_a_plugin(Path::new("/x/b.so"), &backwards).unwrap_err();
    assert!(why.contains("cannot be used") && why.contains("range"), "{why}");
}

// --- the plugins of several libraries ---

fn scanned(path: &str, plugins: Vec<ScannedPlugin>) -> LibraryReport {
    LibraryReport {
        path: PathBuf::from(path),
        outcome: LibraryOutcome::Scanned(LibraryScan {
            plugins,
            problems: Vec::new(),
        }),
        from_cache: false,
    }
}

#[test]
fn the_effects_of_the_libraries_are_in_the_order_of_the_libraries_the_first_of_two_with_one_key_wins_and_what_is_left_out_is_listed()
 {
    let generator = plugin(
        "noise",
        "Noise",
        "M",
        vec![port("Out", PortDirection::Output, PortKind::Audio, hint())],
    );
    let reports = vec![
        scanned("/mine/one.so", vec![a_simple_amp(), generator]),
        LibraryReport {
            path: PathBuf::from("/mine/failed.so"),
            outcome: LibraryOutcome::Failed(ScanFailure::GaveNoAnswer { during: None }),
            from_cache: false,
        },
        scanned("/usr/lib/ladspa/one.so", vec![a_simple_amp()]),
        scanned("/usr/lib/ladspa/two.so", vec![a_simple_amp()]),
    ];
    let found = plugins_found_in(&reports);
    let keys: Vec<String> = found
        .effects
        .iter()
        .map(|effect| effect.descriptor.key.to_string())
        .collect();
    assert_eq!(keys, ["ladspa:one:amp", "ladspa:two:amp"]);
    assert_eq!(
        found.effects[0].library,
        PathBuf::from("/mine/one.so"),
        "the folder that comes first"
    );
    let left_out: Vec<(String, &str)> = found
        .not_used
        .iter()
        .map(|left_out| (left_out.library.display().to_string(), left_out.label.as_str()))
        .collect();
    assert_eq!(
        left_out,
        [
            ("/mine/one.so".to_owned(), "noise"),
            ("/usr/lib/ladspa/one.so".to_owned(), "amp")
        ]
    );
    assert!(found.not_used[0].reason.contains("generator"));
    assert!(found.not_used[1].reason.contains("another plugin with the same key"));
}

// --- where the libraries that plugin libraries need are looked for ---

fn a_library_three_folders_deep(root: &Path) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let top = root.join("lmms");
    let middle = top.join("plugins");
    let inside = middle.join("ladspa");
    fs::create_dir_all(&inside).unwrap();
    let library = inside.join(format!("amp.{}", library_file_extension()));
    fs::write(&library, b"x").unwrap();
    (library, inside, middle, top)
}

#[test]
fn the_folders_a_person_named_come_first_then_the_parent_folders_nearest_first_and_the_folder_of_the_library_is_left_to_the_loader()
 {
    let folder = tempfile::tempdir().unwrap();
    let (library, inside, middle, top) = a_library_three_folders_deep(folder.path());
    let mine = folder.path().join("dependencies");
    fs::create_dir(&mine).unwrap();
    let with_parents = dependency_search_folders(&library, std::slice::from_ref(&mine), true);
    assert_eq!(with_parents[0], mine);
    assert_eq!(
        &with_parents[1..4],
        [middle.clone(), top.clone(), folder.path().to_owned()]
    );
    assert!(!with_parents.contains(&inside), "(the loader looks there first anyway)");
    assert!(
        with_parents.iter().all(|found| found.parent().is_some()),
        "never the top of the file system"
    );
    let without = dependency_search_folders(&library, std::slice::from_ref(&mine), false);
    assert_eq!(
        without,
        std::slice::from_ref(&mine),
        "the parent folders are looked in only if that is chosen"
    );
    assert!(dependency_search_folders(&library, &[], false).is_empty());
}

#[test]
fn a_folder_that_is_not_there_is_left_out_and_a_folder_is_there_once() {
    let folder = tempfile::tempdir().unwrap();
    let (library, _, middle, _) = a_library_three_folders_deep(folder.path());
    let gone = folder.path().join("not_there");
    let found = dependency_search_folders(&library, &[gone, middle.clone(), middle.clone()], true);
    assert_eq!(found.iter().filter(|each| **each == middle).count(), 1);
    assert_eq!(found[0], middle, "the one a person named keeps its place");
    assert!(!found.iter().any(|each| each.ends_with("not_there")));
}

#[test]
fn the_scan_program_is_started_with_the_variable_of_the_loader_listing_the_folders_before_what_it_had() {
    let folder = tempfile::tempdir().unwrap();
    let (library, _, middle, top) = a_library_three_folders_deep(folder.path());
    let mine = folder.path().join("dependencies");
    fs::create_dir(&mine).unwrap();
    let scanner = PluginScanner::new(PathBuf::from("not-a-program"), ScanCache::in_memory())
        .with_environment_variable("FFRS_OTHER", "kept")
        .looking_for_dependencies_in(vec![mine.clone()], true);
    let environment = scanner.environment_for(&library);
    let variable = library_search_variable();
    let listed: Vec<PathBuf> =
        std::env::split_paths(&environment.iter().find(|(name, _)| name == variable).unwrap().1).collect();
    assert_eq!(listed[0], mine);
    assert_eq!(&listed[1..3], [middle, top]);
    let had: Vec<PathBuf> = std::env::var_os(variable)
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();
    assert!(listed.ends_with(&had), "what the editor had is still there, after");
    assert!(environment.contains(&("FFRS_OTHER".to_owned(), "kept".to_owned())));
    // With nothing to add the variable is not touched.
    let plain = PluginScanner::new(PathBuf::from("not-a-program"), ScanCache::in_memory());
    assert!(plain.environment_for(&library).iter().all(|(name, _)| name != variable));
}

#[test]
fn the_variable_of_the_loader_is_the_one_of_the_system() {
    let expected = if cfg!(windows) {
        "PATH"
    } else if cfg!(target_os = "macos") {
        "DYLD_FALLBACK_LIBRARY_PATH"
    } else {
        "LD_LIBRARY_PATH"
    };
    assert_eq!(library_search_variable(), expected);
}

// --- scans that failed for want of a library are tried again when the places to look change ---

fn a_cache_with_each_kind_of_scan(folder: &Path) -> (ScanCache, Vec<PathBuf>) {
    let mut cache = ScanCache::in_memory();
    let paths = fill_with_each_kind_of_scan(&mut cache, folder);
    (cache, paths)
}

fn fill_with_each_kind_of_scan(cache: &mut ScanCache, folder: &Path) -> Vec<PathBuf> {
    let failures = [
        CachedOutcome::Failed(ScanFailure::CouldNotBeLoaded("module missing".to_owned())),
        CachedOutcome::Failed(ScanFailure::NotALadspaLibrary("no symbol".to_owned())),
        CachedOutcome::Failed(ScanFailure::Crashed {
            how: "signal 11".to_owned(),
            during: None,
        }),
        CachedOutcome::Failed(ScanFailure::TookTooLong {
            seconds: 5,
            during: None,
        }),
        CachedOutcome::Scanned(LibraryScan::default()),
    ];
    let mut paths = Vec::new();
    for (number, outcome) in failures.into_iter().enumerate() {
        let path = folder.join(format!("library{number}.{}", library_file_extension()));
        fs::write(&path, b"x").unwrap();
        cache.record(&path, FileFingerprint::of(&path).unwrap(), outcome);
        paths.push(path);
    }
    paths
}

#[test]
fn when_the_places_to_look_for_libraries_change_the_scans_that_failed_for_want_of_one_are_not_believed_and_the_others_are()
 {
    let folder = tempfile::tempdir().unwrap();
    let (mut cache, paths) = a_cache_with_each_kind_of_scan(folder.path());
    let believed = |cache: &ScanCache, path: &Path| cache.lookup(path, &FileFingerprint::of(path).unwrap()).is_some();
    cache.set_search_signature("");
    assert!(
        paths.iter().all(|path| believed(&cache, path)),
        "the same places: nothing is forgotten"
    );
    cache.set_search_signature("/some/folder|parents=true");
    assert!(
        !believed(&cache, &paths[0]),
        "could not be loaded: the library may be found now"
    );
    assert!(!believed(&cache, &paths[1]), "not a plugin library: the same");
    assert!(
        believed(&cache, &paths[2]),
        "a crash is the library's doing, wherever its libraries are"
    );
    assert!(believed(&cache, &paths[3]), "so is taking for ever");
    assert!(believed(&cache, &paths[4]), "and a scan that went well");
}

#[test]
fn the_places_to_look_for_libraries_are_kept_with_the_cache_so_that_the_next_run_knows_whether_they_changed() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("scans.json");
    let (made, paths) = a_cache_with_each_kind_of_scan(folder.path());
    let mut kept = ScanCache::at(file.clone());
    for path in &paths {
        let fingerprint = FileFingerprint::of(path).unwrap();
        kept.record(path, fingerprint, made.lookup(path, &fingerprint).unwrap().clone());
    }
    kept.set_search_signature("/some/folder|parents=true");
    assert_eq!(kept.len(), 3, "the two that depend on the places were forgotten");
    kept.save();

    let mut again = ScanCache::at(file);
    assert_eq!(again.len(), 3);
    let late = folder.path().join(format!("late.{}", library_file_extension()));
    fs::write(&late, b"x").unwrap();
    again.record(
        &late,
        FileFingerprint::of(&late).unwrap(),
        CachedOutcome::Failed(ScanFailure::CouldNotBeLoaded("missing".to_owned())),
    );
    again.set_search_signature("/some/folder|parents=true");
    assert_eq!(
        again.len(),
        4,
        "the places are those the file says they were: nothing is forgotten"
    );
    again.set_search_signature("/another/folder|parents=true");
    assert_eq!(
        again.len(),
        3,
        "other places: what failed for want of a library is forgotten"
    );
}

#[test]
fn a_scanner_is_told_where_to_look_when_it_is_made_so_that_its_cache_is_never_believed_wrongly() {
    let folder = tempfile::tempdir().unwrap();
    // (A cache that was made by a scanner that looked nowhere but beside the library.)
    let mut cache = ScanCache::in_memory();
    cache.set_search_signature("|parents=false");
    let paths = fill_with_each_kind_of_scan(&mut cache, folder.path());
    // `new` says that nothing is looked in but the folder of the library: the cache that was made with the same is believed...
    let scanner = PluginScanner::new(PathBuf::from("not-a-program"), cache);
    assert_eq!(scanner.cache().len(), 5);
    // ...and one that is told of a folder is not (the first two).
    let scanner = scanner.looking_for_dependencies_in(vec![folder.path().to_owned()], false);
    assert_eq!(scanner.cache().len(), 3);
    assert!(paths.len() == 5);
}
