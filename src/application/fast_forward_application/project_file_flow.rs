//! Starting a new project, and loading and saving project files.

use std::path::{Path, PathBuf};

use super::{FastForwardApplication, OpeningPurpose, SourceStatus, ThumbnailState};
use crate::media::ThumbnailStrip;
use crate::project::Project;
use crate::project_file::{read_project_file, write_project_file};

/// What a project file's name ends with. The file is JSON, so that it can be read (and edited) by
/// anyone.
const PROJECT_FILE_EXTENSION: &str = "json";

impl FastForwardApplication {
    /// Starts an empty project and asks for the video it is to be made from.
    pub(super) fn start_new_project(&mut self) {
        self.replace_project(Project::empty(), None);
        self.import_video();
    }

    /// Asks which project file to load, and loads it.
    pub(super) fn choose_and_load_project(&mut self) {
        self.pause_playback_for_dialog();
        let Some(path) = rfd::FileDialog::new()
            .set_title("Load a project")
            .add_filter("Project files", &[PROJECT_FILE_EXTENSION])
            .add_filter("All files", &["*"])
            .pick_file()
        else {
            return;
        };
        self.load_project(path);
    }

    /// Loads the project file at `path`, and starts looking for its video.
    pub(super) fn load_project(&mut self, path: PathBuf) {
        match read_project_file(&path) {
            Ok(project) => {
                self.replace_project(project, Some(path));
                self.locate_source_video_of_loaded_project();
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
                let Some(path) = self.ask_where_to_save_project() else {
                    return;
                };
                path
            }
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

    fn ask_where_to_save_project(&self) -> Option<PathBuf> {
        let video_name = self
            .project
            .source_videos()
            .first()
            .and_then(|video| video.link.absolute_path.file_stem())
            .map_or_else(|| "project".to_owned(), |stem| stem.to_string_lossy().into_owned());
        let path = rfd::FileDialog::new()
            .set_title("Save the project")
            .set_file_name(format!("{video_name}.fastforwardrs.{PROJECT_FILE_EXTENSION}"))
            .add_filter("Project files", &[PROJECT_FILE_EXTENSION])
            .save_file()?;
        Some(if path.extension().is_none() {
            path.with_extension(PROJECT_FILE_EXTENSION)
        } else {
            path
        })
    }

    /// Makes `project` the project being edited, forgetting everything about the previous one.
    pub(super) fn replace_project(&mut self, project: Project, project_file_path: Option<PathBuf>) {
        self.opening = None;
        self.thumbnails = ThumbnailState {
            strip: ThumbnailStrip::default(),
            extraction: None,
        };
        self.picture_supplier.set_source(None);
        self.textures.clear();
        self.timeline_view.reset();
        self.project = project;
        self.project_file_path = project_file_path;
        self.note_everything_is_saved();
        self.source_status = SourceStatus::NoVideo;
        self.missing_source_warning_open = false;
        let frame_rate = self.frame_rate();
        self.clock.reset_for_frame_rate(frame_rate);
    }

    /// Starts looking for the video of a project that has just been loaded: where the project
    /// says it was, and then (as a backup) next to the project file.
    fn locate_source_video_of_loaded_project(&mut self) {
        let Some(video) = self.project.source_videos().first() else {
            return;
        };
        let project_directory = self.project_file_path.as_deref().and_then(Path::parent);
        let candidates = video.link.candidate_paths(project_directory);
        self.start_opening(OpeningPurpose::LocateForLoadedProject, candidates);
    }
}
