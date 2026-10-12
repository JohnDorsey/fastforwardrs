//! The properties of a clip: of a video or a sound, how fast it plays and how loud; of a picture, how long
//! it is shown; and of a text object (see `text_properties`). Of a video, a picture and a text object, where it is on the
//! picture (see `placement_controls`).
//!
//! What is shown of a property that has keyframes is what it is at the playhead.

use egui::Ui;

use super::animatable::{self, Animatable, flag_at_the_playhead, number_at_the_playhead};
use super::controls::{gain_slider, labelled_row, speed_slider};
use super::{PropertiesContent, kind_box, placement_controls, text_properties};
use crate::application::commands::Command;
use crate::application::duration_control::shown_for;
use crate::application::naming::kind_name;
use crate::application::timeline_panel::SourceLook;
use crate::project::{
    AnimationTarget, Clip, ClipId, ClipKind, ClipPlacement, Gain, ImageClip, MediaClip, PropertyId, SpeedFactor,
};

/// What the row of how big a video or a picture is is called.
const SIZE_LABEL: &str = "Size";

/// Shows the properties of `clip`, and adds a command for each change made in them.
pub(super) fn show(ui: &mut Ui, content: &PropertiesContent<'_>, clip: ClipId, commands: &mut Vec<Command>) {
    let Some(placement) = content.project.placement_of(clip) else {
        return;
    };
    match placement.clip {
        Clip::Video(media) | Clip::Audio(media) => {
            show_a_video_or_a_sound(ui, content, clip, &placement, media, commands)
        }
        Clip::Image(image) => show_a_picture(ui, content, clip, image, commands),
        Clip::Text(text) => text_properties::show(ui, content, clip, text, commands),
        // (A crate has nothing of its own to change: what it holds is in its racks, which are shown in the timeline.)
        Clip::Crate(_) => {}
    }
}

/// The name of the file a clip is made from, quietly.
fn show_the_file_name(ui: &mut Ui, look: Option<&SourceLook<'_>>) {
    if let Some(look) = look {
        ui.weak(&look.file_name);
        ui.add_space(4.0);
    }
}

/// If the file the clip is made from cannot be found, says so and offers to choose it again.
fn offer_to_choose_the_file_again(
    ui: &mut Ui,
    look: Option<&SourceLook<'_>>,
    source: crate::project::SourceMediaIdentifier,
    commands: &mut Vec<Command>,
) {
    if look.is_some_and(|look| !look.is_available) {
        ui.add_space(4.0);
        ui.colored_label(
            ui.visuals().warn_fg_color,
            "The file this clip is made from cannot be found.",
        );
        if ui.button("Choose the file again").clicked() {
            commands.push(Command::RelinkSource(source));
        }
    }
}

fn show_a_video_or_a_sound(
    ui: &mut Ui,
    content: &PropertiesContent<'_>,
    clip: ClipId,
    placement: &ClipPlacement<'_>,
    media: &MediaClip,
    commands: &mut Vec<Command>,
) {
    let look = content.source_looks.get(&media.source());
    kind_box::show(ui, kind_name(placement.clip.kind()), |ui| {
        show_the_file_name(ui, look);

        if let Some(speed_factor) = labelled_row(ui, |ui| speed_slider(ui, media.speed())) {
            commands.push(Command::SetClipSpeed { clip, speed_factor });
        }
        if let Some(track) = content.project.track_holding(clip)
            && track.speed() != SpeedFactor::NORMAL
        {
            ui.weak(format!(
                "It plays at {}, because its track plays at {}",
                placement.effective_speed,
                track.speed()
            ));
        }

        let target = AnimationTarget::Clip(clip);
        let gain_now = Gain::from_decibels(
            number_at_the_playhead(content, target, PropertyId::Gain).unwrap_or(media.gain().decibels()),
        );
        let new_gain = animatable::row(
            ui,
            content,
            Animatable::of_a_clip(clip, PropertyId::Gain),
            commands,
            |ui, _| labelled_row(ui, |ui| gain_slider(ui, gain_now)),
        );
        if let Some(gain) = new_gain {
            commands.push(Command::SetClipGain { clip, gain });
        }

        let mut muted = flag_at_the_playhead(content, target, PropertyId::Muted).unwrap_or(media.is_muted());
        let muted_changed = animatable::row(
            ui,
            content,
            Animatable::of_a_clip(clip, PropertyId::Muted),
            commands,
            |ui, _| ui.checkbox(&mut muted, "Mute").changed(),
        );
        if muted_changed {
            commands.push(Command::SetClipMuted { clip, muted });
        }

        if placement.clip.kind() == ClipKind::Video {
            placement_controls::show(ui, content, clip, SIZE_LABEL, commands);
        }
        offer_to_choose_the_file_again(ui, look, media.source(), commands);
    });
}

fn show_a_picture(
    ui: &mut Ui,
    content: &PropertiesContent<'_>,
    clip: ClipId,
    image: &ImageClip,
    commands: &mut Vec<Command>,
) {
    let look = content.source_looks.get(&image.source());
    kind_box::show(ui, kind_name(ClipKind::Image), |ui| {
        show_the_file_name(ui, look);
        if let Some(frames) = shown_for(ui, content.project.frame_rate(), image.duration_in_frames()) {
            commands.push(Command::SetClipDuration { clip, frames });
        }
        placement_controls::show(ui, content, clip, SIZE_LABEL, commands);
        offer_to_choose_the_file_again(ui, look, image.source(), commands);
    });
}
