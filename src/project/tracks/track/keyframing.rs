//! Putting keyframes on a track's own properties and on those of its clips, and taking them away.
//!
//! What a keyframe is, and what is between two, is [`crate::project::Animation`]'s. What is here is where they are
//! kept: those of a clip are counted from the clip's first frame, those of the track from the start of the timeline.
//! The values that the properties have when they are not animated are not touched.

use super::Track;
use super::editing::ClipEditError;
use crate::project::tracks::ids::ClipId;
use crate::project::{PropertyId, PropertyValue, RemovedKeyframe, TimelineFrameIndex};

impl Track {
    /// Puts a keyframe with `value` for the track's own `property` at `timeline_frame`, in place of the one there if
    /// there is one. Returns false, and changes nothing, if the track does not have that property or it is not that
    /// kind of value.
    pub fn set_own_keyframe(
        &mut self,
        property: PropertyId,
        timeline_frame: TimelineFrameIndex,
        value: PropertyValue,
    ) -> bool {
        property.applies_to_a_track() && self.animation.set_keyframe(property, timeline_frame.value(), value)
    }

    /// Takes away the keyframe of the track's own `property` at `timeline_frame`, if there is one.
    pub fn remove_own_keyframe(
        &mut self,
        property: PropertyId,
        timeline_frame: TimelineFrameIndex,
    ) -> Option<RemovedKeyframe> {
        self.animation.remove_keyframe(property, timeline_frame.value())
    }

    /// Puts a keyframe with `value` for the `property` of the clip `clip` at `frames_in` frames after its first frame, in
    /// place of the one there if there is one. Nothing is changed (and an error says why) if the clip does not have that
    /// property, is not that long, or `value` is not the kind of value the property has.
    pub fn set_clip_keyframe(
        &mut self,
        clip: ClipId,
        property: PropertyId,
        frames_in: u64,
        value: PropertyValue,
    ) -> Result<(), ClipEditError> {
        let index = self.index_of(clip).ok_or(ClipEditError::UnknownClip { clip })?;
        let entry = &self.entries[index];
        if !property.applies_to_a_clip_of_kind(entry.clip.kind()) || value.kind() != property.kind() {
            return Err(ClipEditError::NotAPropertyOfThisClip { clip, property });
        }
        if frames_in >= entry.clip.timeline_frame_count(self.speed) {
            return Err(ClipEditError::NotWithinTheClip { clip, frames_in });
        }
        self.entries[index].animation.set_keyframe(property, frames_in, value);
        Ok(())
    }

    /// Takes away the keyframe of the `property` of the clip `clip` at `frames_in` frames after its first frame, if there is
    /// one.
    pub fn remove_clip_keyframe(
        &mut self,
        clip: ClipId,
        property: PropertyId,
        frames_in: u64,
    ) -> Result<Option<RemovedKeyframe>, ClipEditError> {
        let index = self.index_of(clip).ok_or(ClipEditError::UnknownClip { clip })?;
        Ok(self.entries[index].animation.remove_keyframe(property, frames_in))
    }
}
