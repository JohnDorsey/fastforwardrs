// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What an LV2 plugin that was scanned is, as a type of effect (piece 11.2 of `docs/engine-phase-11-lv2-design.md`), with plugins that are described by hand so that each case is
//! exactly what is tried, and the places that bundles are looked for in.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use lv2_host::{PluginDescription, PortDescription, PortDirection, PortKind, ScalePoint, ScannedPlugin};

use super::effects::{effect_of_an_lv2_plugin, key_of_an_lv2_plugin, plugins_found_in};
use super::folders::{folders_looked_in_with, is_an_lv2_bundle, lv2_bundles_in};
use super::scanner::{LibraryOutcome, LibraryReport};
use crate::project::{MOST_VALUES_OF_AN_EFFECT, ParameterControl, PluginAddress, PropertyValue, ValueScale};

fn a_port(index: u32, symbol: &str, direction: PortDirection, kind: PortKind) -> PortDescription {
    PortDescription {
        index,
        symbol: symbol.to_owned(),
        name: symbol.to_uppercase(),
        direction,
        kind,
        default: None,
        minimum: None,
        maximum: None,
        toggled: false,
        integer: false,
        enumeration: false,
        logarithmic: false,
        sample_rate_relative: false,
        connection_is_optional: false,
        reports_latency: false,
        designations: Vec::new(),
        scale_points: Vec::new(),
        unit: None,
        atom: None,
    }
}

fn audio(index: u32, direction: PortDirection) -> PortDescription {
    a_port(index, &format!("audio{index}"), direction, PortKind::Audio)
}

fn a_control(index: u32, symbol: &str) -> PortDescription {
    a_port(index, symbol, PortDirection::Input, PortKind::Control)
}

/// A plugin with one audio input and one audio output, and these other ports (numbered from 2).
fn a_plugin_with(controls: Vec<PortDescription>) -> ScannedPlugin {
    let mut ports = vec![audio(0, PortDirection::Input), audio(1, PortDirection::Output)];
    ports.extend(controls);
    a_plugin_of(ports)
}

fn a_plugin_of(ports: Vec<PortDescription>) -> ScannedPlugin {
    ScannedPlugin {
        description: PluginDescription {
            uri: "http://example.org/plugins/amp".to_owned(),
            name: "The Amp".to_owned(),
            categories: Vec::new(),
            author: String::new(),
            bundle: PathBuf::from("/lv2/amp.lv2"),
            binary: PathBuf::from("/lv2/amp.lv2/amp.so"),
            minor_version: 2,
            micro_version: 5,
            ports,
            required_features: Vec::new(),
            optional_features: Vec::new(),
            extension_data: Vec::new(),
            user_interfaces: Vec::new(),
        },
        problem: None,
        latency: 0,
        has_state: false,
    }
}

fn effect_of(plugin: &ScannedPlugin) -> Result<crate::project::InstalledPlugin, String> {
    effect_of_an_lv2_plugin(Path::new("/lv2/amp.lv2"), plugin)
}

#[test]
fn the_audio_ports_decide_whether_a_plugin_can_be_an_effect_and_the_reason_is_said() {
    let layout = |inputs: u32, outputs: u32| {
        let mut ports = Vec::new();
        for place in 0..inputs {
            ports.push(audio(place, PortDirection::Input));
        }
        for place in 0..outputs {
            ports.push(audio(inputs + place, PortDirection::Output));
        }
        effect_of(&a_plugin_of(ports))
    };

    for (inputs, outputs) in [(1, 1), (2, 2), (1, 2), (2, 1)] {
        let effect = layout(inputs, outputs).unwrap();
        assert_eq!(
            (effect.audio_inputs, effect.audio_outputs),
            (inputs as usize, outputs as usize)
        );
    }
    assert!(layout(0, 1).unwrap_err().contains("no audio input"));
    assert!(layout(1, 0).unwrap_err().contains("no audio output"));
    let error = layout(3, 3).unwrap_err();
    assert!(error.contains("3 audio inputs and 3 audio outputs"), "{error}");
}

#[test]
fn a_plugin_that_could_not_be_made_or_run_is_not_an_effect_and_says_why() {
    let mut plugin = a_plugin_with(Vec::new());
    plugin.problem = Some("it requires log#log".to_owned());

    let error = effect_of(&plugin).unwrap_err();

    assert!(
        error.contains("could not be made or run") && error.contains("log#log"),
        "{error}"
    );
}

