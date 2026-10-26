// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The reading of what a bundle says (`description.rs`), with bundles written in the way that plugins' own are: prefixes, blank nodes for ports, comma lists, names in several
//! languages. Nothing here loads a library.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use crate::description::*;

/// A bundle folder with these files (name, text).
fn bundle_with(files: &[(&str, &str)]) -> TempDir {
    let folder = tempfile::tempdir().unwrap();
    for (name, text) in files {
        let path = folder.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    folder
}

const MANIFEST: &str = r#"
@prefix lv2:  <http://lv2plug.in/ns/lv2core#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

<urn:test:amp>
    a lv2:Plugin ;
    lv2:binary <amp.dll> ;
    rdfs:seeAlso <amp.ttl> .
"#;

/// A plugin of the kind that is in the world: an amplifier with every kind of port.
const AMP: &str = r#"
@prefix rdf:  <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix atom: <http://lv2plug.in/ns/ext/atom#> .
@prefix doap: <http://usefulinc.com/ns/doap#> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .
@prefix lv2:  <http://lv2plug.in/ns/lv2core#> .
@prefix midi: <http://lv2plug.in/ns/ext/midi#> .
@prefix pprop: <http://lv2plug.in/ns/ext/port-props#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rsz:  <http://lv2plug.in/ns/ext/resize-port#> .
@prefix ui:   <http://lv2plug.in/ns/extensions/ui#> .
@prefix unit: <http://lv2plug.in/ns/extensions/units#> .
@prefix urid: <http://lv2plug.in/ns/ext/urid#> .
@prefix work: <http://lv2plug.in/ns/ext/worker#> .
@prefix state: <http://lv2plug.in/ns/ext/state#> .

<urn:test:amp>
    a lv2:Plugin , lv2:AmplifierPlugin , lv2:DynamicsPlugin ;
    doap:name "Verst\u00e4rker"@de , "Amp"@en ;
    doap:maintainer [ foaf:name "Somebody" ] ;
    lv2:minorVersion 3 ;
    lv2:microVersion 7 ;
    lv2:requiredFeature urid:map ;
    lv2:optionalFeature lv2:hardRTCapable , work:schedule ;
    lv2:extensionData state:interface , work:interface ;
    ui:ui <urn:test:amp#ui> ;
    lv2:port [
        a lv2:InputPort , lv2:AudioPort ;
        lv2:index 0 ;
        lv2:symbol "in" ;
        lv2:name "In" ;
    ] , [
        a lv2:OutputPort , lv2:AudioPort ;
        lv2:index 1 ;
        lv2:symbol "out" ;
        lv2:name "Out" ;
    ] , [
        a lv2:InputPort , lv2:ControlPort ;
        lv2:index 2 ;
        lv2:symbol "gain" ;
        lv2:name "Gain" , "Verst\u00e4rkung"@de ;
        lv2:default 0.0 ;
        lv2:minimum -40.0 ;
        lv2:maximum 12.0 ;
        unit:unit unit:db ;
    ] , [
        a lv2:InputPort , lv2:ControlPort ;
        lv2:index 3 ;
        lv2:symbol "bypass" ;
        lv2:name "Bypass" ;
        lv2:default 0 ;
        lv2:minimum 0 ;
        lv2:maximum 1 ;
        lv2:portProperty lv2:toggled , lv2:connectionOptional ;
    ] , [
        a lv2:InputPort , lv2:ControlPort ;
        lv2:index 4 ;
        lv2:symbol "mode" ;
        lv2:name "Mode" ;
        lv2:default 1 ;
        lv2:minimum 0 ;
        lv2:maximum 2 ;
        lv2:portProperty lv2:integer , lv2:enumeration ;
        lv2:scalePoint [ rdfs:label "Warm" ; rdf:value 1 ] , [ rdfs:label "Clean" ; rdf:value 0 ] , [ rdfs:label "Hot" ; rdf:value 2 ] ;
    ] , [
        a lv2:InputPort , lv2:ControlPort ;
        lv2:index 5 ;
        lv2:symbol "cutoff" ;
        lv2:name "Cutoff" ;
        lv2:default 1000.0 ;
        lv2:minimum 20.0 ;
        lv2:maximum 20000.0 ;
        lv2:portProperty pprop:logarithmic ;
        unit:unit unit:hz ;
    ] , [
        a lv2:InputPort , lv2:ControlPort ;
        lv2:index 6 ;
        lv2:symbol "rate" ;
        lv2:name "Rate" ;
        lv2:default 0.25 ;
        lv2:minimum 0.0 ;
        lv2:maximum 0.5 ;
        lv2:portProperty lv2:sampleRate ;
    ] , [
        a lv2:OutputPort , lv2:ControlPort ;
        lv2:index 7 ;
        lv2:symbol "latency" ;
        lv2:name "Latency" ;
        lv2:designation lv2:latency ;
        lv2:portProperty lv2:integer ;
    ] , [
        a lv2:InputPort , atom:AtomPort ;
        atom:bufferType atom:Sequence ;
        atom:supports midi:MidiEvent ;
        rsz:minimumSize 4096 ;
        lv2:index 8 ;
        lv2:symbol "events" ;
        lv2:name "Events" ;
    ] .

<urn:test:amp#ui>
    a ui:WindowsUI ;
    ui:binary <amp_ui.dll> .
"#;

fn the_amp() -> PluginDescription {
    let folder = bundle_with(&[("manifest.ttl", MANIFEST), ("amp.ttl", AMP)]);
    let mut described = describe_bundle(folder.path()).unwrap();
    assert_eq!(described.problems, Vec::<String>::new());
    assert_eq!(described.plugins.len(), 1);
    let mut plugin = described.plugins.remove(0);
    // (The folder goes when the test ends: what is compared is relative to it.)
    plugin.bundle = Path::new("bundle").to_owned();
    plugin.binary = Path::new("bundle").join(plugin.binary.file_name().unwrap());
    for interface in &mut plugin.user_interfaces {
        interface.binary = interface
            .binary
            .take()
            .map(|binary| Path::new("bundle").join(binary.file_name().unwrap()));
    }
    plugin
}

fn port<'plugin>(plugin: &'plugin PluginDescription, symbol: &str) -> &'plugin PortDescription {
    plugin.ports.iter().find(|port| port.symbol == symbol).unwrap()
}

