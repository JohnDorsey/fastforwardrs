// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Changing properties that may be animated, from what the user did to the window.
//!
//! A property that has keyframes is changed *at the playhead*: the change is a keyframe there (made if there was none,
//! changed if there was one), and the keyframes at other frames are as they were. A property that has none is changed as
//! it always was. What the window asks for is the new value of a whole thing (a gain, a place on the picture, a text object),
//! as it always has been, and it is worked out here which of its properties are different from what they are at the
//! playhead now, and only those are changed.
//!
//! "At the playhead" is at the frame the playhead was at when the gesture that asks for the change began (see
//! [`crate::application::gesture_frame`]), so that a gesture that goes on for a time makes one keyframe and not one at every
//! frame the playhead goes over; and a change of what has keyframes stops the video that is playing, which is what keeps the
//! playhead still at the frame it is made at.

use std::time::Instant;

use super::FastForwardApplication;
use crate::application::commands::Command;
use crate::application::history::EditKey;
use crate::project::{
    AnimationTarget, ClipId, Gain, PROPERTIES_OF_A_PLACEMENT, Placement, PropertyId, PropertyValue, TextObject,
    TimelineFrameIndex, TrackId, placement_value, properties_of_a_text_object, values_of_a_text_object,
};

impl FastForwardApplication {
    /// The frame of the timeline the playhead is at.
    pub(super) fn frame_at_the_playhead(&self, now: Instant) -> TimelineFrameIndex {
        self.playback
            .clock
            .position(now, self.document.project.total_frame_count())
    }

    /// The frame of the timeline that the keyframes that the commands being carried out make or change are at: the one that the
    /// gesture they are of began at (see [`Self::frame_of_the_edits_of_what_has_keyframes`]), and the playhead's if they are of no
    /// gesture, or are not about what has keyframes.
    pub(super) fn frame_to_make_keyframes_at(&self, now: Instant) -> TimelineFrameIndex {
        self.interface
            .frame_keyframes_are_made_at
            .unwrap_or_else(|| self.frame_at_the_playhead(now))
    }

    /// Works out the frame that an edit of what has keyframes is made at, which is where the gesture it is of began (the
    /// gesture is what `gesture_key` and `press_began_at` say), and stops the video if it is playing.
    ///
    /// A video that plays while a value is dragged would put the keyframe that the drag makes at the frame it is at, and then,
    /// as the drag asks for the same change in every frame of the window, at every frame after that: a keyframe for each, the
    /// whole movement of the hand written down, when what was wanted was one. So it stops, where it is, and the edit is made
    /// there. (Things that have no keyframes can be changed while the video plays: they are changed for all of it.)
    pub(super) fn frame_of_the_edits_of_what_has_keyframes(
        &mut self,
        gesture_key: Option<EditKey>,
        press_began_at: Option<f64>,
        now: Instant,
    ) -> TimelineFrameIndex {
        let total_frames = self.document.project.total_frame_count();
        self.playback.clock.pause(now, total_frames);
        let playhead = self.frame_at_the_playhead(now);
        self.interface
            .gesture_frame
            .frame_for(gesture_key, press_began_at, playhead)
    }

    /// Whether `command` changes something that has keyframes, so that what it does is a keyframe at the playhead (as
    /// the project is now).
    pub(super) fn command_changes_a_keyframe(&self, command: &Command) -> bool {
        let is_animated = |target, properties: &[PropertyId]| {
            properties
                .iter()
                .any(|property| self.document.project.is_animated(target, *property))
        };
        match command {
            Command::SetClipGain { clip, .. } => is_animated(AnimationTarget::Clip(*clip), &[PropertyId::Gain]),
            Command::SetClipMuted { clip, .. } => is_animated(AnimationTarget::Clip(*clip), &[PropertyId::Muted]),
            Command::SetClipPlacement { clip, .. } => {
                is_animated(AnimationTarget::Clip(*clip), &PROPERTIES_OF_A_PLACEMENT)
            }
            Command::SetTextObject { clip, .. } => self
                .document
                .project
                .placement_of(*clip)
                .and_then(|placement| placement.clip.content())
                .is_some_and(|content| {
                    is_animated(AnimationTarget::Clip(*clip), &properties_of_a_text_object(content))
                }),
            Command::SetTrackGain { track, .. } => is_animated(AnimationTarget::Track(*track), &[PropertyId::Gain]),
            Command::SetTrackMuted { track, .. } => is_animated(AnimationTarget::Track(*track), &[PropertyId::Muted]),
            Command::SetEffectValue { target, property, .. } => is_animated(*target, &[*property]),
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
        let frame = self.frame_to_make_keyframes_at(now);
        // (If that made a keyframe, the drawer it is in opens, so that it is seen where it is.)
        if let Ok(Some(made)) = self.document.project.set_property_at(target, property, value, frame) {
            self.interface.timeline_view.open_the_drawer_of(made.owner);
        }
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
            .any(|property| self.document.project.is_animated(target, property))
        {
            // Nothing about where it is is animated: it is put there, as it always was.
            let _ = self.document.project.set_clip_placement(clip, placement);
            return;
        }
        // (What is different from what it was is worked out from what the window showed, which is what it is at the playhead.)
        let shown_at = self.frame_at_the_playhead(now);
        for property in PROPERTIES_OF_A_PLACEMENT {
            let Some(wanted) = placement_value(&placement, property) else {
                continue;
            };
            if self
                .document
                .project
                .property_value_at(target, property, shown_at)
                .as_ref()
                != Some(&wanted)
            {
                self.change_a_property_at_the_playhead(target, property, wanted, now);
            }
        }
    }

    /// Gives the text object `clip` the words, font, size, colour and place `object` says, at the playhead.
    pub(super) fn set_text_object_at_the_playhead(&mut self, clip: ClipId, object: TextObject, now: Instant) {
        let target = AnimationTarget::Clip(clip);
        // (What a text says and how it looks are the values of its text effect, and where it is the clip's.)
        let Some(content) = self
            .document
            .project
            .placement_of(clip)
            .and_then(|placement| placement.clip.content())
        else {
            return;
        };
        let properties = properties_of_a_text_object(content);
        if !properties
            .into_iter()
            .any(|property| self.document.project.is_animated(target, property))
        {
            let _ = self.document.project.set_text_object(clip, object);
            return;
        }
        let shown_at = self.frame_at_the_playhead(now);
        let values = values_of_a_text_object(&object);
        for property in properties {
            let wanted = match property {
                PropertyId::EffectValue { slot, .. } => values.get(usize::from(slot)).cloned(),
                other => placement_value(&object.placement(), other),
            };
            let Some(wanted) = wanted else {
                continue;
            };
            if self
                .document
                .project
                .property_value_at(target, property, shown_at)
                .as_ref()
                != Some(&wanted)
            {
                self.change_a_property_at_the_playhead(target, property, wanted, now);
            }
        }
    }
}
