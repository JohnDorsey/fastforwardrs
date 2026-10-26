// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What an LV2 bundle says about its plugins, as safe values that can be sent between programs (see `docs/engine-phase-11-lv2-design.md`).
//!
//! A bundle is a folder with `manifest.ttl` (Turtle) that names its plugins, each with the file of its binary and the file that describes it (`rdfs:seeAlso`). **This reads
//! the Turtle and loads nothing**: no code of the plugin runs, so describing a bundle is safe in the editor's own process, and a bundle whose Turtle is not understood is
//! said so and left out. What is read is the little that a host needs (the ports, their kinds, ranges and properties, the features, the extension data, the user interfaces),
//! by the URIs of the LV2 specification, as `lilv` would find it in the same files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use oxrdf::{NamedOrBlankNode, Term};
use oxttl::TurtleParser;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The namespace that relative references in the Turtle are made absolute in, so that the files of a bundle can be told from other things: what is in it is a file of the bundle.
const BUNDLE_BASE: &str = "http://bundle.invalid/";

/// The most files that a bundle's Turtle may lead to (`rdfs:seeAlso`), and the most bytes of one: a bundle that goes past is not one.
const MOST_FILES: usize = 64;
const MOST_BYTES_OF_A_FILE: u64 = 64 * 1024 * 1024;

/// The most ports that a plugin may say it has.
const MOST_PORTS: usize = 4096;

const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const LV2: &str = "http://lv2plug.in/ns/lv2core#";
const DOAP: &str = "http://usefulinc.com/ns/doap#";
const FOAF: &str = "http://xmlns.com/foaf/0.1/";
const ATOM: &str = "http://lv2plug.in/ns/ext/atom#";
const UI: &str = "http://lv2plug.in/ns/extensions/ui#";
const PPROP: &str = "http://lv2plug.in/ns/ext/port-props#";
const UNITS: &str = "http://lv2plug.in/ns/extensions/units#";
const RSZ: &str = "http://lv2plug.in/ns/ext/resize-port#";

/// Whether a port takes sound or values in, or gives them out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortDirection {
    Input,
    Output,
}

/// What a port carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortKind {
    /// One number, read (or written) at each `run`.
    Control,
    /// A buffer of samples.
    Audio,
    /// A buffer of samples that are a control signal.
    ControlVoltage,
    /// Messages (MIDI, and what a plugin and its own interface say to each other).
    Atom,
}

/// A value that a control port can have, with the name that a person is shown for it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScalePoint {
    pub label: String,
    pub value: f64,
}

/// What a port that carries messages says about them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtomPortDescription {
    /// The type of the buffer (`atom:Sequence` is the usual one).
    pub buffer_type: Option<String>,
    /// What the port understands or sends (`midi:MidiEvent`, `patch:Message`...).
    pub supports: Vec<String>,
    /// The least bytes that the buffer must have, if the plugin says (`rsz:minimumSize`).
    pub minimum_size: Option<u32>,
}

/// What a plugin says of one of its ports.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortDescription {
    pub index: u32,
    /// The name that is safe to use in code and in a file (`gain`).
    pub symbol: String,
    /// The name that a person is shown.
    pub name: String,
    pub direction: PortDirection,
    pub kind: PortKind,
    pub default: Option<f64>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    /// A switch (`lv2:toggled`).
    pub toggled: bool,
    /// Whole numbers (`lv2:integer`).
    pub integer: bool,
    /// A choice among the scale points (`lv2:enumeration`).
    pub enumeration: bool,
    /// A range that is felt in ratios (`pprop:logarithmic`).
    pub logarithmic: bool,
    /// The range is a fraction of the sample rate (`lv2:sampleRate`): the minimum, the maximum and the default are multiplied by it.
    pub sample_rate_relative: bool,
    /// The plugin works without the port connected (`lv2:connectionOptional`).
    pub connection_is_optional: bool,
    /// The port says how late the plugin's sound is, in samples (`lv2:designation lv2:latency`, or `lv2:reportsLatency`).
    pub reports_latency: bool,
    /// Other designations (`lv2:enable`, `lv2:freeWheeling`, `time:...`), as URIs.
    pub designations: Vec<String>,
    pub scale_points: Vec<ScalePoint>,
    /// The unit, as a short name (`dB`, `Hz`, `ms`, `%`...) if the plugin says and the unit is one that is known.
    pub unit: Option<String>,
    pub atom: Option<AtomPortDescription>,
}

