// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Putting effects on clips and on tracks, and taking them off.
//!
//! The effects a clip or a track has are in the clip's entry on its track and in the track (see [`super::tracks::Track`]), and
//! their values, when they are not animated, are with them; their keyframes are in racks like those of any property (see
//! [`super::racks`]). So an effect that is taken off takes its keyframes with it, from wherever they are kept.

use super::animating::{AnimationTarget, not_a_property_of};
use super::animation::PropertyId;
use super::effects::{EffectInstanceId, EffectKind, Effects, MOST_EFFECTS_ON_ONE_THING};
use super::tracks::{ClipEditError, KeyframeId, RackKeyframe};
use super::{EditError, Member, Project, TimelineFrameIndex};

impl Project {
    /// The effects of the clip or the track `target`, if it is there.
    pub fn effects_of(&self, target: AnimationTarget) -> Option<&Effects> {
        match target {
            AnimationTarget::Clip(clip) => self.placement_of(clip).map(|placement| placement.effects),
            AnimationTarget::Track(track) => self.track(track).map(|track| track.effects()),
        }
    }

    /// The effects of the clip or the track `target`, or the error that says it is not there.
    fn effects_of_or_error(&self, target: AnimationTarget) -> Result<&Effects, EditError> {
        self.effects_of(target).ok_or(match target {
            AnimationTarget::Clip(clip) => EditError::Clip(ClipEditError::UnknownClip { clip }),
            AnimationTarget::Track(track) => EditError::UnknownTrack { track },
        })
    }

    /// Whether the clip or the track `target` is there and has the effect that is named `instance` (in its chain).
    pub fn has_the_effect(&self, target: AnimationTarget, instance: EffectInstanceId) -> bool {
        self.effects_of(target)
            .is_some_and(|effects| effects.has_the_instance(instance))
    }

    /// Puts the effect `kind` on the clip or the track `target`, last, with the values it begins with, and gives the name it has in the chain
    /// of what has it. (There may be effects of that kind on it already: it is one more.) It is an error, and nothing is changed, if
    /// there is no such clip or track, or it cannot have that effect (a sound cannot have one that changes pictures), or it has [`MOST_EFFECTS_ON_ONE_THING`] already.
    pub fn add_effect(&mut self, target: AnimationTarget, kind: EffectKind) -> Result<EffectInstanceId, EditError> {
        if self.effects_of_or_error(target)?.is_full() {
            return Err(EditError::TooManyEffects {
                limit: MOST_EFFECTS_ON_ONE_THING,
            });
        }
        match target {
            AnimationTarget::Clip(clip) => Ok(self
                .track_holding_mut_or_error(clip)?
                .add_effect_to_the_clip(clip, kind)?),
            AnimationTarget::Track(track) => self
                .track_mut_or_error(track)?
                .add_effect(kind)
                .ok_or(EditError::NotAnEffectOfTheTrack { track, kind }),
        }
    }

    /// Takes the effect that is named `instance` off the clip or the track `target`, with all the keyframes of its values, wherever they are
    /// kept (a rack that has none left, and a keyframe crate that has no rack left, go too). The other effects, of whatever kind, stay as they
    /// are, with their keyframes. It is an error, and nothing is changed, if there is no such clip or track, or it does not have the effect.
    pub fn remove_effect(&mut self, target: AnimationTarget, instance: EffectInstanceId) -> Result<(), EditError> {
        if let AnimationTarget::Clip(clip) = target
            && let Some(clip) = self.the_clip_whose_content_is(Member::Clip(clip), instance)
        {
            return Err(EditError::TheContentOfAGenerator { clip });
        }
        let Some(effect) = self.effects_of_or_error(target)?.instance(instance) else {
            return Err(EditError::DoesNotHaveThatEffect { target, instance });
        };
        // The keyframes go first, while the values are still properties of what has the effect: a property that has no keyframe
        // left is given what its last keyframe said, which only what has the effect can be given.
        let its_values: Vec<PropertyId> = effect.properties().collect();
        let keyframes_of_its_values: Vec<KeyframeId> = self
            .located_racks()
            .filter(|located| located.rack.target() == target && its_values.contains(&located.rack.property()))
            .flat_map(|located| located.rack.keyframes().iter().map(RackKeyframe::id))
            .collect();
        self.remove_keyframes(&keyframes_of_its_values)?;
        match target {
            AnimationTarget::Clip(clip) => {
                self.track_holding_mut_or_error(clip)?
                    .remove_effect_from_the_clip(clip, instance)?;
            }
            AnimationTarget::Track(track) => {
                self.track_mut_or_error(track)?.remove_effect(instance);
            }
        }
        Ok(())
    }

    /// Gives some values of the effect that is named `instance` of the clip or the track `target` what a preset says (piece 11.7 of `docs/engine-phase-11-presets-design.md`): `values` are
    /// the place of each value in the effect (its slot) and what it is to be. **A value that has keyframes is left as it is** (an animation is somebody's work, and a choice in a menu
    /// does not overwrite it); how many were left so is what this gives. A slot that the effect has no value for is ignored. It is an error, and **nothing is changed**, if there is no
    /// such effect or a value is of another kind than the one it is for.
    pub fn apply_values_to_an_effect(
        &mut self,
        target: AnimationTarget,
        instance: EffectInstanceId,
        values: &[(usize, super::PropertyValue)],
        timeline_frame: TimelineFrameIndex,
    ) -> Result<usize, EditError> {
        let Some(effect) = self.effects_of_or_error(target)?.instance(instance) else {
            return Err(EditError::DoesNotHaveThatEffect { target, instance });
        };
        let wanted: Vec<(PropertyId, &super::PropertyValue)> = values
            .iter()
            .filter_map(|(slot, value)| effect.property(*slot).map(|property| (property, value)))
            .collect();
        if let Some((property, _)) = wanted.iter().find(|(property, value)| value.kind() != property.kind()) {
            return Err(not_a_property_of(target, *property));
        }
        let mut kept = 0;
        for (property, value) in wanted {
            if self.is_animated(target, property) {
                kept += 1;
            } else {
                self.set_property_at(target, property, value.clone(), timeline_frame)?;
            }
        }
        Ok(kept)
    }
}