#[test]
fn the_effect_is_named_and_keyed_and_versioned_and_given_its_category_from_what_the_bundle_says() {
    let mut plugin = a_plugin_with(Vec::new());

    let effect = effect_of(&plugin).unwrap();
    assert_eq!(
        effect.descriptor.key, "lv2:http_//example.org/plugins/amp",
        "the colons of the URI are underscores"
    );
    assert_eq!(
        key_of_an_lv2_plugin("http://example.org/plugins/amp"),
        effect.descriptor.key
    );
    assert_eq!(effect.descriptor.name, "the amp");
    assert_eq!(effect.descriptor.version, (2, 5), "the minor and the micro version");
    assert_eq!(effect.descriptor.category, "Plugins", "no author and no kind said");
    assert!(
        effect.descriptor.description.contains("http://example.org/plugins/amp")
            && effect.descriptor.description.contains("amp.lv2")
    );
    assert_eq!(
        effect.library,
        Path::new("/lv2/amp.lv2"),
        "the library of the effect is the bundle's folder"
    );
    assert!(matches!(&effect.address, PluginAddress::Lv2 { uri } if uri == "http://example.org/plugins/amp"));
    assert!(!effect.has_a_window && effect.start_warning.is_none());
    assert_eq!(effect.pre_roll_samples, 24_000, "half a second at 48000");

    plugin.description.categories = vec!["Reverb".to_owned(), "Dynamics".to_owned()];
    assert_eq!(
        effect_of(&plugin).unwrap().descriptor.category,
        "Reverb",
        "the first kind it says it is"
    );
    plugin.description.author = "  Someone  ".to_owned();
    assert_eq!(
        effect_of(&plugin).unwrap().descriptor.category,
        "Someone",
        "the author before the kind"
    );
}

#[test]
fn a_switch_a_whole_number_a_choice_and_a_slider_are_what_the_ports_say() {
    let mut toggled = a_control(2, "bypass");
    toggled.toggled = true;
    toggled.default = Some(1.0);
    let mut steps = a_control(3, "steps");
    steps.integer = true;
    steps.default = Some(3.4);
    steps.minimum = Some(1.0);
    steps.maximum = Some(16.0);
    let mut mode = a_control(4, "mode");
    mode.integer = true;
    mode.enumeration = true;
    mode.default = Some(0.6);
    mode.scale_points = vec![
        ScalePoint {
            label: "Clean".to_owned(),
            value: 0.0,
        },
        ScalePoint {
            label: "Hot".to_owned(),
            value: 1.0,
        },
        ScalePoint {
            label: "Odd".to_owned(),
            value: 2.5,
        },
    ];
    let mut cutoff = a_control(5, "cutoff");
    cutoff.default = Some(1000.0);
    cutoff.minimum = Some(20.0);
    cutoff.maximum = Some(20_000.0);
    cutoff.logarithmic = true;
    cutoff.unit = Some("Hz".to_owned());

    let effect = effect_of(&a_plugin_with(vec![toggled, steps, mode, cutoff])).unwrap();

    let by_id = |id: &str| {
        effect
            .descriptor
            .parameters
            .iter()
            .find(|parameter| parameter.id == id)
            .unwrap()
    };
    assert!(matches!(by_id("bypass").control, ParameterControl::Toggle));
    assert_eq!(by_id("bypass").default, PropertyValue::Flag(true));
    assert!(matches!(by_id("steps").control, ParameterControl::Integer));
    assert_eq!(
        (by_id("steps").default.clone(), by_id("steps").range),
        (PropertyValue::Number(3.0), Some((1.0, 16.0)))
    );
    let ParameterControl::Choice(options) = &by_id("mode").control else {
        panic!("a choice")
    };
    let ids: Vec<&str> = options.iter().map(|option| option.id.as_ref()).collect();
    assert_eq!(
        ids,
        vec!["0", "1", "2.5"],
        "a whole value is a whole number, another is as it is"
    );
    assert_eq!(
        by_id("mode").default,
        PropertyValue::Words("1".to_owned()),
        "the option nearest to the default 0.6"
    );
    assert_eq!(by_id("cutoff").scale, ValueScale::Logarithmic);
    assert_eq!(by_id("cutoff").unit.as_ref(), "Hz");
    assert_eq!(effect.value_handles, vec![2, 3, 4, 5], "by the number of the port");
}

#[test]
fn a_range_with_no_end_or_a_logarithmic_one_that_reaches_zero_is_what_it_can_be_and_a_choice_with_no_points_is_a_slider()
 {
    let mut open = a_control(2, "open");
    open.default = Some(2.0);
    let mut log_from_zero = a_control(3, "log");
    log_from_zero.logarithmic = true;
    log_from_zero.minimum = Some(0.0);
    log_from_zero.maximum = Some(10.0);
    let mut empty_choice = a_control(4, "choice");
    empty_choice.enumeration = true;
    empty_choice.minimum = Some(0.0);
    empty_choice.maximum = Some(3.0);
    let mut no_default = a_control(5, "from_the_least");
    no_default.minimum = Some(-3.0);
    no_default.maximum = Some(3.0);

    let effect = effect_of(&a_plugin_with(vec![open, log_from_zero, empty_choice, no_default])).unwrap();

    let by_id = |id: &str| {
        effect
            .descriptor
            .parameters
            .iter()
            .find(|parameter| parameter.id == id)
            .unwrap()
    };
    assert_eq!(by_id("open").range, None, "no range is a field to drag");
    assert_eq!(by_id("open").default, PropertyValue::Number(2.0));
    assert_eq!(
        by_id("log").scale,
        ValueScale::Linear,
        "a logarithmic scale does not reach zero"
    );
    assert!(matches!(by_id("choice").control, ParameterControl::Automatic));
    assert_eq!(
        by_id("from_the_least").default,
        PropertyValue::Number(-3.0),
        "the default is the least when none is said"
    );
}

