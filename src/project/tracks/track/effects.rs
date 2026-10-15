// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Putting effects on a track and on its clips, taking them off, and changing their values.
//!
//! What an effect does is [`crate::project::EffectKind`]'s. What is here is which effects a track and each of its clips have,
//! and the values of them that are not animated. (Taking an effect off does not take away the keyframes that are of its
//! values: those are the project's to find, wherever they are kept.)

use super::{ClipEditError, Track};
use crate::project::tracks::ids::ClipId;
use crate::project::{EffectInstanceId, EffectKind, Effects, PropertyId, PropertyValue};

impl Track {
    /// The same track with `effects` for the effects of its own, which are used as they are. (For making a track from its parts.)
    #[must_use]
    pub fn with_effects(mut self, effects: Effects) -> Self {
        self.effects = effects;
        self
    }

    /// The same track with `effects` for the effects of its clips, one set for each clip, in the order of the clips, which are used
    /// as they are. (For making a track from its parts.)
    #[must_use]
    pub fn with_effects_on_its_clips(mut self, effects: impl IntoIterator<Item = Effects>) -> Self {
        for (entry, effects) in self.entries.iter_mut().zip(effects) {
            entry.effects = effects;
        }
        self
    }

    /// Puts the effect `kind` on the track, with the values it begins with, and gives the name it has in the track's chain. `None`, and
    /// nothing is changed, if a track cannot have that effect.
    pub fn add_effect(&mut self, kind: EffectKind) -> Option<EffectInstanceId> {
        kind.applies_to_a_track().then(|| self.effects.add(kind))
    }

    /// Takes the effect that is named `instance` off the track. `false` if it did not have it.
    pub fn remove_effect(&mut self, instance: EffectInstanceId) -> bool {
        self.effects.remove(instance)
    }

    /// Gives the value of the track's effect that is `property` the value `value` (when it is not animated). `false`, and nothing
    /// is changed, if the track does not have the effect or `value` is not what it has.
    pub fn set_effect_value(&mut self, property: PropertyId, value: &PropertyValue) -> bool {
        self.effects.set_value(property, value)
    }

    /// The effects of the clip `clip` (with the badges that are in its chain), to change them.
    pub(in crate::project) fn effects_of_the_clip_mut(&mut self, clip: ClipId) -> Result<&mut Effects, ClipEditError> {
        let index = self.index_of(clip).ok_or(ClipEditError::UnknownClip { clip })?;
        Ok(&mut self.entries[index].effects)
    }

    /// The effects of the track itself (its chain), to change them.
    pub(in crate::project) fn effects_mut(&mut self) -> &mut Effects {
        &mut self.effects
    }

    /// Puts the effect `kind` on the clip `clip`, with the values it begins with, and gives the name it has in the chain of the clip. An
    /// error says why not, and nothing is changed, if there is no such clip, or a clip of its kind cannot have that effect.
    pub fn add_effect_to_the_clip(
        &mut self,
        clip: ClipId,
        kind: EffectKind,
    ) -> Result<EffectInstanceId, ClipEditError> {
        let index = self.index_of(clip).ok_or(ClipEditError::UnknownClip { clip })?;
        if !kind.applies_to_a_clip_of_kind(self.entries[index].clip.kind()) {
            return Err(ClipEditError::NotAnEffectOfThisClip { clip, kind });
        }
        Ok(self.entries[index].effects.add(kind))
    }

    /// Takes the effect that is named `instance` off the clip `clip`. `Ok(false)` if it did not have it.
    pub fn remove_effect_from_the_clip(
        &mut self,
        clip: ClipId,
        instance: EffectInstanceId,
    ) -> Result<bool, ClipEditError> {
        Ok(self.effects_of_the_clip_mut(clip)?.remove(instance))
    }

    /// Gives the value of the clip's effect that is `property` the value `value` (when it is not animated). An error says why not,
    /// and nothing is changed, if there is no such clip, the clip does not have the effect, or `value` is not what it has.
    pub fn set_effect_value_of_the_clip(
        &mut self,
        clip: ClipId,
        property: PropertyId,
        value: &PropertyValue,
    ) -> Result<(), ClipEditError> {
        if self.effects_of_the_clip_mut(clip)?.set_value(property, value) {
            Ok(())
        } else {
            Err(ClipEditError::NotAPropertyOfThisClip { clip, property })
        }
    }
}