/// A user interface that a plugin has (not run by anything yet: it says what kind it is, and where).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiDescription {
    pub uri: String,
    /// The classes it says it is (`ui:WindowsUI`, `ui:X11UI`...), as URIs.
    pub kinds: Vec<String>,
    pub binary: Option<PathBuf>,
    /// What it cannot work without that the host gives it (`ui:parent`...), as URIs.
    pub required_features: Vec<String>,
    /// What it uses if the host gives it.
    pub optional_features: Vec<String>,
    /// The URIs of what its `extension_data` function answers for (`ui:idleInterface`, `ui:resize`...).
    pub extension_data: Vec<String>,
}

/// The class of user interface that a window of the kind `api` (as the plugin hosts of this program say it: `win32`, `x11` or `cocoa`) is, as a URI.
pub fn ui_class_of_a_window(api: &str) -> Option<&'static str> {
    match api {
        "win32" => Some("http://lv2plug.in/ns/extensions/ui#WindowsUI"),
        "x11" => Some("http://lv2plug.in/ns/extensions/ui#X11UI"),
        "cocoa" => Some("http://lv2plug.in/ns/extensions/ui#CocoaUI"),
        _ => None,
    }
}

/// The user interface of `plugin` that this program can show in a window of the kind `api`, if it has one: of that system's class, with a binary that is in the bundle, and needing
/// nothing that this host does not give a user interface. (The first, in the order of their URIs, when there are more.)
pub fn window_of<'plugin>(plugin: &'plugin PluginDescription, api: &str) -> Option<&'plugin UiDescription> {
    let class = ui_class_of_a_window(api)?;
    plugin.user_interfaces.iter().find(|interface| {
        interface.kinds.iter().any(|kind| kind == class)
            && interface.binary.is_some()
            && interface
                .required_features
                .iter()
                .all(|feature| crate::abi::SUPPORTED_UI_FEATURES.contains(&feature.as_str()))
    })
}

/// What a bundle says of one of its plugins.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginDescription {
    /// What names the plugin (a URI, which is also its key in a project).
    pub uri: String,
    pub name: String,
    /// What kinds of plugin it says it is (`Reverb` for `lv2:ReverbPlugin`), without the word `Plugin`.
    pub categories: Vec<String>,
    pub author: String,
    /// The folder of the bundle.
    pub bundle: PathBuf,
    /// The file of the library that is the plugin (in the bundle).
    pub binary: PathBuf,
    pub minor_version: u32,
    pub micro_version: u32,
    /// In the order of their indexes (which begin at 0 and have none missing).
    pub ports: Vec<PortDescription>,
    /// What the plugin cannot work without that the host gives it, as URIs.
    pub required_features: Vec<String>,
    /// What it uses if the host gives it.
    pub optional_features: Vec<String>,
    /// The URIs of what its `extension_data` function answers for.
    pub extension_data: Vec<String>,
    pub user_interfaces: Vec<UiDescription>,
}

/// What a bundle says: its plugins, and one line for each thing that could not be understood.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BundleDescription {
    pub plugins: Vec<PluginDescription>,
    pub problems: Vec<String>,
}

/// Why a bundle could not be described at all.
#[derive(Debug, Error)]
pub enum DescribeError {
    #[error("{0} could not be read: {1}")]
    Unreadable(PathBuf, String),
    #[error("the Turtle of {0} is not understood: {1}")]
    NotTurtle(String, String),
    #[error("the bundle leads to more than {MOST_FILES} files of Turtle")]
    TooManyFiles,
}

// ---- the graph ----

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Node {
    Iri(String),
    Blank(String),
}

#[derive(Clone, Debug)]
enum Object {
    Node(Node),
    Literal { value: String, language: Option<String> },
}

/// The triples of the Turtle that was read, by subject.
#[derive(Default)]
struct Graph {
    by_subject: HashMap<Node, Vec<(String, Object)>>,
}

