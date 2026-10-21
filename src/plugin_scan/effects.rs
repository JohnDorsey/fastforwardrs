// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugins that were found, as types of effect (G10 of `docs/engine-phase-7-design.md`).
//!
//! * **The key** of a plugin is `ladspa:` the name of its library file without the ending, a colon, and its label (`ladspa:amp_1181:amp_1181`). The version is 1.0 (LADSPA has none).
//!   The unique id the maker gave is kept but is not the key: plugins in the wild share them.
//! * **A control input is a value**: a number with the range the hint says (a bound that is a fraction of the sample rate is made into the value at the rate of the sound
//!   engine; a range that has only one end has none, and the value is a field to drag), the default the hint says, a logarithmic scale where the hint says so and both ends are
//!   above zero, a toggle where the hint says so, a whole number where it says so. Each value has an id made from the number of its port and its name, so that a plugin whose ports
//!   are not the same is not the same plugin (the effect that a project has of it is then kept whole and does nothing). **A control output is not a value** (nothing can set it).
//! * **The audio ports decide whether it can be an effect**: one input and one output (it is made once for each channel), two and two (once), one input and two outputs, two
//!   inputs and one output. A plugin with no audio input is a generator, and is not used yet (a generator is not an effect; that comes with the master bus); a layout that is
//!   none of those is not used yet either. The category is the maker.

use std::borrow::Cow;
use std::path::Path;

use ladspa_host::{PortDescription, PortDirection, PortKind, ScannedPlugin};

use super::scanner::{LibraryOutcome, LibraryReport};
use crate::project::{
    ChoiceOption, EffectDescriptor, EffectDomain, InstalledPlugin, MOST_VALUES_OF_AN_EFFECT, ParameterControl,
    ParameterDescriptor, PluginAddress, PropertyValue, UnknownFields, ValueScale,
};
use crate::sound_engine::SAMPLE_RATE;

/// What a plugin that cannot be an effect is called in the list of what is not used, and why it cannot be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotUsed {
    pub library: std::path::PathBuf,
    pub label: String,
    pub name: String,
    pub reason: String,
}

/// The plugins of the libraries that were scanned: those that are types of effect, in the order of the libraries, and those that are not and why.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PluginsFound {
    pub effects: Vec<InstalledPlugin>,
    pub not_used: Vec<NotUsed>,
}

/// The effects that the libraries in `reports` have, LADSPA and CLAP alike. Of two plugins with one key the first is used (the person's own folders come first, then those of
/// `LADSPA_PATH`, then the usual ones, then those of CLAP).
pub fn plugins_found_in(reports: &[LibraryReport]) -> PluginsFound {
    let mut found = PluginsFound::default();
    for report in reports {
        match &report.outcome {
            LibraryOutcome::Scanned(scan) => {
                for plugin in &scan.plugins {
                    found.add(
                        &report.path,
                        &plugin.description.label,
                        &plugin.description.name,
                        effect_of_a_plugin(&report.path, plugin),
                    );
                }
            }
            LibraryOutcome::ScannedClap(scan) => {
                for plugin in &scan.plugins {
                    found.add(
                        &report.path,
                        &plugin.description.id,
                        &plugin.description.name,
                        effect_of_a_clap_plugin(&report.path, plugin),
                    );
                }
            }
            LibraryOutcome::Failed(_) => {}
        }
    }
    found
}

impl PluginsFound {
    /// Adds a plugin: an effect (unless one with its key is there already, which is the one of the folder that came first), or a plugin that is not used and why.
    fn add(&mut self, library: &Path, label: &str, name: &str, effect: Result<InstalledPlugin, String>) {
        let reason = match effect {
            Ok(effect)
                if self
                    .effects
                    .iter()
                    .any(|already| already.descriptor.key == effect.descriptor.key) =>
            {
                "another plugin with the same key is used instead (the one in the folder that comes first)".to_owned()
            }
            Ok(effect) => {
                self.effects.push(effect);
                return;
            }
            Err(reason) => reason,
        };
        self.not_used.push(NotUsed {
            library: library.to_owned(),
            label: label.to_owned(),
            name: name.to_owned(),
            reason,
        });
    }
}

/// The key of the plugin `label` of the library file `library`.
pub fn key_of(library: &Path, label: &str) -> String {
    let stem = library
        .file_stem()
        .map(|stem| stem.to_string_lossy().replace(':', "_"))
        .unwrap_or_default();
    format!("ladspa:{stem}:{}", label.replace(':', "_"))
}

