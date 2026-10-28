// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The resources of a project: files that its effects use and that are not pictures or sounds to cut (a `.cube` table of colours, and, since phase 13, a `.mid` file of music that MIDI
//! clips play). **A project has a list of them, and an effect (or a MIDI clip) names the one it uses by its name** (the value of a parameter whose control is a resource: see [`ParameterControl::Resource`](crate::project::ParameterControl::Resource)).
//!
//! A resource is **read into memory when the project is opened** and kept there (RS-07), so that a frame never waits for a disk. Where its bytes are kept for good is the
//! same as for the source files of a project (see [`SourceMediaLink`]): *included* in the project file (an archive, a `.ffrs` file), which is the default, or *linked* by
//! a path, as an absolute path and a path relative to the folder of the project. A linked file that cannot be read when the project is opened leaves the resource
//! **missing** (no bytes): the effects that use it are kept, and do nothing, like an effect that the program does not have.
//!
//! * **Two resources with the same bytes are one** (RS-05): adding a file that is there already gives the name of the one that is there.
//! * **A resource is at most [`MOST_BYTES_OF_A_RESOURCE`] bytes** and a project has at most [`MOST_RESOURCES`] of them (SF-11); the name of one is checked as the name of a
//!   file in an archive is (SF-03): no part of a path, nothing that is a control character, not too long.
//! * **A resource that no effect uses is not saved**: it is kept in memory for as long as the project is open (so that taking an effect off and putting it back on, or
//!   undoing, finds it), and the file has what the project uses.
//!
//! (Not to be mistaken for the entries of an archive that are called resources in `project_file::archive`: those are the source files that were included.)

use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use thiserror::Error;

use super::source_media::SourceMediaLink;
use crate::cube_table::{CubeTable, parse_cube};
use crate::midi_file::{MidiFile, parse_midi_file};

/// The most bytes that one resource may have (64 MiB).
pub const MOST_BYTES_OF_A_RESOURCE: usize = 64 << 20;

/// The most resources that one project may have.
pub const MOST_RESOURCES: usize = 256;

/// The most characters that the name of a resource may have.
pub const MOST_CHARACTERS_IN_THE_NAME_OF_A_RESOURCE: usize = 120;

/// What kind of file a resource is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ResourceKind {
    /// A `.cube` table of colours: a look-up table that is applied to the colours of a picture (see the effect of the look-up table).
    Cube,
    /// A Standard MIDI File: music that MIDI clips play (each clip is a track of it), through the synthesizers on its road.
    Midi,
}

impl ResourceKind {
    pub const ALL: [Self; 2] = [Self::Cube, Self::Midi];

    /// What the kind is called in a project file, and in the control of an effect's value that wants a resource of it (`"cube"`).
    pub fn key(self) -> &'static str {
        match self {
            Self::Cube => "cube",
            Self::Midi => "midi",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }

    /// What a person calls files of the kind, in a sentence.
    pub fn label(self) -> &'static str {
        match self {
            Self::Cube => ".cube table of colours",
            Self::Midi => "Standard MIDI File",
        }
    }
}

/// What a resource of the kind `cube` is read as, once (the first time it is asked for), and shared by the copies of the resource: a project that is cloned for its history
/// does not read its tables again. **It is not a part of what makes two resources equal** (it is a function of the bytes).
#[derive(Clone, Debug, Default)]
struct ReadOnce(Arc<OnceLock<Result<Arc<CubeTable>, String>>>);

impl PartialEq for ReadOnce {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for ReadOnce {}

/// What a resource of the kind `midi` is read as, once, and shared by the copies of the resource (as [`ReadOnce`] is for a table). **Not a part of what makes two resources equal.**
#[derive(Clone, Debug, Default)]
struct ReadOnceMidi(Arc<OnceLock<Result<Arc<MidiFile>, String>>>);

impl PartialEq for ReadOnceMidi {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for ReadOnceMidi {}

/// A file that the effects of a project use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    name: String,
    kind: ResourceKind,
    /// What is in the file, read when the project was opened or when the resource was added. `None` while it is missing: a linked file that was not there to be read.
    bytes: Option<Arc<[u8]>>,
    /// Where the file is kept: included in the project file, or linked by its paths (see the top of this file). For an included resource the paths are where the file
    /// was when it was included, and are kept as they were.
    link: SourceMediaLink,
    read: ReadOnce,
    read_as_music: ReadOnceMidi,
}

impl Resource {
    /// What the file is as a table, for a resource of the kind `cube`: the table, or what is wrong with the file (with the line, as [`crate::cube_table::CubeProblem`] tells it),
    /// or that the program has not got the bytes (it is missing). **Read once** and kept, so that asking again is nothing.
    pub fn cube_table(&self) -> Result<Arc<CubeTable>, String> {
        if self.kind != ResourceKind::Cube {
            return Err("the file is not a table of colours".to_owned());
        }
        let bytes = self.bytes.as_ref().ok_or_else(|| "the file is missing".to_owned())?;
        self.read
            .0
            .get_or_init(|| match std::str::from_utf8(bytes) {
                Ok(text) => parse_cube(text).map(Arc::new).map_err(|problem| problem.to_string()),
                Err(_) => Err("the file is not text".to_owned()),
            })
            .clone()
    }

