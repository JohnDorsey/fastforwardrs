//! Putting keyframes on a track's own properties and on those of its clips, in the racks of their own drawers.
//!
//! What a keyframe is, and what is between two, is [`crate::project::Animation`]'s. What is here is where they are
//! kept: those of a clip are in its drawer and counted from the clip's first frame, those of the track in its drawer and
//! counted from the start of the timeline. (A property of a clip or a track may have keyframes in other drawers too,
//! which is the project's to look at: only the racks that are in the drawer of the thing itself are here.) The values that
//! the properties have when they are not animated are not touched.

use super::Track;
use super::editing::ClipEditError;
use crate::project::tracks::ids::ClipId;
use crate::project::tracks::rack::Rack;
use crate::project::{AnimationTarget, PropertyId, PropertyValue, TimelineFrameIndex};

impl Track {
    /// Puts a keyframe with `value` for the track's own `property` at `timeline_frame`, in place of the one there if
    /// there is one. Returns false, and changes nothing, if the track does not have that property (a value of an effect that
    /// it does not have is not one of its properties) or it is not that kind of value.
    pub fn set_own_keyframe(
        &mut self,
        property: PropertyId,
        timeline_frame: TimelineFrameIndex,
        value: PropertyValue,
    ) -> bool {
        if !property.is_had_by_a_track(&self.effects) || value.kind() != property.kind() {
            return false;
        }
        let target = AnimationTarget::Track(self.id);
        if self.drawer.first_rack_for(target, property).is_none() {
            let rack = Rack::new(self.take_next_rack_id(), target, property);
            self.drawer.push(rack);
        }
        self.drawer
            .first_rack_for_mut(target, property)
            .is_some_and(|rack| rack.set(timeline_frame.value(), value).is_ok())
    }

    /// Puts a keyframe with `value` for the `property` of the clip `clip` at `frames_in` frames after its first frame, in
    /// place of the one there if there is one. Nothing is changed (and an error says why) if the clip does not have that
    /// property (a value of an effect that it does not have is not one of its properties), is not that long, or `value` is not
    /// the kind of value the property has.
    pub fn set_clip_keyframe(
        &mut self,
        clip: ClipId,
        property: PropertyId,
        frames_in: u64,
        value: PropertyValue,
    ) -> Result<(), ClipEditError> {
        let index = self.index_of(clip).ok_or(ClipEditError::UnknownClip { clip })?;
        let entry = &self.entries[index];
        if !property.is_had_by_a_clip(entry.clip.kind(), &entry.effects) || value.kind() != property.kind() {
            return Err(ClipEditError::NotAPropertyOfThisClip { clip, property });
        }
        if frames_in >= entry.clip.timeline_frame_count(self.speed) {
            return Err(ClipEditError::NotWithinTheClip { clip, frames_in });
        }
        let target = AnimationTarget::Clip(clip);
        if self.entries[index].drawer.first_rack_for(target, property).is_none() {
            let rack = Rack::new(self.take_next_rack_id(), target, property);
            self.entries[index].drawer.push(rack);
        }
        if let Some(rack) = self.entries[index].drawer.first_rack_for_mut(target, property) {
            let _ = rack.set(frames_in, value);
        }
        Ok(())
    }
}