#[test]
fn a_range_that_is_a_fraction_of_the_sample_rate_is_made_into_the_value_at_the_rate_of_the_sound_engine() {
    let mut rate = a_control(2, "rate");
    rate.sample_rate_relative = true;
    rate.minimum = Some(0.0);
    rate.maximum = Some(0.5);
    rate.default = Some(0.25);

    let effect = effect_of(&a_plugin_with(vec![rate])).unwrap();

    let parameter = &effect.descriptor.parameters[0];
    assert_eq!(parameter.range, Some((0.0, 24_000.0)));
    assert_eq!(parameter.default, PropertyValue::Number(12_000.0));
}

#[test]
fn what_is_not_a_value_a_meter_a_port_of_sound_a_port_of_messages_or_one_that_the_host_decides_is_left_out() {
    let mut meter = a_port(2, "meter", PortDirection::Output, PortKind::Control);
    meter.default = Some(0.0);
    let mut free_wheeling = a_control(3, "free");
    free_wheeling.designations = vec!["http://lv2plug.in/ns/lv2core#freeWheeling".to_owned()];
    let mut tempo = a_control(4, "bpm");
    tempo.designations = vec!["http://lv2plug.in/ns/ext/time#beatsPerMinute".to_owned()];
    let atoms = a_port(5, "events", PortDirection::Input, PortKind::Atom);
    let cv = a_port(6, "cv", PortDirection::Input, PortKind::ControlVoltage);
    let mut enabled = a_control(7, "enabled");
    enabled.designations = vec!["http://lv2plug.in/ns/lv2core#enabled".to_owned()];
    let amount = a_control(8, "amount");

    let effect = effect_of(&a_plugin_with(vec![
        meter,
        free_wheeling,
        tempo,
        atoms,
        cv,
        enabled,
        amount,
    ]))
    .unwrap();

    let ids: Vec<&str> = effect
        .descriptor
        .parameters
        .iter()
        .map(|parameter| parameter.id.as_ref())
        .collect();
    assert_eq!(
        ids,
        vec!["enabled", "amount"],
        "an enabled port is a switch a person can use; the rest is not for a person"
    );
    assert_eq!(effect.value_handles, vec![7, 8]);
}

#[test]
fn a_plugin_with_more_values_than_an_effect_can_have_keeps_the_first_and_is_still_an_effect() {
    let controls: Vec<PortDescription> = (0..MOST_VALUES_OF_AN_EFFECT + 5)
        .map(|place| a_control(2 + place as u32, &format!("c{place}")))
        .collect();

    let effect = effect_of(&a_plugin_with(controls)).unwrap();

    assert_eq!(effect.descriptor.parameters.len(), MOST_VALUES_OF_AN_EFFECT);
    assert_eq!(effect.descriptor.parameters[0].id, "c0");
    assert_eq!(effect.value_handles.len(), MOST_VALUES_OF_AN_EFFECT);
}

#[test]
fn a_bundle_found_by_a_scan_has_its_plugins_as_effects_or_as_plugins_not_used_and_what_could_not_be_read_of_it_is_said()
{
    let usable = a_plugin_with(vec![a_control(2, "gain")]);
    let mut unusable = a_plugin_with(Vec::new());
    unusable.description.uri = "http://example.org/plugins/broken".to_owned();
    unusable.problem = Some("it gave nothing".to_owned());
    let reports = vec![LibraryReport {
        path: PathBuf::from("/lv2/amp.lv2"),
        outcome: LibraryOutcome::ScannedLv2(lv2_host::BundleScan {
            plugins: vec![usable, unusable],
            problems: vec!["urn:x: its ports are not numbered from 0".to_owned()],
        }),
        from_cache: false,
    }];

    let found = plugins_found_in(&reports);

    assert_eq!(found.effects.len(), 1);
    assert_eq!(found.effects[0].descriptor.key, "lv2:http_//example.org/plugins/amp");
    let reasons: Vec<&str> = found.not_used.iter().map(|not_used| not_used.reason.as_str()).collect();
    assert_eq!(reasons.len(), 2, "{reasons:?}");
    assert!(reasons.iter().any(|reason| reason.contains("it gave nothing")));
    assert!(reasons.iter().any(|reason| reason.contains("not numbered")));
}

