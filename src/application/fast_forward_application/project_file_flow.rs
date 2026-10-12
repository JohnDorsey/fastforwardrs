//! Starting a new project, and loading and saving project files.

use std::path::{Path, PathBuf};

use super::{FastForwardApplication, OpeningPurpose};
use crate::application::file_chooser::{FileKind, FileQuestion};
use crate::application::preview_panel::PreviewView;
use crate::project::Project;
use crate::project_file::{read_project_file, write_project_file};

/// What a project file's name ends with. The file is JSON, so that it can be read (and edited) by
/// anyone.
const PROJECT_FILE_EXTENSION: &str = "json";

impl FastForwardApplication {
    /// Starts an empty project and asks for the first video or sound to put in it.
    pub(super) fn start_new_project(&mut self) {
        self.replace_project(Project::empty(), None);
        self.import_file();
    }

    /// Asks which project file to load, and loads it.
    pub(super) fn choose_and_load_project(&mut self) {
        self.pause_playback_for_dialog();
        let Some(path) = self.file_chooser.choose_a_file_to_open(&FileQuestion {
            title: "Load a project",
            suggested_file_name: None,
            starting_directory: None,
            kinds: &[
                FileKind {
                    name: "Project files",
                    extensions: &[PROJECT_FILE_EXTENSION],
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
        let path = match &self.project_file_path {
            Some(path) => path.clone(),
            None => {
                self.pause_playback_for_dialog();
                let Some(path) = self.ask_where_to_save_project("Save the project") else {
                    return;
                };
                path
            }
        };
        self.save_project_to(path);
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

    /// Saves the project to `path`, which from then on is the project's file.
    pub fn save_project_to(&mut self, path: PathBuf) {
        match write_project_file(&self.project, &path) {
            Ok(()) => {
                self.project_file_path = Some(path);
                self.note_everything_is_saved();
            }
            Err(error) => self.show_message("The project could not be saved", error.to_string()),
        }
    }

    /// Asks where to save the project, offering the name of the file it has now, or if it has none the name of the first
    /// file in it, and the folder of the file it has now (if it has one).
    fn ask_where_to_save_project(&self, title: &str) -> Option<PathBuf> {
        let suggested_file_name = match self
            .project_file_path
            .as_deref()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
        {
            Some(name_of_the_file_it_has) => name_of_the_file_it_has,
            None => {
                let first_file_name = self
                    .project
                    .sources()
                    .first()
                    .and_then(|source| source.link.absolute_path.file_stem())
                    .map_or_else(|| "project".to_owned(), |stem| stem.to_string_lossy().into_owned());
                format!("{first_file_name}.fastforwardrs.{PROJECT_FILE_EXTENSION}")
            }
        };
        let path = self.file_chooser.choose_where_to_save(&FileQuestion {
            title,
            suggested_file_name: Some(&suggested_file_name),
            starting_directory: self.project_file_path.as_deref().and_then(Path::parent),
            kinds: &[FileKind {
                name: "Project files",
                extensions: &[PROJECT_FILE_EXTENSION],
            }],
        })?;
        Some(if path.extension().is_none() {
            path.with_extension(PROJECT_FILE_EXTENSION)
        } else {
            path
        })
    }

    /// Makes `project` the project being edited, forgetting everything about the previous one.
    pub(super) fn replace_project(&mut self, project: Project, project_file_path: Option<PathBuf>) {
        self.openings.clear();
        self.source_files.clear();
        self.selection.clear();
        self.preview_pictures.set_setup(None);
        self.textures.clear();
        self.timeline_view.reset();
        self.preview_view = PreviewView::default();
        self.project = project;
        self.project_file_path = project_file_path;
        self.history.reset();
        self.note_everything_is_saved();
        self.missing_sources_warning_open = false;
        self.clock.reset_for_frame_rate(self.project.frame_rate());
    }

    /// Starts looking for the source files of a project that has just been loaded: where the
    /// project says each was, and then (as a backup) relative to the project file.
    fn locate_source_files_of_loaded_project(&mut self) {
        let project_directory = self.project_file_path.as_deref().and_then(Path::parent);
        let wanted: Vec<_> = self
            .project
            .sources()
            .iter()
            .map(|source| (source.identifier, source.link.candidate_paths(project_directory)))
            .collect();
        for (identifier, candidates) in wanted {
            self.start_opening(OpeningPurpose::LocateForLoadedProject(identifier), candidates);
        }
    }
}
