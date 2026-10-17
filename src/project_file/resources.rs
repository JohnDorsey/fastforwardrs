// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The resources of a project (the files that its effects use: see [`crate::project::Resources`]) in a project file.
//!
//! The file lists the resources that the effects use, each with its name, its kind, the number of bytes and the CRC-32 it had when it was saved (for whoever reads the file;
//! the program trusts the zip file's own check of an included file), and where its bytes are kept: **included** (the name of its entry in the archive, `project-resources/`
//! then the CRC-32, a dash and the name, so a table that is changed is another entry and one that is not changed is not written again, besides the two paths it had when it was
//! included), or **linked** (an absolute path and a path relative to the folder of the project file, as a source file is). **The bytes are read when the project is opened**
//! (RS-07): an included resource from its entry, a linked one from the first of its two paths that is a file. One that cannot be read (the file is gone, or more than
//! [`MOST_BYTES_OF_A_RESOURCE`]) is left *missing*: the effects that use it are kept, and its places are kept to be looked at and saved again as they were.

use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use super::archive::{self, ArchiveError};
use super::document::ResourceDocument;
use super::{ProjectFileError, invalid_project, link_from, path_as_unicode_text, relative_path_as_portable_text};
use crate::project::{MOST_BYTES_OF_A_RESOURCE, Project, ResourceKind, Resources};

/// The name of an entry of an archive and the bytes that are to be in it.
pub(super) type EntryToWrite = (String, Arc<[u8]>);

/// The documents of the resources of `project` that a file is to have: those its effects use. `project_directory` is the folder the file is saved in, which the relative
/// paths are worked out from (as for a source file: the paths of a resource that is included are kept as they were).
pub(super) fn documents_of(
    project: &Project,
    project_directory: &Path,
) -> Result<Vec<ResourceDocument>, ProjectFileError> {
    project
        .resources_to_save()
        .iter()
        .map(|resource| {
            let link = resource.link();
            let relative_path = if !link.is_included() && link.absolute_path.exists() {
                super::relative_path_between(project_directory, &link.absolute_path)
            } else {
                link.relative_path.clone()
            };
            Ok(ResourceDocument {
                name: resource.name().to_owned(),
                kind: resource.kind().key().to_owned(),
                size: resource.bytes().map(|bytes| bytes.len() as u64),
                crc32: resource.crc32(),
                included_as: link.included_as.clone(),
                absolute_path: path_as_unicode_text(&link.absolute_path)?,
                relative_path: relative_path
                    .map(|path| relative_path_as_portable_text(&path))
                    .transpose()?,
            })
        })
        .collect()
}

/// The resources that the documents describe, as they were saved, with no bytes yet (see [`load_the_bytes`]). The reason is what is wrong with the file.
pub(super) fn from_documents(documents: Vec<ResourceDocument>) -> Result<Resources, String> {
    let mut resources = Resources::default();
    for (number, document) in documents.into_iter().enumerate() {
        let describe = |reason: String| format!("resource number {}: {reason}", number + 1);
        let kind = ResourceKind::from_key(&document.kind).ok_or_else(|| {
            describe(format!(
                "\"{}\" is a kind of file that this version does not use",
                document.kind
            ))
        })?;
        if let Some(entry_name) = &document.included_as
            && !archive::is_a_project_resource_entry_name(entry_name)
        {
            return Err(describe(format!(
                "it is said to be included in the project file as \"{entry_name}\", which is not a name that a resource included in it has"
            )));
        }
        let link = link_from(document.absolute_path, document.relative_path, document.included_as);
        resources
            .add_as_it_was_saved(&document.name, kind, None, link)
            .map_err(|problem| describe(problem.to_string()))?;
    }
    Ok(resources)
}

/// The entry and the bytes of each resource of `project` that a file is to have that is included in it, in the order of the resources. It is an error if a resource that is
/// included has no bytes (it is missing, and there is nothing to write), or two have one entry.
pub(super) fn entries_to_write(project: &Project) -> Result<Vec<EntryToWrite>, ProjectFileError> {
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for resource in project
        .resources_to_save()
        .iter()
        .filter(|resource| resource.is_included())
    {
        let entry_name = resource.link().included_as.clone().unwrap_or_default();
        if !archive::is_a_project_resource_entry_name(&entry_name) {
            return Err(invalid_project(format!(
                "the resource \"{}\" is to be included as \"{entry_name}\", which is not a name for it in a project file",
                resource.name()
            )));
        }
        if !seen.insert(entry_name.clone()) {
            return Err(invalid_project(format!(
                "more than one resource is to be included as \"{entry_name}\""
            )));
        }
        let Some(bytes) = resource.bytes() else {
            return Err(ProjectFileError::IncludedFileNotAvailable {
                file_name: resource.name().to_owned(),
            });
        };
        entries.push((entry_name, bytes.clone()));
    }
    Ok(entries)
}

