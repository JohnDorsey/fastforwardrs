// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Saving and loading projects as pretty, human-readable JSON, or as the same JSON in an archive.
//!
//! A project file does not contain the videos or sounds themselves, unless the user chose to include some of them: it links to
//! each source file twice, by absolute path, and by a path relative to the folder holding the project file as a backup for when
//! the project and the files have been moved together. A project file that has files included is an archive (a zip file that
//! nothing in is compressed, called `.ffrs`: see [`archive`]), with the JSON in one entry and each file included in another; the
//! JSON then says of each of those files the name of its entry, besides the two paths it had when it was included. A plain `.json`
//! project file cannot have files included.

pub mod archive;
mod document;
mod drawers;
mod effects;
mod groups;
mod legacy;
mod legacy_effects;
mod plugin_libraries;
mod relative_path;
mod resources;

pub use resources::{ResourceFileProblem, link_of_a_new_resource, read_a_resource_file};

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

use crate::atomic_file::write_file_atomically;
use crate::media::MediaLocation;
use crate::project::{
    AdjustmentClip, Angle, AnimationTarget, CanvasArea, ChainEntry, Clip, ClipId, ClipKind, CrateClip, Drawer, Effect,
    EffectInstanceId, EffectKind, Effects, FontSize, GeneratorClip, ImageClip, InputColourOverride, MediaClip,
    Placement, Project, ProjectSettings, PropertyValue, SourceAudioProperties, SourceImageProperties, SourceMedia,
    SourceMediaIdentifier, SourceMediaLink, SourceMediaProperties, SourceVideoProperties, SpedUpClipSound,
    SpeedIndicatorLayout, TextClip, TextColour, TextObject, Track, TrackHeight, TrackId, ValueKind,
    values_of_a_text_object,
};
use archive::{ArchiveError, ArchiveState, FileToInclude};
use document::{
    AnimationDocument, BadgeDocument, CanvasAreaDocument, ClipDocument, ColourDocument, InputColourDocument,
    KeyframeValueDocument, PlacementDocument, ProjectDocument, ProjectSettingsDocument, RackDocument, SourceDocument,
    SpeedIndicatorDocument, TrackDocument,
};
use effects::{
    EffectDocument, KindsOfEffects, chain_of_the_effects_from_documents, effect_documents_of, plugin_documents_of,
};
use legacy::LegacyProjectDocument;
pub use relative_path::relative_path_between;

/// What the name of a project file ends with (after the dot) when it is an archive, which can have files included in it.
pub const ARCHIVE_FILE_EXTENSION: &str = "ffrs";

/// What the name of a project file ends with when it is plain JSON, which cannot.
pub const PLAIN_FILE_EXTENSION: &str = "json";

/// Where the files that are included in a project can be read from, when it is saved: for each source file that is included, where
/// the same bytes as it are. (A stretch of the archive that the project was loaded from, if it was in it; or the file that it was, if it
/// was included just now.)
pub type IncludedFiles = HashMap<SourceMediaIdentifier, MediaLocation>;

/// How a project that is saved as an archive is written when there is an archive at the place already.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HowToWrite {
    /// All of it is new, and replaces what is there. (There is nothing of no use in it, and the files in it are written again.)
    Anew,
    /// If an archive is there, what is new is added to it: the files that are in it already stay where they are (nothing writes them
    /// again, and what reads them where they are goes on reading them), and so does what is no longer of use, which takes room until the
    /// archive is written anew. If there is none, it is written anew.
    AddingToWhatIsThere,
}

