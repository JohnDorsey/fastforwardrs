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
    Angle, Animation, CanvasArea, Clip, FontSize, ImageClip, Keyframe, Keyframes, MediaClip, Placement, Project,
    ProjectSettings, PropertyId, PropertyValue, SourceAudioProperties, SourceImageProperties, SourceMedia,
    SourceMediaLink, SourceMediaProperties, SourceVideoProperties, SpedUpClipSound, SpeedIndicatorLayout, TextClip,
    TextObject, Track, TrackHeight, TrackId, ValueKind,
};
use document::{
    AnimationDocument, CanvasAreaDocument, ClipDocument, ColourDocument, KeyframeDocument, KeyframeValueDocument,
    PlacementDocument, ProjectDocument, ProjectSettingsDocument, SourceDocument, SpeedIndicatorDocument, TrackDocument,
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
/// and every text is upright). Version 5 added the frame rate chosen for the project, and with it
/// videos of other frame rates than the project's (a file of version 4 has no frame rate of its own:
/// the project follows its first video, as it always did, and all its videos run at the same rate).
/// Version 6 added keyframes, of the clips and of the tracks (a file of version 5 has none: nothing in it
/// is animated).
pub const FILE_FORMAT_VERSION: u32 = 6;

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

/// The keyframes that `document` has, which are of properties that `has_property` says yes to. (A property that is not one of
/// those the file is of has no keyframes: one that the file has all the same is a mistake in it, and is refused, as is a
/// value that is not the kind the property has.)
fn animation_from_document(
    document: AnimationDocument,
    has_property: impl Fn(PropertyId) -> bool,
    what_it_is_of: &str,
) -> Result<Animation, String> {
    let mut properties = Vec::with_capacity(document.0.len());
    for (name, keyframes) in document.0 {
        let property = PropertyId::from_name(&name)
            .ok_or_else(|| format!("the keyframes are of \"{name}\", which is not something that can be animated"))?;
        if !has_property(property) {
            return Err(format!(
                "\"{name}\" cannot be animated for {what_it_is_of}: it has none"
            ));
        }
        let mut converted = Vec::with_capacity(keyframes.len());
        for keyframe in keyframes {
            let value = value_from_document(keyframe.value, property)
                .map_err(|reason| format!("the keyframe of \"{name}\" at frame {}: {reason}", keyframe.frame))?;
            converted.push(Keyframe {
                frame: keyframe.frame,
                value,
            });
        }
        properties.push((property, Keyframes::from_unordered(converted)));
    }
    Ok(Animation::from_keyframes(properties))
}

/// What the value `document` is for `property`.
fn value_from_document(document: KeyframeValueDocument, property: PropertyId) -> Result<PropertyValue, String> {
    let wrong_kind = |found: &str| {
        let wanted = match property.kind() {
            ValueKind::Number => "a number",
            ValueKind::Pair => "two numbers, as [across, down] or [wide, tall]",
            ValueKind::Colour => "a colour such as \"#FFFFFFFF\"",
            ValueKind::Flag => "true or false",
            ValueKind::Words => "words in quotation marks",
        };
        format!("{found}, where {wanted} is wanted")
    };
    let finite = |number: f32| {
        if number.is_finite() {
            Ok(number)
        } else {
            Err("a number that is not a number".to_owned())
        }
    };
    match (property.kind(), document) {
        (ValueKind::Number, KeyframeValueDocument::Number(number)) => Ok(PropertyValue::Number(finite(number)?)),
        (ValueKind::Pair, KeyframeValueDocument::Pair([first, second])) => {
            Ok(PropertyValue::Pair(finite(first)?, finite(second)?))
        }
        (ValueKind::Colour, KeyframeValueDocument::Text(text)) => {
            Ok(PropertyValue::Colour(ColourDocument::from_text(&text)?.0))
        }
        (ValueKind::Flag, KeyframeValueDocument::Flag(flag)) => Ok(PropertyValue::Flag(flag)),
        (ValueKind::Words, KeyframeValueDocument::Text(words)) => Ok(PropertyValue::Words(words)),
        (_, KeyframeValueDocument::Flag(_)) => Err(wrong_kind("true or false")),
        (_, KeyframeValueDocument::Number(_)) => Err(wrong_kind("a number")),
        (_, KeyframeValueDocument::Pair(_)) => Err(wrong_kind("two numbers")),
        (_, KeyframeValueDocument::Text(_)) => Err(wrong_kind("words")),
    }
}

/// The clip a document describes, how many empty frames come before it, and its keyframes.
fn clip_from_document(document: ClipDocument) -> Result<(u64, Clip, Animation), String> {
    let kind = document_kind_of(&document);
    let (gap_before_frames, clip, animation) = clip_without_keyframes_from_document(document)?;
    let animation = animation_from_document(
        animation,
        |property| property.applies_to_a_clip_of_kind(kind),
        &format!("a {}", kind_name(kind)),
    )?;
    Ok((gap_before_frames, clip, animation))
}

fn document_kind_of(document: &ClipDocument) -> crate::project::ClipKind {
    use crate::project::ClipKind;
    match document {
        ClipDocument::Video { .. } => ClipKind::Video,
        ClipDocument::Audio { .. } => ClipKind::Audio,
        ClipDocument::Image { .. } => ClipKind::Image,
        ClipDocument::Text { .. } => ClipKind::Text,
    }
}

fn kind_name(kind: crate::project::ClipKind) -> &'static str {
    use crate::project::ClipKind;
    match kind {
        ClipKind::Video => "video",
        ClipKind::Audio => "sound",
        ClipKind::Image => "picture",
        ClipKind::Text => "text object",
    }
}

