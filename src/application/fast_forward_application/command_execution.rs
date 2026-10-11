//! What each [`Command`] does.

use std::time::Instant;

use egui::Context;

use super::FastForwardApplication;
use crate::application::commands::Command;
use crate::application::dialogs::{AfterConfirmation, Dialog};
use crate::application::editing::{
    delete_clip_keeping_the_picture, set_clip_speed_keeping_the_picture, set_track_speed_keeping_the_picture,
};
use crate::project::{ClipLocation, Gain, SpeedFactor, TextObject, TimelineFrameIndex, TrackId};

/// How long a text object lasts when it is made, in seconds (or less, if it would run into a clip).
const SECONDS_A_NEW_TEXT_OBJECT_LASTS: f64 = 3.0;

impl FastForwardApplication {
    /// Carries out one command. Commands from the file menu may open a window and wait for the
    /// user; the others change the project or the playback.
    pub(super) fn execute(&mut self, command: Command, now: Instant, context: &Context) {
        // Whatever was asked for shows (and sounds) in the next frame of the window, so make sure there is one.
        context.request_repaint();
        let total_frames = self.project.total_frame_count();
        match command {
            Command::LoadProject => self.ask_before_discarding(AfterConfirmation::LoadProject),
            Command::SaveProject => self.save_project(),
            Command::NewProject => self.ask_before_discarding(AfterConfirmation::NewProject),
            Command::ImportFile => self.import_file(),
            Command::ExportVideo => self.export_video(),

            Command::Deselect => self.selected_clip = None,
            Command::ShowProjectSettings => self.settings_windows_open.project = true,
            Command::ShowGlobalSettings => self.settings_windows_open.global = true,
            Command::SetProjectSettings(settings) => self.set_project_settings(settings),
            Command::SetGlobalSettings(settings) => self.set_global_settings(settings),

            Command::Pause => self.clock.pause(now, total_frames),
            Command::Play => self.clock.play(now, total_frames),
            Command::Stop => self.clock.stop(),
            Command::SeekTo(frame) => self.clock.seek(now, frame, total_frames),
            Command::MoveCurrentFrameBy(frame_count) => {
                let current_frame = self.clock.position(now, total_frames).value();
                let moved_frame = current_frame.saturating_add_signed(frame_count);
                self.clock.seek(now, TimelineFrameIndex::new(moved_frame), total_frames);
            }

            Command::SelectClip(clip) => {
                if self.project.placement_of(clip).is_some() {
                    self.selected_clip = Some(clip);
                }
            }
            Command::SplitAt { frame, only } => {
                // A cut that is not possible (it is where one already is) is simply not made.
                if !self.project.split_at(frame, only).is_empty() {
                    self.note_unsaved_change();
                }
            }
            Command::DeleteClip(clip) => self.delete_clip(clip, now),

            Command::SetClipSpeed { clip, speed_factor } => self.set_clip_speed(clip, speed_factor, now),
            Command::SetClipGain { clip, gain } => self.set_clip_gain(clip, gain),
            Command::SetClipMuted { clip, muted } => {
                let already = self
                    .project
                    .placement_of(clip)
                    .and_then(|placement| placement.clip.as_media())
                    .is_some_and(|media| media.is_muted() == muted);
                if !already && self.project.set_clip_muted(clip, muted).is_ok() {
                    self.note_unsaved_change();
                }
            }
            Command::RelinkSource(identifier) => self.relink_source(identifier),

            Command::AddTrack => {
                self.project.add_track_in_front();
                self.note_unsaved_change();
            }
            Command::SetTrackTitle { track, title } => {
                if self
                    .project
                    .track(track)
                    .is_some_and(|existing| existing.title() != title)
                    && self.project.set_track_title(track, title).is_ok()
                {
                    self.note_unsaved_change();
                }
            }
            Command::SetTrackSpeed { track, speed_factor } => self.set_track_speed(track, speed_factor, now),
            Command::SetTrackGain { track, gain } => {
                if self
                    .project
                    .track(track)
                    .is_some_and(|existing| existing.gain() != gain)
                    && self.project.set_track_gain(track, gain).is_ok()
                {
                    self.note_unsaved_change();
                }
            }
            Command::SetTrackMuted { track, muted } => {
                if self
                    .project
                    .track(track)
                    .is_some_and(|existing| existing.is_muted() != muted)
                    && self.project.set_track_muted(track, muted).is_ok()
                {
                    self.note_unsaved_change();
                }
            }

            Command::NewTextObject { track, frame } => self.new_text_object(track, frame, now),
            Command::EditTextObject(clip) => {
                if self
                    .project
                    .placement_of(clip)
                    .is_some_and(|placement| placement.clip.as_text().is_some())
                {
                    self.selected_clip = Some(clip);
                    self.text_window_open = Some(clip);
                }
            }
            Command::SetTextObject { clip, object } => {
                let already = self
                    .project
                    .placement_of(clip)
                    .and_then(|placement| placement.clip.as_text())
                    .is_some_and(|text| text.object == object);
                if !already && self.project.set_text_object(clip, object).is_ok() {
                    self.note_unsaved_change();
                }
            }
            Command::SetTextDuration { clip, frames } => {
                let before = self
                    .project
                    .placement_of(clip)
                    .and_then(|placement| placement.clip.as_text())
                    .map(|text| text.duration_in_frames());
                if let Ok(after) = self.project.set_text_duration(clip, frames)
                    && before != Some(after)
                {
                    self.note_unsaved_change();
                }
            }

            Command::SetSpeedIndicatorLayout(layout) => {
                if layout != self.project.speed_indicator_layout() {
                    self.project.set_speed_indicator_layout(layout);
                    self.note_unsaved_change();
                }
            }
        }
    }

