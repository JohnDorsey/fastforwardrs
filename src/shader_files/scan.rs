// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Finding the shaders that are in folders (phase 6.8, the plugin manager).
//!
//! The folders are looked in for files that end in `.fs` or `.isf` and **begin like an ISF file** (a comment with a JSON header: `.fs` is also the ending of F# files and of plain
//! GLSL, which are not shaders for this program and are left alone, not listed as failures). **Each shader that is found is read and compiled, and listed with what became of it:
//! the kind of effect it is, or the reason it cannot be one**, so that a shader that does not work is never missing without a word. A folder is looked in below itself to a depth, at
//! most so many files are looked at, a name that begins with a dot is left alone (the folders that a download is built in), and nothing is run: a shader is data (J3 of phase 6).

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::file::{ShaderFileProblem, shader_file_from};
use crate::isf::Shape;
use crate::project::{EffectKind, MOST_BYTES_OF_A_SHADER};

/// How far below a folder the search goes (the folder itself is 0).
pub const MOST_DEPTH_OF_THE_SEARCH: usize = 4;

/// How many shader files are looked at in all, in the folders of one scan.
pub const MOST_FILES_LOOKED_AT: usize = 3000;

/// How much of a file that is too long to be a shader is read to see whether it begins like an ISF file.
const BYTES_LOOKED_AT_OF_A_FILE_THAT_IS_TOO_LONG: u64 = 64 * 1024;

/// A shader that was found, and what became of it.
#[derive(Clone, Debug)]
pub struct FoundShader {
    pub path: PathBuf,
    pub file_name: String,
    /// What it is for, if the file could be read as ISF.
    pub shape: Option<Shape>,
    /// The kind of effect it is, if the file could be read as ISF (a shader that this version cannot run has one too: see `why_not_usable`).
    pub kind: Option<EffectKind>,
    /// Why it cannot be used: what is wrong with the file, or what this version cannot give it yet. `None` for one that can be used.
    pub why_not_usable: Option<String>,
}

impl FoundShader {
    /// Whether it can be put on a clip, or made into one.
    pub fn can_be_used(&self) -> bool {
        self.kind.is_some() && self.why_not_usable.is_none()
    }
}

/// What a scan found.
#[derive(Clone, Debug, Default)]
pub struct ShaderScan {
    /// Every shader that was found, in the order of the folders, and in each folder in the order of the names.
    pub shaders: Vec<FoundShader>,
    /// What went wrong that was no shader's doing: a folder that could not be read, the limit of files.
    pub problems: Vec<String>,
}

impl ShaderScan {
    /// The kinds of the shaders that can be used, each once, in the order they were found in: **the first of two files that make one kind is the one that counts** (the folder that comes
    /// first wins, as it does for plugins).
    pub fn usable_kinds(&self) -> Vec<EffectKind> {
        let mut kinds: Vec<EffectKind> = Vec::new();
        for shader in &self.shaders {
            if shader.can_be_used()
                && let Some(kind) = shader.kind
                && !kinds.contains(&kind)
            {
                kinds.push(kind);
            }
        }
        kinds
    }

    /// How many of the shaders can be used (by kind: the same shader twice is one).
    pub fn usable_count(&self) -> usize {
        self.usable_kinds().len()
    }

    /// How many of the files that were found cannot be used.
    pub fn unusable_count(&self) -> usize {
        self.shaders.iter().filter(|shader| !shader.can_be_used()).count()
    }
}

/// The folders that ISF's own readme says shaders are put in: `/Library/Graphics/ISF` and `~/Library/Graphics/ISF` on macOS, `%ProgramData%\ISF` on Windows, none on Linux (nothing is made
/// up for a system that has no such place).
pub fn usual_folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if cfg!(target_os = "macos") {
        folders.push(PathBuf::from("/Library/Graphics/ISF"));
        if let Some(home) = dirs::home_dir() {
            folders.push(home.join("Library").join("Graphics").join("ISF"));
        }
    } else if cfg!(windows)
        && let Some(data) = std::env::var_os("ProgramData")
    {
        folders.push(PathBuf::from(data).join("ISF"));
    }
    folders
}

/// Looks for shaders in `folders` (see the top of the file).
pub fn scan_folders(folders: &[PathBuf]) -> ShaderScan {
    scan_folders_within(folders, MOST_DEPTH_OF_THE_SEARCH, MOST_FILES_LOOKED_AT)
}

