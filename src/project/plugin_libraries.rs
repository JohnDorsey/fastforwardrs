// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The libraries of native code that a project's sound effects come from (phase 7, piece 7.3; the design is `docs/engine-phase-7-design.md`, G14 and G15).
//!
//! A library is a file the operating system loads and runs: **a plugin is a program, and nothing checks what it does** (SF-04). This module only *describes* where such
//! a file is; nothing here loads anything, and whether one may be loaded is decided by `plugin_trust`. A project says of each library either that it is **linked** (it is a
//! file somewhere, named by a path, as a video is) or **embedded** (it is inside the project file, with one **build** for each kind of computer it can run on, because a
//! file of native code runs on one operating system and one processor only).
//!
//! The bytes of an embedded build are held in memory while the project is open (they are data until a person has said that they may be run; the limits below are what makes that
//! safe to do for a file from elsewhere), and are written into the `.ffrs` file with the project.

use std::path::PathBuf;
use std::sync::Arc;

use thiserror::Error;

use super::resources::why_not_the_name_of_a_resource;
use crate::digest::Sha256Digest;

/// How many libraries a project may have.
pub const MOST_PLUGIN_LIBRARIES: usize = 64;

/// How many builds one embedded library may have (one for each system it is for).
pub const MOST_BUILDS_OF_A_LIBRARY: usize = 16;

/// The most bytes that one build may have (256 MiB: SF-10; a plugin is far larger than a table of colours).
pub const MOST_BYTES_OF_A_BUILD: usize = 256 << 20;

/// The most bytes that all the builds of a project may have together. (They are read into memory when the project is opened, so a file from elsewhere cannot ask for
/// more than this: a project that has more is refused when it is read.)
pub const MOST_BYTES_OF_ALL_BUILDS: usize = 256 << 20;

/// The most characters of a platform's name.
const MOST_CHARACTERS_IN_A_PLATFORM: usize = 40;

/// What kind of plugin a library has in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PluginLibraryKind {
    /// LADSPA: a `.dll`, `.so` or `.dylib` that gives sound effects (`ladspa_descriptor`).
    Ladspa,
}

impl PluginLibraryKind {
    /// What a project file calls it.
    pub fn key(self) -> &'static str {
        match self {
            Self::Ladspa => "ladspa",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "ladspa" => Some(Self::Ladspa),
            _ => None,
        }
    }
}

/// A library that is a file somewhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkedLibrary {
    pub absolute_path: PathBuf,
    /// Where it is from the folder of the project file, if the two are on one drive: where it is looked for if it is not where it was.
    pub relative_path: Option<PathBuf>,
}

/// The library as it is made for one kind of computer, inside the project file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmbeddedBuild {
    platform: String,
    /// What the project says the SHA-256 of the bytes is. (A project that was written by this program has it right; a file from elsewhere may not, and the bytes are
    /// not run unless they are what it says.)
    digest: Sha256Digest,
    bytes: Arc<[u8]>,
}

impl EmbeddedBuild {
    /// A build for `platform` (`windows-x86_64`, `linux-x86_64`, `macos-aarch64`...) made of `bytes`, whose digest is worked out.
    pub fn new(platform: &str, bytes: Arc<[u8]>) -> Result<Self, PluginLibraryProblem> {
        let digest = Sha256Digest::of_bytes(&bytes);
        Self::as_recorded(platform, digest, bytes)
    }

    /// A build that says its digest is `digest` (what a project file says; it is not checked here).
    pub fn as_recorded(platform: &str, digest: Sha256Digest, bytes: Arc<[u8]>) -> Result<Self, PluginLibraryProblem> {
        if !is_a_platform_name(platform) {
            return Err(PluginLibraryProblem::BadPlatform {
                platform: platform.to_owned(),
            });
        }
        if bytes.is_empty() {
            return Err(PluginLibraryProblem::EmptyBuild {
                platform: platform.to_owned(),
            });
        }
        if bytes.len() > MOST_BYTES_OF_A_BUILD {
            return Err(PluginLibraryProblem::BuildTooBig {
                platform: platform.to_owned(),
                bytes: bytes.len(),
            });
        }
        Ok(Self {
            platform: platform.to_owned(),
            digest,
            bytes,
        })
    }

    pub fn platform(&self) -> &str {
        &self.platform
    }

    pub fn digest(&self) -> Sha256Digest {
        self.digest
    }

    pub fn bytes(&self) -> &Arc<[u8]> {
        &self.bytes
    }

    /// Whether the bytes are what the project says: the file was not damaged, or changed by whoever made the project.
    pub fn is_what_it_says(&self) -> bool {
        Sha256Digest::of_bytes(&self.bytes) == self.digest
    }
}

/// Whether `name` could be the name of a platform: lower case letters, digits, dashes and underscores, as `windows-x86_64` is.
pub fn is_a_platform_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= MOST_CHARACTERS_IN_A_PLATFORM
        && name.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-' || character == '_'
        })
}

/// Where a library is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginLibrarySource {
    Linked(LinkedLibrary),
    /// Inside the project file: one build for each platform, at most one of each.
    Embedded(Vec<EmbeddedBuild>),
}

/// A library of plugins, as the project knows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginLibrary {
    name: String,
    kind: PluginLibraryKind,
    source: PluginLibrarySource,
}

