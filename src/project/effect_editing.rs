// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Putting effects on clips and on tracks, and taking them off.
//!
//! The effects a clip or a track has are in the clip's entry on its track and in the track (see [`super::tracks::Track`]), and
//! their values, when they are not animated, are with them; their keyframes are in racks like those of any property (see
//! [`super::racks`]). So an effect that is taken off takes its keyframes with it, from wherever they are kept.

use super::animating::AnimationTarget;
use super::animation::PropertyId;
use super::effects::{EffectInstanceId, EffectKind, Effects, MOST_EFFECTS_ON_ONE_THING};
use super::tracks::{ClipEditError, KeyframeId, RackKeyframe};
use super::{EditError, Member, Project};

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
}
