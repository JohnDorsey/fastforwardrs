//! The floating settings boxes of the timeline: the one that opens above a clip when it is right
//! clicked (speed, loudness, mute) and the one that opens above the gear of a track (the same for
//! everything on the track). Each box has a pointer at its bottom edge that touches the thing it is
//! about.
//!
//! When the clip that is right-clicked is one of several that are selected, its box is about all of
//! them: what is changed in it is changed for each of them.

use egui::{Context, Id, Slider, SliderClamping, Ui};

use super::{SourceLook, TimelineContent, TimelineViewState};
use crate::application::bubble;
use crate::application::commands::Command;
use crate::application::duration_control::shown_for;
use crate::project::{Clip, ClipKind, ClipLocation, Gain, ImageClip, SpeedFactor, TrackId};

/// The largest speed the sliders offer. (A project file may hold a larger one.)
pub const LARGEST_SPEED_FACTOR_ON_THE_SLIDER: u32 = 1000;

/// How wide the sliders are, in points.
const SLIDER_WIDTH: f32 = 180.0;

/// What a settings box is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsBoxSubject {
    Clip(ClipLocation),
    Track(TrackId),
}

/// A settings box that is open.
#[derive(Clone, Copy, Debug)]
pub(super) struct OpenSettingsBox {
    pub subject: SettingsBoxSubject,
    /// Where the pointer of the box touches, which is worked out again in every frame from where
    /// the subject is drawn. `None` when it was not drawn in this frame (it is scrolled out of
    /// view, or gone), which closes the box.
    pub anchor: Option<egui::Pos2>,
    /// Where the pointer of the box touched when it was drawn last.
    pub shown_anchor: Option<egui::Pos2>,
    /// Where the box was drawn in the last frame.
    pub rect: Option<egui::Rect>,
}

/// Draws the open settings box, if there is one, closes it when it is time to, and adds a command
/// for each change made in it.
pub(super) fn show(
    context: &Context,
    content: &TimelineContent<'_>,
    state: &mut TimelineViewState,
    commands: &mut Vec<Command>,
) {
    let Some(open) = state.settings_box else { return };
    let Some(new_anchor) = open.anchor else {
        state.close_settings_box();
        return;
    };
    if bubble::escape_pressed(context) {
        state.close_settings_box();
        return;
    }
    // While a button that was pressed on the box is held down, the box stays where it is. What it
    // is about may change shape under the pointer (a clip is made shorter by the speed slider in
    // its box, so its middle moves away), and a box that went with it would slide out from under
    // the pointer, which would change what the pointer is doing to the slider.
    let anchor = match open.shown_anchor {
        Some(shown_anchor) if button_is_held_on(context, open.rect) => shown_anchor,
        _ => new_anchor,
    };
    let response = bubble::show(context, Id::new("timeline settings box"), anchor, |ui| {
        match open.subject {
            SettingsBoxSubject::Clip(clip) => show_clip_settings(ui, content, clip, commands),
            SettingsBoxSubject::Track(track) => show_track_settings(ui, content, track, commands),
        }
    });
    if let Some(open) = state.settings_box.as_mut() {
        open.rect = Some(response.rect);
        open.shown_anchor = Some(anchor);
    }
    // What it was about is gone (it was deleted, perhaps): there is nothing to point at.
    if !response.inner {
        state.close_settings_box();
    }
}

/// Whether a mouse button is down that was pressed on the box, where it was drawn (`box_rectangle`).
fn button_is_held_on(context: &Context, box_rectangle: Option<egui::Rect>) -> bool {
    context.input(|input| {
        input.pointer.any_down()
            && input
                .pointer
                .press_origin()
                .zip(box_rectangle)
                .is_some_and(|(origin, rectangle)| rectangle.expand(2.0).contains(origin))
    })
}

/// The speed slider, which goes from 1 to 1000 times, with the same room for each tenfold.
fn speed_slider(ui: &mut Ui, speed_factor: SpeedFactor) -> Option<SpeedFactor> {
    let mut speed = speed_factor.value();
    let label = ui.label("Speed");
    let changed = ui
        .add(
            Slider::new(&mut speed, 1..=LARGEST_SPEED_FACTOR_ON_THE_SLIDER)
                .logarithmic(true)
                .clamping(SliderClamping::Always)
                .suffix("x")
                .smallest_positive(1.0),
        )
        .labelled_by(label.id)
        .changed();
    if changed { SpeedFactor::new(speed) } else { None }
}

/// The loudness slider, from quieter by 30 dB to louder by 30 dB, with no change in the middle.
fn gain_slider(ui: &mut Ui, gain: Gain) -> Option<Gain> {
    let mut decibels = gain.decibels();
    let label = ui.label("Gain");
    let changed = ui
        .add(
            Slider::new(&mut decibels, Gain::SMALLEST_DECIBELS..=Gain::LARGEST_DECIBELS)
                .clamping(SliderClamping::Always)
                .suffix(" dB")
                .fixed_decimals(1),
        )
        .labelled_by(label.id)
        .changed();
    if changed {
        Some(Gain::from_decibels(decibels))
    } else {
        None
    }
}

/// Puts a label and a slider side by side with the sliders lined up.
fn labelled_row<R>(ui: &mut Ui, add_row: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.spacing_mut().slider_width = SLIDER_WIDTH;
        add_row(ui)
    })
    .inner
}

