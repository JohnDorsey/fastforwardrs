// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Presets of plugins (piece 11.7 of `docs/engine-phase-11-presets-design.md`): choosing one in the *Presets* menu of the box of an effect gives the values of the effect what the preset says,
//! in one step of undo. The presets are those that the scan found for the plugin (`InstalledPlugin::presets`); what they say is already made into values of the effect.

use std::time::Instant;

use super::FastForwardApplication;
use crate::project::{AnimationTarget, EffectInstanceId};

impl FastForwardApplication {
    /// Gives the values of the effect `instance` of `target` what its plugin's preset number `preset` says. A value that has keyframes is left as it is; an effect that is not there, a
    /// plugin that is not installed and a preset that is not there are left alone.
    pub(super) fn apply_an_effect_preset(
        &mut self,
        target: AnimationTarget,
        instance: EffectInstanceId,
        preset: usize,
        now: Instant,
    ) {
        let Some(values) = self
            .document
            .project
            .effects_of(target)
            .and_then(|effects| effects.instance(instance))
            .and_then(|effect| effect.kind().installed_plugin())
            .and_then(|plugin| plugin.presets.get(preset))
            .map(|preset| preset.values.clone())
        else {
            return;
        };
        let frame = self.frame_to_make_keyframes_at(now);
        let _ = self
            .document
            .project
            .apply_values_to_an_effect(target, instance, &values, frame);
    }
}
