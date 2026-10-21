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
use super::effects::{
    effect_of_a_clap_plugin, effect_of_a_plugin, id_of_a_value, key_of, key_of_a_clap_plugin, plugins_found_in,
};
use super::failure::ScanFailure;
use super::scanner::{LibraryOutcome, LibraryReport, PluginScanner};
use super::*;
use crate::project::{EffectDomain, ParameterControl, PluginAddress, PropertyValue, ValueScale};

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
    // (A path of Windows is a path on Windows only: on the other systems a backslash is a letter of a name.)
    #[cfg(windows)]
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
    assert_eq!(
        effect.address,
        PluginAddress::Ladspa {
            index: 3,
            unique_id: 1234
        }
    );
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
    assert_eq!(effect.value_handles, [0]);
    assert_eq!(
        effect.pre_roll_samples, 0,
        "a LADSPA plugin has no way to say how long it remembers, and is given no sound from before a clip"
    );
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
    assert_eq!(effect.value_handles, [1, 4]);
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
    let variable = library_search_variable();
    // (What the editor had is given, and not read from the process: another test that loads a plugin changes the search path for a moment, and the test would be reading it twice.)
    let had_before = std::env::join_paths([folder.path().join("had_first"), folder.path().join("had_second")]).unwrap();
    let environment = scanner.environment_for_with(&library, Some(had_before));
    let listed: Vec<PathBuf> =
        std::env::split_paths(&environment.iter().find(|(name, _)| name == variable).unwrap().1).collect();
    assert_eq!(listed[0], mine);
    assert_eq!(&listed[1..3], [middle, top]);
    assert_eq!(
        listed[listed.len() - 2..],
        [folder.path().join("had_first"), folder.path().join("had_second")],
        "what the editor had is still there, after, in its order"
    );
    assert!(environment.contains(&("FFRS_OTHER".to_owned(), "kept".to_owned())));
    // The one that is used for real reads the variable of the process (compared only when nothing changed it while the test looked).
    let before = std::env::var_os(variable);
    let for_real = scanner.environment_for(&library);
    if std::env::var_os(variable) == before {
        assert_eq!(for_real, scanner.environment_for_with(&library, before));
    }
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

// --- CLAP plugins as effects (piece 7.8.3) ---

use clap_host::scan::{AudioPort, Parameter, ScannedPlugin as ScannedClapPlugin};

const STEPPED: u32 = 1;
const HIDDEN: u32 = 1 << 2;
const READ_ONLY: u32 = 1 << 3;
const BYPASS: u32 = 1 << 4;
const AUTOMATABLE: u32 = 1 << 5;
const ENUM: u32 = 1 << 16;

fn a_port(channels: u32, kind: Option<&str>) -> AudioPort {
    AudioPort {
        id: 0,
        name: "Main".to_owned(),
        channel_count: channels,
        port_type: kind.map(str::to_owned),
        is_main: true,
    }
}

fn a_parameter(id: u32, name: &str, minimum: f64, maximum: f64, default: f64, flags: u32) -> Parameter {
    Parameter {
        id,
        name: name.to_owned(),
        module: String::new(),
        minimum,
        maximum,
        default,
        flags,
        texts: Vec::new(),
    }
}

/// A stereo effect with these parameters, made by `vendor`.
fn a_clap_plugin(id: &str, parameters: Vec<Parameter>) -> ScannedClapPlugin {
    ScannedClapPlugin {
        description: clap_host::PluginDescription {
            id: id.to_owned(),
            name: "Big Reverb".to_owned(),
            vendor: "Some Maker".to_owned(),
            description: "A reverb".to_owned(),
            ..clap_host::PluginDescription::default()
        },
        audio_inputs: vec![a_port(2, Some("stereo"))],
        audio_outputs: vec![a_port(2, Some("stereo"))],
        parameters,
        latency: 0,
        tail: None,
        has_state: false,
        start: clap_host::start_check::StartCheck::NotChecked,
        problem: None,
    }
}

fn the_effect(plugin: &ScannedClapPlugin) -> crate::project::InstalledPlugin {
    effect_of_a_clap_plugin(Path::new("/clap/maker.clap"), plugin).expect("the plugin can be an effect")
}

