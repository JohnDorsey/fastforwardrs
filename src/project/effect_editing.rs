//! Putting effects on clips and on tracks, and taking them off.
//!
//! The effects a clip or a track has are in the clip's entry on its track and in the track (see [`super::tracks::Track`]), and
//! their values, when they are not animated, are with them; their keyframes are in racks like those of any property (see
//! [`super::racks`]). So an effect that is taken off takes its keyframes with it, from wherever they are kept.

use super::animating::AnimationTarget;
use super::effects::{EffectKind, Effects};
use super::tracks::{ClipEditError, KeyframeId, RackKeyframe};
use super::{EditError, Project};

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

    /// Whether the clip or the track `target` is there and has the effect `kind`.
    pub fn has_the_effect(&self, target: AnimationTarget, kind: EffectKind) -> bool {
        self.effects_of(target).is_some_and(|effects| effects.contains(kind))
    }

    /// Puts the effect `kind` on the clip or the track `target`, with the values it begins with. It is an error, and nothing is
    /// changed, if there is no such clip or track, if it cannot have that effect (a sound cannot have one that changes pictures), or
    /// if it has it already.
    pub fn add_effect(&mut self, target: AnimationTarget, kind: EffectKind) -> Result<(), EditError> {
        let added = match target {
            AnimationTarget::Clip(clip) => self
                .track_holding_mut_or_error(clip)?
                .add_effect_to_the_clip(clip, kind)?,
            AnimationTarget::Track(track) => {
                let track_to_change = self.track_mut_or_error(track)?;
                if !kind.applies_to_a_track() {
                    return Err(EditError::NotAnEffectOfTheTrack { track, kind });
                }
                track_to_change.add_effect(kind)
            }
        };
        if added {
            Ok(())
        } else {
            Err(EditError::AlreadyHasThatEffect { target, kind })
        }
    }

    /// Takes the effect `kind` off the clip or the track `target`, with all the keyframes of its values, wherever they are kept
    /// (a rack that has none left, and a keyframe crate that has no rack left, go too). It is an error, and nothing is changed, if
    /// there is no such clip or track, or it does not have the effect.
    pub fn remove_effect(&mut self, target: AnimationTarget, kind: EffectKind) -> Result<(), EditError> {
        if !self.effects_of_or_error(target)?.contains(kind) {
            return Err(EditError::DoesNotHaveThatEffect { target, kind });
        }
        // The keyframes go first, while the values are still properties of what has the effect: a property that has no keyframe
        // left is given what its last keyframe said, which only what has the effect can be given.
        let keyframes_of_its_values: Vec<KeyframeId> = self
            .located_racks()
            .filter(|located| located.rack.target() == target && kind.properties().contains(&located.rack.property()))
            .flat_map(|located| located.rack.keyframes().iter().map(RackKeyframe::id))
            .collect();
        self.remove_keyframes(&keyframes_of_its_values)?;
        match target {
            AnimationTarget::Clip(clip) => {
                self.track_holding_mut_or_error(clip)?
                    .remove_effect_from_the_clip(clip, kind)?;
            }
            AnimationTarget::Track(track) => {
                self.track_mut_or_error(track)?.remove_effect(kind);
            }
        }
        Ok(())
    }
}