impl PluginLibrary {
    /// A library whose `name` is the name of its file (`amp_1181.dll` or `amp_1181`: what the person who made it called it). It is an error, and nothing is made, if the name is
    /// not one that a file can have, or an embedded library has no build, too many, two for one platform, or builds that are too big together.
    pub fn new(name: &str, kind: PluginLibraryKind, source: PluginLibrarySource) -> Result<Self, PluginLibraryProblem> {
        if let Some(reason) = why_not_the_name_of_a_resource(name) {
            return Err(PluginLibraryProblem::BadName {
                name: name.to_owned(),
                reason,
            });
        }
        if let PluginLibrarySource::Embedded(builds) = &source {
            if builds.is_empty() {
                return Err(PluginLibraryProblem::NoBuilds { name: name.to_owned() });
            }
            if builds.len() > MOST_BUILDS_OF_A_LIBRARY {
                return Err(PluginLibraryProblem::TooManyBuilds {
                    name: name.to_owned(),
                    limit: MOST_BUILDS_OF_A_LIBRARY,
                });
            }
            for (place, build) in builds.iter().enumerate() {
                if builds[..place].iter().any(|other| other.platform == build.platform) {
                    return Err(PluginLibraryProblem::TwoBuildsForOnePlatform {
                        name: name.to_owned(),
                        platform: build.platform.clone(),
                    });
                }
            }
        }
        Ok(Self {
            name: name.to_owned(),
            kind,
            source,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> PluginLibraryKind {
        self.kind
    }

    pub fn source(&self) -> &PluginLibrarySource {
        &self.source
    }

    pub fn is_embedded(&self) -> bool {
        matches!(self.source, PluginLibrarySource::Embedded(_))
    }

    /// The build for `platform`, if the library is embedded and has one.
    pub fn build_for(&self, platform: &str) -> Option<&EmbeddedBuild> {
        match &self.source {
            PluginLibrarySource::Embedded(builds) => builds.iter().find(|build| build.platform == platform),
            PluginLibrarySource::Linked(_) => None,
        }
    }

    /// The platforms an embedded library has a build for, in the order of its builds (none for a linked one).
    pub fn platforms(&self) -> Vec<&str> {
        match &self.source {
            PluginLibrarySource::Embedded(builds) => builds.iter().map(|build| build.platform.as_str()).collect(),
            PluginLibrarySource::Linked(_) => Vec::new(),
        }
    }

    /// How many bytes its builds have together.
    pub fn bytes_of_builds(&self) -> usize {
        match &self.source {
            PluginLibrarySource::Embedded(builds) => builds.iter().map(|build| build.bytes.len()).sum(),
            PluginLibrarySource::Linked(_) => 0,
        }
    }
}

/// Why a library could not be made or added.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PluginLibraryProblem {
    #[error("\"{name}\" is not a name for a plugin library: {reason}")]
    BadName { name: String, reason: &'static str },
    #[error(
        "\"{platform}\" is not the name of a platform (lower case letters, digits, dashes and underscores, like windows-x86_64)"
    )]
    BadPlatform { platform: String },
    #[error("the build for {platform} has nothing in it")]
    EmptyBuild { platform: String },
    #[error("the build for {platform} has {bytes} bytes, and a build of more than 256 MiB is not used")]
    BuildTooBig { platform: String, bytes: usize },
    #[error("the library \"{name}\" is embedded and has no build")]
    NoBuilds { name: String },
    #[error("the library \"{name}\" has more than {limit} builds")]
    TooManyBuilds { name: String, limit: usize },
    #[error("the library \"{name}\" has two builds for {platform}")]
    TwoBuildsForOnePlatform { name: String, platform: String },
    #[error("a project has at most {limit} plugin libraries")]
    TooMany { limit: usize },
    #[error("the project has a plugin library called \"{name}\" already")]
    NameTaken { name: String },
    #[error("the builds in the project have more than 256 MiB together")]
    TooMuchInAll,
}

/// The libraries of a project, in the order they were added in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginLibraries {
    list: Vec<PluginLibrary>,
}

impl PluginLibraries {
    pub fn iter(&self) -> std::slice::Iter<'_, PluginLibrary> {
        self.list.iter()
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&PluginLibrary> {
        self.list.iter().find(|library| library.name == name)
    }

    /// How many bytes all the builds have together.
    pub fn bytes_of_builds(&self) -> usize {
        self.list.iter().map(PluginLibrary::bytes_of_builds).sum()
    }

    /// Adds `library`. It is an error, and nothing is added, if the project has too many libraries or one of the name already, or the builds would be too big together.
    pub fn add(&mut self, library: PluginLibrary) -> Result<(), PluginLibraryProblem> {
        if self.list.len() >= MOST_PLUGIN_LIBRARIES {
            return Err(PluginLibraryProblem::TooMany {
                limit: MOST_PLUGIN_LIBRARIES,
            });
        }
        if self.get(&library.name).is_some() {
            return Err(PluginLibraryProblem::NameTaken { name: library.name });
        }
        if self.bytes_of_builds() + library.bytes_of_builds() > MOST_BYTES_OF_ALL_BUILDS {
            return Err(PluginLibraryProblem::TooMuchInAll);
        }
        self.list.push(library);
        Ok(())
    }

    /// Takes the library called `name` away. Whether there was one.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.list.len();
        self.list.retain(|library| library.name != name);
        self.list.len() != before
    }
}

impl<'a> IntoIterator for &'a PluginLibraries {
    type Item = &'a PluginLibrary;
    type IntoIter = std::slice::Iter<'a, PluginLibrary>;

    fn into_iter(self) -> Self::IntoIter {
        self.list.iter()
    }
}

#[cfg(test)]
mod tests;
