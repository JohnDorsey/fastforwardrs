//! What each [`Command`] does.

use std::time::Instant;

use egui::Context;

use super::FastForwardApplication;
use crate::application::commands::Command;
use crate::application::dialogs::{AfterConfirmation, Dialog};
use crate::application::editing::set_clip_speed_keeping_the_picture;
use crate::project::TimelineFrameIndex;

impl FastForwardApplication {
    /// Carries out one command. Commands from the file menu may open a window and wait for the
    /// user; the others change the project or the playback.
    pub(super) fn execute(&mut self, command: Command, now: Instant, _context: &Context) {
        let total_frames = self.project.video_track().total_frame_count();
        match command {
            Command::LoadProject => self.ask_before_discarding(AfterConfirmation::LoadProject),
            Command::SaveProject => self.save_project(),
            Command::NewProject => self.ask_before_discarding(AfterConfirmation::NewProject),
            Command::ImportVideo => self.import_video(),
            Command::ExportVideo => self.export_video(),

            Command::Pause => self.clock.pause(now, total_frames),
            Command::Play => self.clock.play(now, total_frames),
            Command::Stop => self.clock.stop(),
            Command::SeekTo(frame) => self.clock.seek(now, frame, total_frames),
            Command::MoveCurrentFrameBy(frame_count) => {
                let current_frame = self.clock.position(now, total_frames).value();
                let moved_frame = current_frame.saturating_add_signed(frame_count);
                self.clock.seek(now, TimelineFrameIndex::new(moved_frame), total_frames);
            }

            Command::SplitAt(frame) => {
                // A cut that is not possible (it is where one already is) is simply not made.
                if self.project.split_at(frame).is_ok() {
                    self.has_unsaved_changes = true;
                }
            }
            Command::SetClipSpeed {
                clip_index,
                speed_factor,
            } => {
                let current_frame = self.clock.position(now, total_frames);
                if let Ok(new_current_frame) =
                    set_clip_speed_keeping_the_picture(&mut self.project, current_frame, clip_index, speed_factor)
                {
                    self.has_unsaved_changes = true;
                    let new_total_frames = self.project.video_track().total_frame_count();
                    self.clock.seek(now, new_current_frame, new_total_frames);
                }
            }
            Command::SetSpeedIndicatorLayout(layout) => {
                if layout != self.project.speed_indicator_layout() {
                    self.project.set_speed_indicator_layout(layout);
                    self.has_unsaved_changes = true;
                }
            }
        }
    }

    /// Does what was asked, unless there are unsaved changes, in which case the user is asked first.
    pub(super) fn ask_before_discarding(&mut self, then: AfterConfirmation) {
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
