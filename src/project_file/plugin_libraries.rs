// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugin libraries of a project in a project file (format 14; the design is `docs/engine-phase-7-design.md`, G15).
//!
//! The file lists the libraries: a **linked** one by its paths (as a source file is), an **embedded** one by its builds, each with the platform it is for, the SHA-256 of its
//! bytes, how many bytes it has, and the name of its entry in the archive (`plugin-libraries/` then the first 16 characters of the SHA-256, the platform and the name of the
//! library). **Only an archive can have a library inside it.** The bytes are read when the project is opened, because they are data until a person has said that they may be run;
//! **how much of them there can be is limited before any of them is read** (a file from elsewhere must not be able to ask for more than 256 MiB of memory), and a build whose bytes
//! are not what the file says is kept as it is and is never run.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;

use super::archive::{self, ArchiveError};
use super::document::{EmbeddedBuildDocument, EmbeddedLibraryDocument, LinkedLibraryDocument, PluginLibraryDocument};
use super::resources::EntryToWrite;
use super::{ProjectFileError, invalid_project, path_as_unicode_text, relative_path_as_portable_text};
use crate::digest::Sha256Digest;
use crate::project::{
    EmbeddedBuild, LinkedLibrary, MOST_BYTES_OF_A_BUILD, MOST_BYTES_OF_ALL_BUILDS, PluginLibraries, PluginLibrary,
    PluginLibraryKind, PluginLibrarySource, Project,
};

/// The name of the entry of a build.
pub(super) fn entry_name_of(build: &EmbeddedBuild, library_name: &str) -> String {
    archive::plugin_library_entry_name(&build.digest().short_hex(), build.platform(), library_name)
}

/// What a project file has of the libraries, and nothing else of it.
#[derive(Deserialize)]
struct OnlyThePluginLibraries {
    #[serde(default)]
    plugin_libraries: Vec<PluginLibraryDocument>,
}

/// The documents of the libraries of `project`, in their order.
pub(super) fn documents_of(
    project: &Project,
    project_directory: &Path,
) -> Result<Vec<PluginLibraryDocument>, ProjectFileError> {
    project
        .plugin_libraries()
        .iter()
        .map(|library| {
            let (linked, embedded) = match library.source() {
                PluginLibrarySource::Linked(link) => {
                    let relative_path = if link.absolute_path.exists() {
                        super::relative_path_between(project_directory, &link.absolute_path)
                    } else {
                        link.relative_path.clone()
                    };
                    (
                        Some(LinkedLibraryDocument {
                            absolute_path: path_as_unicode_text(&link.absolute_path)?,
                            relative_path: relative_path
                                .map(|path| relative_path_as_portable_text(&path))
                                .transpose()?,
                        }),
                        None,
                    )
                }
                PluginLibrarySource::Embedded(builds) => (
                    None,
                    Some(EmbeddedLibraryDocument {
                        builds: builds
                            .iter()
                            .map(|build| EmbeddedBuildDocument {
                                platform: build.platform().to_owned(),
                                sha256: build.digest().to_hex(),
                                size: build.bytes().len() as u64,
                                entry: entry_name_of(build, library.name()),
                            })
                            .collect(),
                    }),
                ),
            };
            Ok(PluginLibraryDocument {
                name: library.name().to_owned(),
                kind: library.kind().key().to_owned(),
                linked,
                embedded,
            })
        })
        .collect()
}

/// The libraries that the text of a project file lists. `archive` is the archive the text is from, if it is one: **only there can a library be embedded**, and its builds are read
/// from it (after every limit has been checked against what the file says and what the archive has, and before the first byte is read).
pub(super) fn from_text(text: &str, archive: Option<&Path>) -> Result<PluginLibraries, ProjectFileError> {
    let listed: OnlyThePluginLibraries =
        serde_json::from_str(text).map_err(|error| ProjectFileError::NotAProjectFile {
            reason: error.to_string(),
        })?;
    from_documents(listed.plugin_libraries, archive)
}

/// What a library in the file is, after what the file says of it has been checked and before any bytes are read.
enum Listed {
    Linked(PluginLibrary),
    Embedded {
        name: String,
        kind: PluginLibraryKind,
        /// The platform, the digest, the entry and the number of bytes that the file says.
        builds: Vec<(String, Sha256Digest, String, u64)>,
    },
}