/// The clip a document describes, how many empty frames come before it, and the document of its keyframes.
fn clip_without_keyframes_from_document(document: ClipDocument) -> Result<(u64, Clip, AnimationDocument), String> {
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
            animation,
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
                animation,
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
            animation,
        } => {
            let media = MediaClip::new(source_identifier, first_source_frame, end_source_frame, speed_factor)
                .map_err(|error| error.to_string())?;
            Ok((
                gap_before_frames,
                Clip::Audio(media.with_gain(gain_decibels).with_muted(muted)),
                animation,
            ))
        }
        ClipDocument::Image {
            gap_before_frames,
            source_identifier,
            duration_in_frames,
            placement,
            animation,
        } => {
            let image = ImageClip::new(source_identifier, duration_in_frames).map_err(|error| error.to_string())?;
            Ok((
                gap_before_frames,
                Clip::Image(image.with_placement(placement.map(placement_from_document))),
                animation,
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
            animation,
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
            Ok((gap_before_frames, Clip::Text(clip), animation))
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
        let animation = animation_from_document(track.animation, PropertyId::applies_to_a_track, "a track")
            .map_err(|reason| invalid_project(format!("track number {}: {reason}", track_number + 1)))?;
        tracks.push(
            Track::new(
                TrackId::new(u32::try_from(track_number).unwrap_or(u32::MAX)),
                track.title,
            )
            .with_gain(track.gain_decibels)
            .with_speed(track.speed_factor)
            .with_muted(track.muted)
            .with_height(TrackHeight::from_points(track.height_in_points))
            .with_animation(animation)
            .with_animated_clips(clips),
        );
    }

    let layout = speed_indicator_from_document(&document.speed_indicator);
    Project::from_parts(sources, tracks, layout)
        .map(|project| {
            project
                .with_settings(settings_from_document(document.settings))
                .with_frame_rate(document.frame_rate)
        })
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
            animation: animation_document_of(track.animation()),
            clips: track
                .entries()
                .iter()
                .map(|entry| clip_document_of(entry.gap_before_frames(), entry.clip(), entry.animation()))
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
        frame_rate: project.chosen_frame_rate(),
    })
}

fn clip_document_of(gap_before_frames: u64, clip: &Clip, animation: &Animation) -> ClipDocument {
    let animation = animation_document_of(animation);
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
            animation,
        },
        Clip::Audio(media) => ClipDocument::Audio {
            gap_before_frames,
            source_identifier: media.source(),
            first_source_frame: media.first_source_frame(),
            end_source_frame: media.end_source_frame(),
            speed_factor: media.speed(),
            gain_decibels: media.gain(),
            muted: media.is_muted(),
            animation,
        },
        Clip::Image(image) => ClipDocument::Image {
            gap_before_frames,
            source_identifier: image.source(),
            duration_in_frames: image.duration_in_frames(),
            placement: image.placement().map(placement_document_of),
            animation,
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
            animation,
        },
    }
}

fn animation_document_of(animation: &Animation) -> AnimationDocument {
    AnimationDocument(
        animation
            .properties()
            .map(|(property, keyframes)| {
                (
                    property.name().to_owned(),
                    keyframes
                        .iter()
                        .map(|keyframe| KeyframeDocument {
                            frame: keyframe.frame,
                            value: value_document_of(&keyframe.value),
                        })
                        .collect(),
                )
            })
            .collect(),
    )
}

fn value_document_of(value: &PropertyValue) -> KeyframeValueDocument {
    match value {
        PropertyValue::Number(number) => KeyframeValueDocument::Number(*number),
        PropertyValue::Pair(first, second) => KeyframeValueDocument::Pair([*first, *second]),
        PropertyValue::Colour(colour) => KeyframeValueDocument::Text(ColourDocument(*colour).to_text()),
        PropertyValue::Flag(flag) => KeyframeValueDocument::Flag(*flag),
        PropertyValue::Words(words) => KeyframeValueDocument::Text(words.clone()),
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
mod animation_tests;
#[cfg(test)]
mod tests;