/// What was written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Written {
    /// What the archive has in it now and where, if what was written is an archive.
    pub archive: Option<ArchiveState>,
}

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
/// clips and of tracks (a file of version 7 has none). Version 9 added the files that are included in the project file (a file of
/// version 8 has none). Version 10 added groups, and with them the badges in the chains of effects of clips and groups (a file of version 9
/// has none: it is a project with no group, in which each clip has its effects and then those of its track, as it always had). Version 11
/// opens up the effects (see `effects.rs` of this module): each type of effect that a project uses has a snapshot of its descriptor in `plugins`, an effect
/// says the key and the version of its type, and its values are by the id of each; each effect is named in its chain, so that a chain can have
/// several effects of one kind, and the racks of the keyframes of the values of effects say which effect they are of and call the value by
/// its id; and what the program does not know of an effect, a type of effect or a value is kept. (A file of version 10 calls an effect by the
/// name that the program had for it, `hue shift`, has the values in a list, has at most one effect of each kind and names none: they are
/// named in the order they are in, and a rack is of the effect that has a value of the name it says: see `legacy_effects.rs`. So is a file of
/// version 11 as it was written while the effects were being named.) Version 12 added the colours: what the user says the colours of a source file are (`colour`
/// of a video or a picture file, each part of it left out when it is automatic), and the three colour settings (the display transform, the working colour space and
/// the output colour space) that a project may have set for itself. A file of version 11 has none of them: the user said nothing, and the project follows the global
/// colour settings. Version 13 added what the effects need to be chosen and shown (a `category` for a type of effect in its snapshot, and a `scale` and a `control` for a
/// value of it), and the `enabled` flag of an effect (`false` for one that is switched off; left out when on). A file of version 12 has none of them: its effects are on, and its
/// snapshots say nothing of how a value is shown. Version 13 also added the `resources` of the project (the files that its effects use, such as a table of colours): see
/// `resources.rs`. A file of version 12 has none; and a setting for how they are kept, `resource_storage`, that a project may have set for itself. Version 14 added the
/// `plugin_libraries` of the project (the libraries of native code that its sound effects come from, linked or with their builds inside the archive): see `plugin_libraries.rs`. A file
/// of version 13 has none. Version 15 added the adjustment clip (`"kind": "adjustment"`): a clip with no picture of its own whose effects are done to everything behind it. A file of
/// version 14 has none. Version 16 added the `shader` of a snapshot of an effect (`plugins`): the language (`isf`) and the whole text of a shader that runs the effect, so that a project
/// carries what it uses. A file of version 15 has none. Version 17 added the `images` of that `shader`, when the shader brings images with it (an ISF file's `IMPORTED`): each a name
/// and the bytes of its file (a PNG or a JPEG) in base 64. A file of version 16 has none. **Version 18 made a text a clip with a content, as a generator is** (phase 9): what a text says, its
/// font, its size and its colour are the values `text`, `font`, `size` and `colour` of its effect `builtin:text`, which is the first of its chain and which the clip names in `content`
/// (an earlier file has the four in the clip itself, and keyframes of them, in racks or in the `animation` of a file of version 6 or earlier, that call them `"text"`, `"font"`, `"font size"`
/// and `"colour"`: it is read as a text whose chain has that effect first, with those values and those keyframes, and the badges of its chain after it).
///
/// **Version 19 added the `state` of an effect** (phase 10): what the plugin of the effect saved of what is not one of its values, which its window made it say, in base 64 (left out when
/// it saved none). A file of version 18 has none.
pub const FILE_FORMAT_VERSION: u32 = 19;

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
    #[error(
        "the project has files included in the project file, which only a .ffrs file can have, and {path} is not one: save it as a .ffrs file, or take the files out of the project file first"
    )]
    IncludedFilesNeedAnArchive { path: PathBuf },
    #[error("the file {file_name} is to be in the project file, and there is nowhere to read it from just now")]
    IncludedFileNotAvailable { file_name: String },
    #[error("it was stopped before it was done")]
    Stopped,
}

impl From<ArchiveError> for ProjectFileError {
    fn from(error: ArchiveError) -> Self {
        match error {
            ArchiveError::CouldNotRead { path, source } => Self::CouldNotRead { path, source },
            ArchiveError::CouldNotWrite { path, source } => Self::CouldNotWrite { path, source },
            ArchiveError::Stopped => Self::Stopped,
            other @ (ArchiveError::Unreadable { .. } | ArchiveError::NoSuchEntry { .. }) => Self::NotAProjectFile {
                reason: other.to_string(),
            },
        }
    }
}

/// Whether a project file with the name `path` is an archive when it is written: by what its name ends with (whatever the case of
/// the letters). A file that is read is known by what is in it, and not by its name.
pub fn is_the_name_of_an_archive(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(ARCHIVE_FILE_EXTENSION))
}

/// Whether the project in a project file may have files included in it: only if it is an archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FilesIncluded {
    Allowed,
    NotAllowed,
}

/// Loads a project from a file: an archive (a `.ffrs` file, known by what is in it), or plain JSON.
pub fn read_project_file(path: &Path) -> Result<Project, ProjectFileError> {
    if archive::is_an_archive(path) {
        let text = archive::read_project_text(path)?;
        let mut project = project_from_text(&text, FilesIncluded::Allowed)?;
        check_that_the_archive_has_the_files_included_in_it(path, &project)?;
        resources::check_that_the_archive_has_the_entries(path, &project)?;
        resources::load_the_bytes(&mut project, path, true)?;
        let libraries = plugin_libraries::from_text(&text, Some(path))?;
        return Ok(project.with_plugin_libraries(libraries));
    }
    let text = fs::read_to_string(path).map_err(|source| ProjectFileError::CouldNotRead {
        path: path.to_owned(),
        source,
    })?;
    let mut project = project_from_json_text(&text)?;
    resources::load_the_bytes(&mut project, path, false)?;
    Ok(project)
}

/// Saves a project to a file, replacing any file already there: an archive if the name ends in `.ffrs`, and plain JSON otherwise.
/// The new contents are written to a temporary file next to the destination and then renamed over it, so a failure part-way can
/// never leave an existing project file half-written. (A project that has files included needs the places to read them from: see
/// [`write_project_file_including`].)
pub fn write_project_file(project: &Project, destination: &Path) -> Result<(), ProjectFileError> {
    write_project_file_including(project, destination, &IncludedFiles::new(), HowToWrite::Anew).map(|_| ())
}

