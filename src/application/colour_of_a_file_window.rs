// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The window that says what the colours of one source file are, and lets the user say them differently (phase 3, decision C5 of `docs/engine-phase-3-design.md`). It is
//! opened from the menu of a clip made from the file (*Colour space of this file...*), and nothing about it is on the screen otherwise.
//!
//! A video has four things that are said of its colours: the **matrix** that makes red, green and blue of its brightness and colour differences, the **range** of its numbers,
//! the **primaries** of its colours, and the **transfer function** that its codes are made light with. A picture file has the last two. Each has a drop-down list whose first
//! item is *Automatic*, with what automatic is for this file written in it: what the file says, or what the program assumes where it says nothing. Under them is everything that
//! was assumed, in words (CM-03: the assumption is said in the interface).
//!
//! The window changes nothing itself: a choice becomes a command, which is a step of the history that the choices in a row on the same file are joined to.

use std::path::Path;

use egui::{Align2, ComboBox, Context, Grid, Id, Ui, Window, vec2};

use super::commands::Command;
use crate::colour_space::{StandardPrimaries, TransferFunction, VideoMatrix, VideoRange};
use crate::frame_scene::colour_of_a_picture_file;
use crate::media::VideoStreamFormat;
use crate::project::{InputColourOverride, Project, SourceMedia, SourceMediaIdentifier, SourceMediaProperties};

/// What the lines of the window are called.
pub const MATRIX_LABEL: &str = "Matrix";
pub const RANGE_LABEL: &str = "Range";
pub const PRIMARIES_LABEL: &str = "Primaries";
pub const CURVE_LABEL: &str = "Transfer function";

/// What the item that leaves a part to the program says, before what it is for this file.
pub const AUTOMATIC_LABEL: &str = "Automatic";

/// What the title of the window of the file called `name` says.
pub fn title_for(name: &str) -> String {
    format!("Colour space of {name}")
}

/// How the window names a file: its name, without the folders it is in.
fn name_of(source: &SourceMedia) -> String {
    Path::new(&source.link.absolute_path)
        .file_name()
        .map_or_else(|| "the file".to_owned(), |name| name.to_string_lossy().into_owned())
}

/// What the item of a drop-down list says when the part is automatic: *Automatic*, and in brackets what that is for this file (nothing in brackets while it is not known).
fn automatic_item(what_it_is: Option<&str>) -> String {
    match what_it_is {
        Some(what) => format!("{AUTOMATIC_LABEL} ({what})"),
        None => AUTOMATIC_LABEL.to_owned(),
    }
}

/// A line of the window: `label`, and a drop-down list with *Automatic* and then each of `options`; `chosen` is what is chosen now (`None` is automatic) and the answer is
/// what is chosen after whatever the user did.
fn choose<T: Copy + PartialEq>(
    ui: &mut Ui,
    (source, label): (SourceMediaIdentifier, &str),
    chosen: Option<T>,
    automatic: String,
    options: &[(T, &'static str)],
) -> Option<T> {
    let mut answer = chosen;
    ui.label(label);
    let shown = match chosen {
        None => automatic.clone(),
        Some(value) => options
            .iter()
            .find(|(candidate, _)| *candidate == value)
            .map_or_else(String::new, |(_, name)| (*name).to_owned()),
    };
    ComboBox::from_id_salt((source, label))
        .selected_text(shown)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut answer, None, automatic);
            for (value, name) in options {
                ui.selectable_value(&mut answer, Some(*value), *name);
            }
        });
    ui.end_row();
    answer
}

fn labelled<T: Copy>(all: &[T], label: impl Fn(T) -> &'static str) -> Vec<(T, &'static str)> {
    all.iter().map(|value| (*value, label(*value))).collect()
}

