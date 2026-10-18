// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tables of colours (`.cube` files) as resources of the project, from the window (phase 4, piece 4.7).
//!
//! Two things start it: the plus button's *Add effect from file...*, which puts a new look-up table effect on a clip, a track or a group, and the *Choose...* button in the
//! box of such an effect, which gives it another table. Both come to the same: a file is chosen, read (at most [`MOST_BYTES_OF_A_RESOURCE`](crate::project::MOST_BYTES_OF_A_RESOURCE) bytes), and **read as a table
//! before anything is changed**, so that a file that is not one is refused with the line that is wrong, and the project is left as it was. Then how the file is kept is
//! decided by the setting (the project's, or the global one; see [`ResourceStorage`](crate::project::ResourceStorage)): embedded in the project file, linked by its path, or asked about; and where the project
//! file is plain JSON, which has no room to embed anything, the table is linked and the user is told so, once. The edit itself is one step of the history
//! ([`Command::UseATable`]).

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use egui::Context;

use super::FastForwardApplication;
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::application::file_chooser::{FileKind, FileQuestion};
use crate::cube_table::parse_cube;
use crate::project::{
    AnimationTarget, EditError, EffectInstanceId, EffectKind, HowToKeepAResource, Member, ParameterControl,
    ProjectFileKind, PropertyId, PropertyValue, ResourceKind, SourceMediaLink,
};
use crate::project_file::{link_of_a_new_resource, read_a_resource_file};

/// What the dialog and the message about a table say.
pub const NOT_A_TABLE_TITLE: &str = "This is not a table that can be used";
pub const NOT_A_FILE_TITLE: &str = "This file cannot be used";
pub const LINKED_BECAUSE_THE_FILE_IS_PLAIN_JSON: &str = "The table is linked and not embedded: the project is saved as a plain .json file, which has no room for it. \
     Save the project as a .ffrs file (File, Save project as) and add the table again to keep it inside the project file.";

impl FastForwardApplication {
    /// What the file of the project is, as far as keeping a table in it goes.
    fn kind_of_the_project_file(&self) -> ProjectFileKind {
        match &self.document.project_file_path {
            None => ProjectFileKind::NotSavedYet,
            Some(_) if self.project_file_is_an_archive() => ProjectFileKind::Archive,
            Some(_) => ProjectFileKind::PlainJson,
        }
    }

    /// Tells the user why an effect could not be put on something (when it is a reason a person can do something about: it has the most effects there can be).
    pub(super) fn say_why_an_effect_was_refused(&mut self, title: &str, error: &EditError) {
        if matches!(error, EditError::TooManyEffects { .. }) {
            self.show_message(title, error.to_string());
        }
    }

    /// Asks for a table of colours, and puts a new look-up table effect with it on `owner`.
    pub(super) fn add_an_effect_from_a_file(&mut self, owner: Member, now: Instant, context: &Context) {
        self.use_a_table_from_a_file(owner, None, now, context);
    }

    /// Asks for a table of colours, and gives the look-up table effect `instance` of `owner` that table.
    pub(super) fn choose_the_table_of(
        &mut self,
        owner: Member,
        instance: EffectInstanceId,
        now: Instant,
        context: &Context,
    ) {
        self.use_a_table_from_a_file(owner, Some(instance), now, context);
    }

    fn use_a_table_from_a_file(
        &mut self,
        owner: Member,
        instance: Option<EffectInstanceId>,
        now: Instant,
        context: &Context,
    ) {
        self.pause_playback_for_dialog();
        let Some(path) = self.environment.file_chooser.choose_a_file_to_open(&FileQuestion {
            title: "Choose a table of colours",
            suggested_file_name: None,
            starting_directory: None,
            kinds: &[
                FileKind {
                    name: "Tables of colours (.cube)",
                    extensions: &["cube"],
                },
                FileKind {
                    name: "All files",
                    extensions: &["*"],
                },
            ],
        }) else {
            return;
        };
        self.use_the_table_in(&path, owner, instance, now, context);
    }