fn from_documents(
    documents: Vec<PluginLibraryDocument>,
    archive: Option<&Path>,
) -> Result<PluginLibraries, ProjectFileError> {
    // First, what the file says, with the limits, before anything is read from the archive.
    let mut seen_entries = HashSet::new();
    let mut bytes_in_all = 0_u64;
    let mut listed = Vec::with_capacity(documents.len());
    for (number, document) in documents.into_iter().enumerate() {
        let describe = |reason: String| invalid_project(format!("plugin library number {}: {reason}", number + 1));
        let kind = PluginLibraryKind::from_key(&document.kind).ok_or_else(|| {
            describe(format!(
                "\"{}\" is a kind of plugin that this version does not use",
                document.kind
            ))
        })?;
        match (document.linked, document.embedded) {
            (Some(link), None) => {
                let link = LinkedLibrary {
                    absolute_path: PathBuf::from(link.absolute_path),
                    relative_path: link.relative_path.map(PathBuf::from),
                };
                let library = PluginLibrary::new(&document.name, kind, PluginLibrarySource::Linked(link))
                    .map_err(|problem| describe(problem.to_string()))?;
                listed.push(Listed::Linked(library));
            }
            (None, Some(embedded)) => {
                if archive.is_none() {
                    return Err(describe(format!(
                        "\"{}\" is said to be inside the project file, and only a project file that is a .ffrs file can have a plugin inside it",
                        document.name
                    )));
                }
                let mut builds = Vec::new();
                for build in embedded.builds {
                    if !archive::is_a_plugin_library_entry_name(&build.entry) {
                        return Err(describe(format!(
                            "the build for {} is said to be inside the project file as \"{}\", which is not a name that a plugin inside it has",
                            build.platform, build.entry
                        )));
                    }
                    if !seen_entries.insert(build.entry.clone()) {
                        return Err(describe(format!(
                            "more than one build is said to be inside the project file as \"{}\"",
                            build.entry
                        )));
                    }
                    if build.size > MOST_BYTES_OF_A_BUILD as u64 {
                        return Err(describe(format!(
                            "the build for {} has {} bytes, and a build of more than 256 MiB is not used",
                            build.platform, build.size
                        )));
                    }
                    bytes_in_all += build.size;
                    if bytes_in_all > MOST_BYTES_OF_ALL_BUILDS as u64 {
                        return Err(describe(
                            "the builds in the project have more than 256 MiB together".to_owned(),
                        ));
                    }
                    let digest = Sha256Digest::from_hex(&build.sha256).ok_or_else(|| {
                        describe(format!(
                            "the SHA-256 of the build for {} is not 64 hexadecimal digits",
                            build.platform
                        ))
                    })?;
                    builds.push((build.platform, digest, build.entry, build.size));
                }
                listed.push(Listed::Embedded {
                    name: document.name,
                    kind,
                    builds,
                });
            }
            _ => {
                return Err(describe(format!(
                    "\"{}\" must say either where its file is (linked) or what its builds are (embedded), and one of the two",
                    document.name
                )));
            }
        }
    }
    // Now the bytes, which are known to be no more than the limit, are read, and the libraries are put together in the order they were in.
    let mut libraries = PluginLibraries::default();
    for (number, library) in listed.into_iter().enumerate() {
        let describe = |reason: String| invalid_project(format!("plugin library number {}: {reason}", number + 1));
        let library = match library {
            Listed::Linked(library) => library,
            Listed::Embedded { name, kind, builds } => {
                let archive = archive.expect("only an archive has embedded libraries here");
                let mut made = Vec::new();
                for (platform, digest, entry, size) in builds {
                    // What the archive has must be what the file says it has, in size: the bytes are not read before that is so. (The sizes are within the limits already:
                    // all of them were added up before the archive was asked about any.)
                    let facts = archive::facts_of_entry(archive, &entry).map_err(|error| match error {
                        ArchiveError::NoSuchEntry { .. } => describe(format!(
                            "the build for {platform} is said to be inside the project file as \"{entry}\", which the project file does not have"
                        )),
                        other => ProjectFileError::from(other),
                    })?;
                    if facts.size != size {
                        return Err(describe(format!(
                            "the build for {platform} is said to have {size} bytes, and the project file has {} of it",
                            facts.size
                        )));
                    }
                    let bytes = archive::read_entry_bytes(archive, &entry, MOST_BYTES_OF_A_BUILD)?;
                    made.push(
                        EmbeddedBuild::as_recorded(&platform, digest, Arc::from(bytes))
                            .map_err(|problem| describe(problem.to_string()))?,
                    );
                }
                PluginLibrary::new(&name, kind, PluginLibrarySource::Embedded(made))
                    .map_err(|problem| describe(problem.to_string()))?
            }
        };
        libraries
            .add(library)
            .map_err(|problem| describe(problem.to_string()))?;
    }
    Ok(libraries)
}

/// The entry and the bytes of each build of each embedded library of `project`, in order. It is an error if two have one entry.
pub(super) fn entries_to_write(project: &Project) -> Result<Vec<EntryToWrite>, ProjectFileError> {
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for library in project.plugin_libraries() {
        let PluginLibrarySource::Embedded(builds) = library.source() else {
            continue;
        };
        for build in builds {
            let entry = entry_name_of(build, library.name());
            if !archive::is_a_plugin_library_entry_name(&entry) {
                return Err(invalid_project(format!(
                    "the plugin library \"{}\" is to be inside the project file as \"{entry}\", which is not a name for it in a project file",
                    library.name()
                )));
            }
            if !seen.insert(entry.clone()) {
                return Err(invalid_project(format!(
                    "more than one build is to be inside the project file as \"{entry}\""
                )));
            }
            entries.push((entry, build.bytes().clone()));
        }
    }
    Ok(entries)
}
