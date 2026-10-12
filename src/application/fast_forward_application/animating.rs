//! Changing properties that may be animated, from what the user did to the window.
//!
//! A property that has keyframes is changed *at the playhead*: the change is a keyframe there (made if there was none,
//! changed if there was one), and the keyframes at other frames are as they were. A property that has none is changed as
//! it always was. What the window asks for is the new value of a whole thing (a gain, a place on the picture, a text object),
//! as it always has been, and it is worked out here which of its properties are different from what they are at the
//! playhead now, and only those are changed.

use std::time::Instant;

use super::FastForwardApplication;
use crate::application::commands::Command;
use crate::project::{
    Angle, AnimationTarget, ClipId, Gain, PROPERTIES_OF_A_PLACEMENT, PROPERTIES_OF_A_TEXT_OBJECT, Placement,
    PropertyId, PropertyValue, TextObject, TimelineFrameIndex, TrackId, placement_value, text_value,
};

/// Whether `wanted` is already what a property is: `current`. (A turn is the same as itself and a whole number of turns more
/// or less: that is how far a clip that has turned round more than half a turn by keyframes is shown to be turned.)
fn is_already(property: PropertyId, wanted: &PropertyValue, current: Option<&PropertyValue>) -> bool {
    match (property, wanted, current) {
        (PropertyId::Rotation, PropertyValue::Number(wanted), Some(PropertyValue::Number(current))) => {
            Angle::from_degrees(*wanted) == Angle::from_degrees(*current)
        }
        _ => Some(wanted) == current,
    }
}

impl FastForwardApplication {
    /// The frame of the timeline the playhead is at.
    pub(super) fn frame_at_the_playhead(&self, now: Instant) -> TimelineFrameIndex {
        self.clock.position(now, self.project.total_frame_count())
    }

    /// Whether `command` changes something that has keyframes, so that what it does is a keyframe at the playhead (as
    /// the project is now).
    pub(super) fn command_changes_a_keyframe(&self, command: &Command) -> bool {
        let is_animated = |target, properties: &[PropertyId]| {
            properties
                .iter()
                .any(|property| self.project.is_animated(target, *property))
        };
        match command {
            Command::SetClipGain { clip, .. } => is_animated(AnimationTarget::Clip(*clip), &[PropertyId::Gain]),
            Command::SetClipMuted { clip, .. } => is_animated(AnimationTarget::Clip(*clip), &[PropertyId::Muted]),
            Command::SetClipPlacement { clip, .. } => {
                is_animated(AnimationTarget::Clip(*clip), &PROPERTIES_OF_A_PLACEMENT)
            }
            Command::SetTextObject { clip, .. } => {
                is_animated(AnimationTarget::Clip(*clip), &PROPERTIES_OF_A_TEXT_OBJECT)
            }
            Command::SetTrackGain { track, .. } => is_animated(AnimationTarget::Track(*track), &[PropertyId::Gain]),
            Command::SetTrackMuted { track, .. } => is_animated(AnimationTarget::Track(*track), &[PropertyId::Muted]),
            _ => false,
        }
    }

    /// Makes `property` of `target` have `value` at the playhead: see the module documentation. A change that cannot be made
    /// (the playhead is not on a clip whose property is animated, so that there is no frame of it to put a keyframe at) is
    /// not made.
    pub(super) fn change_a_property_at_the_playhead(
        &mut self,
        target: AnimationTarget,
        property: PropertyId,
        value: PropertyValue,
        now: Instant,
    ) {
        let frame = self.frame_at_the_playhead(now);
        let _ = self.project.set_property_at(target, property, value, frame);
    }

    /// Gives the clip `clip` the loudness `gain`, at the playhead.
    pub(super) fn set_clip_gain_at_the_playhead(&mut self, clip: ClipId, gain: Gain, now: Instant) {
        self.change_a_property_at_the_playhead(
            AnimationTarget::Clip(clip),
            PropertyId::Gain,
            PropertyValue::Number(gain.decibels()),
            now,
        );
    }

    /// Gives the track `track` the loudness `gain`, at the playhead.
    pub(super) fn set_track_gain_at_the_playhead(&mut self, track: TrackId, gain: Gain, now: Instant) {
        self.change_a_property_at_the_playhead(
            AnimationTarget::Track(track),
            PropertyId::Gain,
            PropertyValue::Number(gain.decibels()),
            now,
        );
    }

    /// Mutes the clip or the track `target`, or lets it be heard, at the playhead.
    pub(super) fn set_muted_at_the_playhead(&mut self, target: AnimationTarget, muted: bool, now: Instant) {
        self.change_a_property_at_the_playhead(target, PropertyId::Muted, PropertyValue::Flag(muted), now);
    }

    /// Puts the clip `clip` at `placement` on the picture, at the playhead.
    pub(super) fn set_clip_placement_at_the_playhead(&mut self, clip: ClipId, placement: Placement, now: Instant) {
        let target = AnimationTarget::Clip(clip);
        if !PROPERTIES_OF_A_PLACEMENT
            .into_iter()
            .any(|property| self.project.is_animated(target, property))
        {
            // Nothing about where it is is animated: it is put there, as it always was.
            let _ = self.project.set_clip_placement(clip, placement);
            return;
        }
        let frame = self.frame_at_the_playhead(now);
        for property in PROPERTIES_OF_A_PLACEMENT {
            let Some(wanted) = placement_value(&placement, property) else {
                continue;
            };
            if !is_already(
                property,
                &wanted,
                self.project.property_value_at(target, property, frame).as_ref(),
            ) {
                self.change_a_property_at_the_playhead(target, property, wanted, now);
            }
        }
    }

    /// Gives the text object `clip` the words, font, size, colour and place `object` says, at the playhead.
    pub(super) fn set_text_object_at_the_playhead(&mut self, clip: ClipId, object: TextObject, now: Instant) {
        let target = AnimationTarget::Clip(clip);
        if !PROPERTIES_OF_A_TEXT_OBJECT
            .into_iter()
            .any(|property| self.project.is_animated(target, property))
        {
            let _ = self.project.set_text_object(clip, object);
            return;
        }
        let frame = self.frame_at_the_playhead(now);
        for property in PROPERTIES_OF_A_TEXT_OBJECT {
            let Some(wanted) = text_value(&object, property) else {
                continue;
            };
            if !is_already(
                property,
                &wanted,
                self.project.property_value_at(target, property, frame).as_ref(),
            ) {
                self.change_a_property_at_the_playhead(target, property, wanted, now);
            }
        }
    }
}
