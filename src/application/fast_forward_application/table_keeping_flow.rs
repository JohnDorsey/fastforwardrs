// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Switching the tables of colours of a project between being **in the project file** and being **linked** by a path, afterwards (RS-04), with the same two actions that a
//! video has: *Include in project file* and *Take out of project file* (see `included_files_flow`).
//!
//! Where a table is kept is part of the project and not an edit that can be undone, as it is not for a video. **Including** needs a project file that is an archive (the user is
//! asked to save as one if it is not) and writes the table into it with the next save, which is made now. **Taking out** puts the table back where it was when it was
//! included (the place it had then, and if that has gone, the same place from the folder of the project file): where the same table already is nothing is copied, where
//! another file is the place is not used, and where nothing is it is written if the folder is there. If there is no place the table stays in the project file, and the user is told.

use std::io::Write;
use std::path::{Path, PathBuf};

use super::FastForwardApplication;
use super::included_files_flow::{places_it_was_at, put_back_where};
use crate::application::dialogs::Dialog;
use crate::project::SourceMediaLink;
use crate::project_file::archive::project_resource_entry_name;
use crate::project_file::relative_path_between;

impl FastForwardApplication {
    /// Puts the tables called `names` in the project file, the ones that the project has the bytes of and are not in it already. If the file of the project is not an archive the
    /// user is asked first to save it as one. A table that is missing (its linked file is gone) is not included, and is said so.
    pub(super) fn include_tables_in_the_project_file(&mut self, names: &[String]) {
        let (to_include, missing) = self.tables_that_can_be_included(names);
        if to_include.is_empty() {
            if !missing.is_empty() {
                self.show_message_about_tables_that_are_missing(&missing);
            }
            return;
        }
        if let Some(archive) = self
            .document
            .project_file_path
            .clone()
            .filter(|_| self.project_file_is_an_archive())
        {
            let project_directory = self.project_directory_for_tables();
            let project_before = self.document.project.clone();
            self.mark_tables_as_included(&to_include, project_directory.as_deref());
            if self.save_the_project(archive, Some(project_before)) && !missing.is_empty() {
                self.show_message_about_tables_that_are_missing(&missing);
            }
        } else {
            self.pause_playback_for_dialog();
            self.interface.dialog = Some(Dialog::MustSaveAsAnArchiveToIncludeTables {
                names: to_include,
                has_a_plain_project_file: self.document.project_file_path.is_some(),
            });
        }
    }

    /// Asks where to save the project as an archive, and saves it there with the tables `names` included: if it cannot be saved there, nothing is included.
    pub(super) fn save_as_an_archive_with_tables_included(&mut self, names: &[String]) {
        self.pause_playback_for_dialog();
        let Some(path) = self.ask_where_to_save_an_archive("Save the project with the tables included") else {
            return;
        };
        let project_before = self.document.project.clone();
        let project_directory = std::path::absolute(&path)
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf));
        self.mark_tables_as_included(names, project_directory.as_deref());
        self.save_the_project(path, Some(project_before));
    }

    /// Of the tables called `names`, those that can be included (the project has their bytes and they are not in the file already), and the names of those that are missing.
    /// Each once, in the order given.
    fn tables_that_can_be_included(&self, names: &[String]) -> (Vec<String>, Vec<String>) {
        let (mut to_include, mut missing) = (Vec::new(), Vec::new());
        for name in names {
            let Some(resource) = self.document.project.resources().get(name) else {
                continue;
            };
            if resource.is_included() || to_include.contains(name) || missing.contains(name) {
                continue;
            }
            if resource.is_missing() {
                missing.push(name.clone());
            } else {
                to_include.push(name.clone());
            }
        }
        (to_include, missing)
    }

    fn show_message_about_tables_that_are_missing(&mut self, names: &[String]) {
        self.show_message(
            if names.len() == 1 {
                "The table cannot be included"
            } else {
                "The tables cannot be included"
            },
            format!(
                "A table can only be included in the project file when the project has it, and {} not found:\n{}",
                if names.len() == 1 { "this one was" } else { "these were" },
                names.join("\n")
            ),
        );
    }

    /// The folder the project file is in, as a path from the root.
    fn project_directory_for_tables(&self) -> Option<PathBuf> {
        let folder = self.document.project_file_path.as_deref()?.parent()?;
        std::path::absolute(folder).ok()
    }

    /// Says in the project that the tables called `names` are included, as the entries that the project file will have them as. The places they were at stay as they were.
    fn mark_tables_as_included(&mut self, names: &[String], project_directory: Option<&Path>) {
        for name in names {
            let Some(resource) = self.document.project.resources().get(name) else {
                continue;
            };
            let Some(crc32) = resource.crc32() else {
                continue;
            };
            let link = resource.link().clone();
            let relative_path = link
                .relative_path
                .clone()
                .or_else(|| project_directory.and_then(|folder| relative_path_between(folder, &link.absolute_path)));
            self.document.project.keep_a_resource_at(
                name,
                SourceMediaLink {
                    absolute_path: link.absolute_path,
                    relative_path,
                    included_as: Some(project_resource_entry_name(crc32, name)),
                },
            );
        }
    }

    /// Takes the tables called `names` out of the project file, and puts each where it was when it was included, as far as that can be done (see the top of this file).
    /// The user is told of the tables that could not be.
    pub(super) fn take_tables_out_of_the_project_file(&mut self, names: &[String]) {
        let project_directory = self.project_directory_for_tables();
        let mut problems = Vec::new();
        let mut took_out_any = false;
        let mut done: Vec<&String> = Vec::new();
        for name in names {
            if done.contains(&name) {
                continue;
            }
            done.push(name);
            let Some(resource) = self.document.project.resources().get(name) else {
                continue;
            };
            if !resource.is_included() {
                continue;
            }
            let Some(bytes) = resource.bytes().cloned() else {
                problems.push(format!("{name}: the project does not have it just now"));
                continue;
            };
            let link = resource.link().clone();
            let places = places_it_was_at(&link, project_directory.as_deref());
            let same_already = |place: &Path| std::fs::read(place).is_ok_and(|there| there == *bytes);
            match put_back_where(&places, same_already, |file| file.write_all(&bytes)) {
                Ok(place) => {
                    let relative_path = project_directory
                        .as_deref()
                        .and_then(|folder| relative_path_between(folder, &place))
                        .or(link.relative_path);
                    self.document.project.keep_a_resource_at(
                        name,
                        SourceMediaLink {
                            absolute_path: place,
                            relative_path,
                            included_as: None,
                        },
                    );
                    took_out_any = true;
                }
                Err(problem) => problems.push(format!("{name}: {problem}")),
            }
        }
        if took_out_any {
            self.note_unsaved_change();
        }
        if !problems.is_empty() {
            self.show_message(
                if problems.len() == 1 {
                    "The table could not be taken out of the project file"
                } else {
                    "Some tables could not be taken out of the project file"
                },
                problems.join("\n"),
            );
        }
    }
}
