// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The switch that makes a synthesizer pass on the notes that reach it (phase 13, piece 13.5; see docs/engine-phase-13-midi-design.md). It is off when the synthesizer is put on: a synthesizer
//! consumes the notes that reach it.

use super::{ClipEditError, EditError, EffectInstanceId, Member, Project};

impl Project {
    /// Says whether the synthesizer that is named `instance` in the chain of `owner` (a clip, a track or a group) passes on the notes that reach it, to the next synthesizer on their road. It is an
    /// error, and nothing is changed, if there is no such thing as `owner`, if its chain has no such effect, or if the effect is not a synthesizer (an effect that is not has no notes to pass).
    pub fn set_effect_passes_notes_on(
        &mut self,
        owner: Member,
        instance: EffectInstanceId,
        passes_notes_on: bool,
    ) -> Result<(), EditError> {
        let chain = self.chain_of_the_member_mut(owner).ok_or(match owner {
            Member::Clip(clip) => EditError::Clip(ClipEditError::UnknownClip { clip }),
            Member::Track(track) => EditError::UnknownTrack { track },
            Member::Group(group) => EditError::UnknownGroup { group },
        })?;
        let Some(effect) = chain.instance(instance) else {
            return Err(EditError::NotInTheChain {
                owner,
                item: super::ChainItemKey::Effect(instance),
            });
        };
        if !effect.kind().makes_sound_from_notes() {
            return Err(EditError::NotASynthesizer { owner, instance });
        }
        chain.set_passes_notes_on(instance, passes_notes_on);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