#[test]
fn a_clap_plugin_is_an_effect_with_a_key_made_of_its_id_the_category_of_its_maker_and_a_parameter_for_each_value() {
    let plugin = a_clap_plugin(
        "com.example.reverb",
        vec![a_parameter(7, "Decay Time", 0.0, 10.0, 2.5, AUTOMATABLE)],
    );

    let effect = the_effect(&plugin);

    let descriptor = &effect.descriptor;
    assert_eq!(descriptor.key, "clap:com.example.reverb");
    assert_eq!(descriptor.version, (1, 0));
    assert_eq!(descriptor.name, "big reverb");
    assert_eq!(descriptor.category, "Some Maker");
    assert_eq!(descriptor.domain, EffectDomain::Sound);
    assert_eq!(descriptor.description, "A reverb (com.example.reverb, in maker.clap)");
    assert_eq!(
        effect.address,
        PluginAddress::Clap {
            id: "com.example.reverb".to_owned()
        }
    );
    assert_eq!((effect.audio_inputs, effect.audio_outputs), (2, 2));
    assert_eq!(effect.value_handles, [7]);
    let value = &descriptor.parameters[0];
    assert_eq!(
        value.id, "param7",
        "the id of a value is the id the plugin gave the parameter"
    );
    assert_eq!(value.name, "decay time");
    assert_eq!(value.label, "Decay Time");
    assert_eq!(value.default, PropertyValue::Number(2.5));
    assert_eq!(value.range, Some((0.0, 10.0)));
    assert_eq!(
        (value.control.clone(), value.scale),
        (ParameterControl::Automatic, ValueScale::Linear)
    );
}

#[test]
fn the_key_does_not_name_the_file_so_the_same_plugin_in_two_folders_is_one_effect_and_the_first_wins() {
    assert_eq!(key_of_a_clap_plugin("a:b.c"), "clap:a_b.c");
    let plugin = a_clap_plugin("com.example.reverb", Vec::new());
    let reports = |paths: &[&str]| -> Vec<LibraryReport> {
        paths
            .iter()
            .map(|path| LibraryReport {
                path: PathBuf::from(path),
                outcome: LibraryOutcome::ScannedClap(clap_host::scan::LibraryScan {
                    plugins: vec![plugin.clone()],
                    problems: Vec::new(),
                }),
                from_cache: false,
            })
            .collect()
    };

    let found = plugins_found_in(&reports(&["/first/reverb.clap", "/second/other_name.clap"]));

    assert_eq!(found.effects.len(), 1);
    assert_eq!(found.effects[0].library, PathBuf::from("/first/reverb.clap"));
    assert_eq!(found.not_used.len(), 1);
    assert_eq!(found.not_used[0].library, PathBuf::from("/second/other_name.clap"));
    assert_eq!(
        found.not_used[0].label, "com.example.reverb",
        "a CLAP plugin is named by its id"
    );
    assert!(found.not_used[0].reason.contains("same key"));
}

#[test]
fn clap_and_ladspa_plugins_found_together_are_all_there_in_the_order_of_the_libraries() {
    let ladspa = LibraryReport {
        path: PathBuf::from("/ladspa/lib.so"),
        outcome: LibraryOutcome::Scanned(LibraryScan {
            plugins: vec![a_simple_amp()],
            problems: Vec::new(),
        }),
        from_cache: false,
    };
    let clap = LibraryReport {
        path: PathBuf::from("/clap/lib.clap"),
        outcome: LibraryOutcome::ScannedClap(clap_host::scan::LibraryScan {
            plugins: vec![a_clap_plugin("com.example.reverb", Vec::new())],
            problems: Vec::new(),
        }),
        from_cache: false,
    };

    let found = plugins_found_in(&[ladspa, clap]);

    let keys: Vec<&str> = found
        .effects
        .iter()
        .map(|effect| effect.descriptor.key.as_ref())
        .collect();
    assert_eq!(keys.len(), 2, "{keys:?}");
    assert!(keys[0].starts_with("ladspa:") && keys[1] == "clap:com.example.reverb");
}

