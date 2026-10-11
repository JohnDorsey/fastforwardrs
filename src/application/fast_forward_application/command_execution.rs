//! What each [`Command`] does.
//!
//! The commands of one frame of the window are carried out together ([`FastForwardApplication::execute_all`]),
//! and if they changed the project they are one edit of the history, which undoing takes back.

use std::time::Instant;

use egui::Context;

use super::FastForwardApplication;
use crate::application::commands::{Command, gesture_key_of_the_edits_among};
use crate::application::dialogs::{AfterConfirmation, Dialog};
use crate::application::editing::{
    delete_clip_keeping_the_picture, restore_keeping_the_picture, set_clip_speed_keeping_the_picture,
    set_track_speed_keeping_the_picture,
};
use crate::application::history::Restored;
use crate::application::selection::Selection;
use crate::project::{ClipLocation, Gain, SpeedFactor, TextObject, TimelineFrameIndex, TrackId};

/// How long a text object lasts when it is made, in seconds (or less, if it would run into a clip).
const SECONDS_A_NEW_TEXT_OBJECT_LASTS: f64 = 3.0;

impl FastForwardApplication {
    /// Carries out the commands of one frame of the window, one after another. If that changed the
    /// project, the change is one edit of the history (joined to the edit before it if both are the
    /// same gesture, see [`crate::application::history`]).
    pub(super) fn execute_all(&mut self, commands: Vec<Command>, now: Instant, context: &Context) {
        let may_edit = commands.iter().any(Command::may_change_the_project);
        let gesture_key = gesture_key_of_the_edits_among(&commands);
        let pointer_is_down = context.input(|input| input.pointer.any_down());
        // A copy of the project is only made in a frame in which something asks for a change.
        let before = may_edit.then(|| (self.project.clone(), self.selection.to_vec(), self.history.generation()));
        for command in commands {
            self.execute(command, now, context);
        }
        if let Some((project_before, selection_before, generation)) = before
            // (A project that was loaded or begun in the meantime is not an edit of the one before it.)
            && self.history.generation() == generation
            && self.project != project_before
        {
            self.history
                .record_change(project_before, selection_before, gesture_key, now, pointer_is_down);
            self.note_the_project_was_edited(now);
        }
    }

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

            Command::Undo => self.undo(now),
            Command::Redo => self.redo(now),
            Command::Deselect => self.selection.clear(),
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

            Command::Select { clips, change } => {
                let clips_that_exist = clips
                    .into_iter()
                    .filter(|clip| self.project.placement_of(*clip).is_some())
                    .collect::<Vec<_>>();
                self.selection.change(clips_that_exist, change);
            }
            Command::SplitAt { frame, only } => {
                // A cut that is not possible (it is where one already is) is simply not made.
                self.project.split_at(frame, only.as_deref());
            }
            Command::DeleteClips(clips) => self.delete_clips(clips, now),
            Command::MoveClips(starts) => {
                // A move that cannot be made (clips would be on top of one another) is not made.
                let _ = self.project.move_clips(&starts);
            }

            Command::SetClipSpeed { clip, speed_factor } => self.set_clip_speed(clip, speed_factor, now),
            Command::SetClipGain { clip, gain } => self.set_clip_gain(clip, gain),
            Command::SetClipMuted { clip, muted } => {
                let _ = self.project.set_clip_muted(clip, muted);
            }
            Command::SetClipPlacement { clip, placement } => {
                let _ = self.project.set_clip_placement(clip, placement);
            }
            Command::RelinkSource(identifier) => self.relink_source(identifier),

            Command::AddTrack => {
                self.project.add_track_in_front();
            }
            Command::DeleteTrack(track) => self.delete_track(track, now),
            Command::SetTrackTitle { track, title } => {
                let _ = self.project.set_track_title(track, title);
            }
            Command::SetTrackSpeed { track, speed_factor } => self.set_track_speed(track, speed_factor, now),
            Command::SetTrackGain { track, gain } => {
                let _ = self.project.set_track_gain(track, gain);
            }
            Command::SetTrackMuted { track, muted } => {
                let _ = self.project.set_track_muted(track, muted);
            }
            Command::SetTrackHeight { track, height } => {
                let _ = self.project.set_track_height(track, height);
            }