impl Graph {
    fn read(&mut self, text: &str, file: &str) -> Result<(), DescribeError> {
        let base = format!("{BUNDLE_BASE}{file}");
        let parser = TurtleParser::new()
            .with_base_iri(base)
            .map_err(|error| DescribeError::NotTurtle(file.to_owned(), error.to_string()))?;
        for triple in parser.for_slice(text) {
            let triple = triple.map_err(|error| DescribeError::NotTurtle(file.to_owned(), error.to_string()))?;
            let subject = match triple.subject {
                NamedOrBlankNode::NamedNode(node) => Node::Iri(node.into_string()),
                NamedOrBlankNode::BlankNode(node) => Node::Blank(node.into_string()),
            };
            let object = match triple.object {
                Term::NamedNode(node) => Object::Node(Node::Iri(node.into_string())),
                Term::BlankNode(node) => Object::Node(Node::Blank(node.into_string())),
                Term::Literal(literal) => Object::Literal {
                    value: literal.value().to_owned(),
                    language: literal.language().map(str::to_owned),
                },
            };
            self.by_subject
                .entry(subject)
                .or_default()
                .push((triple.predicate.into_string(), object));
        }
        Ok(())
    }

    fn objects<'graph>(&'graph self, subject: &Node, predicate: &str) -> impl Iterator<Item = &'graph Object> {
        let predicate = predicate.to_owned();
        self.by_subject
            .get(subject)
            .into_iter()
            .flatten()
            .filter(move |(candidate, _)| *candidate == predicate)
            .map(|(_, object)| object)
    }

    /// The nodes (not the literals) that `subject` has for `predicate`.
    fn nodes(&self, subject: &Node, predicate: &str) -> Vec<Node> {
        self.objects(subject, predicate)
            .filter_map(|object| match object {
                Object::Node(node) => Some(node.clone()),
                Object::Literal { .. } => None,
            })
            .collect()
    }

    /// The IRIs that `subject` has for `predicate`.
    fn iris(&self, subject: &Node, predicate: &str) -> Vec<String> {
        self.nodes(subject, predicate)
            .into_iter()
            .filter_map(|node| match node {
                Node::Iri(iri) => Some(iri),
                Node::Blank(_) => None,
            })
            .collect()
    }

    /// The text that `subject` has for `predicate`, in the language that a person here reads if there is one (English, or none), and otherwise the first.
    fn text(&self, subject: &Node, predicate: &str) -> Option<String> {
        let mut first = None;
        for object in self.objects(subject, predicate) {
            let Object::Literal { value, language } = object else {
                continue;
            };
            let is_english_or_none = language.as_deref().is_none_or(|language| {
                language.eq_ignore_ascii_case("en") || language.to_ascii_lowercase().starts_with("en-")
            });
            if is_english_or_none {
                return Some(value.clone());
            }
            first.get_or_insert_with(|| value.clone());
        }
        first
    }

    fn number(&self, subject: &Node, predicate: &str) -> Option<f64> {
        self.objects(subject, predicate).find_map(|object| match object {
            Object::Literal { value, .. } => value.trim().parse::<f64>().ok().filter(|number| number.is_finite()),
            Object::Node(_) => None,
        })
    }

    fn whole_number(&self, subject: &Node, predicate: &str) -> Option<u32> {
        self.objects(subject, predicate).find_map(|object| match object {
            Object::Literal { value, .. } => value.trim().parse::<u32>().ok(),
            Object::Node(_) => None,
        })
    }

    fn types(&self, subject: &Node) -> Vec<String> {
        self.iris(subject, &format!("{RDF}type"))
    }
}

// ---- reading a bundle ----

/// What a reference in the Turtle says, if it is a file of the bundle: its place relative to the bundle's folder.
fn file_of_the_bundle(iri: &str) -> Option<String> {
    let relative = iri.strip_prefix(BUNDLE_BASE)?;
    let relative = relative.split(['#', '?']).next().unwrap_or(relative);
    let decoded = percent_decoded(relative);
    // (A reference that leaves the bundle is not a file of it.)
    (!decoded.is_empty() && !decoded.split(['/', '\\']).any(|part| part == "..") && !decoded.starts_with('/'))
        .then_some(decoded)
}

fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut place = 0;
    while place < bytes.len() {
        let hex = |digit: u8| (digit as char).to_digit(16);
        if bytes[place] == b'%'
            && place + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[place + 1]), hex(bytes[place + 2]))
        {
            out.push((high * 16 + low) as u8);
            place += 3;
        } else {
            out.push(bytes[place]);
            place += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn read_text(path: &Path) -> Result<String, DescribeError> {
    let length = std::fs::metadata(path)
        .map_err(|error| DescribeError::Unreadable(path.to_owned(), error.to_string()))?
        .len();
    if length > MOST_BYTES_OF_A_FILE {
        return Err(DescribeError::Unreadable(
            path.to_owned(),
            format!("it is of {length} bytes, which is too many for a description"),
        ));
    }
    let bytes = std::fs::read(path).map_err(|error| DescribeError::Unreadable(path.to_owned(), error.to_string()))?;
    // (A byte order mark is not Turtle.)
    let text = String::from_utf8_lossy(&bytes);
    Ok(text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned())
}

/// Says what the bundle in the folder `bundle` says of its plugins: its `manifest.ttl`, and the files that it leads to.
pub fn describe_bundle(bundle: &Path) -> Result<BundleDescription, DescribeError> {
    let mut graph = Graph::default();
    let mut read: Vec<String> = Vec::new();
    graph.read(&read_text(&bundle.join("manifest.ttl"))?, "manifest.ttl")?;
    read.push("manifest.ttl".to_owned());

    let mut description = BundleDescription::default();
    // The files that the manifest leads to are read (and the files that those lead to, to a limit): the plugins' descriptions, and the interfaces'.
    let mut queue: Vec<String> = Vec::new();
    let subjects: Vec<Node> = graph.by_subject.keys().cloned().collect();
    for subject in &subjects {
        for iri in graph.iris(subject, &format!("{RDFS}seeAlso")) {
            if let Some(file) = file_of_the_bundle(&iri) {
                queue.push(file);
            }
        }
    }
    while let Some(file) = queue.pop() {
        if read.contains(&file) {
            continue;
        }
        if read.len() >= MOST_FILES {
            return Err(DescribeError::TooManyFiles);
        }
        read.push(file.clone());
        let path = bundle.join(&file);
        match read_text(&path).and_then(|text| graph.read(&text, &file)) {
            Ok(()) => {}
            Err(error) => description.problems.push(error.to_string()),
        }
        let new_subjects: Vec<Node> = graph.by_subject.keys().cloned().collect();
        for subject in &new_subjects {
            for iri in graph.iris(subject, &format!("{RDFS}seeAlso")) {
                if let Some(next) = file_of_the_bundle(&iri)
                    && !read.contains(&next)
                {
                    queue.push(next);
                }
            }
        }
    }

    // The plugins: what the manifest says is a plugin and has a binary.
    let mut plugin_nodes: Vec<(String, Node)> = Vec::new();
    for subject in graph.by_subject.keys() {
        let Node::Iri(uri) = subject else { continue };
        if graph.types(subject).iter().any(|class| is_a_kind_of_plugin(class)) {
            plugin_nodes.push((uri.clone(), subject.clone()));
        }
    }
    plugin_nodes.sort_by(|a, b| a.0.cmp(&b.0));
    for (uri, node) in plugin_nodes {
        match describe_plugin(&graph, &uri, &node, bundle) {
            Ok(plugin) => description.plugins.push(plugin),
            Err(why) => description.problems.push(format!("{uri}: {why}")),
        }
    }
    Ok(description)
}

fn is_a_kind_of_plugin(class: &str) -> bool {
    class
        .strip_prefix(LV2)
        .is_some_and(|name| name == "Plugin" || name.ends_with("Plugin"))
}

fn describe_plugin(graph: &Graph, uri: &str, node: &Node, bundle: &Path) -> Result<PluginDescription, String> {
    if uri.starts_with(BUNDLE_BASE) {
        return Err("its name is not a whole URI (it is written relative to a file)".to_owned());
    }
    let binary = graph
        .iris(node, &format!("{LV2}binary"))
        .into_iter()
        .find_map(|iri| file_of_the_bundle(&iri))
        .map(|file| bundle.join(file))
        .ok_or_else(|| "it has no binary (lv2:binary) in the bundle".to_owned())?;
    let name = graph
        .text(node, &format!("{DOAP}name"))
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| uri.to_owned());
    let author = graph
        .nodes(node, &format!("{DOAP}maintainer"))
        .iter()
        .find_map(|maintainer| graph.text(maintainer, &format!("{FOAF}name")))
        .unwrap_or_default();
    let mut categories: Vec<String> = graph
        .types(node)
        .iter()
        .filter_map(|class| class.strip_prefix(LV2))
        .filter(|name| *name != "Plugin" && name.ends_with("Plugin"))
        .map(|name| name.trim_end_matches("Plugin").to_owned())
        .collect();
    categories.sort();
    categories.dedup();

    let mut ports = Vec::new();
    for port in graph.nodes(node, &format!("{LV2}port")) {
        if ports.len() >= MOST_PORTS {
            return Err(format!("it says it has more than {MOST_PORTS} ports"));
        }
        ports.push(describe_port(graph, &port)?);
    }
    if ports.is_empty() {
        return Err("it says nothing of its ports (it has none, or the file that says is not there)".to_owned());
    }
    ports.sort_by_key(|port: &PortDescription| port.index);
    for (place, port) in ports.iter().enumerate() {
        if port.index as usize != place {
            return Err(format!(
                "its ports are not numbered from 0 with none missing or repeated (port {} is where {place} should be)",
                port.index
            ));
        }
    }

    let mut user_interfaces = Vec::new();
    for interface in graph.nodes(node, &format!("{UI}ui")) {
        let Node::Iri(interface_uri) = &interface else { continue };
        let mut required_features = graph.iris(&interface, &format!("{LV2}requiredFeature"));
        let mut optional_features = graph.iris(&interface, &format!("{LV2}optionalFeature"));
        let mut extension_data = graph.iris(&interface, &format!("{LV2}extensionData"));
        for list in [&mut required_features, &mut optional_features, &mut extension_data] {
            list.sort();
            list.dedup();
        }
        user_interfaces.push(UiDescription {
            uri: interface_uri.clone(),
            kinds: graph.types(&interface),
            binary: graph
                .iris(&interface, &format!("{UI}binary"))
                .into_iter()
                .chain(graph.iris(&interface, &format!("{LV2}binary")))
                .find_map(|iri| file_of_the_bundle(&iri))
                .map(|file| bundle.join(file)),
            required_features,
            optional_features,
            extension_data,
        });
    }
    user_interfaces.sort_by(|a: &UiDescription, b: &UiDescription| a.uri.cmp(&b.uri));

    let mut required_features = graph.iris(node, &format!("{LV2}requiredFeature"));
    let mut optional_features = graph.iris(node, &format!("{LV2}optionalFeature"));
    let mut extension_data = graph.iris(node, &format!("{LV2}extensionData"));
    for list in [&mut required_features, &mut optional_features, &mut extension_data] {
        list.sort();
        list.dedup();
    }

    Ok(PluginDescription {
        uri: uri.to_owned(),
        name,
        categories,
        author,
        bundle: bundle.to_owned(),
        binary,
        minor_version: graph.whole_number(node, &format!("{LV2}minorVersion")).unwrap_or(0),
        micro_version: graph.whole_number(node, &format!("{LV2}microVersion")).unwrap_or(0),
        ports,
        required_features,
        optional_features,
        extension_data,
        user_interfaces,
    })
}

