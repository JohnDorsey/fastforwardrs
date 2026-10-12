//! Saving and loading projects as pretty, human-readable JSON.
//!
//! A project file never contains the videos or sounds themselves. It links to each source file
//! twice: by absolute path, and by a path relative to the folder holding the project file as a
//! backup for when the project and the files have been moved together.

mod document;
mod drawers;
mod legacy;
mod relative_path;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

use crate::atomic_file::write_file_atomically;
use crate::project::{
    Angle, AnimationTarget, CanvasArea, Clip, ClipId, ClipKind, CrateClip, Drawer, Effect, EffectKind, Effects,
    FontSize, ImageClip, MediaClip, Placement, Project, ProjectSettings, PropertyId, PropertyValue,
    SourceAudioProperties, SourceImageProperties, SourceMedia, SourceMediaLink, SourceMediaProperties,
    SourceVideoProperties, SpedUpClipSound, SpeedIndicatorLayout, TextClip, TextObject, Track, TrackHeight, TrackId,
    ValueKind,
};
use document::{
    AnimationDocument, CanvasAreaDocument, ClipDocument, ColourDocument, EffectDocument, KeyframeValueDocument,
    PlacementDocument, ProjectDocument, ProjectSettingsDocument, RackDocument, SourceDocument, SpeedIndicatorDocument,
    TrackDocument,
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
/// is animated). Version 7 keeps keyframes in racks, in the drawers of the clips and of the tracks, which may be of any clip or
/// track and not only the one that has the drawer (a file of version 6 has an `animation` object in each clip and track: it is read
/// as a rack of the keyframes of its own properties in its own drawer), and has keyframe crates. Version 8 added the effects of
/// clips and of tracks (a file of version 7 has none).
pub const FILE_FORMAT_VERSION: u32 = 8;

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

/// The effect that `document` describes, if it describes one: of a kind there is, with a value of the kind it has for each of its
/// properties, or none (which are what it begins with).
fn effect_from_document(document: EffectDocument) -> Result<Effect, String> {
    let kind = EffectKind::from_name(&document.effect)
        .ok_or_else(|| format!("\"{}\" is not an effect there is", document.effect))?;
    if document.values.is_empty() {
        return Ok(Effect::new(kind));
    }
    if document.values.len() != kind.properties().len() {
        return Err(format!(
            "the values of the {kind} effect should be a list of {}, and the file's list is of {}",
            kind.properties().len(),
            document.values.len()
        ));
    }
    let values = document
        .values
        .into_iter()
        .zip(kind.properties())
        .map(|(value, property)| {
            value_from_document(value, *property).map_err(|reason| format!("the {kind} effect: {reason}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Effect::with_values(kind, values).ok_or_else(|| format!("the values of the {kind} effect are not the kind it has"))
}

/// The effects that the documents describe, in their order. It is an error to put an effect on a clip that cannot have it (a
/// sound), or to put on one twice.
fn effects_from_documents(
    documents: Vec<EffectDocument>,
    can_have: impl Fn(EffectKind) -> bool,
) -> Result<Effects, String> {
    let mut effects = Effects::none();
    for document in documents {
        let effect = effect_from_document(document)?;
        let kind = effect.kind();
        if !can_have(kind) {
            return Err(format!("it cannot have the {kind} effect"));
        }
        if !effects.put_on(effect) {
            return Err(format!("it has the {kind} effect twice"));
        }
    }
    Ok(effects)
}

/// The documents of the effects `effects`, in their order, with the values left out of each that has the values it begins with.
fn effect_documents_of(effects: &Effects) -> Vec<EffectDocument> {
    effects
        .iter()
        .map(|effect| EffectDocument {
            effect: effect.kind().name().to_owned(),
            values: if effect.values() == effect.kind().default_values() {
                Vec::new()
            } else {
                effect.values().iter().map(value_document_of).collect()
            },
        })
        .collect()
}

/// A clip as a document describes it.
struct ClipFromDocument {
    /// How many empty frames come before it.
    gap_before_frames: u64,
    clip: Clip,
    /// The effects the clip has.
    effects: Effects,
    /// The keyframes that a file of a version before 7 had in an `animation` object of the clip.
    legacy_animation: AnimationDocument,
    /// The racks that are in the drawer of the clip.
    racks: Vec<RackDocument>,
}

/// The clip a document describes: with what is in its drawer as it is in the document, which is made into racks when what the
/// racks can be of is known (see [`project_from_document`]).
fn clip_from_document(document: ClipDocument) -> Result<ClipFromDocument, String> {
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
            drawer,
            effects,
        } => {
            let media = MediaClip::new(source_identifier, first_source_frame, end_source_frame, speed_factor)
                .map_err(|error| error.to_string())?;
            Ok(ClipFromDocument {
                gap_before_frames,
                clip: Clip::Video(
                    media
                        .with_gain(gain_decibels)
                        .with_muted(muted)
                        .with_placement(placement.map(placement_from_document)),
                ),
                effects: effects_from_documents(effects, |kind| kind.applies_to_a_clip_of_kind(ClipKind::Video))?,
                legacy_animation: animation,
                racks: drawer,
            })
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
            drawer,
        } => {
            let media = MediaClip::new(source_identifier, first_source_frame, end_source_frame, speed_factor)
                .map_err(|error| error.to_string())?;
            Ok(ClipFromDocument {
                gap_before_frames,
                clip: Clip::Audio(media.with_gain(gain_decibels).with_muted(muted)),
                effects: Effects::none(),
                legacy_animation: animation,
                racks: drawer,
            })
        }
        ClipDocument::Image {
            gap_before_frames,
            source_identifier,
            duration_in_frames,
            placement,
            animation,
            drawer,
            effects,
        } => {
            let image = ImageClip::new(source_identifier, duration_in_frames).map_err(|error| error.to_string())?;
            Ok(ClipFromDocument {
                gap_before_frames,
                clip: Clip::Image(image.with_placement(placement.map(placement_from_document))),
                effects: effects_from_documents(effects, |kind| kind.applies_to_a_clip_of_kind(ClipKind::Image))?,
                legacy_animation: animation,
                racks: drawer,
            })
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
            drawer,
            effects,
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
            Ok(ClipFromDocument {
                gap_before_frames,
                clip: Clip::Text(clip),
                effects: effects_from_documents(effects, |kind| kind.applies_to_a_clip_of_kind(ClipKind::Text))?,
                legacy_animation: animation,
                racks: drawer,
            })
        }
        ClipDocument::Crate {
            gap_before_frames,
            duration_in_frames,
            drawer,
        } => {
            let crate_clip = CrateClip::new(duration_in_frames).map_err(|error| error.to_string())?;
            Ok(ClipFromDocument {
                gap_before_frames,
                clip: Clip::Crate(crate_clip),
                effects: Effects::none(),
                legacy_animation: AnimationDocument::default(),
                racks: drawer,
            })
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

    // The clips are made first: the racks of keyframes may be of any clip of any track, and which clips there are, of what
    // kind, and how long each lasts is what the racks are checked against.
    let mut loaded_tracks = Vec::with_capacity(document.tracks.len());
    for (track_number, track) in document.tracks.into_iter().enumerate() {
        let TrackDocument {
            title,
            gain_decibels,
            speed_factor,
            muted,
            height_in_points,
            animation,
            drawer,
            effects,
            clips: clip_documents,
        } = track;
        let effects = effects_from_documents(effects, EffectKind::applies_to_a_track)
            .map_err(|reason| invalid_project(format!("track number {}: {reason}", track_number + 1)))?;
        let mut clips = Vec::with_capacity(clip_documents.len());
        for (clip_number, clip) in clip_documents.into_iter().enumerate() {
            clips.push(clip_from_document(clip).map_err(|reason| {
                invalid_project(format!(
                    "clip number {} of track number {}: {reason}",
                    clip_number + 1,
                    track_number + 1
                ))
            })?);
        }
        let parts = Track::new(TrackId::new(u32::try_from(track_number).unwrap_or(u32::MAX)), title)
            .with_gain(gain_decibels)
            .with_speed(speed_factor)
            .with_muted(muted)
            .with_height(TrackHeight::from_points(height_in_points));
        loaded_tracks.push((parts.with_effects(effects), animation, drawer, clips));
    }
    let contents = drawers::FileContents {
        kinds_of_the_clips: loaded_tracks
            .iter()
            .map(|(_, _, _, clips)| clips.iter().map(|clip| clip.clip.kind()).collect())
            .collect(),
        effects_of_the_clips: loaded_tracks
            .iter()
            .map(|(_, _, _, clips)| clips.iter().map(|clip| clip.effects.clone()).collect())
            .collect(),
        effects_of_the_tracks: loaded_tracks
            .iter()
            .map(|(track, ..)| track.effects().clone())
            .collect(),
    };

    let mut tracks = Vec::with_capacity(loaded_tracks.len());
    for (track_number, (track, animation, drawer, clips)) in loaded_tracks.into_iter().enumerate() {
        let mut next_rack_number = 0;
        let drawer_of_the_track = drawers::drawer_from_documents(
            AnimationTarget::Track(track.id()),
            track_number,
            drawer,
            animation,
            None,
            &contents,
            &mut next_rack_number,
        )
        .map_err(|reason| invalid_project(format!("track number {}: {reason}", track_number + 1)))?;
        let mut entries = Vec::with_capacity(clips.len());
        let mut effects_of_the_clips = Vec::with_capacity(clips.len());
        for (clip_number, loaded) in clips.into_iter().enumerate() {
            let clip_id = ClipId::new(track.id(), u32::try_from(clip_number).unwrap_or(u32::MAX));
            let length = loaded.clip.timeline_frame_count(track.speed());
            let drawer_of_the_clip = drawers::drawer_from_documents(
                AnimationTarget::Clip(clip_id),
                track_number,
                loaded.racks,
                loaded.legacy_animation,
                Some(length),
                &contents,
                &mut next_rack_number,
            )
            .map_err(|reason| {
                invalid_project(format!(
                    "clip number {} of track number {}: {reason}",
                    clip_number + 1,
                    track_number + 1
                ))
            })?;
            entries.push((loaded.gap_before_frames, loaded.clip, drawer_of_the_clip));
            effects_of_the_clips.push(loaded.effects);
        }
        tracks.push(
            track
                .with_drawer(drawer_of_the_track)
                .with_clips_and_drawers(entries)
                .with_effects_on_its_clips(effects_of_the_clips),
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

    let positions = drawers::Positions::of(project.tracks());
    let tracks = project
        .tracks()
        .iter()
        .map(|track| {
            let racks_of = |owner: AnimationTarget, drawer: &Drawer| {
                drawers::rack_documents_of(owner, drawer, &positions).map_err(invalid_project)
            };
            Ok(TrackDocument {
                title: track.title().to_owned(),
                gain_decibels: track.gain(),
                speed_factor: track.speed(),
                muted: track.is_muted(),
                height_in_points: track.height().points(),
                animation: AnimationDocument::default(),
                drawer: racks_of(AnimationTarget::Track(track.id()), track.drawer())?,
                effects: effect_documents_of(track.effects()),
                clips: track
                    .entries()
                    .iter()
                    .map(|entry| {
                        Ok(clip_document_of(
                            entry.gap_before_frames(),
                            entry.clip(),
                            racks_of(AnimationTarget::Clip(entry.id()), entry.drawer())?,
                            effect_documents_of(entry.effects()),
                        ))
                    })
                    .collect::<Result<Vec<_>, ProjectFileError>>()?,
            })
        })
        .collect::<Result<Vec<_>, ProjectFileError>>()?;

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

/// The document of `clip`, with the racks of its drawer and its effects. (A sound and a crate have no effects: those it is
/// given are not written.)
fn clip_document_of(
    gap_before_frames: u64,
    clip: &Clip,
    drawer: Vec<RackDocument>,
    effects: Vec<EffectDocument>,
) -> ClipDocument {
    let animation = AnimationDocument::default();
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
            drawer,
            effects,
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
            drawer,
        },
        Clip::Image(image) => ClipDocument::Image {
            gap_before_frames,
            source_identifier: image.source(),
            duration_in_frames: image.duration_in_frames(),
            placement: image.placement().map(placement_document_of),
            animation,
            drawer,
            effects,
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
            drawer,
            effects,
        },
        Clip::Crate(crate_clip) => ClipDocument::Crate {
            gap_before_frames,
            duration_in_frames: crate_clip.duration_in_frames(),
            drawer,
        },
    }
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
mod effects_tests;
#[cfg(test)]
mod tests;