    /// Takes a clip off its track, keeping the current frame on the picture it showed if it can.
    fn delete_clip(&mut self, clip: ClipLocation, now: Instant) {
        let current_frame = self.clock.position(now, self.project.total_frame_count());
        if let Ok(new_current_frame) = delete_clip_keeping_the_picture(&mut self.project, current_frame, clip) {
            self.note_unsaved_change();
            if self.selected_clip == Some(clip) {
                self.selected_clip = None;
            }
            let new_total_frames = self.project.total_frame_count();
            self.clock.seek(now, new_current_frame, new_total_frames);
        }
    }

    fn set_clip_speed(&mut self, clip: ClipLocation, speed_factor: SpeedFactor, now: Instant) {
        let already = self
            .project
            .placement_of(clip)
            .and_then(|placement| placement.clip.as_media())
            .is_some_and(|media| media.speed() == speed_factor);
        if already {
            return;
        }
        let current_frame = self.clock.position(now, self.project.total_frame_count());
        if let Ok(new_current_frame) =
            set_clip_speed_keeping_the_picture(&mut self.project, current_frame, clip, speed_factor)
        {
            self.note_unsaved_change();
            let new_total_frames = self.project.total_frame_count();
            self.clock.seek(now, new_current_frame, new_total_frames);
        }
    }

    fn set_track_speed(&mut self, track: TrackId, speed_factor: SpeedFactor, now: Instant) {
        if self
            .project
            .track(track)
            .is_none_or(|existing| existing.speed() == speed_factor)
        {
            return;
        }
        let current_frame = self.clock.position(now, self.project.total_frame_count());
        if let Ok(new_current_frame) =
            set_track_speed_keeping_the_picture(&mut self.project, current_frame, track, speed_factor)
        {
            self.note_unsaved_change();
            let new_total_frames = self.project.total_frame_count();
            self.clock.seek(now, new_current_frame, new_total_frames);
        }
    }

    fn set_clip_gain(&mut self, clip: ClipLocation, gain: Gain) {
        let already = self
            .project
            .placement_of(clip)
            .and_then(|placement| placement.clip.as_media())
            .is_some_and(|media| media.gain() == gain);
        if !already && self.project.set_clip_gain(clip, gain).is_ok() {
            self.note_unsaved_change();
        }
    }

    /// Puts a new text object on `track`, starting at `frame`, and opens the window to write it in.
    /// The playhead goes to it if it is not there, so that it shows in the preview.
    fn new_text_object(&mut self, track: TrackId, frame: TimelineFrameIndex, now: Instant) {
        let wanted_frames = self
            .project
            .frame_rate()
            .nearest_frame_to_seconds(SECONDS_A_NEW_TEXT_OBJECT_LASTS)
            .max(1);
        let Some((clip, duration_in_frames)) =
            self.project
                .insert_text(track, frame, TextObject::default(), wanted_frames)
        else {
            self.show_message(
                "There is no room for a text object there",
                "Choose a place on the track that has nothing on it.",
            );
            return;
        };
        self.note_unsaved_change();
        self.selected_clip = Some(clip);
        self.text_window_open = Some(clip);
        let total_frames = self.project.total_frame_count();
        let current_frame = self.clock.position(now, total_frames);
        let shown_from = frame.value();
        if !(shown_from..shown_from + duration_in_frames).contains(&current_frame.value()) {
            self.clock.seek(now, frame, total_frames);
        }
    }

    /// Does what was asked, unless there are unsaved changes, in which case the user is asked first.
    /// (With autosave on and a file to save to, the changes are saved instead, and nothing is asked.)
    pub(super) fn ask_before_discarding(&mut self, then: AfterConfirmation) {
        self.autosave_now();
        if self.has_unsaved_changes {
            self.pause_playback_for_dialog();
            self.dialog = Some(Dialog::ConfirmDiscardChanges { then });
        } else {
            match then {
                AfterConfirmation::NewProject => self.start_new_project(),
                AfterConfirmation::LoadProject => self.choose_and_load_project(),
                AfterConfirmation::CloseWindow => {}
            }
        }
    }
}