fn describe_port(graph: &Graph, port: &Node) -> Result<PortDescription, String> {
    let index = graph
        .whole_number(port, &format!("{LV2}index"))
        .ok_or_else(|| "a port has no index (lv2:index)".to_owned())?;
    let types = graph.types(port);
    let has = |class: &str| types.iter().any(|candidate| candidate == class);
    let direction = match (has(&format!("{LV2}InputPort")), has(&format!("{LV2}OutputPort"))) {
        (true, false) => PortDirection::Input,
        (false, true) => PortDirection::Output,
        _ => return Err(format!("port {index} is not an input or an output port, or is both")),
    };
    let kind = if has(&format!("{LV2}ControlPort")) {
        PortKind::Control
    } else if has(&format!("{LV2}AudioPort")) {
        PortKind::Audio
    } else if has(&format!("{LV2}CVPort")) {
        PortKind::ControlVoltage
    } else if has(&format!("{ATOM}AtomPort")) {
        PortKind::Atom
    } else {
        return Err(format!("port {index} is not a control, audio, CV or atom port"));
    };
    let symbol = graph
        .text(port, &format!("{LV2}symbol"))
        .filter(|symbol| !symbol.is_empty())
        .ok_or_else(|| format!("port {index} has no symbol (lv2:symbol)"))?;
    let name = graph
        .text(port, &format!("{LV2}name"))
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| symbol.clone());
    let properties = graph.iris(port, &format!("{LV2}portProperty"));
    let property = |iri: String| properties.contains(&iri);
    let designations = graph.iris(port, &format!("{LV2}designation"));
    let reports_latency = designations
        .iter()
        .any(|designation| *designation == format!("{LV2}latency"))
        || property(format!("{LV2}reportsLatency"));

    let mut scale_points = Vec::new();
    for point in graph.nodes(port, &format!("{LV2}scalePoint")) {
        if let (Some(label), Some(value)) = (
            graph.text(&point, &format!("{RDFS}label")),
            graph.number(&point, &format!("{RDF}value")),
        ) {
            scale_points.push(ScalePoint { label, value });
        }
    }
    scale_points.sort_by(|a, b| a.value.total_cmp(&b.value));

    let atom = (kind == PortKind::Atom).then(|| AtomPortDescription {
        buffer_type: graph.iris(port, &format!("{ATOM}bufferType")).into_iter().next(),
        supports: {
            let mut supports = graph.iris(port, &format!("{ATOM}supports"));
            supports.sort();
            supports
        },
        minimum_size: graph.whole_number(port, &format!("{RSZ}minimumSize")),
    });

    Ok(PortDescription {
        index,
        symbol,
        name,
        direction,
        kind,
        default: graph.number(port, &format!("{LV2}default")),
        minimum: graph.number(port, &format!("{LV2}minimum")),
        maximum: graph.number(port, &format!("{LV2}maximum")),
        toggled: property(format!("{LV2}toggled")),
        integer: property(format!("{LV2}integer")),
        enumeration: property(format!("{LV2}enumeration")),
        logarithmic: property(format!("{PPROP}logarithmic")),
        sample_rate_relative: property(format!("{LV2}sampleRate")),
        connection_is_optional: property(format!("{LV2}connectionOptional")),
        reports_latency,
        designations,
        scale_points,
        unit: graph
            .iris(port, &format!("{UNITS}unit"))
            .into_iter()
            .next()
            .and_then(|unit| short_name_of_a_unit(&unit)),
        atom,
    })
}