#[test]
fn a_plugin_is_described_as_its_bundle_says_with_the_name_in_the_language_that_is_read_here() {
    let plugin = the_amp();

    assert_eq!(plugin.uri, "urn:test:amp");
    assert_eq!(
        plugin.name, "Amp",
        "English is preferred to the language that is listed first"
    );
    assert_eq!(plugin.author, "Somebody");
    assert_eq!(plugin.categories, vec!["Amplifier".to_owned(), "Dynamics".to_owned()]);
    assert_eq!((plugin.minor_version, plugin.micro_version), (3, 7));
    assert_eq!(plugin.binary, Path::new("bundle").join("amp.dll"));
    assert_eq!(
        plugin.required_features,
        vec!["http://lv2plug.in/ns/ext/urid#map".to_owned()]
    );
    assert_eq!(
        plugin.optional_features,
        vec![
            "http://lv2plug.in/ns/ext/worker#schedule".to_owned(),
            "http://lv2plug.in/ns/lv2core#hardRTCapable".to_owned()
        ]
    );
    assert_eq!(
        plugin.extension_data,
        vec![
            "http://lv2plug.in/ns/ext/state#interface".to_owned(),
            "http://lv2plug.in/ns/ext/worker#interface".to_owned()
        ]
    );
    assert_eq!(plugin.user_interfaces.len(), 1);
    assert_eq!(
        plugin.user_interfaces[0].kinds,
        vec!["http://lv2plug.in/ns/extensions/ui#WindowsUI".to_owned()]
    );
    assert_eq!(
        plugin.user_interfaces[0].binary,
        Some(Path::new("bundle").join("amp_ui.dll"))
    );
}