#[test]
fn the_same_lv2_plugin_in_two_folders_is_one_effect_the_first_wins() {
    let plugin = a_plugin_with(Vec::new());
    let report = |bundle: &str| LibraryReport {
        path: PathBuf::from(bundle),
        outcome: LibraryOutcome::ScannedLv2(lv2_host::BundleScan {
            plugins: vec![plugin.clone()],
            problems: Vec::new(),
        }),
        from_cache: false,
    };

    let found = plugins_found_in(&[report("/first/amp.lv2"), report("/second/amp.lv2")]);

    assert_eq!(found.effects.len(), 1);
    assert_eq!(found.effects[0].library, Path::new("/first/amp.lv2"));
    assert_eq!(found.not_used.len(), 1);
    assert!(found.not_used[0].reason.contains("same key"));
}

// --- where bundles are looked for ---

#[test]
fn a_folder_with_a_manifest_is_a_bundle_and_the_bundles_of_a_folder_are_found_in_the_order_of_their_names() {
    let folder = tempfile::tempdir().unwrap();
    for name in ["b.lv2", "a.lv2", "not_one"] {
        std::fs::create_dir(folder.path().join(name)).unwrap();
    }
    std::fs::write(folder.path().join("b.lv2/manifest.ttl"), "").unwrap();
    std::fs::write(folder.path().join("a.lv2/manifest.ttl"), "").unwrap();
    std::fs::write(folder.path().join("manifest.ttl"), "").unwrap();

    assert!(is_an_lv2_bundle(&folder.path().join("a.lv2")));
    assert!(!is_an_lv2_bundle(&folder.path().join("not_one")));
    assert!(
        !is_an_lv2_bundle(&folder.path().join("manifest.ttl")),
        "a file is not a bundle"
    );
    assert!(!is_an_lv2_bundle(&folder.path().join("not_there")));
    let bundles = lv2_bundles_in(folder.path()).unwrap();
    let names: Vec<&str> = bundles
        .iter()
        .map(|path| path.file_name().unwrap().to_str().unwrap())
        .collect();
    assert_eq!(names, vec!["a.lv2", "b.lv2"]);
    assert_eq!(
        lv2_bundles_in(&folder.path().join("not_there")).unwrap(),
        Vec::<PathBuf>::new()
    );
}

#[test]
fn the_folders_that_bundles_are_looked_for_in_come_after_those_of_the_other_kinds_and_before_the_bundled_ones() {
    let lv2_folder = if cfg!(windows) {
        "C:\\somewhere\\lv2"
    } else {
        "/somewhere/lv2"
    };
    let bundled = vec![PathBuf::from(if cfg!(windows) { "C:\\bundled" } else { "/bundled" })];
    let environment = |name: &str| (name == "LV2_PATH").then(|| OsString::from(lv2_folder));

    let folders = folders_looked_in_with(&environment, Some(Path::new("/home/someone")), &bundled);

    let at = |wanted: &str| folders.iter().position(|folder| folder == Path::new(wanted));
    let lv2_place = at(lv2_folder).expect("the folder of LV2_PATH is looked in");
    let bundled_place = at(&bundled[0].to_string_lossy()).expect("the bundled folder is looked in");
    assert!(lv2_place < bundled_place, "{folders:?}");
    // (A CLAP folder that is always there on this system comes before.)
    let clap_place = folders
        .iter()
        .position(|folder| folder.to_string_lossy().to_ascii_lowercase().contains("clap"));
    assert!(clap_place.is_none_or(|clap| clap < lv2_place), "{folders:?}");
}

#[test]
fn a_switch_whose_default_is_exactly_a_half_is_on() {
    let mut toggled = a_control(2, "half");
    toggled.toggled = true;
    toggled.default = Some(0.5);

    let effect = effect_of(&a_plugin_with(vec![toggled])).unwrap();

    assert_eq!(effect.descriptor.parameters[0].default, PropertyValue::Flag(true));
}

#[test]
fn a_default_outside_the_range_is_brought_into_it() {
    let mut too_high = a_control(2, "high");
    too_high.default = Some(20.0);
    too_high.minimum = Some(0.0);
    too_high.maximum = Some(10.0);
    let mut too_low = a_control(3, "low");
    too_low.default = Some(-5.0);
    too_low.minimum = Some(0.0);
    too_low.maximum = Some(10.0);

    let effect = effect_of(&a_plugin_with(vec![too_high, too_low])).unwrap();

    assert_eq!(effect.descriptor.parameters[0].default, PropertyValue::Number(10.0));
    assert_eq!(effect.descriptor.parameters[1].default, PropertyValue::Number(0.0));
}