            Command::NewTextObject { track, frame } => self.new_text_object(track, frame, now),
            Command::EditTextObject(clip) => {
                if self
                    .project
                    .placement_of(clip)
                    .is_some_and(|placement| placement.clip.as_text().is_some())
                {
                    self.selection = Selection::only(clip);
                    self.text_window_open = Some(clip);
                }
            }
            Command::SetTextObject { clip, object } => {
                let _ = self.project.set_text_object(clip, object);
            }
            Command::SetClipDuration { clip, frames } => {
                let _ = self.project.set_clip_duration(clip, frames);
            }

            Command::SetSpeedIndicatorLayout(layout) => self.project.set_speed_indicator_layout(layout),
        }
    }

    /// Takes the latest edit back.
    fn undo(&mut self, now: Instant) {
        if let Some(restored) = self.history.undo(&self.project, self.selection.to_vec()) {
            self.put_back(restored, now);
        }
    }

    /// Does again the edit that was last taken back.
    fn redo(&mut self, now: Instant) {
        if let Some(restored) = self.history.redo(&self.project, self.selection.to_vec()) {
            self.put_back(restored, now);
        }
    }

    /// Makes the project, and what is selected, what undoing or redoing gave back.
    fn put_back(&mut self, restored: Restored, now: Instant) {
        let frame_rate_before = self.project.frame_rate();
        let current_frame = self.clock.position(now, self.project.total_frame_count());
        // The picture that is on screen stays on screen, if it is still there, however the clips moved.
        let new_current_frame = restore_keeping_the_picture(&mut self.project, current_frame, restored.project);
        self.selection = Selection::from_clips(restored.selection);
        self.selection.forget_clips_that_are_gone(&self.project);
        if self.project.frame_rate() != frame_rate_before {
            // The first video came or went, and the frame rate with it.
            self.clock.reset_for_frame_rate(self.project.frame_rate());
            self.timeline_view.reset();
        }
        self.clock
            .seek(now, new_current_frame, self.project.total_frame_count());
        self.note_the_project_was_edited(now);
    }

    /// Takes clips off their tracks, keeping the current frame on the picture it showed if it can.
    fn delete_clips(&mut self, clips: Vec<ClipLocation>, now: Instant) {
        let mut current_frame = self.clock.position(now, self.project.total_frame_count());
        let mut deleted_any = false;
        for clip in clips {
            if let Ok(new_current_frame) = delete_clip_keeping_the_picture(&mut self.project, current_frame, clip) {
                current_frame = new_current_frame;
                deleted_any = true;
            }
        }
        if deleted_any {
            self.selection.forget_clips_that_are_gone(&self.project);
            let new_total_frames = self.project.total_frame_count();
            self.clock.seek(now, current_frame, new_total_frames);
        }
    }

    /// Takes a track off the project. What was selected on it is not selected any more, and the
    /// playhead stays where it was, unless that is past the end of what is left.
    fn delete_track(&mut self, track: TrackId, now: Instant) {
        let current_frame = self.clock.position(now, self.project.total_frame_count());
        if self.project.delete_track(track).is_ok() {
            self.selection.forget_clips_that_are_gone(&self.project);
            let new_total_frames = self.project.total_frame_count();
            self.clock.seek(now, current_frame, new_total_frames);
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
            let new_total_frames = self.project.total_frame_count();
            self.clock.seek(now, new_current_frame, new_total_frames);
        }
    }

    fn set_clip_gain(&mut self, clip: ClipLocation, gain: Gain) {
        let _ = self.project.set_clip_gain(clip, gain);
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
        self.selection = Selection::only(clip);
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
        if self.has_unsaved_changes() {
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
