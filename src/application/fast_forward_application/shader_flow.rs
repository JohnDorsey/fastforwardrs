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
use crate::isf::Shape;
use crate::project::{AnimationTarget, EffectKind, Member, ShaderImage, TimelineFrameIndex, TrackId};
use crate::shader_files::{ShaderFile, ShaderFileProblem, kind_of_a_shader, read_a_shader_file};

/// What the message about a file that is not a shader that can be used says.
pub const NOT_A_SHADER_TITLE: &str = "This is not a shader that can be used";

/// What a person wants a shader file for: to put on a picture as an effect (a filter), or to make a clip with (a generator).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wanted {
    Filter,
    Generator,
}

/// What is wrong with a shader that is (or is not) a generator, for what is `wanted` of it, in words, if something is: a generator where an effect is wanted, or the other way round.
fn what_is_wrong_with_the_shape(wanted: Wanted, is_a_generator: bool) -> Option<&'static str> {
    match (wanted, is_a_generator) {
        (Wanted::Filter, true) => Some(
            "this shader is a generator: it makes a picture, and is made into a clip with New, then Generator from file..., on an empty part of a track",
        ),
        (Wanted::Generator, false) => Some(
            "this shader is a filter: it works on a picture, and is put on one with Add effect from file..., not made into a clip",
        ),
        _ => None,
    }
}

/// What is wrong with the shader `file` for what is `wanted` of it, in words, if something is: this version cannot run it, or it is the wrong shape.
fn what_is_wrong_with(file: &ShaderFile, wanted: Wanted) -> Option<String> {
    file.why_not_usable
        .clone()
        .or_else(|| what_is_wrong_with_the_shape(wanted, file.shape == Shape::Generator).map(str::to_owned))
}

/// The kind of effect that the shader `text` of the file `file_name`, which brings `images`, is, if it is a good one and is what is `wanted` of it: **what is wrong with it, in words, if not.**
fn kind_of_the_shader(
    file_name: &str,
    text: &str,
    images: Vec<ShaderImage>,
    wanted: Wanted,
) -> Result<EffectKind, String> {
    let made = kind_of_a_shader(file_name, text, images)?;
    if let Some(why) = made.why_not_usable {
        return Err(why);
    }
    match what_is_wrong_with_the_shape(wanted, made.shape == Shape::Generator) {
        Some(why) => Err(why.to_owned()),
        None => Ok(made.kind),
    }
}

impl FastForwardApplication {
    /// Reads the file at `path` as a shader that is `wanted`, or tells the person why it is not one and gives nothing.
    fn read_the_shader_that_was_chosen(&mut self, path: &Path, wanted: Wanted) -> Option<ShaderFile> {
        let name = path
            .file_name()
            .map_or_else(|| "shader.fs".to_owned(), |name| name.to_string_lossy().into_owned());
        match read_a_shader_file(path) {
            Err(ShaderFileProblem::CouldNotBeRead(words)) => {
                self.show_message(NOT_A_FILE_TITLE, words);
                None
            }
            Err(ShaderFileProblem::NotAShader(words)) => {
                self.show_message(NOT_A_SHADER_TITLE, format!("{name}: {words}"));
                None
            }
            Ok(file) => {
                if let Some(why) = what_is_wrong_with(&file, wanted) {
                    self.show_message(NOT_A_SHADER_TITLE, format!("{name}: {why}"));
                    return None;
                }
                Some(file)
            }
        }
    }

    /// Reads the file at `path` as a shader and puts an effect of it on `owner` (see the top of this file). The user is told why not if it cannot be done.
    pub(super) fn use_the_shader_in(&mut self, path: &Path, owner: Member, now: Instant, context: &Context) {
        let Some(file) = self.read_the_shader_that_was_chosen(path, Wanted::Filter) else {
            return;
        };
        self.carry_out_all(
            vec![Command::UseAShader {
                owner,
                file_name: file.file_name,
                text: Arc::from(file.text),
                images: file.images,
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
        let Some(file) = self.read_the_shader_that_was_chosen(&path, Wanted::Generator) else {
            return;
        };
        self.carry_out_all(
            vec![Command::NewGeneratorClip {
                track,
                frame,
                file_name: file.file_name,
                text: Arc::from(file.text),
                images: file.images,
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
        self.new_generator_clip_of_kind(track, frame, kind, now);
    }

    /// Puts a new generator clip made by the shader of the kind `kind` on `track`, starting at `frame`, and selects it. (What [`Command::NewGeneratorClipOfKind`] is: the shader is
    /// one that the plugin manager found; a kind that is not a generator is no clip.)
    pub(super) fn new_generator_clip_of_kind(
        &mut self,
        track: TrackId,
        frame: TimelineFrameIndex,
        kind: EffectKind,
        now: Instant,
    ) {
        if !kind.is_a_generator() {
            return;
        }
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
