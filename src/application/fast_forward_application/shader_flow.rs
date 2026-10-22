// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shaders (ISF files) as effects, from the window (phase 6, piece 6.3).
//!
//! The plus button's *Add effect from file...* takes a `.fs` or `.isf` file as it takes a `.cube` file: the file is chosen, read, and **read as a shader and compiled before anything is
//! changed**, so that a file that is not one is refused with the line that is wrong, and the project is left as it was. Then a new effect of the kind that the file is is put on the clip,
//! the track or the group, in one step of the history ([`Command::UseAShader`]). **The text of the file is kept in the kind of effect, and so in the project's file** (a shader is data:
//! there is no question to ask about it, and a project that is opened somewhere that the shader was never installed still runs it). See `docs/engine-phase-6-design.md`, J3 and J4.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use egui::Context;

use super::FastForwardApplication;
use super::command_execution::SECONDS_A_NEW_GENERATOR_CLIP_LASTS;
use super::table_flow::NOT_A_FILE_TITLE;
use crate::application::commands::Command;
use crate::application::file_chooser::{FileKind, FileQuestion};
use crate::isf;
use crate::project::{
    AnimationTarget, EffectKind, MOST_BYTES_OF_A_SHADER, MOST_IMAGES_A_SHADER_BRINGS, Member, ShaderImage,
    TimelineFrameIndex, TrackId,
};
use crate::project_file::read_a_resource_file;

/// What the message about a file that is not a shader that can be used says.
pub const NOT_A_SHADER_TITLE: &str = "This is not a shader that can be used";

/// What a person wants a shader file for: to put on a picture as an effect (a filter), or to make a clip with (a generator).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wanted {
    Filter,
    Generator,
}

/// The images that the shader `text`, the file `path`, brings with it (an ISF file's `IMPORTED`), read from beside it: **each is a file in the folder of the shader or in a folder below it**
/// (a path that is absolute, or goes up, or says nothing, is refused: a shader is not a way to read the files of a computer), and must be a PNG or a JPEG that the program can use.
/// What is wrong, in words, with the first that is wrong, if anything is.
fn images_of_the_shader(path: &Path, text: &str) -> Result<Vec<ShaderImage>, String> {
    let Ok(file) = isf::read(text) else {
        // (A file that cannot be read says so when it is made into a kind.)
        return Ok(Vec::new());
    };
    if file.imported.len() > MOST_IMAGES_A_SHADER_BRINGS {
        return Err(format!(
            "the shader brings {} images, and {MOST_IMAGES_A_SHADER_BRINGS} are the most",
            file.imported.len()
        ));
    }
    let folder = path.parent().unwrap_or_else(|| Path::new("."));
    file.imported
        .iter()
        .map(|imported| {
            let relative = Path::new(&imported.path);
            let stays_below = !imported.path.trim().is_empty()
                && relative.is_relative()
                && !relative.has_root()
                && relative
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_) | std::path::Component::CurDir));
            if !stays_below {
                return Err(format!(
                    "the image {} is at {:?}, which is not a file in the folder of the shader or below it",
                    imported.name, imported.path
                ));
            }
            let bytes = read_a_resource_file(&folder.join(relative))
                .map_err(|problem| format!("the image {} ({}): {problem}", imported.name, imported.path))?;
            let image = ShaderImage::new(imported.name.clone(), bytes);
            image
                .pixels()
                .map_err(|why| format!("the image {} ({}): {why}", imported.name, imported.path))?;
            Ok(image)
        })
        .collect()
}

/// The kind of effect that the shader `text` of the file `file_name`, which brings `images`, is, if it is a good one and is what is `wanted` of it: **what is wrong with it, in words, if not.**
fn kind_of_the_shader(
    file_name: &str,
    text: &str,
    images: Vec<ShaderImage>,
    wanted: Wanted,
) -> Result<EffectKind, String> {
    if text.len() > MOST_BYTES_OF_A_SHADER {
        return Err(format!(
            "the file is {} bytes, and {MOST_BYTES_OF_A_SHADER} are the most a shader can be",
            text.len()
        ));
    }
    let file = isf::read(text).map_err(|problem| problem.to_string())?;
    let descriptor = isf::descriptor_with_images_of(&file, file_name, text, images);
    let kind = EffectKind::from_snapshot(descriptor)?;
    if let Some(why) = isf::why_not_usable(kind.descriptor()) {
        return Err(why);
    }
    match (wanted, kind.is_a_generator()) {
        (Wanted::Filter, true) => Err("this shader is a generator: it makes a picture, and is made into a clip with New, then Generator from file..., on an empty part of a track".to_owned()),
        (Wanted::Generator, false) => Err("this shader is a filter: it works on a picture, and is put on one with Add effect from file..., not made into a clip".to_owned()),
        _ => Ok(kind),
    }
}

