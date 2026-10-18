// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! An embedded plugin, written out to where it can be loaded from (SF-10).
//!
//! A library of native code cannot be loaded from inside a zip file: the operating system wants a file. So after the person has said yes, the build for this computer is written
//! to **a folder of its own in the temporary files**, and the file that is written is **checked against the hash that the project recorded for it** before anything may load it.
//! It is no more than 256 MiB. The folder is removed when the cache is dropped (when the program ends), and the folders that earlier runs left behind (a program that was
//! killed cannot clean up) are removed the next time, if they are old.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use thiserror::Error;

use crate::digest::Sha256Digest;
use crate::project::{EmbeddedBuild, MOST_BYTES_OF_A_BUILD};

/// The most bytes that a plugin written out may have.
pub const MOST_BYTES_OF_A_PLUGIN: usize = MOST_BYTES_OF_A_BUILD;

/// What the folders of the plugins that are written out begin with.
const FOLDER_PREFIX: &str = "fastforwardrs-plugins-";

/// A folder left by a program that ended badly is removed when it is this old.
pub(super) const OLD_ENOUGH_TO_BE_REMOVED: Duration = Duration::from_secs(24 * 60 * 60);

/// Why a plugin could not be written out.
#[derive(Debug, Error)]
pub enum ExtractionError {
    #[error("the plugin has {bytes} bytes, and one of more than 256 MiB is not used")]
    TooBig { bytes: usize },
    #[error(
        "the plugin is not the one the project says it is (it should be {recorded}, and it is {found}): the file was changed or damaged, and it is not loaded"
    )]
    DoesNotMatch {
        recorded: Sha256Digest,
        found: Sha256Digest,
    },
    #[error("the plugin could not be written out to {path}: {source}")]
    CouldNotWrite {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// The folder that embedded plugins are written out to.
#[derive(Debug)]
pub struct PluginCache {
    folder: PathBuf,
}

impl PluginCache {
    /// A folder of its own in the temporary files, new for this run. The folders that earlier runs left are removed if they are old.
    pub fn in_the_temporary_files() -> io::Result<Self> {
        let base = std::env::temp_dir();
        remove_folders_left_long_ago(&base, OLD_ENOUGH_TO_BE_REMOVED);
        let nanoseconds = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        Self::in_folder(base.join(format!("{FOLDER_PREFIX}{}-{nanoseconds}", std::process::id())))
    }

    /// The cache in `folder`, which is made (and removed when the cache is dropped).
    pub fn in_folder(folder: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&folder)?;
        Ok(Self { folder })
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Writes the build out and gives the path of the file. **The bytes are checked against the digest the build says it has, and nothing is written if they are not that.**
    /// A build that was written out already is not written again.
    pub fn extract(&self, library_name: &str, build: &EmbeddedBuild) -> Result<PathBuf, ExtractionError> {
        let bytes = build.bytes();
        if bytes.len() > MOST_BYTES_OF_A_PLUGIN {
            return Err(ExtractionError::TooBig { bytes: bytes.len() });
        }
        let found = Sha256Digest::of_bytes(bytes);
        if found != build.digest() {
            return Err(ExtractionError::DoesNotMatch {
                recorded: build.digest(),
                found,
            });
        }
        // (The name has been checked as the name of a file by the project; the digest and the platform make it different for every build.)
        let path = self
            .folder
            .join(format!("{}-{}-{}", found.short_hex(), build.platform(), library_name));
        if fs::metadata(&path).is_ok_and(|metadata| metadata.len() == bytes.len() as u64)
            && Sha256Digest::of_file(&path).is_ok_and(|on_disk| on_disk == found)
        {
            return Ok(path);
        }
        let could_not_write = |source: io::Error| ExtractionError::CouldNotWrite {
            path: path.clone(),
            source,
        };
        let mut file = fs::File::create(&path).map_err(could_not_write)?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|source| ExtractionError::CouldNotWrite {
                path: path.clone(),
                source,
            })?;
        Ok(path)
    }
}

impl Drop for PluginCache {
    fn drop(&mut self) {
        // (A library that is still loaded cannot be removed on Windows: then the folder is left, and an old one is removed by a later run.)
        let _ = fs::remove_dir_all(&self.folder);
    }
}

/// Removes the folders in `base` that were made for plugins and have not been touched for `older_than`.
fn remove_folders_left_long_ago(base: &Path, older_than: Duration) {
    let Ok(entries) = fs::read_dir(base) else {
        return;
    };
    for entry in entries.flatten() {
        let is_ours = entry.file_name().to_string_lossy().starts_with(FOLDER_PREFIX);
        let age = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| SystemTime::now().duration_since(modified).ok());
        if is_ours && age.is_some_and(|age| age >= older_than) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
pub(super) fn remove_old_folders_for_a_test(base: &Path, older_than: Duration) {
    remove_folders_left_long_ago(base, older_than);
}
