//! Saving and loading projects as pretty, human-readable JSON.
//!
//! A project file never contains the videos or sounds themselves. It links to each source file
//! twice: by absolute path, and by a path relative to the folder holding the project file as a
//! backup for when the project and the files have been moved together.

mod document;
mod legacy;
mod relative_path;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

use crate::atomic_file::write_file_atomically;
use crate::project::{
    Angle, CanvasArea, Clip, FontSize, ImageClip, MediaClip, Placement, Project, ProjectSettings,
    SourceAudioProperties, SourceImageProperties, SourceMedia, SourceMediaLink, SourceMediaProperties,
    SourceVideoProperties, SpedUpClipSound, SpeedIndicatorLayout, TextClip, TextObject, Track, TrackHeight, TrackId,
};
use document::{
    CanvasAreaDocument, ClipDocument, ColourDocument, PlacementDocument, ProjectDocument, ProjectSettingsDocument,
    SourceDocument, SpeedIndicatorDocument, TrackDocument,
};
use legacy::LegacyProjectDocument;
pub use relative_path::relative_path_between;

/// The value of the `file_format` field that marks a JSON file as a fastforwardrs project.
pub const FILE_FORMAT_NAME: &str = "fastforwardrs-project";

/// The version of the file layout this build writes. It also reads every older version: versions
/// 1 and 2 had a list of videos and a single video track (and version 1 no `settings`) where
/// version 3 has a list of source files and a list of tracks, and version 4 added the height of
/// the row of each track on the timeline (a file of version 3 has none: its tracks are of the
/// normal height), pictures, and where a video, a picture or a text is on the picture and how far
/// it is turned (a file of version 3 has none of those: every video is fitted inside the picture
/// and every text is upright).
pub const FILE_FORMAT_VERSION: u32 = 4;

/// The newest version that has the old layout.
const NEWEST_LEGACY_FILE_FORMAT_VERSION: u32 = 2;

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

/// What every version of the file has at its start: enough to tell which version it is.
#[derive(Deserialize)]
struct FileHeader {
    file_format: String,
    file_format_version: u32,
}

/// Turns JSON text into a project, checking that it really is a project file this version can
/// understand and that its parts agree with each other.
pub fn project_from_json_text(text: &str) -> Result<Project, ProjectFileError> {
    let not_a_project_file = |error: serde_json::Error| ProjectFileError::NotAProjectFile {
        reason: error.to_string(),
    };
    let header: FileHeader = serde_json::from_str(text).map_err(not_a_project_file)?;
    if header.file_format != FILE_FORMAT_NAME {
        return Err(ProjectFileError::NotAProjectFile {
            reason: format!(
                "its file_format is \"{}\", not \"{FILE_FORMAT_NAME}\"",
                header.file_format
            ),
        });
    }
    if header.file_format_version > FILE_FORMAT_VERSION {
        return Err(ProjectFileError::NewerFileFormat {
            found: header.file_format_version,
            supported: FILE_FORMAT_VERSION,
        });
    }
    if header.file_format_version <= NEWEST_LEGACY_FILE_FORMAT_VERSION {
        let document: LegacyProjectDocument = serde_json::from_str(text).map_err(not_a_project_file)?;
        project_from_legacy_document(document)
    } else {
        let document: ProjectDocument = serde_json::from_str(text).map_err(not_a_project_file)?;
        project_from_document(document)
    }
}

/// Turns a project into pretty JSON text.
///
/// `project_directory` is the folder the text will be saved in. It is used to work out each source
/// file's relative path. If a file cannot currently be found, the relative path the project
/// already knows is kept as it is, so that a project saved while a file is missing does not lose
/// its backup link.
pub fn project_to_json_text(project: &Project, project_directory: &Path) -> Result<String, ProjectFileError> {
    let document = document_from_project(project, project_directory)?;
    let mut text = serde_json::to_string_pretty(&document).map_err(|error| ProjectFileError::InvalidProject {
        reason: error.to_string(),
    })?;
    text.push('\n');
    Ok(text)
}

fn invalid_project(reason: impl Into<String>) -> ProjectFileError {
    ProjectFileError::InvalidProject { reason: reason.into() }
}