/// The id of a value: the number of its port and what it is called, in lower case letters and digits.
pub(super) fn id_of_a_value(port_number: usize, name: &str) -> String {
    let mut slug = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('_') {
            slug.push('_');
        }
    }
    format!("port{port_number}_{}", slug.trim_matches('_'))
}

/// The value that a control input is.
fn value_of_a_control(port_number: usize, port: &PortDescription) -> ParameterDescriptor {
    let rate = SAMPLE_RATE;
    let hint = &port.hint;
    let default = hint.default_value(rate);
    let (lower, upper) = hint.bounds(rate);
    let (control, default, range, scale) = if hint.toggled {
        (
            ParameterControl::Toggle,
            PropertyValue::Flag(default > 0.0),
            None,
            ValueScale::Linear,
        )
    } else {
        let range = match (lower, upper) {
            (Some(lower), Some(upper)) => Some((lower as f32, upper as f32)),
            _ => None,
        };
        let scale = match range {
            Some((lower, _)) if hint.logarithmic && lower > 0.0 => ValueScale::Logarithmic,
            _ => ValueScale::Linear,
        };
        let control = if hint.integer {
            ParameterControl::Integer
        } else {
            ParameterControl::Automatic
        };
        (control, PropertyValue::Number(default), range, scale)
    };
    ParameterDescriptor {
        id: Cow::Owned(id_of_a_value(port_number, &port.name)),
        name: Cow::Owned(port.name.to_lowercase()),
        label: Cow::Owned(port.name.clone()),
        default,
        range,
        unit: Cow::Borrowed(""),
        decimals: None,
        unknown_fields: UnknownFields::none(),
        scale,
        control,
    }
}

/// The type of effect that the plugin `plugin` of the library file `library` is, or why it is not one.
pub fn effect_of_a_plugin(library: &Path, plugin: &ScannedPlugin) -> Result<InstalledPlugin, String> {
    if let Some(problem) = &plugin.problem {
        return Err(format!("it could not be made or run: {problem}"));
    }
    let description = &plugin.description;
    let (inputs, outputs) = description.audio_port_counts();
    match (inputs, outputs) {
        (0, _) => {
            return Err(
                "it has no audio input: it makes sound and does not change any (a generator is not used yet)"
                    .to_owned(),
            );
        }
        (_, 0) => return Err("it has no audio output".to_owned()),
        (1, 1) | (2, 2) | (1, 2) | (2, 1) => {}
        (inputs, outputs) => {
            return Err(format!(
                "it has {inputs} audio inputs and {outputs} audio outputs, which no effect uses yet"
            ));
        }
    }
    let mut parameters = Vec::new();
    let mut value_ports = Vec::new();
    for number in description.ports_of(PortDirection::Input, PortKind::Control) {
        parameters.push(value_of_a_control(number, &description.ports[number]));
        value_ports.push(number as u32);
    }
    let descriptor = EffectDescriptor {
        key: Cow::Owned(key_of(library, &description.label)),
        version: (1, 0),
        name: Cow::Owned(if description.name.trim().is_empty() {
            description.label.to_lowercase()
        } else {
            description.name.trim().to_lowercase()
        }),
        description: Cow::Owned(format!(
            "{}, in {}",
            description.label,
            library
                .file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_default()
        )),
        category: Cow::Owned(if description.maker.trim().is_empty() {
            "Plugins".to_owned()
        } else {
            description.maker.trim().to_owned()
        }),
        domain: EffectDomain::Sound,
        changes_each_pixel_by_itself: false,
        parameters: Cow::Owned(parameters),
        unknown_fields: UnknownFields::none(),
    };
    if let Some(problem) = descriptor.problem() {
        return Err(format!("what it says of itself cannot be used: {problem}"));
    }
    Ok(InstalledPlugin {
        descriptor,
        library: library.to_owned(),
        address: PluginAddress::Ladspa {
            index: description.index,
            unique_id: description.unique_id,
        },
        audio_inputs: inputs,
        audio_outputs: outputs,
        value_handles: value_ports,
        pre_roll_samples: 0,
        start_warning: None,
    })
}

// --- CLAP plugins (piece 7.8.3; C4 to C6 of `docs/engine-phase-7-clap-design.md`) ---