    /// What the file is as music, for a resource of the kind `midi`: the tracks of the Standard MIDI File, or what is wrong with it (what [`MidiFileProblem`](crate::midi_file::MidiFileProblem)
    /// says), or that the program has not got the bytes (it is missing). **Read once** and kept, so that asking again is nothing: the music is the same `Arc`.
    pub fn midi_file(&self) -> Result<Arc<MidiFile>, String> {
        if self.kind != ResourceKind::Midi {
            return Err("the file is not music".to_owned());
        }
        let bytes = self.bytes.as_ref().ok_or_else(|| "the file is missing".to_owned())?;
        self.read_as_music
            .0
            .get_or_init(|| {
                parse_midi_file(bytes)
                    .map(Arc::new)
                    .map_err(|problem| problem.to_string())
            })
            .clone()
    }

    /// What the effects that use it name it by.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> ResourceKind {
        self.kind
    }

    /// What is in the file, if the program has it.
    pub fn bytes(&self) -> Option<&Arc<[u8]>> {
        self.bytes.as_ref()
    }

    /// Whether the file could not be read (a linked file that is gone), so that the effects that use it cannot do their work.
    pub fn is_missing(&self) -> bool {
        self.bytes.is_none()
    }

    pub fn link(&self) -> &SourceMediaLink {
        &self.link
    }

    pub fn is_included(&self) -> bool {
        self.link.is_included()
    }

    /// The CRC-32 of the bytes, which with their number is what a project file keeps to tell the file from another (`None` while it is missing).
    pub fn crc32(&self) -> Option<u32> {
        self.bytes.as_ref().map(|bytes| crc32fast::hash(bytes))
    }
}

/// Why a resource could not be added.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ResourceProblem {
    #[error("the file has {bytes} bytes, and a file of more than {limit} is not used as a resource")]
    TooBig { bytes: usize, limit: usize },
    #[error("a project has at most {limit} resources, and this would be one more")]
    TooMany { limit: usize },
    #[error("\"{name}\" is not a name for a resource: {reason}")]
    BadName { name: String, reason: &'static str },
    #[error("the file is empty")]
    Empty,
}

/// Why `name` cannot be the name of a resource, if it cannot: the same rules as the name of a file in an archive.
pub fn why_not_the_name_of_a_resource(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        return Some("it is empty");
    }
    if name.chars().count() > MOST_CHARACTERS_IN_THE_NAME_OF_A_RESOURCE {
        return Some("it is too long");
    }
    if name == "." || name == ".." {
        return Some("it is the name of a folder");
    }
    if name.starts_with(' ') || name.ends_with(' ') || name.ends_with('.') {
        return Some("it begins or ends with a space, or ends with a dot");
    }
    if name
        .chars()
        .any(|character| character.is_control() || "/\\:*?\"<>|".contains(character))
    {
        return Some("it has a control character or one that a name of a file cannot have");
    }
    None
}

/// The resources of a project, in the order they were added in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resources {
    list: Vec<Resource>,
}

impl Resources {
    pub fn iter(&self) -> std::slice::Iter<'_, Resource> {
        self.list.iter()
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// The resource with this name.
    pub fn get(&self, name: &str) -> Option<&Resource> {
        self.list.iter().find(|resource| resource.name == name)
    }

    /// Adds a resource called `wanted_name` (or, if a resource of that name is there that has other bytes, a name made from it that is not taken, such as `grade (2).cube`),
    /// with `bytes` and kept at `link`, and gives its name. **If a resource of the kind has the same bytes already, nothing is added and the name of that one is given**
    /// (RS-05).
    pub fn add(
        &mut self,
        wanted_name: &str,
        kind: ResourceKind,
        bytes: Arc<[u8]>,
        link: SourceMediaLink,
    ) -> Result<String, ResourceProblem> {
        if let Some(reason) = why_not_the_name_of_a_resource(wanted_name) {
            return Err(ResourceProblem::BadName {
                name: wanted_name.to_owned(),
                reason,
            });
        }
        if bytes.is_empty() {
            return Err(ResourceProblem::Empty);
        }
        if bytes.len() > MOST_BYTES_OF_A_RESOURCE {
            return Err(ResourceProblem::TooBig {
                bytes: bytes.len(),
                limit: MOST_BYTES_OF_A_RESOURCE,
            });
        }
        if let Some(same) = self
            .list
            .iter()
            .find(|resource| resource.kind == kind && resource.bytes.as_deref() == Some(&*bytes))
        {
            return Ok(same.name.clone());
        }
        if self.list.len() >= MOST_RESOURCES {
            return Err(ResourceProblem::TooMany { limit: MOST_RESOURCES });
        }
        let name = self.a_name_that_is_not_taken(wanted_name);
        self.list.push(Resource {
            name: name.clone(),
            kind,
            bytes: Some(bytes),
            link,
            read: ReadOnce::default(),
            read_as_music: ReadOnceMidi::default(),
        });
        Ok(name)
    }