/// [`scan_folders`], going down at most `most_depth` folders below each folder and looking at at most `most_files` shader files in all.
pub fn scan_folders_within(folders: &[PathBuf], most_depth: usize, most_files: usize) -> ShaderScan {
    let mut search = Search {
        most_depth,
        most_files,
        files_looked_at: 0,
        stopped: false,
        scan: ShaderScan::default(),
    };
    for folder in folders {
        if search.stopped {
            break;
        }
        search.look_in(folder, 0);
    }
    search.scan
}

struct Search {
    most_depth: usize,
    most_files: usize,
    files_looked_at: usize,
    stopped: bool,
    scan: ShaderScan,
}

impl Search {
    fn look_in(&mut self, folder: &Path, depth: usize) {
        let entries = match fs::read_dir(folder) {
            Ok(entries) => entries,
            // (A folder that is not there is a place that was looked in and has nothing, which is what most of the usual places are on most computers.)
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => {
                self.scan.problems.push(format!(
                    "the folder {} could not be looked in for shaders: {error}",
                    folder.display()
                ));
                return;
            }
        };
        let mut entries: Vec<fs::DirEntry> = entries.filter_map(Result::ok).collect();
        entries.sort_by_key(fs::DirEntry::file_name);
        for entry in entries {
            if self.stopped {
                return;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                if depth < self.most_depth {
                    self.look_in(&path, depth + 1);
                }
            } else if kind.is_file() && has_the_ending_of_a_shader(&name) {
                self.look_at(&path, name);
            }
        }
    }

    fn look_at(&mut self, path: &Path, file_name: String) {
        if self.files_looked_at >= self.most_files {
            self.stopped = true;
            self.scan.problems.push(format!(
                "{} shader files were looked at, which is as many as are looked at in one scan: the rest were not",
                self.most_files
            ));
            return;
        }
        self.files_looked_at += 1;
        let Some(content) = read_the_content_of(path) else {
            return;
        };
        let beginning = match &content {
            Content::Whole(bytes) => bytes.as_slice(),
            Content::TooLong { prefix, .. } => prefix.as_slice(),
        };
        if !begins_like_an_isf_file(&String::from_utf8_lossy(beginning)) {
            // (Not a shader at all: an F# file, a plain GLSL file...)
            return;
        }
        let found = |shape, kind, why: Option<String>| FoundShader {
            path: path.to_owned(),
            file_name: file_name.clone(),
            shape,
            kind,
            why_not_usable: why,
        };
        let shader = match content {
            Content::TooLong { length, .. } => found(
                None,
                None,
                Some(format!(
                    "the file is {length} bytes, and {MOST_BYTES_OF_A_SHADER} are the most a shader can be"
                )),
            ),
            Content::Whole(bytes) => match String::from_utf8(bytes) {
                Err(_) => found(None, None, Some("the file is not text".to_owned())),
                Ok(text) => match shader_file_from(path, text) {
                    Ok(file) => found(Some(file.shape), Some(file.kind), file.why_not_usable),
                    Err(ShaderFileProblem::CouldNotBeRead(why) | ShaderFileProblem::NotAShader(why)) => {
                        found(None, None, Some(why))
                    }
                },
            },
        };
        self.scan.shaders.push(shader);
    }
}

/// Whether the name of a file ends as the name of a shader's does.
fn has_the_ending_of_a_shader(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|ending| ending.eq_ignore_ascii_case("fs") || ending.eq_ignore_ascii_case("isf"))
}

/// What was read of a file.
enum Content {
    /// All of a file that is short enough to be a shader.
    Whole(Vec<u8>),
    /// The first bytes of a file that is too long to be one (enough to see whether it begins like an ISF file), and how long it is.
    TooLong { length: u64, prefix: Vec<u8> },
}

/// What is in the file. `None` for a file that cannot be read at all (it is left out: it cannot be told what it was).
fn read_the_content_of(path: &Path) -> Option<Content> {
    let mut file = fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    if length > MOST_BYTES_OF_A_SHADER as u64 {
        let mut prefix = Vec::new();
        (&mut file)
            .take(BYTES_LOOKED_AT_OF_A_FILE_THAT_IS_TOO_LONG)
            .read_to_end(&mut prefix)
            .ok()?;
        return Some(Content::TooLong { length, prefix });
    }
    let mut bytes = Vec::with_capacity(usize::try_from(length).unwrap_or(0));
    (&mut file)
        .take(MOST_BYTES_OF_A_SHADER as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(Content::Whole(bytes))
}

/// Whether `text` has, as ISF files do, a block comment that begins with a brace (the JSON of its header).
pub fn begins_like_an_isf_file(text: &str) -> bool {
    text.match_indices("/*")
        .any(|(at, _)| text[at + 2..].trim_start().starts_with('{'))
}

#[cfg(test)]
mod tests;