#[test]
fn stepped_parameters_are_whole_numbers_enums_are_choices_by_number_and_a_bypass_is_a_switch() {
    let mut mode = a_parameter(2, "Mode", 1.0, 3.0, 2.0, AUTOMATABLE | STEPPED | ENUM);
    mode.texts = vec!["off".to_owned(), String::new(), "triple".to_owned()];
    let plugin = a_clap_plugin(
        "com.example.multi",
        vec![
            a_parameter(1, "Steps", 0.0, 8.0, 3.6, AUTOMATABLE | STEPPED),
            mode,
            a_parameter(3, "Bypass", 0.0, 1.0, 1.0, STEPPED | BYPASS),
            a_parameter(4, "Broken Enum", 0.0, 2.0, 7.0, STEPPED | ENUM),
        ],
    );

    let parameters = the_effect(&plugin).descriptor.parameters.into_owned();

    assert_eq!(parameters[0].control, ParameterControl::Integer);
    assert_eq!(parameters[0].default, PropertyValue::Number(4.0));
    assert_eq!(parameters[0].range, Some((0.0, 8.0)));
    let ParameterControl::Choice(options) = &parameters[1].control else {
        panic!("an enum is a choice");
    };
    let shown: Vec<(&str, &str)> = options
        .iter()
        .map(|option| (option.id.as_ref(), option.label.as_ref()))
        .collect();
    assert_eq!(
        shown,
        [("1", "off"), ("2", "2"), ("3", "triple")],
        "the id is the number, a blank text is the number"
    );
    assert_eq!(parameters[1].default, PropertyValue::Words("2".to_owned()));
    assert_eq!(parameters[2].control, ParameterControl::Toggle);
    assert_eq!(parameters[2].default, PropertyValue::Flag(true));
    assert_eq!(
        (parameters[3].control.clone(), parameters[3].default.clone()),
        (ParameterControl::Integer, PropertyValue::Number(2.0)),
        "an enum with no texts is a whole number, and a default past the range is the most"
    );
}

#[test]
fn an_enum_whose_default_is_not_one_of_its_values_begins_as_the_first() {
    let mut mode = a_parameter(2, "Mode", 0.0, 1.0, 5.0, STEPPED | ENUM);
    mode.texts = vec!["a".to_owned(), "b".to_owned()];
    let plugin = a_clap_plugin("com.example.mode", vec![mode]);

    let parameters = the_effect(&plugin).descriptor.parameters.into_owned();

    assert_eq!(
        parameters[0].default,
        PropertyValue::Words("1".to_owned()),
        "5 is clamped to the most, 1"
    );
    let mut odd = a_parameter(2, "Mode", 0.0, 1.0, f64::NAN, STEPPED | ENUM);
    odd.texts = vec!["a".to_owned(), "b".to_owned()];
    let parameters = the_effect(&a_clap_plugin("com.example.mode", vec![odd]))
        .descriptor
        .parameters
        .into_owned();
    assert_eq!(
        parameters[0].default,
        PropertyValue::Words("0".to_owned()),
        "a default that is not a number is the least"
    );
}

#[test]
fn hidden_and_read_only_parameters_are_not_values_and_the_others_keep_their_order() {
    let plugin = a_clap_plugin(
        "com.example.hidden",
        vec![
            a_parameter(1, "First", 0.0, 1.0, 0.0, AUTOMATABLE),
            a_parameter(2, "Hidden", 0.0, 1.0, 0.0, HIDDEN),
            a_parameter(3, "Meter", 0.0, 1.0, 0.0, READ_ONLY),
            a_parameter(4, "Last", 0.0, 1.0, 0.0, AUTOMATABLE),
        ],
    );

    let effect = the_effect(&plugin);

    assert_eq!(effect.value_handles, [1, 4]);
    let ids: Vec<&str> = effect
        .descriptor
        .parameters
        .iter()
        .map(|value| value.id.as_ref())
        .collect();
    assert_eq!(ids, ["param1", "param4"]);
}

#[test]
fn a_range_of_a_thousand_times_above_zero_is_laid_out_along_a_logarithmic_slider_and_no_other_is() {
    let scale_of = |minimum: f64, maximum: f64| {
        let plugin = a_clap_plugin(
            "com.example.scale",
            vec![a_parameter(1, "Frequency", minimum, maximum, minimum, 0)],
        );
        the_effect(&plugin).descriptor.parameters[0].scale
    };

    assert_eq!(scale_of(20.0, 20_000.0), ValueScale::Logarithmic);
    assert_eq!(scale_of(20.0, 19_999.0), ValueScale::Linear);
    assert_eq!(
        scale_of(0.0, 20_000.0),
        ValueScale::Linear,
        "a range that begins at zero has no logarithm"
    );
    assert_eq!(scale_of(-20.0, 20_000.0), ValueScale::Linear);
}