/// Whether the archive at `path` has an entry for each resource that the project in it says is included.
pub(super) fn check_that_the_archive_has_the_entries(path: &Path, project: &Project) -> Result<(), ProjectFileError> {
    for resource in project.resources().iter() {
        let Some(entry_name) = &resource.link().included_as else {
            continue;
        };
        match archive::facts_of_entry(path, entry_name) {
            Ok(_) => {}
            Err(ArchiveError::NoSuchEntry { .. }) => {
                return Err(invalid_project(format!(
                    "the resource \"{}\" is said to be included in the project file as \"{entry_name}\", which the project file does not have",
                    resource.name()
                )));
            }
            Err(other) => return Err(other.into()),
        }
    }
    Ok(())
}

/// Reads the bytes of the resources of `project`, which was read from `project_file`: an included resource from its entry in the archive (`project_file` is one if
/// `in_archive`), a linked one from the first of its paths that is a file the program can read and is not too big. The ones that cannot be read stay missing.
/// An included resource that is damaged is an error.
pub(super) fn load_the_bytes(
    project: &mut Project,
    project_file: &Path,
    in_archive: bool,
) -> Result<(), ProjectFileError> {
    let project_directory = project_file.parent();
    let wanted: Vec<_> = project
        .resources()
        .iter()
        .map(|resource| (resource.name().to_owned(), resource.link().clone()))
        .collect();
    for (name, link) in wanted {
        let bytes = match &link.included_as {
            Some(entry_name) if in_archive => {
                archive::read_entry_bytes(project_file, entry_name, MOST_BYTES_OF_A_RESOURCE)?
            }
            Some(_) => continue,
            None => {
                let Some(bytes) = link
                    .candidate_paths(project_directory)
                    .iter()
                    .find_map(|path| read_a_file_that_is_not_too_big(path))
                else {
                    continue;
                };
                bytes
            }
        };
        if !bytes.is_empty() {
            project.give_the_bytes_of_a_resource(&name, bytes.into());
        }
    }
    Ok(())
}

/// Why a file cannot be added as a resource.
#[derive(Debug, thiserror::Error)]
pub enum ResourceFileProblem {
    #[error("{path} could not be read: {source}")]
    CouldNotRead {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    #[error("{path} has {bytes} bytes, and a file of more than {limit} is not used as a resource")]
    TooBig {
        path: std::path::PathBuf,
        bytes: u64,
        limit: usize,
    },
    #[error("{path} is empty")]
    Empty { path: std::path::PathBuf },
}

/// What is in the file at `path`, to be added to a project as a resource: all of it, if it is a file that can be read and is not empty and at most
/// [`MOST_BYTES_OF_A_RESOURCE`] long. (The length is asked before the file is read, so that a file that is far too big is not read.)
pub fn read_a_resource_file(path: &Path) -> Result<Arc<[u8]>, ResourceFileProblem> {
    let could_not_read = |source| ResourceFileProblem::CouldNotRead {
        path: path.to_owned(),
        source,
    };
    let mut file = fs::File::open(path).map_err(could_not_read)?;
    let metadata = file.metadata().map_err(could_not_read)?;
    if !metadata.is_file() {
        return Err(could_not_read(std::io::Error::other("it is not a file")));
    }
    if metadata.len() > MOST_BYTES_OF_A_RESOURCE as u64 {
        return Err(ResourceFileProblem::TooBig {
            path: path.to_owned(),
            bytes: metadata.len(),
            limit: MOST_BYTES_OF_A_RESOURCE,
        });
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    (&mut file)
        .take(MOST_BYTES_OF_A_RESOURCE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(could_not_read)?;
    if bytes.len() > MOST_BYTES_OF_A_RESOURCE {
        return Err(ResourceFileProblem::TooBig {
            path: path.to_owned(),
            bytes: bytes.len() as u64,
            limit: MOST_BYTES_OF_A_RESOURCE,
        });
    }
    if bytes.is_empty() {
        return Err(ResourceFileProblem::Empty { path: path.to_owned() });
    }
    Ok(bytes.into())
}

/// The link of a resource that was made from the file at `path`, whose bytes are `bytes` and that is to be called `name`: **kept in the project file** if `embedded`
/// (the entry of the archive, made of the checksum of the bytes and the name, besides the path of the file where it was), and linked by its path otherwise.
pub fn link_of_a_new_resource(
    path: &Path,
    name: &str,
    bytes: &[u8],
    embedded: bool,
) -> crate::project::SourceMediaLink {
    crate::project::SourceMediaLink {
        absolute_path: std::path::absolute(path).unwrap_or_else(|_| path.to_owned()),
        relative_path: None,
        included_as: embedded.then(|| archive::project_resource_entry_name(crc32fast::hash(bytes), name)),
    }
}

/// What is in the file at `path`, if it is a file that can be read and is at most [`MOST_BYTES_OF_A_RESOURCE`] long.
pub(super) fn read_a_file_that_is_not_too_big(path: &Path) -> Option<Vec<u8>> {
    let mut file = fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    if !file.metadata().ok()?.is_file() || length > MOST_BYTES_OF_A_RESOURCE as u64 {
        return None;
    }
    let mut bytes = Vec::with_capacity(usize::try_from(length).ok()?);
    // (One byte past the most is read, so that a file that grew after its length was asked is found out as too big.)
    (&mut file)
        .take(MOST_BYTES_OF_A_RESOURCE as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= MOST_BYTES_OF_A_RESOURCE).then_some(bytes)
}