fn settings_from_document(document: Option<ProjectSettingsDocument>) -> ProjectSettings {
    document.map_or(ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS, |settings| {
        ProjectSettings {
            autosave: settings.autosave,
            sped_up_clip_sound: settings.sped_up_clip_sound.or(settings
                .mute_sped_up_clips
                .map(SpedUpClipSound::from_the_old_mute_setting)),
        }
    })
}

fn speed_indicator_from_document(document: &SpeedIndicatorDocument) -> SpeedIndicatorLayout {
    SpeedIndicatorLayout::new(
        document.left_fraction_of_frame_width,
        document.top_fraction_of_frame_height,
        document.height_fraction_of_frame_height,
    )
}

fn link_from(absolute_path: String, relative_path: Option<String>) -> SourceMediaLink {
    SourceMediaLink {
        absolute_path: PathBuf::from(absolute_path),
        relative_path: relative_path.map(PathBuf::from),
    }
}

fn source_from_document(document: SourceDocument) -> SourceMedia {
    match document {
        SourceDocument::Video {
            identifier,
            absolute_path,
            relative_path,
            frame_rate,
            total_frame_count,
            display_width_in_pixels,
            display_height_in_pixels,
            has_audio,
        } => SourceMedia::video(
            identifier,
            link_from(absolute_path, relative_path),
            SourceVideoProperties {
                frame_rate,
                total_frame_count,
                display_width_in_pixels,
                display_height_in_pixels,
                has_audio,
            },
        ),
        SourceDocument::Audio {
            identifier,
            absolute_path,
            relative_path,
            duration_in_microseconds,
        } => SourceMedia::audio(
            identifier,
            link_from(absolute_path, relative_path),
            SourceAudioProperties {
                duration_in_microseconds,
            },
        ),
        SourceDocument::Image {
            identifier,
            absolute_path,
            relative_path,
            display_width_in_pixels,
            display_height_in_pixels,
        } => SourceMedia::image(
            identifier,
            link_from(absolute_path, relative_path),
            SourceImageProperties {
                display_width_in_pixels,
                display_height_in_pixels,
            },
        ),
    }
}

/// The clip a document describes, and how many empty frames come before it.
fn clip_from_document(document: ClipDocument) -> Result<(u64, Clip), String> {
    match document {
        ClipDocument::Video {
            gap_before_frames,
            source_identifier,
            first_source_frame,
            end_source_frame,
            speed_factor,
            gain_decibels,
            muted,
            placement,
        } => {
            let media = MediaClip::new(source_identifier, first_source_frame, end_source_frame, speed_factor)
                .map_err(|error| error.to_string())?;
            Ok((
                gap_before_frames,
                Clip::Video(
                    media
                        .with_gain(gain_decibels)
                        .with_muted(muted)
                        .with_placement(placement.map(placement_from_document)),
                ),
            ))
        }
        ClipDocument::Audio {
            gap_before_frames,
            source_identifier,
            first_source_frame,
            end_source_frame,
            speed_factor,
            gain_decibels,
            muted,
        } => {
            let media = MediaClip::new(source_identifier, first_source_frame, end_source_frame, speed_factor)
                .map_err(|error| error.to_string())?;
            Ok((
                gap_before_frames,
                Clip::Audio(media.with_gain(gain_decibels).with_muted(muted)),
            ))
        }
        ClipDocument::Image {
            gap_before_frames,
            source_identifier,
            duration_in_frames,
            placement,
        } => {
            let image = ImageClip::new(source_identifier, duration_in_frames).map_err(|error| error.to_string())?;
            Ok((
                gap_before_frames,
                Clip::Image(image.with_placement(placement.map(placement_from_document))),
            ))
        }
        ClipDocument::Text {
            gap_before_frames,
            duration_in_frames,
            text,
            font_family,
            font_size_fraction_of_frame_height,
            colour,
            area,
            rotation_degrees,
        } => {
            let object = TextObject {
                text,
                font_family,
                font_size: FontSize::from_fraction_of_picture_height(font_size_fraction_of_frame_height),
                colour: colour.0,
                area: area_from_document(&area),
                rotation: Angle::from_degrees(rotation_degrees),
            };
            let clip = TextClip::new(object, duration_in_frames).map_err(|error| error.to_string())?;
            Ok((gap_before_frames, Clip::Text(clip)))
        }
    }
}