#[test]
fn the_ports_are_in_the_order_of_their_indexes_with_their_kinds_and_directions() {
    let plugin = the_amp();

    let seen: Vec<(u32, &str, PortDirection, PortKind)> = plugin
        .ports
        .iter()
        .map(|port| (port.index, port.symbol.as_str(), port.direction, port.kind))
        .collect();
    assert_eq!(
        seen,
        vec![
            (0, "in", PortDirection::Input, PortKind::Audio),
            (1, "out", PortDirection::Output, PortKind::Audio),
            (2, "gain", PortDirection::Input, PortKind::Control),
            (3, "bypass", PortDirection::Input, PortKind::Control),
            (4, "mode", PortDirection::Input, PortKind::Control),
            (5, "cutoff", PortDirection::Input, PortKind::Control),
            (6, "rate", PortDirection::Input, PortKind::Control),
            (7, "latency", PortDirection::Output, PortKind::Control),
            (8, "events", PortDirection::Input, PortKind::Atom),
        ]
    );
}

#[test]
fn a_control_port_has_its_range_its_default_its_unit_and_the_name_in_the_language_that_is_read_here() {
    let plugin = the_amp();
    let gain = port(&plugin, "gain");

    assert_eq!(gain.name, "Gain");
    assert_eq!(
        (gain.minimum, gain.maximum, gain.default),
        (Some(-40.0), Some(12.0), Some(0.0))
    );
    assert_eq!(gain.unit.as_deref(), Some("dB"));
    assert_eq!(port(&plugin, "cutoff").unit.as_deref(), Some("Hz"));
    assert_eq!(port(&plugin, "in").default, None, "an audio port has no default");
}

#[test]
fn the_properties_of_a_port_are_read_toggled_integer_enumeration_logarithmic_the_sample_rate_and_an_optional_connection()
 {
    let plugin = the_amp();

    let bypass = port(&plugin, "bypass");
    assert!(bypass.toggled && bypass.connection_is_optional && !bypass.integer);
    let mode = port(&plugin, "mode");
    assert!(mode.integer && mode.enumeration && !mode.toggled);
    assert!(port(&plugin, "cutoff").logarithmic);
    assert!(port(&plugin, "rate").sample_rate_relative);
    assert!(!port(&plugin, "gain").logarithmic && !port(&plugin, "gain").sample_rate_relative);
}

#[test]
fn the_scale_points_of_an_enumeration_are_in_the_order_of_their_values_and_not_of_the_file() {
    let plugin = the_amp();

    let points: Vec<(&str, f64)> = port(&plugin, "mode")
        .scale_points
        .iter()
        .map(|point| (point.label.as_str(), point.value))
        .collect();
    assert_eq!(points, vec![("Clean", 0.0), ("Warm", 1.0), ("Hot", 2.0)]);
}

#[test]
fn the_port_that_says_how_late_the_plugin_is_is_found_and_so_is_what_an_atom_port_says() {
    let plugin = the_amp();

    assert!(port(&plugin, "latency").reports_latency);
    assert!(!port(&plugin, "gain").reports_latency);
    let events = port(&plugin, "events")
        .atom
        .as_ref()
        .expect("an atom port says what it carries");
    assert_eq!(
        events.buffer_type.as_deref(),
        Some("http://lv2plug.in/ns/ext/atom#Sequence")
    );
    assert_eq!(
        events.supports,
        vec!["http://lv2plug.in/ns/ext/midi#MidiEvent".to_owned()]
    );
    assert_eq!(events.minimum_size, Some(4096));
    assert!(port(&plugin, "gain").atom.is_none());
}

#[test]
fn a_plugin_whose_ports_are_not_numbered_from_0_without_gaps_is_left_out_and_the_others_of_the_bundle_are_not() {
    let manifest = r#"
@prefix lv2:  <http://lv2plug.in/ns/lv2core#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<urn:test:broken> a lv2:Plugin ; lv2:binary <a.dll> ; rdfs:seeAlso <broken.ttl> .
<urn:test:fine> a lv2:Plugin ; lv2:binary <a.dll> ; rdfs:seeAlso <fine.ttl> .
"#;
    let ports = |first: u32, second: u32| {
        format!(
            r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
@prefix doap: <http://usefulinc.com/ns/doap#> .
<urn:test:{name}> doap:name "x" ;
  lv2:port [ a lv2:InputPort , lv2:AudioPort ; lv2:index {first} ; lv2:symbol "a" ] ,
           [ a lv2:OutputPort , lv2:AudioPort ; lv2:index {second} ; lv2:symbol "b" ] ."#,
            name = if second == 3 { "broken" } else { "fine" }
        )
    };
    let folder = bundle_with(&[
        ("manifest.ttl", manifest),
        ("broken.ttl", &ports(0, 3)),
        ("fine.ttl", &ports(0, 1)),
    ]);

    let described = describe_bundle(folder.path()).unwrap();

    assert_eq!(described.plugins.len(), 1);
    assert_eq!(described.plugins[0].uri, "urn:test:fine");
    assert_eq!(described.problems.len(), 1, "{:?}", described.problems);
    assert!(described.problems[0].contains("urn:test:broken") && described.problems[0].contains("numbered"));
}