/// The short name of a unit of the units extension, if it is one that is known.
fn short_name_of_a_unit(unit: &str) -> Option<String> {
    let name = unit.strip_prefix(UNITS)?;
    Some(
        match name {
            "db" => "dB",
            "hz" => "Hz",
            "khz" => "kHz",
            "mhz" => "MHz",
            "ms" => "ms",
            "s" => "s",
            "min" => "min",
            "pc" => "%",
            "semitone12TET" => "semitones",
            "cent" => "cents",
            "degree" => "\u{b0}",
            "bpm" => "BPM",
            "oct" => "octaves",
            "midiNote" => "MIDI note",
            "coef" => "",
            other => other,
        }
        .to_owned(),
    )
}

// ---- finding bundles ----

/// The folders that LV2 bundles are looked for in, when nobody says: `LV2_PATH` if it is set, and otherwise the usual ones of the system. (A person adds more in the plugin manager.
/// The places that a system gives are passed in, so that this does not look at the environment: `app_data` and `common_program_files` are `%APPDATA%` and `%COMMONPROGRAMFILES%`.)
pub fn usual_folders(
    lv2_path: Option<&std::ffi::OsStr>,
    home: Option<&Path>,
    app_data: Option<&Path>,
    common_program_files: Option<&Path>,
) -> Vec<PathBuf> {
    if let Some(path) = lv2_path.filter(|path| !path.is_empty()) {
        return std::env::split_paths(path)
            .filter(|folder| !folder.as_os_str().is_empty())
            .collect();
    }
    let mut folders = Vec::new();
    if cfg!(windows) {
        for base in [app_data, common_program_files].into_iter().flatten() {
            folders.push(base.join("LV2"));
        }
    } else if cfg!(target_os = "macos") {
        if let Some(home) = home {
            folders.push(home.join("Library/Audio/Plug-Ins/LV2"));
        }
        folders.push(PathBuf::from("/Library/Audio/Plug-Ins/LV2"));
    } else {
        if let Some(home) = home {
            folders.push(home.join(".lv2"));
        }
        folders.push(PathBuf::from("/usr/lib/lv2"));
        folders.push(PathBuf::from("/usr/local/lib/lv2"));
    }
    folders
}

/// The bundles in a folder: its sub-folders (named `*.lv2`, by custom, but any folder that has a `manifest.ttl` is one) that have a `manifest.ttl`, in the order of their names.
pub fn bundles_in(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut bundles: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.join("manifest.ttl").is_file())
        .collect();
    bundles.sort();
    bundles
}