impl FastForwardApplication {
    /// Reads the file at `path` as a shader and puts an effect of it on `owner` (see the top of this file). The user is told why not if it cannot be done.
    pub(super) fn use_the_shader_in(&mut self, path: &Path, owner: Member, now: Instant, context: &Context) {
        let name = path
            .file_name()
            .map_or_else(|| "shader.fs".to_owned(), |name| name.to_string_lossy().into_owned());
        let bytes = match read_a_resource_file(path) {
            Ok(bytes) => bytes,
            Err(problem) => {
                self.show_message(NOT_A_FILE_TITLE, problem.to_string());
                return;
            }
        };
        let Ok(text) = String::from_utf8(bytes.to_vec()) else {
            self.show_message(NOT_A_SHADER_TITLE, format!("{name}: the file is not text"));
            return;
        };
        let images = match images_of_the_shader(path, &text) {
            Ok(images) => images,
            Err(why) => {
                self.show_message(NOT_A_SHADER_TITLE, format!("{name}: {why}"));
                return;
            }
        };
        if let Err(why) = kind_of_the_shader(&name, &text, images.clone(), Wanted::Filter) {
            self.show_message(NOT_A_SHADER_TITLE, format!("{name}: {why}"));
            return;
        }
        self.carry_out_all(
            vec![Command::UseAShader {
                owner,
                file_name: name,
                text: Arc::from(text),
                images,
            }],
            now,
            context,
        );
    }

    /// Asks for a shader that is a generator, and makes a clip with it on `track` at `frame` (see the top of this file).
    pub(super) fn choose_a_generator_file(
        &mut self,
        track: TrackId,
        frame: TimelineFrameIndex,
        now: Instant,
        context: &Context,
    ) {
        self.pause_playback_for_dialog();
        let Some(path) = self.environment.file_chooser.choose_a_file_to_open(&FileQuestion {
            title: "Choose a generator",
            suggested_file_name: None,
            starting_directory: None,
            kinds: &[
                FileKind {
                    name: "Shaders (.fs, .isf)",
                    extensions: &["fs", "isf"],
                },
                FileKind {
                    name: "All files",
                    extensions: &["*"],
                },
            ],
        }) else {
            return;
        };
        let name = path
            .file_name()
            .map_or_else(|| "generator.fs".to_owned(), |name| name.to_string_lossy().into_owned());
        let bytes = match read_a_resource_file(&path) {
            Ok(bytes) => bytes,
            Err(problem) => {
                self.show_message(NOT_A_FILE_TITLE, problem.to_string());
                return;
            }
        };
        let Ok(text) = String::from_utf8(bytes.to_vec()) else {
            self.show_message(NOT_A_SHADER_TITLE, format!("{name}: the file is not text"));
            return;
        };
        let images = match images_of_the_shader(&path, &text) {
            Ok(images) => images,
            Err(why) => {
                self.show_message(NOT_A_SHADER_TITLE, format!("{name}: {why}"));
                return;
            }
        };
        if let Err(why) = kind_of_the_shader(&name, &text, images.clone(), Wanted::Generator) {
            self.show_message(NOT_A_SHADER_TITLE, format!("{name}: {why}"));
            return;
        }
        self.carry_out_all(
            vec![Command::NewGeneratorClip {
                track,
                frame,
                file_name: name,
                text: Arc::from(text),
                images,
            }],
            now,
            context,
        );
    }

    /// Puts a new generator clip made by the shader `text` of the file `file_name` on `track`, starting at `frame`, and selects it. (What [`Command::NewGeneratorClip`] is.)
    pub(super) fn new_generator_clip(
        &mut self,
        track: TrackId,
        frame: TimelineFrameIndex,
        file_name: &str,
        text: &str,
        images: Vec<ShaderImage>,
        now: Instant,
    ) {
        let Ok(kind) = kind_of_the_shader(file_name, text, images, Wanted::Generator) else {
            return;
        };
        self.new_clip_with_no_media(
            frame,
            now,
            ("a generator clip", SECONDS_A_NEW_GENERATOR_CLIP_LASTS),
            |project, wanted_frames| project.try_insert_generator_clip(track, frame, kind, wanted_frames),
        );
    }

    /// Puts a new effect of the kind that the shader `text` of the file `file_name` is on `owner`. (What [`Command::UseAShader`] is.)
    pub(super) fn use_a_shader(&mut self, owner: Member, file_name: &str, text: &str, images: Vec<ShaderImage>) {
        let Ok(kind) = kind_of_the_shader(file_name, text, images, Wanted::Filter) else {
            return;
        };
        let added = match owner {
            Member::Clip(clip) => self.document.project.add_effect(AnimationTarget::Clip(clip), kind),
            Member::Track(track) => self.document.project.add_effect(AnimationTarget::Track(track), kind),
            Member::Group(group) => self.document.project.add_effect_to_the_group(group, kind),
        };
        if let Err(error) = added {
            self.say_why_an_effect_was_refused("The effect cannot be added", &error);
        }
    }
}