#[test]
fn a_plugin_with_no_binary_or_no_ports_or_a_port_of_no_known_kind_is_left_out_with_the_reason() {
    let manifest = r#"
@prefix lv2:  <http://lv2plug.in/ns/lv2core#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<urn:test:nobinary> a lv2:Plugin ; rdfs:seeAlso <nobinary.ttl> .
<urn:test:noports> a lv2:Plugin ; lv2:binary <a.dll> .
<urn:test:oddport> a lv2:Plugin ; lv2:binary <a.dll> ; rdfs:seeAlso <oddport.ttl> .
"#;
    let odd = r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
<urn:test:oddport> lv2:port [ a lv2:InputPort ; lv2:index 0 ; lv2:symbol "a" ] ."#;
    let nobinary = r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
<urn:test:nobinary> lv2:port [ a lv2:InputPort , lv2:AudioPort ; lv2:index 0 ; lv2:symbol "a" ] ."#;
    let folder = bundle_with(&[
        ("manifest.ttl", manifest),
        ("oddport.ttl", odd),
        ("nobinary.ttl", nobinary),
    ]);

    let described = describe_bundle(folder.path()).unwrap();

    assert!(described.plugins.is_empty(), "{:?}", described.plugins);
    let problems = described.problems.join("\n");
    assert!(
        problems.contains("urn:test:nobinary") && problems.contains("no binary"),
        "{problems}"
    );
    assert!(
        problems.contains("urn:test:noports") && problems.contains("ports"),
        "{problems}"
    );
    assert!(
        problems.contains("urn:test:oddport") && problems.contains("control, audio, CV or atom"),
        "{problems}"
    );
}

#[test]
fn a_description_file_that_is_not_turtle_is_a_problem_and_its_plugin_is_left_out() {
    let folder = bundle_with(&[("manifest.ttl", MANIFEST), ("amp.ttl", "this is <not turtle")]);

    let described = describe_bundle(folder.path()).unwrap();

    assert!(described.plugins.is_empty());
    assert!(
        described.problems.iter().any(|problem| problem.contains("amp.ttl")),
        "{:?}",
        described.problems
    );
}

#[test]
fn a_manifest_that_is_not_turtle_or_not_there_is_an_error_that_says_which_file() {
    let bad = bundle_with(&[("manifest.ttl", "@prefix lv2: .")]);
    let error = describe_bundle(bad.path()).unwrap_err().to_string();
    assert!(error.contains("manifest.ttl"), "{error}");

    let empty = tempfile::tempdir().unwrap();
    let error = describe_bundle(empty.path()).unwrap_err().to_string();
    assert!(
        error.contains("manifest.ttl") && error.contains("could not be read"),
        "{error}"
    );
}

#[test]
fn a_reference_that_goes_up_out_of_the_bundle_cannot_reach_a_file_outside_it_and_a_name_with_a_percent_sign_is_a_file_with_a_space()
 {
    let manifest = r#"
@prefix lv2:  <http://lv2plug.in/ns/lv2core#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<urn:test:amp> a lv2:Plugin ; lv2:binary <my%20amp.dll> ; rdfs:seeAlso <amp.ttl> , <../outside.ttl> .
"#;
    let folder = bundle_with(&[("manifest.ttl", manifest), ("amp.ttl", AMP)]);
    // (A file outside of the bundle that would be read if the reference were followed, and that is not Turtle.)
    fs::write(
        folder.path().parent().unwrap().join("outside.ttl"),
        "not turtle at all <",
    )
    .unwrap();

    let described = describe_bundle(folder.path()).unwrap();

    // (The reference cannot go above the folder: it is a file of the bundle that is not there. The file outside, which is not Turtle, was not read: that would be a different problem.)
    assert_eq!(described.problems.len(), 1, "{:?}", described.problems);
    assert!(
        described.problems[0].contains("could not be read"),
        "{:?}",
        described.problems
    );
    assert_eq!(described.plugins.len(), 1);
    assert_eq!(described.plugins[0].binary, folder.path().join("my amp.dll"));
    let _ = fs::remove_file(folder.path().parent().unwrap().join("outside.ttl"));
}

