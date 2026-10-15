// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The properties of a track: how fast everything on it plays, how loud it is, and whether it is
//! muted. (They are on top of those of each clip on it.) What is shown of how loud it is and whether it is
//! muted, when it has keyframes for them, is what they are at the playhead.

use egui::Ui;

use super::animatable::{self, Animatable, flag_at_the_playhead, number_at_the_playhead};
use super::controls::{gain_slider, labelled_row, speed_slider};
use super::{PropertiesContent, chain, kind_box};
use crate::application::commands::Command;
use crate::project::{AnimationTarget, Gain, Member, PropertyId, TrackId};

/// What the box of a track is headed with.
pub(super) const KIND_NAME: &str = "Track";

/// Shows the properties of the track `track_id`, and adds a command for each change made in them.
pub(super) fn show(ui: &mut Ui, content: &PropertiesContent<'_>, track_id: TrackId, commands: &mut Vec<Command>) {
    let Some(track) = content.project.track(track_id) else {
        return;
    };
    kind_box::show(ui, KIND_NAME, |ui| {
        ui.weak(track.title());
        ui.add_space(4.0);

        if let Some(speed_factor) = labelled_row(ui, |ui| speed_slider(ui, track.speed())) {
            commands.push(Command::SetTrackSpeed {
                track: track_id,
                speed_factor,
            });
        }

        let target = AnimationTarget::Track(track_id);
        let gain_now = Gain::from_decibels(
            number_at_the_playhead(content, target, PropertyId::Gain).unwrap_or(track.gain().decibels()),
        );
        let new_gain = animatable::row(
            ui,
            content,
            Animatable::of_a_track(track_id, PropertyId::Gain),
            commands,
            |ui, _| labelled_row(ui, |ui| gain_slider(ui, gain_now)),
        );
        if let Some(gain) = new_gain {
            commands.push(Command::SetTrackGain { track: track_id, gain });
        }

        let mut muted = flag_at_the_playhead(content, target, PropertyId::Muted).unwrap_or(track.is_muted());
        let muted_changed = animatable::row(
            ui,
            content,
            Animatable::of_a_track(track_id, PropertyId::Muted),
            commands,
            |ui, _| ui.checkbox(&mut muted, "Mute").changed(),
        );
        if muted_changed {
            commands.push(Command::SetTrackMuted { track: track_id, muted });
        }
    });
    chain::show(ui, content, Member::Track(track_id), commands);
}