/// Like [`write_project_file`], for a project that may have files included in it: `files` says where to read each of them from, and
/// `how` how an archive that is there already is written. Only an archive can have files included: nothing is written to a file that
/// is not one, if the project has any.
pub fn write_project_file_including(
    project: &Project,
    destination: &Path,
    files: &IncludedFiles,
    how: HowToWrite,
) -> Result<Written, ProjectFileError> {
    plan_the_write(project, destination, files, how)?.carry_out(&mut |_| Ok(()))
}

/// What writing a project file is going to do, worked out before anything is written, so that how much of a file is to be copied is
/// known first (and a copy that takes a long time can be done in the background, and be stopped).
#[derive(Debug)]
pub struct WritePlan {
    /// Where it is written (as an absolute path).
    destination: PathBuf,
    /// The text of the project.
    text: String,
    what: WhatToWrite,
}

#[derive(Debug)]
enum WhatToWrite {
    PlainJson,
    /// An archive that is written anew, with these files in it.
    ArchiveAnew {
        files: Vec<FileToInclude>,
    },
    /// The archive that is there is added to: the files that are in it and are named in `keep` stay, and `new_files` are put in.
    AddToTheArchive {
        keep: Vec<String>,
        new_files: Vec<FileToInclude>,
    },
    /// The archive that is there is written again without the garbage in it (see [`archive::compact_archive`]), which is as long as
    /// copying the `bytes_in_use`. Nothing in the project is written: the text of the project in the archive stays as it is.
    CompactTheArchive {
        bytes_in_use: u64,
    },
}

impl WritePlan {
    /// How many bytes of files are to be copied into the archive. (The text of the project is not counted.)
    pub fn bytes_to_copy(&self) -> u64 {
        let files = match &self.what {
            WhatToWrite::PlainJson => return 0,
            WhatToWrite::ArchiveAnew { files } => files,
            WhatToWrite::AddToTheArchive { new_files, .. } => new_files,
            WhatToWrite::CompactTheArchive { bytes_in_use } => return *bytes_in_use,
        };
        files.iter().map(|file| file.source.length()).sum()
    }

    /// Writes it. `on_progress` is told how many bytes of files have been copied so far, and an error from it (of kind
    /// [`io::ErrorKind::Interrupted`]) stops the writing, which leaves what is there as it was.
    pub fn carry_out(self, on_progress: archive::OnProgress<'_>) -> Result<Written, ProjectFileError> {
        let could_not_write = |source: io::Error| ProjectFileError::CouldNotWrite {
            path: self.destination.clone(),
            source,
        };
        let state = match self.what {
            WhatToWrite::PlainJson => {
                write_file_atomically(&self.destination, &self.text).map_err(could_not_write)?;
                return Ok(Written { archive: None });
            }
            WhatToWrite::ArchiveAnew { files } => {
                // (The text of the project is written first, and is not counted in what is to be copied: nor is it told.)
                let text_bytes = self.text.len() as u64;
                let mut only_of_the_files = |copied: u64| on_progress(copied.saturating_sub(text_bytes));
                archive::write_archive_telling(&self.destination, &self.text, &files, &mut only_of_the_files)?;
                archive::state_of_archive(&self.destination)?
            }
            WhatToWrite::AddToTheArchive { keep, new_files } => {
                let text_bytes = self.text.len() as u64;
                let mut only_of_the_files = |copied: u64| on_progress(copied.saturating_sub(text_bytes));
                archive::append_to_archive(&self.destination, &self.text, &keep, &new_files, &mut only_of_the_files)?
            }
            WhatToWrite::CompactTheArchive { .. } => archive::compact_archive(&self.destination, on_progress)?,
        };
        Ok(Written { archive: Some(state) })
    }
}

/// Works out what writing the archive at `archive` again without its garbage is going to do, if it is worth doing (see
/// [`archive::ArchiveState::is_worth_compacting`]): `None` if `archive` is not an archive that can be read, or has too little garbage.
/// **The files in it are in other places afterwards**, so nothing may be reading them in place when the plan is carried out.
pub fn plan_the_compaction(archive: &Path) -> Option<WritePlan> {
    let absolute_archive = std::path::absolute(archive).ok()?;
    let state = archive::state_of_archive(&absolute_archive).ok()?;
    state.is_worth_compacting().then(|| WritePlan {
        destination: absolute_archive,
        text: String::new(),
        what: WhatToWrite::CompactTheArchive {
            bytes_in_use: state.bytes_in_use,
        },
    })
}

