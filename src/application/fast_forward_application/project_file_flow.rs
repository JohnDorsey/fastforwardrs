// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Starting a new project, and loading and saving project files.
//!
//! A project file is a `.ffrs` file, which is an archive that can have files included in it, or plain JSON, which is a file that
//! can be read (and edited) by anyone. The first is what is offered first and made when no name is given, for it can do all that the
//! other can and more; both are read, and the one that a project was loaded from or saved as is the one that it is saved as again.

use std::path::{Path, PathBuf};

use super::{FastForwardApplication, OpeningPurpose};
use crate::application::file_chooser::{FileKind, FileQuestion};
use crate::application::preview_panel::PreviewView;
use crate::project::Project;
use crate::project_file::{ARCHIVE_FILE_EXTENSION, PLAIN_FILE_EXTENSION, read_project_file};

impl FastForwardApplication {
    /// Starts an empty project and asks for the first video or sound to put in it.
    pub(super) fn start_new_project(&mut self) {
        self.replace_project(Project::empty(), None);
        self.link_and_import_file();
    }

    /// Asks which project file to load, and loads it.
    pub(super) fn choose_and_load_project(&mut self) {
        self.pause_playback_for_dialog();
        let Some(path) = self.environment.file_chooser.choose_a_file_to_open(&FileQuestion {
            title: "Load a project",
            suggested_file_name: None,
            starting_directory: None,
            kinds: &[
                FileKind {
                    name: "Project files",
                    extensions: &[ARCHIVE_FILE_EXTENSION, PLAIN_FILE_EXTENSION],
                },
                FileKind {
                    name: "All files",
                    extensions: &["*"],
                },
            ],
        }) else {
            return;
        };
        self.load_project(path);
    }

    /// Loads the project file at `path`, and starts looking for its source files.
    pub(super) fn load_project(&mut self, path: PathBuf) {
        match read_project_file(&path) {
            Ok(project) => {
                self.replace_project(project, Some(path));
                self.locate_source_files_of_loaded_project();
            }
            Err(error) => self.show_message("The project could not be loaded", error.to_string()),
        }
    }

    /// Saves the project to the file it came from, or asks where to save it the first time.
    pub(super) fn save_project(&mut self) {
        let path = match &self.document.project_file_path {
            Some(path) => path.clone(),
            None => {
                self.pause_playback_for_dialog();
                let Some(path) = self.ask_where_to_save_project("Save the project") else {
                    return;
                };
                path
            }
        };
        if self.save_project_to(path) {
            self.tidy_the_project_file_after_saving();
        }
    }

    /// Asks where to save the project, even if it has a file already (the dialog opens in the folder of that file, with its
    /// name offered), and saves it there: that file is the project's from then on, and the file it came from is left as it
    /// was. Nothing happens if no place is chosen.
    pub(super) fn save_project_as(&mut self) {
        self.pause_playback_for_dialog();
        let Some(path) = self.ask_where_to_save_project("Save the project as") else {
            return;
        };
        self.save_project_to(path);
    }

    /// The name that a project file is offered as when it is saved: the one it has now, or if it has none, the name of the first
    /// file in the project with `extension` after it (`.fastforwardrs.json` for a plain project file, as it always was).
    pub(super) fn suggested_name_for_a_project_file(&self, extension: &str) -> String {
        let first_file_name = self
            .document
            .project
            .sources()
            .first()
            .and_then(|source| source.link.absolute_path.file_stem())
            .map_or_else(|| "project".to_owned(), |stem| stem.to_string_lossy().into_owned());
        if extension == PLAIN_FILE_EXTENSION {
            format!("{first_file_name}.fastforwardrs.{extension}")
        } else {
            format!("{first_file_name}.{extension}")
        }
    }

    /// Asks where to save the project, offering the name of the file it has now, or if it has none the name of the first
    /// file in it with the ending of an archive, and the folder of the file it has now (if it has one). An archive is what is
    /// offered first, and what is made if the name chosen has no ending.
    fn ask_where_to_save_project(&self, title: &str) -> Option<PathBuf> {
        let suggested_file_name = match self
            .document
            .project_file_path
            .as_deref()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
        {
            Some(name_of_the_file_it_has) => name_of_the_file_it_has,
            None => self.suggested_name_for_a_project_file(ARCHIVE_FILE_EXTENSION),
        };
        let path = self.environment.file_chooser.choose_where_to_save(&FileQuestion {
            title,
            suggested_file_name: Some(&suggested_file_name),
            starting_directory: self.document.project_file_path.as_deref().and_then(Path::parent),
            kinds: &[
                FileKind {
                    name: "Project files (.ffrs)",
                    extensions: &[ARCHIVE_FILE_EXTENSION],
                },
                FileKind {
                    name: "Plain project files (.json)",
                    extensions: &[PLAIN_FILE_EXTENSION],
                },
            ],
        })?;
        Some(if path.extension().is_none() {
            path.with_extension(ARCHIVE_FILE_EXTENSION)
        } else {
            path
        })
    }

    /// Makes `project` the project being edited, forgetting everything about the previous one.
    pub(super) fn replace_project(&mut self, project: Project, project_file_path: Option<PathBuf>) {
        self.sources.openings.clear();
        self.sources.files.clear();
        self.interface.gesture_frame.forget();
        self.interface.selection.clear();
        self.playback.preview_pictures.set_setup(None);
        self.interface.let_go_of_the_pictures_of_files();
        self.interface.timeline_view.reset();
        self.interface.preview_view = PreviewView::default();
        self.document.project = project;
        self.document.project_file_path = project_file_path;
        self.document.history.reset();
        self.note_everything_is_saved();
        self.interface.missing_sources_warning_open = false;
        self.playback
            .clock
            .reset_for_frame_rate(self.document.project.frame_rate());
    }

    /// Starts looking for the source files of a project that has just been loaded: where the
    /// project says each was, and then (as a backup) relative to the project file; and for each that is included in the project
    /// file, taking it out of it (see [`Self::start_opening_an_included_file`]).
    fn locate_source_files_of_loaded_project(&mut self) {
        let project_directory = self.document.project_file_path.as_deref().and_then(Path::parent);
        let wanted: Vec<_> = self
            .document
            .project
            .sources()
            .iter()
            .map(|source| {
                (
                    source.identifier,
                    source.link.included_as.clone(),
                    source.link.candidate_paths(project_directory),
                )
            })
            .collect();
        for (identifier, included_as, candidates) in wanted {
            match (included_as, self.document.project_file_path.clone()) {
                (Some(entry_name), Some(archive)) => {
                    self.start_opening_an_included_file(identifier, archive, entry_name)
                }
                _ => self.start_opening(OpeningPurpose::LocateForLoadedProject(identifier), candidates),
            }
        }
    }
}
