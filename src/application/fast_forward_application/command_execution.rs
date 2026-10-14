//! What each [`Command`] does.
//!
//! The commands of one frame of the window are carried out together ([`FastForwardApplication::execute_all`], after the user has
//! been asked about any that would lose keyframes), and if they changed the project they are one edit of the history, which
//! undoing takes back.

use std::time::Instant;

use egui::Context;

use super::FastForwardApplication;
use crate::application::commands::{Command, gesture_key_of_the_edits_among};
use crate::application::dialogs::{AfterConfirmation, Dialog};
use crate::application::editing::{
    delete_clip_keeping_the_picture, restore_keeping_the_picture, set_clip_speed_keeping_the_picture,
    set_track_speed_keeping_the_picture,
};
use crate::application::history::{EditKey, Restored};
use crate::application::preview_panel::PreviewView;
use crate::application::selection::{Selection, SelectionChange, SelectionItem};
use crate::playback::SoundEffect;
use crate::project::{
    AnimationTarget, ClipId, ClipSide, EditError, FrameRate, SpeedFactor, TextObject, TimelineFrameIndex, TrackId,
    TrackSide,
};

/// How long a text object lasts when it is made, in seconds (or less, if it would run into a clip).
const SECONDS_A_NEW_TEXT_OBJECT_LASTS: f64 = 3.0;

impl FastForwardApplication {
    /// Carries out the commands of one frame of the window, one after another, except the one that would lose keyframes if there is
    /// one, which the user is asked about instead (see [`Self::ask_before_losing_keyframes`]).
    pub(super) fn execute_all(&mut self, commands: Vec<Command>, now: Instant, context: &Context) {
        let commands = self.ask_before_losing_keyframes(commands, context);
        self.carry_out_all(commands, now, context);
    }

