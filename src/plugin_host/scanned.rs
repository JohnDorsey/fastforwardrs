// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the scan said a plugin is, as far as a copy that is made of it must be that: the same ports, and a parameter or a port for each value of the effect. **The one place that decides
//! whether a copy is the plugin that was scanned**, for the copy that is made in the editor's process and for the copy that is made in a process of its own (`plugin_process`), which is
//! told these facts in its request because it has no `InstalledPlugin`.

use serde::{Deserialize, Serialize};

use crate::project::InstalledPlugin;

/// What the scan said of a plugin that a copy must be.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhatWasScanned {
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    /// For each value of the effect: the number of the port (LADSPA, LV2) or the id of the parameter (CLAP) that it is the value of.
    pub value_handles: Vec<u32>,
    /// A LADSPA plugin's label, as its key spells it (a colon in a label is an underscore there).
    pub ladspa_label: Option<String>,
}

impl WhatWasScanned {
    pub fn of(plugin: &InstalledPlugin) -> Self {
        Self {
            audio_inputs: plugin.audio_inputs,
            audio_outputs: plugin.audio_outputs,
            value_handles: plugin.value_handles.clone(),
            ladspa_label: plugin
                .address
                .ladspa_index()
                .map(|_| plugin.descriptor.key.rsplit(':').next().unwrap_or_default().to_owned()),
        }
    }

    /// Whether what the library gave for the CLAP plugin is the plugin that was scanned: the main ports have the channels that were scanned, and the plugin has every parameter that a
    /// value of the effect is the value of (it may have more).
    pub fn is_this_clap_plugin(&self, instance: &clap_host::Instance) -> bool {
        let channels = |port: Option<&clap_host::scan::AudioPort>| port.map_or(0, |port| port.channel_count as usize);
        (
            channels(instance.main_audio_input()),
            channels(instance.audio_outputs().first()),
        ) == (self.audio_inputs, self.audio_outputs)
            && self
                .value_handles
                .iter()
                .all(|handle| instance.parameters().iter().any(|parameter| parameter.id == *handle))
    }

    /// Whether what the bundle says now of the LV2 plugin is the plugin that was scanned: the same audio ports and the same control inputs for the values.
    pub fn is_this_lv2_plugin(&self, description: &lv2_host::PluginDescription) -> bool {
        use lv2_host::{PortDirection, PortKind};
        let count = |direction: PortDirection| {
            description
                .ports
                .iter()
                .filter(|port| port.kind == PortKind::Audio && port.direction == direction)
                .count()
        };
        (count(PortDirection::Input), count(PortDirection::Output)) == (self.audio_inputs, self.audio_outputs)
            && self.value_handles.iter().all(|&index| {
                description
                    .ports
                    .get(index as usize)
                    .is_some_and(|port| port.direction == PortDirection::Input && port.kind == PortKind::Control)
            })
    }

    /// Whether what the library gave for the LADSPA plugin is the plugin that was scanned: the same label and ports of the same kinds, so that no value is given to a port that is
    /// not the one it was made for.
    pub fn is_this_ladspa_plugin(&self, instance: &ladspa_host::Instance) -> bool {
        use ladspa_host::{PortDirection, PortKind};
        let description = instance.description();
        let (inputs, outputs) = description.audio_port_counts();
        description.label.replace(':', "_") == self.ladspa_label.as_deref().unwrap_or_default()
            && (inputs, outputs) == (self.audio_inputs, self.audio_outputs)
            && self.value_handles.iter().all(|&port| {
                description
                    .ports
                    .get(port as usize)
                    .is_some_and(|port| port.direction == PortDirection::Input && port.kind == PortKind::Control)
            })
    }
}