/// Works out what writing `project` to `destination` is going to do (see [`write_project_file_including`]).
pub fn plan_the_write(
    project: &Project,
    destination: &Path,
    files: &IncludedFiles,
    how: HowToWrite,
) -> Result<WritePlan, ProjectFileError> {
    let could_not_write = |source: io::Error| ProjectFileError::CouldNotWrite {
        path: destination.to_owned(),
        source,
    };
    let absolute_destination = std::path::absolute(destination).map_err(could_not_write)?;
    let project_directory = absolute_destination
        .parent()
        .ok_or_else(|| could_not_write(io::Error::other("the destination has no containing folder")))?;
    let included_sources: Vec<&SourceMedia> = project
        .sources()
        .iter()
        .filter(|source| source.link.is_included())
        .collect();
    let mut included_resources = resources::entries_to_write(project)?;
    // (The builds of the plugin libraries that are inside the file are written the same way as the resources are: their bytes are in memory.)
    included_resources.extend(plugin_libraries::entries_to_write(project)?);
    if !is_the_name_of_an_archive(destination) {
        if !included_sources.is_empty() || !included_resources.is_empty() {
            return Err(ProjectFileError::IncludedFilesNeedAnArchive {
                path: destination.to_owned(),
            });
        }
        let text = project_to_json_text(project, project_directory)?;
        return Ok(WritePlan {
            destination: absolute_destination,
            text,
            what: WhatToWrite::PlainJson,
        });
    }
    let entries = entries_of_the_included_sources(&included_sources)?;
    let text = project_to_json_text(project, project_directory)?;
    let what = if how == HowToWrite::AddingToWhatIsThere && archive::is_an_archive(&absolute_destination) {
        let state = archive::state_of_archive(&absolute_destination)?;
        let (in_it_already, not_yet): (Vec<_>, Vec<_>) = entries
            .iter()
            .cloned()
            .partition(|(entry_name, _)| state.entries.contains_key(entry_name));
        let (resources_in_it_already, resources_not_yet): (Vec<_>, Vec<_>) = included_resources
            .into_iter()
            .partition(|(entry_name, _)| state.entries.contains_key(entry_name));
        let keep = in_it_already
            .into_iter()
            .map(|(entry_name, _)| entry_name)
            .chain(resources_in_it_already.into_iter().map(|(entry_name, _)| entry_name))
            .collect();
        let mut new_files = files_to_include_in_an_archive(&not_yet, files)?;
        new_files.extend(files_of_the_resources(resources_not_yet));
        WhatToWrite::AddToTheArchive { keep, new_files }
    } else {
        let mut all = files_to_include_in_an_archive(&entries, files)?;
        all.extend(files_of_the_resources(included_resources));
        WhatToWrite::ArchiveAnew { files: all }
    };
    Ok(WritePlan {
        destination: absolute_destination,
        text,
        what,
    })
}

/// The files to put in an archive for the resources that are included: each its bytes, in memory.
fn files_of_the_resources(entries: Vec<resources::EntryToWrite>) -> Vec<FileToInclude> {
    entries
        .into_iter()
        .map(|(entry_name, bytes)| FileToInclude {
            entry_name,
            source: archive::FileSource::Bytes(bytes),
        })
        .collect()
}

/// The entry of each of the sources that are included. Their entries must be names that an archive has for the files included in it,
/// and no two of them the same.
fn entries_of_the_included_sources<'a>(
    included_sources: &[&'a SourceMedia],
) -> Result<Vec<(String, &'a SourceMedia)>, ProjectFileError> {
    let mut entry_names = HashSet::new();
    let mut entries = Vec::with_capacity(included_sources.len());
    for source in included_sources {
        let entry_name = source.link.included_as.clone().unwrap_or_default();
        if !archive::is_a_resource_entry_name(&entry_name) {
            return Err(invalid_project(format!(
                "the file {} is to be included as \"{entry_name}\", which is not a name for it in a project file",
                file_name_of(source)
            )));
        }
        if !entry_names.insert(entry_name.clone()) {
            return Err(invalid_project(format!(
                "more than one file is to be included as \"{entry_name}\""
            )));
        }
        entries.push((entry_name, *source));
    }
    Ok(entries)
}