/// The key of the CLAP plugin with the id `id`: `clap:` and the id, which is a reverse-domain string that is unique in the world (so no file name is in it, and the same plugin
/// in two folders is one effect). A colon in an id is made an underscore, as in the keys of LADSPA plugins.
pub fn key_of_a_clap_plugin(id: &str) -> String {
    format!("clap:{}", id.replace(':', "_"))
}

/// The id of the value that a CLAP parameter is: `param` and the number the plugin gave the parameter (which CLAP says never changes, so a plugin that renames a parameter is still
/// the same plugin).
pub(super) fn id_of_a_clap_value(parameter_id: u32) -> String {
    format!("param{parameter_id}")
}

/// The most options that the choice of an enum parameter is made of: the scan reads no more texts than this.
const MOST_OPTIONS_OF_AN_ENUM: usize = 256;

/// A range whose most is at least this many times its least, above zero at both ends, is laid out along a logarithmic slider (a frequency from 20 to 20000 Hz is a thousand).
const RATIO_OF_A_LOGARITHMIC_SLIDER: f64 = 1000.0;

/// The value that a parameter of a CLAP plugin is, or `Ok(None)` if it is not one (hidden, or only the plugin can set it), or why the plugin cannot be used.
fn value_of_a_clap_parameter(parameter: &clap_host::scan::Parameter) -> Result<Option<ParameterDescriptor>, String> {
    if parameter.is_hidden() || parameter.is_read_only() {
        return Ok(None);
    }
    let (least, most) = (parameter.minimum, parameter.maximum);
    if !(least.is_finite() && most.is_finite() && least <= most) {
        return Err(format!(
            "its parameter \u{ab}{}\u{bb} has a range that is not a least that is no more than a most ({least} to {most})",
            parameter.name
        ));
    }
    let default = if parameter.default.is_finite() {
        parameter.default.clamp(least, most)
    } else {
        least
    };
    let name = if parameter.name.trim().is_empty() {
        format!("parameter {}", parameter.id)
    } else {
        parameter.name.trim().to_owned()
    };
    let label = if parameter.module.trim().is_empty() {
        name.clone()
    } else {
        format!("{}: {name}", parameter.module.trim())
    };
    let (control, default, range, scale) = if parameter.is_bypass() {
        (
            ParameterControl::Toggle,
            PropertyValue::Flag(default >= 0.5),
            None,
            ValueScale::Linear,
        )
    } else if parameter.is_enum() && !parameter.texts.is_empty() {
        let first = least.round() as i64;
        let options: Vec<ChoiceOption> = parameter
            .texts
            .iter()
            .take(MOST_OPTIONS_OF_AN_ENUM)
            .enumerate()
            .map(|(place, text)| ChoiceOption {
                id: Cow::Owned((first + place as i64).to_string()),
                label: Cow::Owned(if text.trim().is_empty() {
                    (first + place as i64).to_string()
                } else {
                    text.trim().to_owned()
                }),
            })
            .collect();
        let wanted = (default.round() as i64).to_string();
        let default = if options.iter().any(|option| option.id == wanted) {
            wanted
        } else {
            options[0].id.to_string()
        };
        (
            ParameterControl::Choice(Cow::Owned(options)),
            PropertyValue::Words(default),
            None,
            ValueScale::Linear,
        )
    } else if parameter.is_stepped() {
        (
            ParameterControl::Integer,
            PropertyValue::Number(default.round() as f32),
            Some((least as f32, most as f32)),
            ValueScale::Linear,
        )
    } else {
        let logarithmic = least > 0.0 && most / least >= RATIO_OF_A_LOGARITHMIC_SLIDER;
        (
            ParameterControl::Automatic,
            PropertyValue::Number(default as f32),
            Some((least as f32, most as f32)),
            if logarithmic {
                ValueScale::Logarithmic
            } else {
                ValueScale::Linear
            },
        )
    };
    Ok(Some(ParameterDescriptor {
        id: Cow::Owned(id_of_a_clap_value(parameter.id)),
        name: Cow::Owned(name.to_lowercase()),
        label: Cow::Owned(label),
        default,
        range,
        unit: Cow::Borrowed(""),
        decimals: None,
        unknown_fields: UnknownFields::none(),
        scale,
        control,
    }))
}