/// The contents of the box of a clip. Returns whether the clip is still there to point at.
fn show_clip_settings(
    ui: &mut Ui,
    content: &TimelineContent<'_>,
    clip: ClipLocation,
    commands: &mut Vec<Command>,
) -> bool {
    let Some(placement) = content.project.placement_of(clip) else {
        return false;
    };
    if let Some(image) = placement.clip.as_image() {
        return show_picture_settings(ui, content, clip, image, commands);
    }
    let Some(media) = placement.clip.as_media() else {
        return false;
    };
    let Some(track) = content.project.track(clip.track) else {
        return false;
    };
    let look = content.source_looks.get(&media.source());
    let clips = clips_the_box_is_about(content, clip, Clip::is_media);

    let heading = if clips.len() > 1 {
        format!("{} clips", clips.len())
    } else {
        describe_clip(placement.clip.kind(), look)
    };
    ui.label(egui::RichText::new(heading).strong());
    ui.add_space(4.0);

    if let Some(speed_factor) = labelled_row(ui, |ui| speed_slider(ui, media.speed())) {
        commands.extend(clips.iter().map(|clip| Command::SetClipSpeed {
            clip: *clip,
            speed_factor,
        }));
    }
    if track.speed() != SpeedFactor::NORMAL {
        ui.weak(format!(
            "It plays at {}, because its track plays at {}",
            placement.effective_speed,
            track.speed()
        ));
    }
    if let Some(gain) = labelled_row(ui, |ui| gain_slider(ui, media.gain())) {
        commands.extend(clips.iter().map(|clip| Command::SetClipGain { clip: *clip, gain }));
    }
    let mut muted = media.is_muted();
    if ui.checkbox(&mut muted, "Mute").changed() {
        commands.extend(clips.iter().map(|clip| Command::SetClipMuted { clip: *clip, muted }));
    }

    if look.is_some_and(|look| !look.is_available) {
        ui.add_space(4.0);
        ui.colored_label(
            ui.visuals().warn_fg_color,
            "The file this clip is made from cannot be found.",
        );
        if ui.button("Choose the file again").clicked() {
            commands.push(Command::RelinkSource(media.source()));
        }
    }
    true
}

/// The contents of the box of a picture clip: for how long it is shown. Returns whether the clip is
/// still there to point at.
fn show_picture_settings(
    ui: &mut Ui,
    content: &TimelineContent<'_>,
    clip: ClipLocation,
    image: &ImageClip,
    commands: &mut Vec<Command>,
) -> bool {
    let look = content.source_looks.get(&image.source());
    let pictures = clips_the_box_is_about(content, clip, |clip| clip.as_image().is_some());

    let heading = if pictures.len() > 1 {
        format!("{} clips", pictures.len())
    } else {
        describe_clip(ClipKind::Image, look)
    };
    ui.label(egui::RichText::new(heading).strong());
    ui.add_space(4.0);

    if let Some(frames) = shown_for(ui, content.project.frame_rate(), image.duration_in_frames()) {
        commands.extend(
            pictures
                .iter()
                .map(|picture| Command::SetClipDuration { clip: *picture, frames }),
        );
    }

    if look.is_some_and(|look| !look.is_available) {
        ui.add_space(4.0);
        ui.colored_label(
            ui.visuals().warn_fg_color,
            "The file this clip is made from cannot be found.",
        );
        if ui.button("Choose the file again").clicked() {
            commands.push(Command::RelinkSource(image.source()));
        }
    }
    true
}

/// The clips that the box of `clip` is about: the selected ones of the same sort (`is_the_sort`),
/// if `clip` is one of the selected ones, and otherwise `clip` alone.
fn clips_the_box_is_about(
    content: &TimelineContent<'_>,
    clip: ClipLocation,
    is_the_sort: impl Fn(&Clip) -> bool,
) -> Vec<ClipLocation> {
    if !content.selection.contains(clip) {
        return vec![clip];
    }
    content
        .selection
        .iter()
        .filter(|selected| {
            content
                .project
                .placement_of(*selected)
                .is_some_and(|placement| is_the_sort(placement.clip))
        })
        .collect()
}

/// What the box of a clip is headed with: the kind of clip and the name of its file.
fn describe_clip(kind: ClipKind, look: Option<&SourceLook<'_>>) -> String {
    let kind_name = match kind {
        ClipKind::Video => "Video clip",
        ClipKind::Audio => "Sound clip",
        ClipKind::Image => "Picture clip",
        ClipKind::Text => "Text clip",
    };
    match look {
        Some(look) => format!("{kind_name}: {}", look.file_name),
        None => kind_name.to_owned(),
    }
}

/// The contents of the box of a track. Returns whether the track is still there to point at.
fn show_track_settings(
    ui: &mut Ui,
    content: &TimelineContent<'_>,
    track_id: TrackId,
    commands: &mut Vec<Command>,
) -> bool {
    let Some(track) = content.project.track(track_id) else {
        return false;
    };
    ui.label(egui::RichText::new(format!("Everything on {}", track.title())).strong());
    ui.add_space(4.0);

    if let Some(speed_factor) = labelled_row(ui, |ui| speed_slider(ui, track.speed())) {
        commands.push(Command::SetTrackSpeed {
            track: track_id,
            speed_factor,
        });
    }
    if let Some(gain) = labelled_row(ui, |ui| gain_slider(ui, track.gain())) {
        commands.push(Command::SetTrackGain { track: track_id, gain });
    }
    let mut muted = track.is_muted();
    if ui.checkbox(&mut muted, "Mute").changed() {
        commands.push(Command::SetTrackMuted { track: track_id, muted });
    }
    true
}
