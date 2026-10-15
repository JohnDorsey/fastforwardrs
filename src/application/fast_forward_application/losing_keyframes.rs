// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Asking before keyframes are lost.
//!
//! Making something last less time (playing a clip or a track faster, asking a clip to last less, counting the project again at a
//! lower frame rate, putting a clip on a track that plays faster) puts the keyframes of its drawer at other frames. Each is put at a
//! frame of its own as near to where it should be as can be, which makes room by giving up the empty frames between the keyframes
//! (see [`crate::project::Rack::rescaled`]), so nothing is lost while there is a frame for each keyframe. When there is not, some are
//! lost, and that is for the user to say: it is asked about before the change is made, and the change is made only if they agree.

use egui::Context;

use super::FastForwardApplication;
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;

impl FastForwardApplication {
    /// The commands among `commands` that are to be carried out now. The first of them that would lose keyframes is not among them:
    /// a question about it is asked instead (see [`Dialog::ConfirmLosingKeyframes`]), and it is carried out if the user agrees. (The
    /// others that would lose some are not carried out either: one question at a time, which the user can ask the others by doing
    /// them again.)
    ///
    /// While the question is open nothing is edited behind it: a gesture that is still going on, and sends commands for as long as
    /// the mouse button is held, does not go on changing the project. And while another window is waiting for an answer, nothing
    /// is asked in its place, and nothing that would lose keyframes is done either.
    pub(super) fn ask_before_losing_keyframes(&mut self, commands: Vec<Command>, context: &Context) -> Vec<Command> {
        match &self.interface.dialog {
            Some(Dialog::ConfirmLosingKeyframes { .. }) => {
                return commands
                    .into_iter()
                    .filter(|command| !command.may_change_the_project())
                    .collect();
            }
            Some(_) => {
                return commands
                    .into_iter()
                    .filter(|command| !command.may_lose_keyframes())
                    .collect();
            }
            None => {}
        }
        let mut to_carry_out = Vec::with_capacity(commands.len());
        let mut asked = false;
        for command in commands {
            if !command.may_lose_keyframes() {
                to_carry_out.push(command);
                continue;
            }
            let how_many = self.keyframes_it_would_lose(&command);
            if how_many == 0 {
                to_carry_out.push(command);
            } else if !asked {
                asked = true;
                self.pause_playback_for_dialog();
                self.interface.dialog = Some(Dialog::ConfirmLosingKeyframes {
                    change: Box::new(command),
                    how_many,
                });
                context.request_repaint();
            }
        }
        to_carry_out
    }

    /// How many keyframes carrying `command` out would lose, worked out by carrying it out on a copy of the project, so that what is
    /// said is what would be done. Nothing for a command that does not change how long anything lasts.
    fn keyframes_it_would_lose(&self, command: &Command) -> usize {
        let mut copy = self.document.project.clone();
        // (A change that cannot be made loses nothing.)
        match command {
            Command::SetClipSpeed { clip, speed_factor } => {
                let _ = copy.set_clip_speed(*clip, *speed_factor);
            }
            Command::SetTrackSpeed { track, speed_factor } => {
                let _ = copy.set_track_speed(*track, *speed_factor);
            }
            Command::SetClipDuration { clip, frames } => {
                let _ = copy.set_clip_duration(*clip, *frames);
            }
            Command::SetProjectFrameRate(frame_rate) => copy.set_frame_rate(*frame_rate),
            Command::MoveClips { moves, before_the_drag } => {
                let _ = copy.move_clips_remembering(moves, before_the_drag);
            }
            other => {
                debug_assert!(
                    !other.may_lose_keyframes(),
                    "{other:?} may lose keyframes, and what it would lose is not worked out"
                );
                return 0;
            }
        }
        self.document
            .project
            .keyframe_count()
            .saturating_sub(copy.keyframe_count())
    }
}