#[test]
fn the_bundles_of_a_folder_are_the_folders_with_a_manifest_in_the_order_of_their_names() {
    let folder = tempfile::tempdir().unwrap();
    for name in ["b.lv2", "a.lv2", "no_manifest.lv2"] {
        fs::create_dir(folder.path().join(name)).unwrap();
    }
    fs::write(folder.path().join("b.lv2/manifest.ttl"), MANIFEST).unwrap();
    fs::write(folder.path().join("a.lv2/manifest.ttl"), MANIFEST).unwrap();
    fs::write(folder.path().join("a_file.ttl"), "").unwrap();

    let bundles = bundles_in(folder.path());

    let names: Vec<&str> = bundles
        .iter()
        .map(|path| path.file_name().unwrap().to_str().unwrap())
        .collect();
    assert_eq!(names, vec!["a.lv2", "b.lv2"]);
    assert!(bundles_in(&folder.path().join("not_there")).is_empty());
}

#[test]
fn the_folders_that_bundles_are_looked_for_in_are_those_of_lv2_path_when_it_is_set_and_the_usual_ones_when_it_is_not() {
    let separator = if cfg!(windows) { ";" } else { ":" };
    let path = std::ffi::OsString::from(format!(
        "{}a{separator}{}b",
        if cfg!(windows) { "C:\\" } else { "/" },
        if cfg!(windows) { "C:\\" } else { "/" }
    ));

    let given = usual_folders(Some(&path), None, None, None);
    assert_eq!(given.len(), 2);
    assert!(given[0].ends_with("a") && given[1].ends_with("b"));

    let usual = usual_folders(
        None,
        Some(Path::new("/home/someone")),
        Some(Path::new("C:\\Users\\someone\\AppData\\Roaming")),
        Some(Path::new("C:\\Program Files\\Common Files")),
    );
    assert!(!usual.is_empty());
    assert!(
        usual
            .iter()
            .all(|folder| folder.to_string_lossy().to_ascii_lowercase().contains("lv2")),
        "{usual:?}"
    );
    assert_eq!(
        usual_folders(Some(std::ffi::OsStr::new("")), Some(Path::new("/h")), None, None),
        usual_folders(None, Some(Path::new("/h")), None, None),
        "an empty LV2_PATH is not set"
    );
}

#[test]
fn ports_that_are_written_in_any_order_are_in_the_order_of_their_indexes() {
    let manifest = r#"
@prefix lv2:  <http://lv2plug.in/ns/lv2core#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<urn:test:shuffled> a lv2:Plugin ; lv2:binary <a.dll> ; rdfs:seeAlso <shuffled.ttl> .
"#;
    let shuffled = r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
<urn:test:shuffled> lv2:port
  [ a lv2:OutputPort , lv2:AudioPort ; lv2:index 2 ; lv2:symbol "c" ] ,
  [ a lv2:InputPort , lv2:AudioPort ; lv2:index 0 ; lv2:symbol "a" ] ,
  [ a lv2:InputPort , lv2:ControlPort ; lv2:index 1 ; lv2:symbol "b" ] ."#;
    let folder = bundle_with(&[("manifest.ttl", manifest), ("shuffled.ttl", shuffled)]);

    let described = describe_bundle(folder.path()).unwrap();

    assert_eq!(described.problems, Vec::<String>::new());
    let symbols: Vec<&str> = described.plugins[0]
        .ports
        .iter()
        .map(|port| port.symbol.as_str())
        .collect();
    assert_eq!(symbols, vec!["a", "b", "c"]);
}