    /// Carries out `commands` one after another, without asking about anything: for those that the user has been asked about, and
    /// agreed to. If that changed the project, the change is one edit of the history (joined to the edit before it if both are
    /// the same gesture, see [`crate::application::history`]).
    pub(super) fn carry_out_all(&mut self, commands: Vec<Command>, now: Instant, context: &Context) {
        let may_edit = commands.iter().any(Command::may_change_the_project);
        let (pointer_is_down, press_began_at) =
            context.input(|input| (input.pointer.any_down(), input.pointer.press_start_time()));
        let gesture_key = gesture_key_of_the_edits_among(&commands);
        // An edit that makes or changes a keyframe is about the frame it is at as well: what is done at another frame is
        // another edit, not more of this gesture, however soon after it.
        let frame_of_the_keyframes = commands
            .iter()
            .any(|command| self.command_changes_a_keyframe(command))
            .then(|| self.frame_of_the_edits_of_what_has_keyframes(gesture_key, press_began_at, now));
        let gesture_key = match (gesture_key, frame_of_the_keyframes) {
            (Some(key), Some(frame)) => Some(EditKey::about(&(key, frame))),
            (key, _) => key,
        };
        self.frame_keyframes_are_made_at = frame_of_the_keyframes;
        // A copy of the project is only made in a frame in which something asks for a change.
        let before = may_edit.then(|| {
            (
                self.project.clone(),
                self.selection.to_items(),
                self.history.generation(),
            )
        });
        for command in commands {
            self.execute(command, now, context);
        }
        self.frame_keyframes_are_made_at = None;
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
            Command::SaveProjectAs => self.save_project_as(),
            Command::NewProject => self.ask_before_discarding(AfterConfirmation::NewProject),
            Command::EmbedAndImportFile => self.embed_and_import_file(),
            Command::LinkAndImportFile => self.link_and_import_file(),
            Command::ExportVideo => self.export_video(),

            Command::Undo => self.undo(now),
            Command::Redo => self.redo(now),
            Command::Deselect => self.selection.clear(),
            Command::ShowProjectSettings => self.settings_windows_open.project = true,
            Command::ShowGlobalSettings => self.settings_windows_open.global = true,
            Command::SetProjectSettings(settings) => self.set_project_settings(settings),
            Command::SetProjectFrameRate(frame_rate) => self.set_project_frame_rate(frame_rate, now),
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

            Command::FitThePreview => self.preview_view = PreviewView::FITTING,
            Command::FrameTheSelectionInTheTimeline => {
                if let Some((first_frame, end_frame)) = self.selection.frames_taken_up(&self.project) {
                    let normal = self.timeline_geometry();
                    self.timeline_view
                        .frame_these_frames(normal, total_frames, first_frame, end_frame);
                }
            }

            Command::SelectItems { items, change } => {
                let asked_for_something = !items.is_empty();
                let items_that_exist = items
                    .into_iter()
                    .filter(|item| item.is_in(&self.project))
                    .collect::<Vec<_>>();
                // (Asking for things that are not there is asking for nothing to change: it does not let go of what is
                // selected. Asking for nothing at all does, as a rectangle that has come to go round nothing does.)
                if !asked_for_something || !items_that_exist.is_empty() {
                    self.selection.change_items(items_that_exist, change);
                }
            }
            Command::SplitAt { frame, only } => self.split_at(frame, only, now),
            Command::DeleteClips(clips) => self.delete_clips(clips, now),
            Command::DuplicateClips { clips, side } => self.duplicate_clips(clips, side),
            Command::MoveClips(starts) => {
                // A move that cannot be made (clips would be on top of one another) is not made, and the user is told if the rules of groups
                // are why. (A drag stops against what is in its way, so this is not what a drag comes to: it is for a move that is asked
                // for by other means.)
                if let Err(error) = self.project.move_clips(&starts) {
                    self.say_why_a_group_edit_was_refused("The clips cannot be moved there", &error);
                }
            }

            Command::SetClipSpeed { clip, speed_factor } => self.set_clip_speed(clip, speed_factor, now),
            Command::SetClipGain { clip, gain } => self.set_clip_gain_at_the_playhead(clip, gain, now),
            Command::SetClipMuted { clip, muted } => {
                self.set_muted_at_the_playhead(AnimationTarget::Clip(clip), muted, now);
            }
            Command::SetClipPlacement { clip, placement } => {
                self.set_clip_placement_at_the_playhead(clip, placement, now);
            }
            Command::RelinkSource(identifier) => self.relink_source(identifier),
            Command::IncludeFilesInTheProjectFile(files) => self.include_files_in_the_project_file(&files),
            Command::TakeFilesOutOfTheProjectFile(files) => self.take_files_out_of_the_project_file(&files),

            Command::AddTrack => {
                self.project.add_track_in_front();
            }
            Command::DeleteTrack(track) => self.delete_track(track, now),
            Command::DuplicateTrack { track, side } => self.duplicate_track(track, side),
            Command::MoveTrack { track, to_position } => {
                if let Err(error) = self.project.move_track(track, to_position) {
                    self.say_why_a_group_edit_was_refused("The track cannot go there", &error);
                }
            }
            Command::SetTrackTitle { track, title } => {
                let _ = self.project.set_track_title(track, title);
            }
            Command::SetTrackSpeed { track, speed_factor } => self.set_track_speed(track, speed_factor, now),
            Command::SetTrackGain { track, gain } => self.set_track_gain_at_the_playhead(track, gain, now),
            Command::SetTrackMuted { track, muted } => {
                self.set_muted_at_the_playhead(AnimationTarget::Track(track), muted, now);
            }
            Command::SetTrackHeight { track, height } => {
                let _ = self.project.set_track_height(track, height);
            }

            Command::NewTextObject { track, frame } => self.new_text_object(track, frame, now),
            Command::SetTextObject { clip, object } => self.set_text_object_at_the_playhead(clip, object, now),
            Command::SetClipDuration { clip, frames } => {
                if let Err(error) = self.project.set_clip_duration(clip, frames) {
                    self.say_why_a_group_edit_was_refused("The length cannot be changed", &error);
                }
            }

            Command::AddEffect { target, kind } => {
                let _ = self.project.add_effect(target, kind);
            }
            Command::RemoveEffect { target, kind } => {
                let _ = self.project.remove_effect(target, kind);
                self.selection.forget_what_is_gone(&self.project);
            }
            Command::SetEffectValue {
                target,
                property,
                value,
            } => self.change_a_property_at_the_playhead(target, property, value, now),

            Command::CreateGroup { kind, members } => self.create_a_group(kind, &members),
            Command::AddToGroup { group, members } => self.add_to_a_group(group, &members),
            Command::RemoveFromGroup { group, member } => self.remove_from_a_group(group, member),
            Command::DisbandGroup(group) => self.disband_a_group(group),
            Command::DeleteGroupAndEverythingInIt(group) => self.delete_a_group_and_everything_in_it(group, now),
            Command::RenameGroup { group, name } => {
                let _ = self.project.rename_group(group, name);
            }
            Command::SetGroupColour { group, colour } => {
                let _ = self.project.set_group_colour(group, colour);
            }
            Command::AddGroupEffect { group, kind } => {
                let _ = self.project.add_effect_to_the_group(group, kind);
            }
            Command::RemoveGroupEffect { group, kind } => {
                let _ = self.project.remove_effect_from_the_group(group, kind);
            }
            Command::SetGroupEffectValue { group, property, value } => {
                let _ = self.project.set_effect_value_of_the_group(group, property, &value);
            }
            Command::MoveChainItem { owner, item, before } => {
                let _ = self.project.move_chain_item(owner, item, before);
            }

            Command::AddKeyframe {
                target,
                property,
                frame,
            } => {
                // (The drawer the keyframe is in opens, so that it is seen where it is.)
                if let Ok(Some(made)) = self.project.add_keyframe(target, property, frame) {
                    self.timeline_view.open_the_drawer_of(made.owner);
                }
            }
            Command::RemoveKeyframe {
                target,
                property,
                frame,
            } => {
                let _ = self.project.remove_keyframe(target, property, frame);
                self.selection.forget_what_is_gone(&self.project);
            }
            Command::MoveKeyframes(moves) => {
                // Keyframes that cannot all be moved where they are asked to go are not moved.
                let _ = self.project.move_keyframes(&moves);
                self.selection.forget_what_is_gone(&self.project);
            }
            Command::DeleteKeyframes(keyframes) => {
                let _ = self.project.remove_keyframes(&keyframes);
                self.selection.forget_what_is_gone(&self.project);
            }
            Command::DuplicateKeyframes {
                keyframes,
                towards,
                gap_in_frames,
            } => {
                // Keyframes that cannot all be copied are not copied. The copies are what is selected afterwards, so that copying
                // again goes on from them.
                if let Ok(copies) = self.project.duplicate_keyframes(&keyframes, towards, gap_in_frames)
                    && !copies.is_empty()
                {
                    self.selection.change_items(
                        copies.into_iter().map(SelectionItem::Keyframe),
                        SelectionChange::Replace,
                    );
                }
            }
            Command::DeleteRacks(racks) => {
                let _ = self.project.remove_racks(&racks);
                self.selection.forget_what_is_gone(&self.project);
            }
            Command::MergeRacks(racks) => {
                // (The rack that is kept stays selected if it was: the others are gone.)
                let _ = self.project.merge_racks(&racks);
                self.selection.forget_what_is_gone(&self.project);
            }
            Command::MoveRack {
                rack,
                to,
                shift_in_frames,
                slot,
            } => {
                // A rack that cannot go where it is asked to is not moved (the user is told if the rules of groups are why: a new crate
                // cannot be made inside the rectangle of a clip group).
                if let Err(error) = self.project.move_rack(rack, to, shift_in_frames, slot) {
                    self.say_why_a_group_edit_was_refused("The rack cannot go there", &error);
                }
                self.selection.forget_what_is_gone(&self.project);
            }

            Command::SetSpeedIndicatorLayout(layout) => self.project.set_speed_indicator_layout(layout),
        }
    }

    /// Cuts the clips that are at `frame` (see [`crate::project::Project::split_at`]) and makes the cut felt: it
    /// snips, once however many clips were cut, and a flash of light shows at the cut in each track it was made
    /// in. A cut that is not possible (it is where one already is) is simply not made, and does neither.
    fn split_at(&mut self, frame: TimelineFrameIndex, only: Option<Vec<ClipId>>, now: Instant) {
        let cuts = self.project.split_at(frame, only.as_deref());
        if cuts.is_empty() {
            return;
        }
        self.sound_effects.play(SoundEffect::Snip, now);
        for (track, _) in cuts {
            self.timeline_view.flash_a_cut(track, frame);
        }
    }

    /// Takes the latest edit back.
    fn undo(&mut self, now: Instant) {
        if let Some(restored) = self.history.undo(&self.project, self.selection.to_items()) {
            self.put_back(restored, now);
        }
    }

    /// Does again the edit that was last taken back.
    fn redo(&mut self, now: Instant) {
        if let Some(restored) = self.history.redo(&self.project, self.selection.to_items()) {
            self.put_back(restored, now);
        }
    }

    /// Makes the project, and what is selected, what undoing or redoing gave back.
    fn put_back(&mut self, restored: Restored, now: Instant) {
        self.gesture_frame.forget();
        let frame_rate_before = self.project.frame_rate();
        let current_frame = self.clock.position(now, self.project.total_frame_count());
        // The picture that is on screen stays on screen, if it is still there, however the clips moved.
        let new_current_frame = restore_keeping_the_picture(&mut self.project, current_frame, restored.project);
        self.selection = Selection::from_items(restored.selection);
        self.selection.forget_what_is_gone(&self.project);
        if self.project.frame_rate() != frame_rate_before {
            // The first video came or went, and the frame rate with it.
            self.clock.reset_for_frame_rate(self.project.frame_rate());
            self.timeline_view.reset();
        }
        self.clock
            .seek(now, new_current_frame, self.project.total_frame_count());
        self.note_the_project_was_edited(now);
    }

    /// Makes `frame_rate` the rate of the project, which counts everything on the timeline again at it
    /// (see [`crate::project::Project::set_frame_rate`]). The current frame goes to the same moment.
    fn set_project_frame_rate(&mut self, frame_rate: FrameRate, now: Instant) {
        let rate_before = self.project.frame_rate();
        let current_frame = self.clock.position(now, self.project.total_frame_count());
        // (Counting everything again rounds each start and length to a whole frame, which can put a clip a frame inside the rectangle of a
        // clip group that it was a frame beside: that is refused, and the rate stays what it was.)
        if let Err(error) = self.project.try_set_frame_rate(frame_rate) {
            self.say_why_a_group_edit_was_refused("The frame rate cannot be changed", &error);
            return;
        }
        if frame_rate != rate_before {
            self.clock.reset_for_frame_rate(frame_rate);
            self.timeline_view.reset();
            let same_moment = rate_before.convert_frame_count_to(current_frame.value(), frame_rate);
            self.clock.seek(
                now,
                TimelineFrameIndex::new(same_moment),
                self.project.total_frame_count(),
            );
        }
    }

    /// Takes clips off their tracks, keeping the current frame on the picture it showed if it can.
    fn delete_clips(&mut self, clips: Vec<ClipId>, now: Instant) {
        let mut current_frame = self.clock.position(now, self.project.total_frame_count());
        let mut deleted_any = false;
        for clip in clips {
            if let Ok(new_current_frame) = delete_clip_keeping_the_picture(&mut self.project, current_frame, clip) {
                current_frame = new_current_frame;
                deleted_any = true;
            }
        }
        if deleted_any {
            self.selection.forget_what_is_gone(&self.project);
            let new_total_frames = self.project.total_frame_count();
            self.clock.seek(now, current_frame, new_total_frames);
        }
    }

    /// Puts a copy of each of `clips` beside it, on `side` of it (see [`crate::project::Project::duplicate_clip`]), and selects
    /// the copies, which are what is wanted next: to be seen, and to be moved or changed. A clip that has no room beside it is
    /// not copied (and a clip that is not there is not either); when none of them can be because there is no room, the user is
    /// told, and nothing changes.
    fn duplicate_clips(&mut self, clips: Vec<ClipId>, side: ClipSide) {
        let mut some_had_no_room = false;
        let mut refused_by_groups = None;
        let mut copies = Vec::new();
        for clip in clips {
            match self.project.duplicate_clip(clip, side) {
                Ok(copy) => copies.push(copy),
                Err(EditError::NoRoomForACopy { .. }) => some_had_no_room = true,
                // (A copy is in the groups that the clip is in: if that would put it inside the rectangle of a clip group that it is not
                // in, or would break another rule of groups, it is not made, and the user is told which, once.)
                Err(error @ EditError::BreaksTheRulesOfGroups(_)) => {
                    refused_by_groups.get_or_insert(error);
                }
                Err(_) => {}
            }
        }
        if let Some(error) = refused_by_groups {
            self.say_why_a_group_edit_was_refused("The copy cannot be made there", &error);
        } else if copies.is_empty() && some_had_no_room {
            self.show_message(
                "There is no room for a copy there",
                "A copy is put right beside the clip, where nothing else is.",
            );
        }
        if !copies.is_empty() {
            self.selection = Selection::from_clips(copies);
        }
    }

    /// Puts a copy of `track` in front of it or behind it, as `side` says (see [`crate::project::Project::duplicate_track`]),
    /// and selects the copy. The user is told why if the rules of groups do not let the copy be where it would go.
    fn duplicate_track(&mut self, track: TrackId, side: TrackSide) {
        match self.project.duplicate_track(track, side) {
            Ok(copy) => self.selection = Selection::from_items([SelectionItem::Track(copy)]),
            Err(error) => {
                self.say_why_a_group_edit_was_refused("The track cannot be copied there", &error);
            }
        }
    }

    /// Takes a track off the project. What was selected on it is not selected any more, and the
    /// playhead stays where it was, unless that is past the end of what is left.
    fn delete_track(&mut self, track: TrackId, now: Instant) {
        let current_frame = self.clock.position(now, self.project.total_frame_count());
        if self.project.delete_track(track).is_ok() {
            self.selection.forget_what_is_gone(&self.project);
            let new_total_frames = self.project.total_frame_count();
            self.clock.seek(now, current_frame, new_total_frames);
        }
    }

    fn set_clip_speed(&mut self, clip: ClipId, speed_factor: SpeedFactor, now: Instant) {
        let already = self
            .project
            .placement_of(clip)
            .and_then(|placement| placement.clip.as_media())
            .is_some_and(|media| media.speed() == speed_factor);
        if already {
            return;
        }
        let current_frame = self.clock.position(now, self.project.total_frame_count());
        match set_clip_speed_keeping_the_picture(&mut self.project, current_frame, clip, speed_factor) {
            Ok(new_current_frame) => {
                let new_total_frames = self.project.total_frame_count();
                self.clock.seek(now, new_current_frame, new_total_frames);
            }
            Err(error) => {
                self.say_why_a_group_edit_was_refused("The speed cannot be changed", &error);
            }
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
        match set_track_speed_keeping_the_picture(&mut self.project, current_frame, track, speed_factor) {
            Ok(new_current_frame) => {
                let new_total_frames = self.project.total_frame_count();
                self.clock.seek(now, new_current_frame, new_total_frames);
            }
            Err(error) => {
                self.say_why_a_group_edit_was_refused("The speed cannot be changed", &error);
            }
        }
    }

    /// Puts a new text object on `track`, starting at `frame`, and selects it, so that its properties (the words
    /// of it, to begin with) are in the properties panel. The playhead goes to it if it is not there, so
    /// that it shows in the preview.
    fn new_text_object(&mut self, track: TrackId, frame: TimelineFrameIndex, now: Instant) {
        let wanted_frames = self
            .project
            .frame_rate()
            .nearest_frame_to_seconds(SECONDS_A_NEW_TEXT_OBJECT_LASTS)
            .max(1);
        let (clip, duration_in_frames) =
            match self
                .project
                .try_insert_text(track, frame, TextObject::default(), wanted_frames)
            {
                Ok(inserted) => inserted,
                Err(error) => {
                    // (A place that has nothing on it can still be inside the rectangle of a clip group, where only its clips may be.)
                    if !self.say_why_a_group_edit_was_refused("A text object cannot be put there", &error) {
                        self.show_message(
                            "There is no room for a text object there",
                            "Choose a place on the track that has nothing on it.",
                        );
                    }
                    return;
                }
            };
        self.selection = Selection::only(clip);
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