#[test]
fn a_parameter_range_that_is_not_one_makes_the_plugin_not_an_effect_and_says_which_parameter() {
    for (minimum, maximum) in [
        (f64::NEG_INFINITY, 1.0),
        (0.0, f64::INFINITY),
        (f64::NAN, 1.0),
        (2.0, 1.0),
    ] {
        let plugin = a_clap_plugin(
            "com.example.range",
            vec![a_parameter(1, "Odd One", minimum, maximum, 0.0, 0)],
        );

        let reason = effect_of_a_clap_plugin(Path::new("/x.clap"), &plugin).unwrap_err();

        assert!(reason.contains("Odd One") && reason.contains("range"), "{reason}");
    }
    // A hidden parameter is not looked at.
    let hidden = a_clap_plugin(
        "com.example.range",
        vec![a_parameter(1, "Odd One", 2.0, 1.0, 0.0, HIDDEN)],
    );
    assert!(effect_of_a_clap_plugin(Path::new("/x.clap"), &hidden).is_ok());
}

#[test]
fn the_module_of_a_parameter_is_in_its_label_and_a_blank_name_is_made_up() {
    let mut in_a_module = a_parameter(1, "Attack", 0.0, 1.0, 0.0, 0);
    in_a_module.module = "filter/envelope".to_owned();
    let plugin = a_clap_plugin(
        "com.example.module",
        vec![in_a_module, a_parameter(9, "  ", 0.0, 1.0, 0.0, 0)],
    );

    let parameters = the_effect(&plugin).descriptor.parameters.into_owned();

    assert_eq!(parameters[0].label, "filter/envelope: Attack");
    assert_eq!(parameters[0].name, "attack");
    assert_eq!(parameters[1].label, "parameter 9");
}

#[test]
fn the_layouts_of_the_main_ports_decide_whether_a_clap_plugin_can_be_an_effect_and_the_reasons_are_said() {
    let with_ports = |inputs: Vec<AudioPort>, outputs: Vec<AudioPort>| {
        let mut plugin = a_clap_plugin("com.example.ports", Vec::new());
        plugin.audio_inputs = inputs;
        plugin.audio_outputs = outputs;
        effect_of_a_clap_plugin(Path::new("/x.clap"), &plugin)
    };
    for (inputs, outputs) in [(1, 1), (2, 2), (1, 2), (2, 1)] {
        let effect = with_ports(vec![a_port(inputs, None)], vec![a_port(outputs, None)]).unwrap();
        assert_eq!(
            (effect.audio_inputs, effect.audio_outputs),
            (inputs as usize, outputs as usize)
        );
    }
    // (The ports after the main one are given silence and are not looked at.)
    assert!(
        with_ports(
            vec![a_port(2, Some("stereo")), a_port(6, Some("surround"))],
            vec![a_port(1, Some("mono"))]
        )
        .is_ok()
    );

    let reason = |inputs: Vec<AudioPort>, outputs: Vec<AudioPort>| with_ports(inputs, outputs).unwrap_err();
    assert!(reason(Vec::new(), vec![a_port(2, None)]).contains("no audio input"));
    assert!(reason(vec![a_port(2, None)], Vec::new()).contains("no audio output"));
    let surround = reason(vec![a_port(6, Some("surround"))], vec![a_port(2, None)]);
    assert!(
        surround.contains("input") && surround.contains("surround") && surround.contains("6 channels"),
        "{surround}"
    );
    let odd_output = reason(vec![a_port(2, None)], vec![a_port(2, Some("ambisonic"))]);
    assert!(
        odd_output.contains("output") && odd_output.contains("ambisonic"),
        "{odd_output}"
    );
    let wrong_count = reason(vec![a_port(1, Some("stereo"))], vec![a_port(2, None)]);
    assert!(
        wrong_count.contains("1 channel,") && wrong_count.contains("stereo"),
        "{wrong_count}"
    );
    assert!(reason(vec![a_port(0, None)], vec![a_port(2, None)]).contains("0 channels"));
}

#[test]
fn a_clap_plugin_that_could_not_be_made_or_run_or_has_no_name_is_not_an_effect() {
    let mut broken = a_clap_plugin("com.example.broken", Vec::new());
    broken.problem = Some("the plugin said that processing failed".to_owned());
    let reason = effect_of_a_clap_plugin(Path::new("/x.clap"), &broken).unwrap_err();
    assert!(
        reason.contains("could not be made or run") && reason.contains("processing failed"),
        "{reason}"
    );

    let mut nameless = a_clap_plugin("com.example.nameless", Vec::new());
    nameless.description.name = "   ".to_owned();
    let reason = effect_of_a_clap_plugin(Path::new("/x.clap"), &nameless).unwrap_err();
    assert!(
        reason.contains("cannot be used") && reason.contains("no name"),
        "{reason}"
    );
}