#[test]
fn a_port_that_says_it_reports_latency_the_old_way_is_found_as_well_as_one_that_is_designated_the_latency() {
    let manifest = r#"
@prefix lv2:  <http://lv2plug.in/ns/lv2core#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<urn:test:old> a lv2:Plugin ; lv2:binary <a.dll> ; rdfs:seeAlso <old.ttl> .
"#;
    let old = r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
<urn:test:old> lv2:port
  [ a lv2:InputPort , lv2:AudioPort ; lv2:index 0 ; lv2:symbol "in" ] ,
  [ a lv2:OutputPort , lv2:ControlPort ; lv2:index 1 ; lv2:symbol "latency" ; lv2:portProperty lv2:reportsLatency ] ,
  [ a lv2:OutputPort , lv2:ControlPort ; lv2:index 2 ; lv2:symbol "meter" ] ."#;
    let folder = bundle_with(&[("manifest.ttl", manifest), ("old.ttl", old)]);

    let described = describe_bundle(folder.path()).unwrap();

    let ports = &described.plugins[0].ports;
    assert!(ports[1].reports_latency);
    assert!(!ports[2].reports_latency, "a meter is not the latency");
}

#[test]
fn a_plugin_whose_name_is_a_reference_relative_to_a_file_is_left_out_because_a_name_is_a_whole_uri() {
    let manifest = r#"
@prefix lv2:  <http://lv2plug.in/ns/lv2core#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<relative-name> a lv2:Plugin ; lv2:binary <a.dll> ; rdfs:seeAlso <rel.ttl> .
"#;
    let rel = r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
<relative-name> lv2:port [ a lv2:InputPort , lv2:AudioPort ; lv2:index 0 ; lv2:symbol "in" ] ."#;
    let folder = bundle_with(&[("manifest.ttl", manifest), ("rel.ttl", rel)]);

    let described = describe_bundle(folder.path()).unwrap();

    assert!(described.plugins.is_empty());
    assert!(
        described.problems.iter().any(|problem| problem.contains("whole URI")),
        "{:?}",
        described.problems
    );
}

#[test]
fn a_port_that_is_both_an_input_and_an_output_or_neither_is_a_problem_and_a_designation_that_is_not_the_latency_is_not_the_latency()
 {
    let manifest = r#"
@prefix lv2:  <http://lv2plug.in/ns/lv2core#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<urn:test:both> a lv2:Plugin ; lv2:binary <a.dll> ; rdfs:seeAlso <both.ttl> .
<urn:test:neither> a lv2:Plugin ; lv2:binary <a.dll> ; rdfs:seeAlso <neither.ttl> .
<urn:test:enable> a lv2:Plugin ; lv2:binary <a.dll> ; rdfs:seeAlso <enable.ttl> .
"#;
    let both = r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
<urn:test:both> lv2:port [ a lv2:InputPort , lv2:OutputPort , lv2:AudioPort ; lv2:index 0 ; lv2:symbol "a" ] ."#;
    let neither = r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
<urn:test:neither> lv2:port [ a lv2:AudioPort ; lv2:index 0 ; lv2:symbol "a" ] ."#;
    let enable = r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
<urn:test:enable> lv2:port [ a lv2:InputPort , lv2:ControlPort ; lv2:index 0 ; lv2:symbol "on" ; lv2:designation lv2:enabled ] ."#;
    let folder = bundle_with(&[
        ("manifest.ttl", manifest),
        ("both.ttl", both),
        ("neither.ttl", neither),
        ("enable.ttl", enable),
    ]);

    let described = describe_bundle(folder.path()).unwrap();

    let problems = described.problems.join(
        "
",
    );
    assert!(
        problems.contains("urn:test:both") && problems.contains("input or an output"),
        "{problems}"
    );
    assert!(
        problems.contains("urn:test:neither") && problems.contains("input or an output"),
        "{problems}"
    );
    assert_eq!(described.plugins.len(), 1);
    assert_eq!(described.plugins[0].uri, "urn:test:enable");
    assert!(
        !described.plugins[0].ports[0].reports_latency,
        "a designation of another kind is not the latency"
    );
    assert_eq!(
        described.plugins[0].ports[0].designations,
        vec!["http://lv2plug.in/ns/lv2core#enabled".to_owned()]
    );
}