fn area_from_document(area: &CanvasAreaDocument) -> CanvasArea {
    CanvasArea::new(
        area.left_fraction_of_frame_width,
        area.top_fraction_of_frame_height,
        area.width_fraction_of_frame_width,
        area.height_fraction_of_frame_height,
    )
}

fn area_document_of(area: CanvasArea) -> CanvasAreaDocument {
    CanvasAreaDocument {
        left_fraction_of_frame_width: area.left(),
        top_fraction_of_frame_height: area.top(),
        width_fraction_of_frame_width: area.width(),
        height_fraction_of_frame_height: area.height(),
    }
}

fn placement_from_document(placement: PlacementDocument) -> Placement {
    Placement::new(
        area_from_document(&placement.area),
        Angle::from_degrees(placement.rotation_degrees),
    )
}

fn placement_document_of(placement: Placement) -> PlacementDocument {
    PlacementDocument {
        area: area_document_of(placement.area),
        rotation_degrees: placement.rotation.degrees(),
    }
}

fn project_from_document(document: ProjectDocument) -> Result<Project, ProjectFileError> {
    let sources = document.sources.into_iter().map(source_from_document).collect();

    let mut tracks = Vec::with_capacity(document.tracks.len());
    for (track_number, track) in document.tracks.into_iter().enumerate() {
        let mut clips = Vec::with_capacity(track.clips.len());
        for (clip_number, clip) in track.clips.into_iter().enumerate() {
            clips.push(clip_from_document(clip).map_err(|reason| {
                invalid_project(format!(
                    "clip number {} of track number {}: {reason}",
                    clip_number + 1,
                    track_number + 1
                ))
            })?);
        }
        tracks.push(
            Track::new(
                TrackId::new(u32::try_from(track_number).unwrap_or(u32::MAX)),
                track.title,
            )
            .with_gain(track.gain_decibels)
            .with_speed(track.speed_factor)
            .with_muted(track.muted)
            .with_height(TrackHeight::from_points(track.height_in_points))
            .with_clips(clips),
        );
    }

    let layout = speed_indicator_from_document(&document.speed_indicator);
    Project::from_parts(sources, tracks, layout)
        .map(|project| project.with_settings(settings_from_document(document.settings)))
        .map_err(|inconsistency| invalid_project(inconsistency.to_string()))
}

/// A project from a file of version 1 or 2: all of its clips go on one track.
fn project_from_legacy_document(document: LegacyProjectDocument) -> Result<Project, ProjectFileError> {
    let sources = document
        .source_videos
        .into_iter()
        .map(|video| {
            SourceMedia::video(
                video.identifier,
                link_from(video.absolute_path, video.relative_path),
                SourceVideoProperties {
                    frame_rate: video.frame_rate,
                    total_frame_count: video.total_frame_count,
                    display_width_in_pixels: video.display_width_in_pixels,
                    display_height_in_pixels: video.display_height_in_pixels,
                    has_audio: video.has_audio,
                },
            )
        })
        .collect();

    let mut clips = Vec::with_capacity(document.video_track.clips.len());
    for (clip_number, clip) in document.video_track.clips.into_iter().enumerate() {
        let media = MediaClip::new(
            clip.source_video_identifier,
            clip.first_source_frame,
            clip.end_source_frame,
            clip.speed_factor,
        )
        .map_err(|error| invalid_project(format!("clip number {}: {error}", clip_number + 1)))?;
        clips.push((0, Clip::Video(media)));
    }
    let track = Track::new(TrackId::new(0), "track 0").with_clips(clips);

    Project::from_parts(
        sources,
        vec![track],
        speed_indicator_from_document(&document.speed_indicator),
    )
    .map(|project| project.with_settings(settings_from_document(document.settings)))
    .map_err(|inconsistency| invalid_project(inconsistency.to_string()))
}

