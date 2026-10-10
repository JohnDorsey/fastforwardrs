//! Saving and loading projects as pretty, human-readable JSON.
//!
//! A project file never contains the video itself. It links to each source video twice: by
//! absolute path, and by a path relative to the folder holding the project file as a backup for
//! when the project and the video have been moved together.

mod document;
mod relative_path;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::atomic_file::write_file_atomically;
use crate::project::{
    Clip, InconsistentProject, Project, ProjectSettings, SourceVideo, SourceVideoLink, SourceVideoProperties,
    SpeedIndicatorLayout, VideoTrack,
};
use document::{
    ClipDocument, ProjectDocument, ProjectSettingsDocument, SourceVideoDocument, SpeedIndicatorDocument,
    VideoTrackDocument,
};
pub use relative_path::relative_path_between;

/// The value of the `file_format` field that marks a JSON file as a fastforwardrs project.
pub const FILE_FORMAT_NAME: &str = "fastforwardrs-project";

/// The version of the file layout this build writes. It also reads every older version: version 1
/// is the same as 2 but has no `settings`, which means that the project follows every global one.
pub const FILE_FORMAT_VERSION: u32 = 2;

#[derive(Debug, Error)]
pub enum ProjectFileError {
    #[error("the project file {path} could not be read: {source}")]
    CouldNotRead { path: PathBuf, source: io::Error },
    #[error("the project file {path} could not be written: {source}")]
    CouldNotWrite { path: PathBuf, source: io::Error },
    #[error("this is not a readable fastforwardrs project file: {reason}")]
    NotAProjectFile { reason: String },
    #[error(
        "this project file was made by a newer version of fastforwardrs (file format version {found}, but this version understands up to {supported})"
    )]
    NewerFileFormat { found: u32, supported: u32 },
    #[error("the project file contradicts itself: {reason}")]
    InvalidProject { reason: String },
    #[error("the path {path:?} cannot be saved in a project file because it is not valid Unicode text")]
    PathIsNotUnicode { path: PathBuf },
}

/// Loads a project from a file.
pub fn read_project_file(path: &Path) -> Result<Project, ProjectFileError> {
    let text = fs::read_to_string(path).map_err(|source| ProjectFileError::CouldNotRead {
        path: path.to_owned(),
        source,
    })?;
    project_from_json_text(&text)
}

/// Saves a project to a file, replacing any file already there. The new contents are written to a
/// temporary file next to the destination and then renamed over it, so a failure part-way can
/// never leave an existing project file half-written.
pub fn write_project_file(project: &Project, destination: &Path) -> Result<(), ProjectFileError> {
    let could_not_write = |source: io::Error| ProjectFileError::CouldNotWrite {
        path: destination.to_owned(),
        source,
    };
    let absolute_destination = std::path::absolute(destination).map_err(could_not_write)?;
    let project_directory = absolute_destination
        .parent()
        .ok_or_else(|| could_not_write(io::Error::other("the destination has no containing folder")))?;
    let text = project_to_json_text(project, project_directory)?;
    write_file_atomically(&absolute_destination, &text).map_err(could_not_write)
}

/// Turns JSON text into a project, checking that it really is a project file this version can
/// understand and that its parts agree with each other.
pub fn project_from_json_text(text: &str) -> Result<Project, ProjectFileError> {
    let document: ProjectDocument = serde_json::from_str(text).map_err(|error| ProjectFileError::NotAProjectFile {
        reason: error.to_string(),
    })?;
    if document.file_format != FILE_FORMAT_NAME {
        return Err(ProjectFileError::NotAProjectFile {
            reason: format!(
                "its file_format is \"{}\", not \"{FILE_FORMAT_NAME}\"",
                document.file_format
            ),
        });
    }
    if document.file_format_version > FILE_FORMAT_VERSION {
        return Err(ProjectFileError::NewerFileFormat {
            found: document.file_format_version,
            supported: FILE_FORMAT_VERSION,
        });
    }
    project_from_document(document)
}

/// Turns a project into pretty JSON text.
///
/// `project_directory` is the folder the text will be saved in. It is used to work out each source
/// video's relative path. If a video's file cannot currently be found, the relative path the
/// project already knows is kept as it is, so that a project saved while its video is missing does
/// not lose its backup link.
pub fn project_to_json_text(project: &Project, project_directory: &Path) -> Result<String, ProjectFileError> {
    let document = document_from_project(project, project_directory)?;
    let mut text = serde_json::to_string_pretty(&document).map_err(|error| ProjectFileError::InvalidProject {
        reason: error.to_string(),
    })?;
    text.push('\n');
    Ok(text)
}