    /// Reads the file at `path` as a table and uses it (see the top of this file). The user is told why not if it cannot be used.
    pub(super) fn use_the_table_in(
        &mut self,
        path: &Path,
        owner: Member,
        instance: Option<EffectInstanceId>,
        now: Instant,
        context: &Context,
    ) {
        let name = path
            .file_name()
            .map_or_else(|| "table.cube".to_owned(), |name| name.to_string_lossy().into_owned());
        let bytes = match read_a_resource_file(path) {
            Ok(bytes) => bytes,
            Err(problem) => {
                self.show_message(NOT_A_FILE_TITLE, problem.to_string());
                return;
            }
        };
        let read_as_a_table = std::str::from_utf8(&bytes)
            .map_err(|_| "the file is not text".to_owned())
            .and_then(|text| parse_cube(text).map_err(|problem| problem.to_string()));
        if let Err(why) = read_as_a_table {
            self.show_message(NOT_A_TABLE_TITLE, format!("{name}: {why}"));
            return;
        }
        let command = |embedded: bool| Command::UseATable {
            owner,
            instance,
            wanted_name: name.clone(),
            bytes: bytes.clone(),
            link: link_of_a_new_resource(path, &name, &bytes, embedded),
        };
        let file = self.kind_of_the_project_file();
        match self.effective_settings().resource_storage.how_to_keep(file) {
            HowToKeepAResource::Embed => self.carry_out_all(vec![command(true)], now, context),
            HowToKeepAResource::Link => self.carry_out_all(vec![command(false)], now, context),
            HowToKeepAResource::LinkBecauseTheFileIsPlainJson => {
                self.carry_out_all(vec![command(false)], now, context);
                if !self.interface.told_that_a_table_was_linked_for_plain_json {
                    self.interface.told_that_a_table_was_linked_for_plain_json = true;
                    self.show_message("The table is linked", LINKED_BECAUSE_THE_FILE_IS_PLAIN_JSON);
                }
            }
            HowToKeepAResource::Ask => {
                self.interface.dialog = Some(Dialog::HowToKeepATable {
                    file_name: name.clone(),
                    can_embed: file != ProjectFileKind::PlainJson,
                    embed: Box::new(command(true)),
                    link: Box::new(command(false)),
                });
            }
        }
    }

    /// Adds the table `bytes` as a resource called `wanted_name` of the project, kept as `link` says, and uses it for the look-up table effect `instance` of `owner` or, if
    /// there is none, for a new one put on `owner`. (What [`Command::UseATable`] is.)
    pub(super) fn use_a_table(
        &mut self,
        owner: Member,
        instance: Option<EffectInstanceId>,
        wanted_name: &str,
        bytes: Arc<[u8]>,
        link: SourceMediaLink,
        now: Instant,
    ) {
        // (An effect that is not a look-up table is not given a table, whatever was asked.)
        if let Some(instance) = instance
            && self
                .document
                .project
                .chain_of_the_member(owner)
                .and_then(|chain| chain.instance(instance))
                .is_none_or(|effect| effect.kind() != EffectKind::LUT)
        {
            return;
        }
        let name = match self
            .document
            .project
            .add_resource(wanted_name, ResourceKind::Cube, bytes, link)
        {
            Ok(name) => name,
            Err(problem) => {
                self.show_message(NOT_A_FILE_TITLE, problem.to_string());
                return;
            }
        };
        let kind = EffectKind::LUT;
        let Some(slot) = kind
            .parameters()
            .iter()
            .position(|parameter| matches!(parameter.control, ParameterControl::Resource(_)))
        else {
            return;
        };
        let instance = match instance {
            Some(instance) => instance,
            None => {
                let added = match owner {
                    Member::Clip(clip) => self.document.project.add_effect(AnimationTarget::Clip(clip), kind),
                    Member::Track(track) => self.document.project.add_effect(AnimationTarget::Track(track), kind),
                    Member::Group(group) => self.document.project.add_effect_to_the_group(group, kind),
                };
                match added {
                    Ok(instance) => instance,
                    Err(error) => {
                        self.say_why_an_effect_was_refused("The effect cannot be added", &error);
                        return;
                    }
                }
            }
        };
        let property = PropertyId::effect_value(kind, instance, u8::try_from(slot).unwrap_or(0));
        let value = PropertyValue::Words(name);
        match owner {
            Member::Clip(clip) => {
                self.change_a_property_at_the_playhead(AnimationTarget::Clip(clip), property, value, now);
            }
            Member::Track(track) => {
                self.change_a_property_at_the_playhead(AnimationTarget::Track(track), property, value, now);
            }
            Member::Group(group) => {
                let _ = self
                    .document
                    .project
                    .set_effect_value_of_the_group(group, property, &value);
            }
        }
    }
}
