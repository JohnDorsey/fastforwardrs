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
use super::table_flow::NOT_A_FILE_TITLE;
use crate::application::commands::Command;
use crate::isf;
use crate::project::{AnimationTarget, EffectKind, MOST_BYTES_OF_A_SHADER, Member};
use crate::project_file::read_a_resource_file;

/// What the message about a file that is not a shader that can be used says.
pub const NOT_A_SHADER_TITLE: &str = "This is not a shader that can be used";

/// The kind of effect that the shader `text` of the file `file_name` is, if it is a good one: **what is wrong with it, in words, if not.**
fn kind_of_the_shader(file_name: &str, text: &str) -> Result<EffectKind, String> {
    if text.len() > MOST_BYTES_OF_A_SHADER {
        return Err(format!(
            "the file is {} bytes, and {MOST_BYTES_OF_A_SHADER} are the most a shader can be",
            text.len()
        ));
    }
    let file = isf::read(text).map_err(|problem| problem.to_string())?;
    let descriptor = isf::descriptor_of(&file, file_name, text);
    let kind = EffectKind::from_snapshot(descriptor)?;
    match isf::why_not_usable(kind.descriptor()) {
        Some(why) => Err(why),
        None => Ok(kind),
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
        if let Err(why) = kind_of_the_shader(&name, &text) {
            self.show_message(NOT_A_SHADER_TITLE, format!("{name}: {why}"));
            return;
        }
        self.carry_out_all(
            vec![Command::UseAShader {
                owner,
                file_name: name,
                text: Arc::from(text),
            }],
            now,
            context,
        );
    }

    /// Puts a new effect of the kind that the shader `text` of the file `file_name` is on `owner`. (What [`Command::UseAShader`] is.)
    pub(super) fn use_a_shader(&mut self, owner: Member, file_name: &str, text: &str) {
        let Ok(kind) = kind_of_the_shader(file_name, text) else {
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
