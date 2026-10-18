// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The linked plugins that a person has said may be run: remembered **by their path and a hash of the file** (SF-05). A file that has changed since is another file, so it has
//! to be asked about again; a file that is somewhere else is another one too.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic_file::write_file_atomically;
use crate::digest::Sha256Digest;

/// What the file of approvals is called, beside the settings file.
const APPROVALS_FILE_NAME: &str = "plugin_approvals.json";

#[derive(Serialize, Deserialize, Default)]
struct ApprovalsDocument {
    #[serde(default)]
    approved: Vec<ApprovalDocument>,
}

#[derive(Serialize, Deserialize)]
struct ApprovalDocument {
    path: String,
    sha256: String,
}

/// The plugins that have been approved. It lives in a file (or only in memory, for a computer with no settings folder, and for the tests); a file that cannot be read is an
/// empty list, so that the worst that can happen to a person is that they are asked again, and **a file that cannot be written is said** ([`Self::last_problem`]).
#[derive(Debug, Default)]
pub struct ApprovalStore {
    file: Option<PathBuf>,
    approved: HashSet<(PathBuf, Sha256Digest)>,
    last_problem: Option<String>,
}

impl ApprovalStore {
    /// A store that is not kept anywhere.
    pub fn in_memory() -> Self {
        Self::default()
    }

    /// The store that is kept in `file`, with what is in it now.
    pub fn at(file: PathBuf) -> Self {
        let mut store = Self {
            file: Some(file.clone()),
            ..Self::default()
        };
        match fs::read_to_string(&file) {
            Ok(text) => match serde_json::from_str::<ApprovalsDocument>(&text) {
                Ok(document) => {
                    for entry in document.approved {
                        if let Some(digest) = Sha256Digest::from_hex(&entry.sha256) {
                            store.approved.insert((PathBuf::from(entry.path), digest));
                        }
                    }
                }
                Err(error) => {
                    store.last_problem = Some(format!(
                        "the file of the plugins that were approved, {}, cannot be read ({error}), so they will be asked about again",
                        file.display()
                    ));
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                store.last_problem = Some(format!(
                    "the file of the plugins that were approved, {}, cannot be read ({error}), so they will be asked about again",
                    file.display()
                ));
            }
        }
        store
    }

    /// Where the file of approvals is kept: beside the settings file. `None` if this system has no settings folder.
    pub fn default_file() -> Option<PathBuf> {
        crate::settings_file::default_settings_file_path()
            .and_then(|settings| Some(settings.parent()?.join(APPROVALS_FILE_NAME)))
    }

    /// Whether the plugin file at `path`, whose SHA-256 is `digest`, has been approved.
    pub fn is_approved(&self, path: &Path, digest: Sha256Digest) -> bool {
        self.approved.contains(&(path.to_owned(), digest))
    }

    /// How many approvals there are.
    pub fn len(&self) -> usize {
        self.approved.len()
    }

    pub fn is_empty(&self) -> bool {
        self.approved.is_empty()
    }

    /// Remembers that the plugin file at `path`, whose SHA-256 is `digest`, may be run, and writes the file again. (If it cannot be written the approval holds until the
    /// program ends, and [`Self::last_problem`] says so.)
    pub fn approve(&mut self, path: &Path, digest: Sha256Digest) {
        self.approved.insert((path.to_owned(), digest));
        self.save();
    }

    /// What last went wrong with the file, if anything did.
    pub fn last_problem(&self) -> Option<&str> {
        self.last_problem.as_deref()
    }

    fn save(&mut self) {
        let Some(file) = self.file.clone() else {
            return;
        };
        let mut approved: Vec<ApprovalDocument> = self
            .approved
            .iter()
            .filter_map(|(path, digest)| {
                Some(ApprovalDocument {
                    path: path.to_str()?.to_owned(),
                    sha256: digest.to_hex(),
                })
            })
            .collect();
        approved.sort_by(|a, b| (&a.path, &a.sha256).cmp(&(&b.path, &b.sha256)));
        let text = match serde_json::to_string_pretty(&ApprovalsDocument { approved }) {
            Ok(text) => text,
            Err(error) => {
                self.last_problem = Some(format!("the approvals of plugins could not be written: {error}"));
                return;
            }
        };
        let written = file
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| write_file_atomically(&file, &text));
        self.last_problem = written.err().map(|error| {
            format!(
                "the approvals of plugins could not be written to {} ({error}), so they are kept only until the program ends",
                file.display()
            )
        });
    }
}