fn project_from_document(document: ProjectDocument) -> Result<Project, ProjectFileError> {
    let source_videos = document
        .source_videos
        .into_iter()
        .map(|source_video| SourceVideo {
            identifier: source_video.identifier,
            link: SourceVideoLink {
                absolute_path: PathBuf::from(source_video.absolute_path),
                relative_path: source_video.relative_path.map(PathBuf::from),
            },
            properties: SourceVideoProperties {
                frame_rate: source_video.frame_rate,
                total_frame_count: source_video.total_frame_count,
                display_width_in_pixels: source_video.display_width_in_pixels,
                display_height_in_pixels: source_video.display_height_in_pixels,
                has_audio: source_video.has_audio,
            },
        })
        .collect();

    let clips = document
        .video_track
        .clips
        .into_iter()
        .enumerate()
        .map(|(clip_number, clip)| {
            Clip::new(
                clip.source_video_identifier,
                clip.first_source_frame,
                clip.end_source_frame,
                clip.speed_factor,
            )
            .map_err(|error| ProjectFileError::InvalidProject {
                reason: format!("clip number {}: {error}", clip_number + 1),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let speed_indicator_layout = SpeedIndicatorLayout::new(
        document.speed_indicator.left_fraction_of_frame_width,
        document.speed_indicator.top_fraction_of_frame_height,
        document.speed_indicator.height_fraction_of_frame_height,
    );

    let settings = document
        .settings
        .map_or(ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS, |settings| {
            ProjectSettings {
                autosave: settings.autosave,
                mute_sped_up_clips: settings.mute_sped_up_clips,
            }
        });

    Project::from_parts(source_videos, VideoTrack::new(clips), speed_indicator_layout)
        .map(|project| project.with_settings(settings))
        .map_err(|inconsistency: InconsistentProject| ProjectFileError::InvalidProject {
            reason: inconsistency.to_string(),
        })
}

fn document_from_project(project: &Project, project_directory: &Path) -> Result<ProjectDocument, ProjectFileError> {
    let source_videos = project
        .source_videos()
        .iter()
        .map(|source_video| {
            let relative_path = if source_video.link.absolute_path.exists() {
                relative_path_between(project_directory, &source_video.link.absolute_path)
            } else {
                source_video.link.relative_path.clone()
            };
            Ok(SourceVideoDocument {
                identifier: source_video.identifier,
                absolute_path: path_as_unicode_text(&source_video.link.absolute_path)?,
                relative_path: relative_path
                    .map(|path| relative_path_as_portable_text(&path))
                    .transpose()?,
                frame_rate: source_video.properties.frame_rate,
                total_frame_count: source_video.properties.total_frame_count,
                display_width_in_pixels: source_video.properties.display_width_in_pixels,
                display_height_in_pixels: source_video.properties.display_height_in_pixels,
                has_audio: source_video.properties.has_audio,
            })
        })
        .collect::<Result<Vec<_>, ProjectFileError>>()?;

    let clips = project
        .video_track()
        .clips()
        .iter()
        .map(|clip| ClipDocument {
            source_video_identifier: clip.source_video_identifier(),
            first_source_frame: clip.first_source_frame(),
            end_source_frame: clip.end_source_frame(),
            speed_factor: clip.speed_factor(),
        })
        .collect();

    let layout = project.speed_indicator_layout();
    let settings = project.settings();
    Ok(ProjectDocument {
        file_format: FILE_FORMAT_NAME.to_owned(),
        file_format_version: FILE_FORMAT_VERSION,
        source_videos,
        video_track: VideoTrackDocument { clips },
        speed_indicator: SpeedIndicatorDocument {
            left_fraction_of_frame_width: layout.left_fraction_of_frame_width(),
            top_fraction_of_frame_height: layout.top_fraction_of_frame_height(),
            height_fraction_of_frame_height: layout.height_fraction_of_frame_height(),
        },
        settings: (settings != ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS).then_some(ProjectSettingsDocument {
            autosave: settings.autosave,
            mute_sped_up_clips: settings.mute_sped_up_clips,
        }),
    })
}

fn path_as_unicode_text(path: &Path) -> Result<String, ProjectFileError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| ProjectFileError::PathIsNotUnicode { path: path.to_owned() })
}

/// Writes a relative path with `/` between its parts whatever the operating system, so that a
/// project saved on Windows still finds its video when opened on macOS or Linux (and back).
fn relative_path_as_portable_text(relative_path: &Path) -> Result<String, ProjectFileError> {
    let mut parts = Vec::new();
    for component in relative_path.components() {
        let part = component
            .as_os_str()
            .to_str()
            .ok_or_else(|| ProjectFileError::PathIsNotUnicode {
                path: relative_path.to_owned(),
            })?;
        parts.push(part);
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests;