/// How many channels the main port of a plugin has, if it is one an effect can use: mono or stereo (a port that says nothing of its kind is read by its channels).
fn channels_of_a_main_port(port: &clap_host::scan::AudioPort, direction: &str) -> Result<usize, String> {
    let (kind, count) = (port.port_type.as_deref(), port.channel_count);
    if matches!((kind, count), (None | Some("mono"), 1) | (None | Some("stereo"), 2)) {
        return Ok(count as usize);
    }
    Err(format!(
        "its main audio {direction} is {} with {count} channel{}, and an effect uses a mono or a stereo one",
        kind.map_or_else(
            || "of no kind".to_owned(),
            |kind| format!("of the kind \u{ab}{kind}\u{bb}")
        ),
        if count == 1 { "" } else { "s" }
    ))
}

/// The most sound from before a clip that a plugin is given: two seconds at the rate of the sound engine.
pub const MOST_PRE_ROLL_SAMPLES: usize = 2 * SAMPLE_RATE as usize;

/// How many samples of sound from before a clip a CLAP plugin that says its tail is `tail` samples (`u32::MAX` for ever, `None` for nothing said) is given: the tail, up to
/// [`MOST_PRE_ROLL_SAMPLES`]; for ever is the most; nothing said is half a second.
pub fn pre_roll_of_a_clap_plugin(tail: Option<u32>) -> usize {
    match tail {
        None => SAMPLE_RATE as usize / 2,
        Some(u32::MAX) => MOST_PRE_ROLL_SAMPLES,
        Some(samples) => (samples as usize).min(MOST_PRE_ROLL_SAMPLES),
    }
}

/// The type of effect that the CLAP plugin `plugin` of the library file `library` is, or why it is not one.
pub fn effect_of_a_clap_plugin(
    library: &Path,
    plugin: &clap_host::scan::ScannedPlugin,
) -> Result<InstalledPlugin, String> {
    if let Some(problem) = &plugin.problem {
        return Err(format!("it could not be made or run: {problem}"));
    }
    let description = &plugin.description;
    let Some(input) = plugin.audio_inputs.first() else {
        return Err(
            "it has no audio input: it makes sound and does not change any (a generator is not used yet)".to_owned(),
        );
    };
    let Some(output) = plugin.audio_outputs.first() else {
        return Err("it has no audio output".to_owned());
    };
    let inputs = channels_of_a_main_port(input, "input")?;
    let outputs = channels_of_a_main_port(output, "output")?;
    let mut parameters = Vec::new();
    let mut value_handles = Vec::new();
    for parameter in &plugin.parameters {
        if let Some(value) = value_of_a_clap_parameter(parameter)? {
            if parameters.len() == MOST_VALUES_OF_AN_EFFECT {
                // (The first ones are kept: a plugin with more parameters than an effect can have is rare, and the rest are the ones it lists last.)
                break;
            }
            parameters.push(value);
            value_handles.push(parameter.id);
        }
    }
    let file = library
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut about = if description.description.trim().is_empty() {
        format!("{}, in {file}", description.id)
    } else {
        format!("{} ({}, in {file})", description.description.trim(), description.id)
    };
    if plugin.has_state && parameters.len() <= 2 {
        about.push_str(". It keeps settings that only its own window can change, which cannot be shown here");
    }
    let descriptor = EffectDescriptor {
        key: Cow::Owned(key_of_a_clap_plugin(&description.id)),
        version: (1, 0),
        name: Cow::Owned(description.name.trim().to_lowercase()),
        description: Cow::Owned(about),
        category: Cow::Owned(if description.vendor.trim().is_empty() {
            "Plugins".to_owned()
        } else {
            description.vendor.trim().to_owned()
        }),
        domain: EffectDomain::Sound,
        changes_each_pixel_by_itself: false,
        parameters: Cow::Owned(parameters),
        unknown_fields: UnknownFields::none(),
    };
    if let Some(problem) = descriptor.problem() {
        return Err(format!("what it says of itself cannot be used: {problem}"));
    }
    Ok(InstalledPlugin {
        descriptor,
        library: library.to_owned(),
        address: PluginAddress::Clap {
            id: description.id.clone(),
        },
        audio_inputs: inputs,
        audio_outputs: outputs,
        value_handles,
        pre_roll_samples: pre_roll_of_a_clap_plugin(plugin.tail),
        start_warning: plugin.start.warning(),
    })
}