/// What the window shows and offers for the file `source`, whose video format is `format` if it is a video and that is known: the lines, and what is said of them.
/// Returns what the colours are said to be after the user's choices in this frame.
fn show_the_lines(ui: &mut Ui, source: &SourceMedia, format: Option<&VideoStreamFormat>) -> InputColourOverride {
    let mut said = source.colour;
    let id = source.identifier;
    match &source.properties {
        SourceMediaProperties::Video(video) => {
            let (width, height) = (video.display_width_in_pixels, video.display_height_in_pixels);
            // (A video whose format is not known yet says nothing: what is assumed of it is shown, and the lines change when the format is found out.)
            let automatic = format
                .cloned()
                .unwrap_or_default()
                .colour_of_a_picture_of_size(width, height);
            Grid::new(("colours of a file", id))
                .num_columns(2)
                .spacing([24.0, 8.0])
                .show(ui, |ui| {
                    said.matrix = choose(
                        ui,
                        (id, MATRIX_LABEL),
                        said.matrix,
                        automatic_item(Some(automatic.matrix.map_or("ffmpeg's own", VideoMatrix::label))),
                        &labelled(&VideoMatrix::ALL, VideoMatrix::label),
                    );
                    said.range = choose(
                        ui,
                        (id, RANGE_LABEL),
                        said.range,
                        automatic_item(Some(match automatic.full_range {
                            Some(true) => VideoRange::Full.label(),
                            Some(false) => VideoRange::Limited.label(),
                            None => "ffmpeg's own",
                        })),
                        &labelled(&VideoRange::ALL, VideoRange::label),
                    );
                    said.primaries = choose(
                        ui,
                        (id, PRIMARIES_LABEL),
                        said.primaries,
                        automatic_item(Some(automatic.primaries.label())),
                        &labelled(&StandardPrimaries::ALL, StandardPrimaries::label),
                    );
                    said.curve = choose(
                        ui,
                        (id, CURVE_LABEL),
                        said.curve,
                        automatic_item(Some(automatic.curve.label())),
                        &labelled(&TransferFunction::ALL, TransferFunction::label),
                    );
                });
        }
        SourceMediaProperties::Image(_) => {
            let automatic = colour_of_a_picture_file(&InputColourOverride::AUTOMATIC);
            Grid::new(("colours of a file", id))
                .num_columns(2)
                .spacing([24.0, 8.0])
                .show(ui, |ui| {
                    said.primaries = choose(
                        ui,
                        (id, PRIMARIES_LABEL),
                        said.primaries,
                        automatic_item(Some(automatic.primaries.label())),
                        &labelled(&StandardPrimaries::ALL, StandardPrimaries::label),
                    );
                    said.curve = choose(
                        ui,
                        (id, CURVE_LABEL),
                        said.curve,
                        automatic_item(Some(automatic.curve.label())),
                        &labelled(&TransferFunction::ALL, TransferFunction::label),
                    );
                });
        }
        SourceMediaProperties::Audio(_) => {}
    }
    said
}

/// What is said, in words, of how this file's colours were arrived at.
pub fn what_is_assumed_in_words(source: &SourceMedia, format: Option<&VideoStreamFormat>) -> Vec<String> {
    match &source.properties {
        SourceMediaProperties::Video(video) => {
            let (width, height) = (video.display_width_in_pixels, video.display_height_in_pixels);
            let Some(format) = format else {
                return vec!["The file is being examined: what it says of its colours is not known yet.".to_owned()];
            };
            let mut lines: Vec<String> = format
                .colour_of_a_picture_of_size_with(width, height, &source.colour)
                .assumptions
                .iter()
                .map(crate::media::Assumption::in_words)
                .collect();
            if format.as_native_yuv420p_with(width, height, &source.colour).is_none() {
                lines.push(format!(
                    "The pictures of this video are stored as {}, which ffmpeg turns into colours itself; it is told the matrix and the range above, where they are known.",
                    format.pixel_format.as_deref().unwrap_or("something the program cannot tell")
                ));
            }
            lines
        }
        SourceMediaProperties::Image(_) => {
            vec!["A picture file is read as sRGB unless you choose another primaries or transfer function.".to_owned()]
        }
        SourceMediaProperties::Audio(_) => Vec::new(),
    }
}

/// Shows the window of `open_for`, if there is one and the file is still in the project, and adds a command for a change made in it. The window closes (`open_for` becomes
/// `None`) when it is closed with its cross, or when the file is not in the project any more. `format` gives the format of a video, once it is known.
pub fn show<'a>(
    context: &Context,
    open_for: &mut Option<SourceMediaIdentifier>,
    project: &Project,
    format: impl Fn(SourceMediaIdentifier) -> Option<&'a VideoStreamFormat>,
    commands: &mut Vec<Command>,
) {
    let Some(identifier) = *open_for else {
        return;
    };
    let Some(source) = project.source(identifier) else {
        *open_for = None;
        return;
    };
    let mut open = true;
    Window::new(title_for(&name_of(source)))
        .id(Id::new(("colour space of a file", identifier)))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .pivot(Align2::CENTER_CENTER)
        .default_pos(context.content_rect().center() + vec2(0.0, 40.0))
        .show(context, |ui| {
            let format = format(identifier);
            let said = show_the_lines(ui, source, format);
            if said != source.colour {
                commands.push(Command::SetTheColoursOfAFile {
                    source: identifier,
                    colour: said,
                });
            }
            ui.add_space(8.0);
            ui.separator();
            for line in what_is_assumed_in_words(source, format) {
                ui.label(egui::RichText::new(line).small());
            }
        });
    if !open {
        *open_for = None;
    }
}

#[cfg(test)]
mod tests;