    /// Puts back a resource that a project file had, as it was: its name, kind and link, and its bytes if the program has them (a missing resource has none). **Used when a
    /// project is opened**, so it is an error only if what the file says cannot be (the name, the kind's size and a name taken twice).
    pub fn add_as_it_was_saved(
        &mut self,
        name: &str,
        kind: ResourceKind,
        bytes: Option<Arc<[u8]>>,
        link: SourceMediaLink,
    ) -> Result<(), ResourceProblem> {
        if let Some(reason) = why_not_the_name_of_a_resource(name) {
            return Err(ResourceProblem::BadName {
                name: name.to_owned(),
                reason,
            });
        }
        if self.get(name).is_some() {
            return Err(ResourceProblem::BadName {
                name: name.to_owned(),
                reason: "another resource has the name",
            });
        }
        if self.list.len() >= MOST_RESOURCES {
            return Err(ResourceProblem::TooMany { limit: MOST_RESOURCES });
        }
        if let Some(bytes) = &bytes
            && bytes.len() > MOST_BYTES_OF_A_RESOURCE
        {
            return Err(ResourceProblem::TooBig {
                bytes: bytes.len(),
                limit: MOST_BYTES_OF_A_RESOURCE,
            });
        }
        self.list.push(Resource {
            name: name.to_owned(),
            kind,
            bytes,
            link,
            read: ReadOnce::default(),
            read_as_music: ReadOnceMidi::default(),
        });
        Ok(())
    }

    /// Gives the missing resource `name` the bytes that were found for it (a linked file that was found after all). Does nothing if there is no such resource or it has
    /// bytes already.
    pub fn found_the_bytes_of(&mut self, name: &str, bytes: Arc<[u8]>) -> bool {
        match self.list.iter_mut().find(|resource| resource.name == name) {
            Some(resource) if resource.bytes.is_none() && bytes.len() <= MOST_BYTES_OF_A_RESOURCE => {
                resource.bytes = Some(bytes);
                true
            }
            _ => false,
        }
    }

    /// Makes the resource `name` kept at `link` (included in the project file, or linked by a path) and not where it was kept. Does nothing if there is no such resource.
    pub fn keep_at(&mut self, name: &str, link: SourceMediaLink) -> bool {
        match self.list.iter_mut().find(|resource| resource.name == name) {
            Some(resource) => {
                resource.link = link;
                true
            }
            None => false,
        }
    }

    /// Takes away the resources whose names are not in `used`. (What a project is saved with.)
    #[must_use]
    pub fn only_those_named_in(&self, used: &BTreeSet<String>) -> Self {
        Self {
            list: self
                .list
                .iter()
                .filter(|resource| used.contains(&resource.name))
                .cloned()
                .collect(),
        }
    }

    fn a_name_that_is_not_taken(&self, wanted: &str) -> String {
        if self.get(wanted).is_none() {
            return wanted.to_owned();
        }
        // ("grade.cube" becomes "grade (2).cube": the number goes before the ending, which the name of a file is known by. An ending that is long is not one, and the
        // number goes at the end of the name.)
        let (stem, ending) = match wanted.rfind('.') {
            Some(dot) if dot > 0 && wanted.len() - dot <= 16 => wanted.split_at(dot),
            _ => (wanted, ""),
        };
        (2..)
            .map(|number| {
                let suffix = format!(" ({number}){ending}");
                // (The beginning of the name goes, if the name with the number would be too long.)
                let room = MOST_CHARACTERS_IN_THE_NAME_OF_A_RESOURCE.saturating_sub(suffix.chars().count());
                let beginning: String = stem.chars().take(room).collect();
                format!("{beginning}{suffix}")
            })
            .find(|candidate| self.get(candidate).is_none())
            .expect("there are more numbers than resources")
    }
}

impl<'a> IntoIterator for &'a Resources {
    type Item = &'a Resource;
    type IntoIter = std::slice::Iter<'a, Resource>;

    fn into_iter(self) -> Self::IntoIter {
        self.list.iter()
    }
}

#[cfg(test)]
mod tests;