fn document_from_project(project: &Project, project_directory: &Path) -> Result<ProjectDocument, ProjectFileError> {
    let sources = project
        .sources()
        .iter()
        .map(|source| {
            let relative_path = if source.link.absolute_path.exists() {
                relative_path_between(project_directory, &source.link.absolute_path)
            } else {
                source.link.relative_path.clone()
            };
            let absolute_path = path_as_unicode_text(&source.link.absolute_path)?;
            let relative_path = relative_path
                .map(|path| relative_path_as_portable_text(&path))
                .transpose()?;
            Ok(match source.properties {
                SourceMediaProperties::Video(video) => SourceDocument::Video {
                    identifier: source.identifier,
                    absolute_path,
                    relative_path,
                    frame_rate: video.frame_rate,
                    total_frame_count: video.total_frame_count,
                    display_width_in_pixels: video.display_width_in_pixels,
                    display_height_in_pixels: video.display_height_in_pixels,
                    has_audio: video.has_audio,
                },
                SourceMediaProperties::Audio(audio) => SourceDocument::Audio {
                    identifier: source.identifier,
                    absolute_path,
                    relative_path,
                    duration_in_microseconds: audio.duration_in_microseconds,
                },
                SourceMediaProperties::Image(image) => SourceDocument::Image {
                    identifier: source.identifier,
                    absolute_path,
                    relative_path,
                    display_width_in_pixels: image.display_width_in_pixels,
                    display_height_in_pixels: image.display_height_in_pixels,
                },
            })
        })
        .collect::<Result<Vec<_>, ProjectFileError>>()?;

    let tracks = project
        .tracks()
        .iter()
        .map(|track| TrackDocument {
            title: track.title().to_owned(),
            gain_decibels: track.gain(),
            speed_factor: track.speed(),
            muted: track.is_muted(),
            height_in_points: track.height().points(),
            clips: track
                .entries()
                .iter()
                .map(|entry| clip_document_of(entry.gap_before_frames(), entry.clip()))
                .collect(),
        })
        .collect();

    let layout = project.speed_indicator_layout();
    let settings = project.settings();
    Ok(ProjectDocument {
        file_format: FILE_FORMAT_NAME.to_owned(),
        file_format_version: FILE_FORMAT_VERSION,
        sources,
        tracks,
        speed_indicator: SpeedIndicatorDocument {
            left_fraction_of_frame_width: layout.left_fraction_of_frame_width(),
            top_fraction_of_frame_height: layout.top_fraction_of_frame_height(),
            height_fraction_of_frame_height: layout.height_fraction_of_frame_height(),
        },
        settings: (settings != ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS).then_some(ProjectSettingsDocument {
            autosave: settings.autosave,
            sped_up_clip_sound: settings.sped_up_clip_sound,
            mute_sped_up_clips: None,
        }),
    })
}

fn clip_document_of(gap_before_frames: u64, clip: &Clip) -> ClipDocument {
    match clip {
        Clip::Video(media) => ClipDocument::Video {
            gap_before_frames,
            source_identifier: media.source(),
            first_source_frame: media.first_source_frame(),
            end_source_frame: media.end_source_frame(),
            speed_factor: media.speed(),
            gain_decibels: media.gain(),
            muted: media.is_muted(),
            placement: media.placement().map(placement_document_of),
        },
        Clip::Audio(media) => ClipDocument::Audio {
            gap_before_frames,
            source_identifier: media.source(),
            first_source_frame: media.first_source_frame(),
            end_source_frame: media.end_source_frame(),
            speed_factor: media.speed(),
            gain_decibels: media.gain(),
            muted: media.is_muted(),
        },
        Clip::Image(image) => ClipDocument::Image {
            gap_before_frames,
            source_identifier: image.source(),
            duration_in_frames: image.duration_in_frames(),
            placement: image.placement().map(placement_document_of),
        },
        Clip::Text(text) => ClipDocument::Text {
            gap_before_frames,
            duration_in_frames: text.duration_in_frames(),
            text: text.object.text.clone(),
            font_family: text.object.font_family.clone(),
            font_size_fraction_of_frame_height: text.object.font_size.fraction_of_picture_height(),
            colour: ColourDocument(text.object.colour),
            area: area_document_of(text.object.area),
            rotation_degrees: text.object.rotation.degrees(),
        },
    }
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