#[test]
fn a_plugin_that_keeps_a_state_and_shows_few_parameters_says_that_only_its_window_can_change_it() {
    let mut plugin = a_clap_plugin("com.example.state", vec![a_parameter(1, "Only", 0.0, 1.0, 0.0, 0)]);
    plugin.has_state = true;
    assert!(
        the_effect(&plugin)
            .descriptor
            .description
            .contains("only its own window can change")
    );

    let many: Vec<Parameter> = (0..3)
        .map(|number| a_parameter(number, "P", 0.0, 1.0, 0.0, 0))
        .collect();
    let mut with_many = a_clap_plugin("com.example.state", many);
    with_many.has_state = true;
    assert!(!the_effect(&with_many).descriptor.description.contains("window"));
    plugin.has_state = false;
    assert!(!the_effect(&plugin).descriptor.description.contains("window"));
    // A plugin that says nothing of itself is described by its id and the file.
    plugin.description.description = String::new();
    assert_eq!(
        the_effect(&plugin).descriptor.description,
        "com.example.state, in maker.clap"
    );
}

#[test]
fn a_plugin_with_more_parameters_than_an_effect_can_have_values_keeps_the_first_ones() {
    let parameters: Vec<Parameter> = (0..300)
        .map(|number| a_parameter(number, "P", 0.0, 1.0, 0.0, 0))
        .collect();
    let plugin = a_clap_plugin("com.example.big", parameters);

    let effect = the_effect(&plugin);

    assert_eq!(effect.value_handles.len(), crate::project::MOST_VALUES_OF_AN_EFFECT);
    assert_eq!(effect.value_handles[0], 0);
    assert_eq!(*effect.value_handles.last().unwrap(), 254);
}

#[test]
fn the_edges_of_what_a_clap_plugin_says_are_kept_as_they_are() {
    // A range of one number is a range; a bypass at one half is on; the vendor is trimmed; a stereo input that is a surround port of two channels is not stereo.
    let plugin = a_clap_plugin(
        "com.example.edges",
        vec![
            a_parameter(1, "Fixed", 3.0, 3.0, 3.0, 0),
            a_parameter(2, "Bypass", 0.0, 1.0, 0.5, STEPPED | BYPASS),
        ],
    );
    let mut padded = plugin.clone();
    padded.description.vendor = "  Some Maker  ".to_owned();

    let effect = the_effect(&padded);

    assert_eq!(effect.descriptor.parameters[0].range, Some((3.0, 3.0)));
    assert_eq!(effect.descriptor.parameters[1].default, PropertyValue::Flag(true));
    assert_eq!(effect.descriptor.category, "Some Maker");
    let mut surround = plugin;
    surround.audio_inputs = vec![a_port(2, Some("surround"))];
    let reason = effect_of_a_clap_plugin(Path::new("/x.clap"), &surround).unwrap_err();
    assert!(reason.contains("surround") && reason.contains("2 channels"), "{reason}");
}

#[test]
fn a_clap_plugin_asks_for_sound_from_before_a_clip_as_long_as_its_tail_up_to_two_seconds_and_half_a_second_where_it_says_nothing()
 {
    let asks = |tail: Option<u32>| {
        let mut plugin = a_clap_plugin("com.example.tail", Vec::new());
        plugin.tail = tail;
        the_effect(&plugin).pre_roll_samples
    };

    assert_eq!(asks(Some(1000)), 1000);
    assert_eq!(asks(Some(0)), 0, "a plugin that says it has no tail remembers nothing");
    assert_eq!(asks(Some(200_000)), 96_000, "two seconds at most");
    assert_eq!(asks(Some(u32::MAX)), 96_000, "a tail for ever is two seconds");
    assert_eq!(asks(None), 24_000, "nothing said is half a second");
}

#[test]
fn a_clap_plugin_whose_start_depends_on_memory_it_did_not_write_carries_the_warning_and_no_other_does() {
    use clap_host::start_check::StartCheck;
    let mut plugin = a_clap_plugin("com.example.reverb", Vec::new());
    for steady in [StartCheck::NotChecked, StartCheck::Steady, StartCheck::DiffersByItself] {
        plugin.start = steady;
        assert_eq!(the_effect(&plugin).start_warning, None);
    }

    plugin.start = StartCheck::Unreliable {
        filled_with: 0xFF,
        relative_difference: 0.9,
    };

    let warning = the_effect(&plugin)
        .start_warning
        .expect("the warning goes with the effect");
    assert_eq!(Some(warning), plugin.start.warning());
}