fn file_name_of(source: &SourceMedia) -> String {
    source
        .link
        .absolute_path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

/// The entry and the place to read the file from, for each of `entries`.
fn files_to_include_in_an_archive(
    entries: &[(String, &SourceMedia)],
    files: &IncludedFiles,
) -> Result<Vec<FileToInclude>, ProjectFileError> {
    entries
        .iter()
        .map(|(entry_name, source)| {
            let location = files
                .get(&source.identifier)
                .ok_or_else(|| ProjectFileError::IncludedFileNotAvailable {
                    file_name: file_name_of(source),
                })?;
            Ok(FileToInclude {
                entry_name: entry_name.clone(),
                source: location.clone().into(),
            })
        })
        .collect()
}

/// Whether the archive has an entry for each file that the project in it says is included, which it must for the project to be
/// opened: what it says is looked up in the archive, and made the name of a file, so it is not believed before it is checked.
fn check_that_the_archive_has_the_files_included_in_it(path: &Path, project: &Project) -> Result<(), ProjectFileError> {
    for source in project.sources() {
        let Some(entry_name) = &source.link.included_as else {
            continue;
        };
        match archive::facts_of_entry(path, entry_name) {
            Ok(_) => {}
            Err(ArchiveError::NoSuchEntry { .. }) => {
                return Err(invalid_project(format!(
                    "the file {} is said to be included in the project file as \"{entry_name}\", which the project file does not have",
                    source.link.absolute_path.display()
                )));
            }
            Err(other) => return Err(other.into()),
        }
    }
    Ok(())
}

/// What every version of the file has at its start: enough to tell which version it is.
#[derive(Deserialize)]
struct FileHeader {
    file_format: String,
    file_format_version: u32,
}

/// Turns JSON text into a project, checking that it really is a project file this version can
/// understand and that its parts agree with each other. (It is the text of a plain project file: one that says that a file is
/// included in it is refused, as only an archive has files in it.)
pub fn project_from_json_text(text: &str) -> Result<Project, ProjectFileError> {
    project_from_text(text, FilesIncluded::NotAllowed)
}

fn project_from_text(text: &str, files_included: FilesIncluded) -> Result<Project, ProjectFileError> {
    let mut project = project_from_text_of_any_kind(text)?;
    // (The libraries of a plain file are the linked ones; a file that says a plugin is inside it is refused. The libraries of an archive are read by `read_project_file`, which
    // has the archive to read their builds from.)
    if files_included == FilesIncluded::NotAllowed {
        project = project.with_plugin_libraries(plugin_libraries::from_text(text, None)?);
    }
    for source in project.sources() {
        let Some(entry_name) = &source.link.included_as else {
            continue;
        };
        if files_included == FilesIncluded::NotAllowed {
            return Err(invalid_project(format!(
                "the file {} is said to be included in the project file, and only a project file that is a .ffrs file can have files included in it",
                source.link.absolute_path.display()
            )));
        }
        if !archive::is_a_resource_entry_name(entry_name) {
            return Err(invalid_project(format!(
                "the file {} is said to be included in the project file as \"{entry_name}\", which is not a name that a file included in it has",
                source.link.absolute_path.display()
            )));
        }
    }
    if files_included == FilesIncluded::NotAllowed
        && let Some(resource) = project.resources().iter().find(|resource| resource.is_included())
    {
        return Err(invalid_project(format!(
            "the resource \"{}\" is said to be included in the project file, and only a project file that is a .ffrs file can have files included in it",
            resource.name()
        )));
    }
    Ok(project)
}

fn project_from_text_of_any_kind(text: &str) -> Result<Project, ProjectFileError> {
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
            display_transform: settings.display_transform,
            working_colour_space: settings.working_colour_space,
            output_colour_space: settings.output_colour_space,
            resource_storage: settings.resource_storage,
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

fn link_from(absolute_path: String, relative_path: Option<String>, included_as: Option<String>) -> SourceMediaLink {
    SourceMediaLink {
        absolute_path: PathBuf::from(absolute_path),
        relative_path: relative_path.map(PathBuf::from),
        included_as,
    }
}

fn source_from_document(document: SourceDocument) -> SourceMedia {
    match document {
        SourceDocument::Video {
            identifier,
            absolute_path,
            relative_path,
            included_as,
            frame_rate,
            total_frame_count,
            display_width_in_pixels,
            display_height_in_pixels,
            has_audio,
            colour,
        } => SourceMedia {
            colour: colour.map(InputColourOverride::from).unwrap_or_default(),
            ..SourceMedia::video(
                identifier,
                link_from(absolute_path, relative_path, included_as),
                SourceVideoProperties {
                    frame_rate,
                    total_frame_count,
                    display_width_in_pixels,
                    display_height_in_pixels,
                    has_audio,
                },
            )
        },
        SourceDocument::Audio {
            identifier,
            absolute_path,
            relative_path,
            included_as,
            duration_in_microseconds,
        } => SourceMedia::audio(
            identifier,
            link_from(absolute_path, relative_path, included_as),
            SourceAudioProperties {
                duration_in_microseconds,
            },
        ),
        SourceDocument::Image {
            identifier,
            absolute_path,
            relative_path,
            included_as,
            display_width_in_pixels,
            display_height_in_pixels,
            colour,
        } => SourceMedia {
            colour: colour.map(InputColourOverride::from).unwrap_or_default(),
            ..SourceMedia::image(
                identifier,
                link_from(absolute_path, relative_path, included_as),
                SourceImageProperties {
                    display_width_in_pixels,
                    display_height_in_pixels,
                },
            )
        },
    }
}

/// What the value `document` is, for a value of the kind `value_kind`.
fn value_from_document(document: KeyframeValueDocument, value_kind: ValueKind) -> Result<PropertyValue, String> {
    let wrong_kind = |found: &str| {
        let wanted = match value_kind {
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
    match (value_kind, document) {
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

/// A clip as a document describes it.
struct ClipFromDocument {
    /// How many empty frames come before it.
    gap_before_frames: u64,
    clip: Clip,
    /// The effects the clip has, with the badges that are in its chain.
    effects: Effects,
    /// The keyframes that a file of a version before 7 had in an `animation` object of the clip.
    legacy_animation: AnimationDocument,
    /// The racks that are in the drawer of the clip.
    racks: Vec<RackDocument>,
}

/// The clip a document describes: with what is in its drawer as it is in the document, which is made into racks when what the
/// racks can be of is known (see [`project_from_document`]). `group_count` is how many groups the file has, which the badges in the
/// chain of the clip are of, and `kinds` are the types of effect that the effects of the clip can be of.
fn clip_from_document(
    document: ClipDocument,
    group_count: usize,
    kinds: &KindsOfEffects,
) -> Result<ClipFromDocument, String> {
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
            badges,
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
                effects: groups::chain_from_documents(
                    effects,
                    badges,
                    kinds,
                    |kind| kind.applies_to_a_clip_of_kind(ClipKind::Video),
                    group_count,
                )?,
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
            effects,
            badges,
        } => {
            let media = MediaClip::new(source_identifier, first_source_frame, end_source_frame, speed_factor)
                .map_err(|error| error.to_string())?;
            Ok(ClipFromDocument {
                gap_before_frames,
                clip: Clip::Audio(media.with_gain(gain_decibels).with_muted(muted)),
                effects: groups::chain_from_documents(
                    effects,
                    badges,
                    kinds,
                    |kind| kind.applies_to_a_clip_of_kind(ClipKind::Audio),
                    group_count,
                )?,
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
            badges,
        } => {
            let image = ImageClip::new(source_identifier, duration_in_frames).map_err(|error| error.to_string())?;
            Ok(ClipFromDocument {
                gap_before_frames,
                clip: Clip::Image(image.with_placement(placement.map(placement_from_document))),
                effects: groups::chain_from_documents(
                    effects,
                    badges,
                    kinds,
                    |kind| kind.applies_to_a_clip_of_kind(ClipKind::Image),
                    group_count,
                )?,
                legacy_animation: animation,
                racks: drawer,
            })
        }
        ClipDocument::Text {
            gap_before_frames,
            duration_in_frames,
            content,
            text,
            font_family,
            font_size_fraction_of_frame_height,
            colour,
            area,
            rotation_degrees,
            animation,
            drawer,
            effects,
            badges,
        } => {
            let placement = Placement::new(area_from_document(&area), Angle::from_degrees(rotation_degrees));
            let chain = groups::chain_from_documents(
                effects,
                badges,
                kinds,
                |kind| kind.applies_to_a_clip_of_kind(ClipKind::Text),
                group_count,
            )?;
            let (content, chain) = match content {
                Some(content) => {
                    let content = EffectInstanceId::new(content);
                    if !chain
                        .instance(content)
                        .is_some_and(|effect| effect.kind() == EffectKind::TEXT)
                    {
                        return Err(format!(
                            "the text is made by the effect number {}, which is not a text effect that it has",
                            content.number()
                        ));
                    }
                    (content, chain)
                }
                // A file of format 17 or earlier: what the text says is in the clip, and becomes the values of a text effect, **the first of its chain** (the badges that were before the
                // first effect come after it: what they do is done to what the text makes). Its keyframes are of the properties of the clip, which are the values of that effect now: see
                // `drawers::FileContents`.
                None => {
                    let object = TextObject {
                        text: text.ok_or("a text that has no content must say what it says")?,
                        font_family: font_family.unwrap_or_default(),
                        font_size: FontSize::from_fraction_of_picture_height(
                            font_size_fraction_of_frame_height
                                .unwrap_or(FontSize::DEFAULT.fraction_of_picture_height()),
                        ),
                        colour: colour.map_or(TextColour::WHITE, |colour| colour.0),
                        ..TextObject::default()
                    };
                    let effect = Effect::with_values(EffectKind::TEXT, values_of_a_text_object(&object).to_vec())
                        .ok_or("the values of a text could not be made into the text effect")?
                        .named(chain.next_instance());
                    let content = effect.instance();
                    let mut entries = chain.entries();
                    entries.insert(0, ChainEntry::Effect(effect));
                    let chain =
                        Effects::from_entries(entries).ok_or("the effects of a text could not be put in a chain")?;
                    (content, chain)
                }
            };
            let clip = TextClip::new(duration_in_frames, content)
                .map_err(|error| error.to_string())?
                .with_placement(placement);
            Ok(ClipFromDocument {
                gap_before_frames,
                clip: Clip::Text(clip),
                effects: chain,
                legacy_animation: animation,
                racks: drawer,
            })
        }
        ClipDocument::Generator {
            gap_before_frames,
            duration_in_frames,
            content,
            area,
            rotation_degrees,
            drawer,
            effects,
            badges,
        } => {
            let content = EffectInstanceId::new(content);
            let clip = GeneratorClip::new(duration_in_frames, content)
                .map_err(|error| error.to_string())?
                .with_placement(Placement::new(
                    area_from_document(&area),
                    Angle::from_degrees(rotation_degrees),
                ));
            let effects = groups::chain_from_documents(
                effects,
                badges,
                kinds,
                |kind| kind.applies_to_a_clip_of_kind(ClipKind::Generator),
                group_count,
            )?;
            if effects.instance(content).is_none() {
                return Err(format!(
                    "the generator clip is made by the effect number {}, which it does not have",
                    content.number()
                ));
            }
            Ok(ClipFromDocument {
                gap_before_frames,
                clip: Clip::Generator(clip),
                effects,
                legacy_animation: AnimationDocument::default(),
                racks: drawer,
            })
        }
        ClipDocument::Adjustment {
            gap_before_frames,
            duration_in_frames,
            drawer,
            effects,
            badges,
        } => {
            let adjustment = AdjustmentClip::new(duration_in_frames).map_err(|error| error.to_string())?;
            Ok(ClipFromDocument {
                gap_before_frames,
                clip: Clip::Adjustment(adjustment),
                effects: groups::chain_from_documents(
                    effects,
                    badges,
                    kinds,
                    |kind| kind.applies_to_a_clip_of_kind(ClipKind::Adjustment),
                    group_count,
                )?,
                legacy_animation: AnimationDocument::default(),
                racks: drawer,
            })
        }
        ClipDocument::Crate {
            gap_before_frames,
            duration_in_frames,
            drawer,
            badges,
        } => {
            let crate_clip = CrateClip::new(duration_in_frames).map_err(|error| error.to_string())?;
            Ok(ClipFromDocument {
                gap_before_frames,
                clip: Clip::Crate(crate_clip),
                effects: groups::chain_from_documents(Vec::new(), badges, kinds, |_| false, group_count)?,
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
    let group_count = document.groups.len();
    let kinds = KindsOfEffects::from_documents(document.plugins).map_err(invalid_project)?;
    let resources = resources::from_documents(document.resources).map_err(invalid_project)?;

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
        let effects = chain_of_the_effects_from_documents(effects, &kinds, EffectKind::applies_to_a_track)
            .map_err(|reason| invalid_project(format!("track number {}: {reason}", track_number + 1)))?;
        let mut clips = Vec::with_capacity(clip_documents.len());
        for (clip_number, clip) in clip_documents.into_iter().enumerate() {
            clips.push(clip_from_document(clip, group_count, &kinds).map_err(|reason| {
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
    let clips_in_each_track: Vec<usize> = loaded_tracks.iter().map(|(_, _, _, clips)| clips.len()).collect();
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
        contents_of_the_clips: loaded_tracks
            .iter()
            .map(|(_, _, _, clips)| clips.iter().map(|clip| clip.clip.content()).collect())
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

    let groups =
        groups::groups_from_documents(document.groups, &clips_in_each_track, &kinds).map_err(invalid_project)?;
    let layout = speed_indicator_from_document(&document.speed_indicator);
    Project::from_parts_and_groups(sources, tracks, layout, groups)
        .map(|project| {
            project
                .with_settings(settings_from_document(document.settings))
                .with_resources(resources)
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
                link_from(video.absolute_path, video.relative_path, None),
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

/// What a file says of the colours the user has said for a source: nothing, if they have said nothing.
fn colour_document_of(said: &InputColourOverride) -> Option<InputColourDocument> {
    (!said.is_automatic()).then(|| InputColourDocument::from(said))
}

fn document_from_project(project: &Project, project_directory: &Path) -> Result<ProjectDocument, ProjectFileError> {
    let sources = project
        .sources()
        .iter()
        .map(|source| {
            // (The places a file that is included was at are kept as they were when it was included: it is not where they say any
            // more, as far as the project knows, and they are what it is put back at.)
            let relative_path = if !source.link.is_included() && source.link.absolute_path.exists() {
                relative_path_between(project_directory, &source.link.absolute_path)
            } else {
                source.link.relative_path.clone()
            };
            let included_as = source.link.included_as.clone();
            let absolute_path = path_as_unicode_text(&source.link.absolute_path)?;
            let relative_path = relative_path
                .map(|path| relative_path_as_portable_text(&path))
                .transpose()?;
            Ok(match source.properties {
                SourceMediaProperties::Video(video) => SourceDocument::Video {
                    identifier: source.identifier,
                    absolute_path,
                    relative_path,
                    included_as,
                    frame_rate: video.frame_rate,
                    total_frame_count: video.total_frame_count,
                    display_width_in_pixels: video.display_width_in_pixels,
                    display_height_in_pixels: video.display_height_in_pixels,
                    has_audio: video.has_audio,
                    colour: colour_document_of(&source.colour),
                },
                SourceMediaProperties::Audio(audio) => SourceDocument::Audio {
                    identifier: source.identifier,
                    absolute_path,
                    relative_path,
                    included_as,
                    duration_in_microseconds: audio.duration_in_microseconds,
                },
                SourceMediaProperties::Image(image) => SourceDocument::Image {
                    identifier: source.identifier,
                    absolute_path,
                    relative_path,
                    included_as,
                    display_width_in_pixels: image.display_width_in_pixels,
                    display_height_in_pixels: image.display_height_in_pixels,
                    colour: colour_document_of(&source.colour),
                },
            })
        })
        .collect::<Result<Vec<_>, ProjectFileError>>()?;

    let positions = drawers::Positions::of(project.tracks());
    let group_numbers = groups::numbers_of_the_groups(project);
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
                            groups::badge_documents_of(entry.effects(), &group_numbers).map_err(invalid_project)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, ProjectFileError>>()?,
            })
        })
        .collect::<Result<Vec<_>, ProjectFileError>>()?;

    let groups = groups::group_documents_of(project, &positions, &group_numbers).map_err(invalid_project)?;
    let layout = project.speed_indicator_layout();
    let settings = project.settings();
    Ok(ProjectDocument {
        file_format: FILE_FORMAT_NAME.to_owned(),
        file_format_version: FILE_FORMAT_VERSION,
        sources,
        plugins: plugin_documents_of(project),
        resources: resources::documents_of(project, project_directory)?,
        plugin_libraries: plugin_libraries::documents_of(project, project_directory)?,
        tracks,
        groups,
        speed_indicator: SpeedIndicatorDocument {
            left_fraction_of_frame_width: layout.left_fraction_of_frame_width(),
            top_fraction_of_frame_height: layout.top_fraction_of_frame_height(),
            height_fraction_of_frame_height: layout.height_fraction_of_frame_height(),
        },
        settings: (settings != ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS).then_some(ProjectSettingsDocument {
            autosave: settings.autosave,
            sped_up_clip_sound: settings.sped_up_clip_sound,
            mute_sped_up_clips: None,
            display_transform: settings.display_transform,
            working_colour_space: settings.working_colour_space,
            output_colour_space: settings.output_colour_space,
            resource_storage: settings.resource_storage,
        }),
        frame_rate: project.chosen_frame_rate(),
    })
}

/// The document of `clip`, with the racks of its drawer, its effects, and the badges in its chain. (A sound and a crate have no effects:
/// those it is given are not written.)
fn clip_document_of(
    gap_before_frames: u64,
    clip: &Clip,
    drawer: Vec<RackDocument>,
    effects: Vec<EffectDocument>,
    badges: Vec<BadgeDocument>,
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
            badges,
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
            effects,
            badges,
        },
        Clip::Image(image) => ClipDocument::Image {
            gap_before_frames,
            source_identifier: image.source(),
            duration_in_frames: image.duration_in_frames(),
            placement: image.placement().map(placement_document_of),
            animation,
            drawer,
            effects,
            badges,
        },
        Clip::Text(text) => ClipDocument::Text {
            gap_before_frames,
            duration_in_frames: text.duration_in_frames(),
            content: Some(text.content().number()),
            text: None,
            font_family: None,
            font_size_fraction_of_frame_height: None,
            colour: None,
            area: area_document_of(text.placement().area),
            rotation_degrees: text.placement().rotation.degrees(),
            animation,
            drawer,
            effects,
            badges,
        },
        Clip::Crate(crate_clip) => ClipDocument::Crate {
            gap_before_frames,
            duration_in_frames: crate_clip.duration_in_frames(),
            drawer,
            badges,
        },
        Clip::Generator(generator) => ClipDocument::Generator {
            gap_before_frames,
            duration_in_frames: generator.duration_in_frames(),
            content: generator.content().number(),
            area: area_document_of(generator.placement().area),
            rotation_degrees: generator.placement().rotation.degrees(),
            drawer,
            effects,
            badges,
        },
        Clip::Adjustment(adjustment) => ClipDocument::Adjustment {
            gap_before_frames,
            duration_in_frames: adjustment.duration_in_frames(),
            drawer,
            effects,
            badges,
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
mod adjustment_clip_tests;
#[cfg(test)]
mod animation_tests;
#[cfg(test)]
mod audio_effects_tests;
#[cfg(test)]
mod built_in_effects_tests;
#[cfg(test)]
mod colour_tests;
#[cfg(test)]
mod effects_tests;
#[cfg(test)]
mod format_13_tests;
#[cfg(test)]
mod format_19_tests;
#[cfg(test)]
mod generator_clip_tests;
#[cfg(test)]
mod groups_tests;
#[cfg(test)]
mod included_files_tests;
#[cfg(test)]
mod old_text_files_tests;
#[cfg(test)]
mod plugin_libraries_tests;
#[cfg(test)]
mod plugins_tests;
#[cfg(test)]
mod resources_tests;
#[cfg(test)]
mod shader_image_tests;
#[cfg(test)]
mod shader_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod text_effect_tests;
