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
    EffectDescriptor, EffectDomain, InstalledPlugin, ParameterControl, ParameterDescriptor, PropertyValue,
    UnknownFields, ValueScale,
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

/// The effects that the libraries in `reports` have. Of two plugins with one key the first is used (the person's own folders come first, then those of `LADSPA_PATH`, then the
/// usual ones).
pub fn plugins_found_in(reports: &[LibraryReport]) -> PluginsFound {
    let mut found = PluginsFound::default();
    for report in reports {
        let LibraryOutcome::Scanned(scan) = &report.outcome else {
            continue;
        };
        for plugin in &scan.plugins {
            match effect_of_a_plugin(&report.path, plugin) {
                Ok(effect) => {
                    if found
                        .effects
                        .iter()
                        .any(|already| already.descriptor.key == effect.descriptor.key)
                    {
                        found.not_used.push(not_used(
                            &report.path,
                            plugin,
                            "another plugin with the same key is used instead (the one in the folder that comes first)",
                        ));
                    } else {
                        found.effects.push(effect);
                    }
                }
                Err(reason) => found.not_used.push(not_used(&report.path, plugin, &reason)),
            }
        }
    }
    found
}

fn not_used(library: &Path, plugin: &ScannedPlugin, reason: &str) -> NotUsed {
    NotUsed {
        library: library.to_owned(),
        label: plugin.description.label.clone(),
        name: plugin.description.name.clone(),
        reason: reason.to_owned(),
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
        value_ports.push(number);
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
        plugin_index: description.index,
        unique_id: description.unique_id,
        audio_inputs: inputs,
        audio_outputs: outputs,
        value_ports,
    })
}
