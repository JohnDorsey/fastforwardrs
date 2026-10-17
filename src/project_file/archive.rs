// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The archive that a project file can be: a zip file that has the project in it, as the same JSON that a plain project file is,
//! and the files that the user chose to include in it (see [`crate::project::SourceMediaLink::included_as`]).
//!
//! Nothing in an archive is compressed: the files are the kind that are compressed already (videos, sounds, pictures), so it would
//! take time and give nothing, and as they are stored they are in the archive as they are. Any zip program can read it, and the
//! editor reads a file *where it is in the archive*, as a stretch of it (see [`crate::media::MediaLocation`]), without taking it out.
//!
//! **A file in an archive is written once and never changes or moves.** Saving a project that has several gigabytes of video included
//! in it must not write the video again (and on Windows could not replace the archive while a program has it open for reading: the
//! video is read in place). So a save *adds* to the archive: the new project text, a new directory and a new end record, after what is
//! there (see the `layout` module for how that is done so that a save that is cut off leaves the archive as it was). What the archive had
//! before is garbage in the middle of it, which is left, and is counted: [`compact_archive`] writes the archive again without it, as a
//! new file that replaces the old, which can be done when no program is reading the archive. An archive that is written *anew*
//! ([`write_archive`]) has no garbage in it.
//!
//! The entries are `project.fastforwardrs.json` and, for each file that is included, `resources/<number>-<file name>` (the
//! number is the identifier of the source file in the project, so that two files of one name are two entries). Names of entries
//! that come from a file are never used as paths: they are looked up in the archive, and a file that is taken out of one is put
//! where the program says, under a name the program makes.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use thiserror::Error;
use zip::ZipArchive;
use zip::result::ZipError;

mod layout;

use crate::atomic_file::replace_file_atomically;
use crate::media::MediaLocation;
use crate::project::SourceMediaIdentifier;
use layout::{
    Directory, FirstBytesOf, LayoutProblem, Placed, Writer, Zip64Thresholds, cut_off_the_unfinished_save,
    read_directory,
};

/// The name of the entry that has the project.
pub const PROJECT_ENTRY_NAME: &str = "project.fastforwardrs.json";

/// What the names of the entries of included files begin with.
const RESOURCES_FOLDER: &str = "resources/";

/// What the names of the entries of the resources of the project (the files that its effects use) begin with. (Not the same folder: the source files that are included
/// are numbered by the identifier of the source, and these by the CRC-32 of their bytes.)
const PROJECT_RESOURCES_FOLDER: &str = "project-resources/";

/// How long the name of an entry of an included file may be, in bytes (the most that a name of a file can be on most systems).
const LONGEST_ENTRY_NAME_IN_BYTES: usize = 255;

/// How much text a project may have in an archive, which a program that reads an archive that somebody made up should not
/// believe more of than there can be: a project of a few hundred clips is some hundreds of kilobytes.
const MOST_BYTES_OF_PROJECT_TEXT: u64 = 256 << 20;

/// How many bytes of a file are copied at a time.
const COPY_BUFFER_BYTES: usize = 1 << 20;

/// How many bytes an archive may have in use for writing it again to be a moment's work (a few seconds of a video is as much), which is
/// worth doing for any garbage (see [`ArchiveState::is_worth_compacting`]).
const A_LITTLE_TO_WRITE_AGAIN: u64 = 8 << 20;

/// The first four bytes of a zip file that has something in it.
const MARK_OF_A_ZIP_FILE: [u8; 4] = *b"PK\x03\x04";

/// What a program that is told how far a copy has got is told: how many bytes of the included files have been written so far, of all
/// that are to be. It may stop the writing by giving an error (of kind [`io::ErrorKind::Interrupted`]), and nothing is changed then.
pub type OnProgress<'a> = &'a mut dyn FnMut(u64) -> io::Result<()>;

#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("{path} could not be read: {source}")]
    CouldNotRead { path: PathBuf, source: io::Error },
    #[error("{path} could not be written: {source}")]
    CouldNotWrite { path: PathBuf, source: io::Error },
    #[error("{path} is not a project archive that can be read: {reason}")]
    Unreadable { path: PathBuf, reason: String },
    #[error("the archive {path} has no \"{entry_name}\" in it")]
    NoSuchEntry { path: PathBuf, entry_name: String },
    /// The progress said to stop, and the archive is as it was.
    #[error("it was stopped before it was done")]
    Stopped,
}

/// Where the bytes of a file to put in an archive are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileSource {
    /// A file of its own, or a stretch of another (an archive that has it), which they are read from.
    Location(MediaLocation),
    /// Bytes that the program has in memory (a resource of the project).
    Bytes(Arc<[u8]>),
}

impl FileSource {
    /// How long the file is, as far as can be told (a file that cannot be found has none).
    pub fn length(&self) -> u64 {
        match self {
            Self::Location(location) => location
                .stretch()
                .map(|stretch| stretch.length)
                .or_else(|| fs::metadata(location.file()).ok().map(|metadata| metadata.len()))
                .unwrap_or(0),
            Self::Bytes(bytes) => bytes.len() as u64,
        }
    }
}

impl From<MediaLocation> for FileSource {
    fn from(location: MediaLocation) -> Self {
        Self::Location(location)
    }
}

impl From<&Path> for FileSource {
    fn from(path: &Path) -> Self {
        Self::Location(path.into())
    }
}

impl From<PathBuf> for FileSource {
    fn from(path: PathBuf) -> Self {
        Self::Location(path.into())
    }
}

/// A file to put in an archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileToInclude {
    /// The name of its entry (see [`resource_entry_name`]).
    pub entry_name: String,
    /// Where its bytes are, which they are read from.
    pub source: FileSource,
}

/// What an archive says about one of its entries, without taking it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryFacts {
    /// How many bytes the file has.
    pub size: u64,
    /// The CRC-32 of the file, which is what tells it from another file of the same size.
    pub crc32: u32,
}

/// Where the bytes of an entry are in its archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryPlace {
    /// How far into the archive the first byte of the file is.
    pub data_start: u64,
    /// How many bytes of the archive it takes up, which is how long the file is if it is stored as it is.
    pub stored_length: u64,
    /// How long the file is.
    pub length: u64,
    pub crc32: u32,
    /// Whether the file is stored as it is (all that this program writes), and so can be read where it is. A file that another program
    /// compressed cannot.
    pub is_stored: bool,
}

impl EntryPlace {
    /// Where the file can be read where it is in the archive `archive`, if it is stored as it is.
    pub fn as_a_location_in(&self, archive: &Path) -> Option<MediaLocation> {
        self.is_stored
            .then(|| MediaLocation::of_a_stretch_of(archive, self.data_start, self.length))
    }
}

/// What an archive has in it, and how much of it is of no use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveState {
    pub entries: HashMap<String, EntryPlace>,
    /// How many bytes of the file are the entries, the directory and the end record.
    pub bytes_in_use: u64,
    /// How many bytes are what was in the archive before (see the beginning of this file), which [`compact_archive`] takes away.
    pub bytes_of_garbage: u64,
}

impl ArchiveState {
    fn of(directory: &Directory) -> Self {
        let entries = directory
            .entries
            .iter()
            .map(|entry| {
                (
                    entry.name.clone(),
                    EntryPlace {
                        data_start: entry.data_start,
                        stored_length: entry.stored_length,
                        length: entry.length,
                        crc32: entry.crc32,
                        is_stored: entry.is_stored(),
                    },
                )
            })
            .collect();
        Self {
            entries,
            bytes_in_use: directory.bytes_in_use(),
            bytes_of_garbage: directory.bytes_of_garbage(),
        }
    }

    /// How many bytes of the archive are in the files that are included in it (not the project text, nor the garbage).
    pub fn bytes_of_included_files(&self) -> u64 {
        self.entries
            .iter()
            .filter(|(name, _)| name.as_str() != PROJECT_ENTRY_NAME)
            .map(|(_, place)| place.stored_length)
            .sum()
    }

    /// Whether the garbage is enough to be worth taking away by writing the archive again, which takes as long as copying what is in
    /// use. **An archive that has little in use (8 MiB at most: a project that has no file included in it is only the text
    /// of the project) is written again for any garbage at all**, for that takes a moment, and a file that grew by the text of the
    /// project at every save would never stop. A bigger archive is written again when the garbage is at least half of what is in use
    /// (so that a project of a few gigabytes of video is not written again to take away a few hundred kilobytes of text that saves left).
    pub fn is_worth_compacting(&self) -> bool {
        if self.bytes_in_use <= A_LITTLE_TO_WRITE_AGAIN {
            return self.bytes_of_garbage > 0;
        }
        self.bytes_of_garbage >= self.bytes_in_use / 2
    }
}

/// Whether the file at `path` is a zip file, by the first bytes of it (a project file that is only JSON begins with a brace).
pub fn is_an_archive(path: &Path) -> bool {
    let mut first_bytes = [0_u8; 4];
    File::open(path)
        .and_then(|mut file| file.read_exact(&mut first_bytes))
        .is_ok()
        && first_bytes == MARK_OF_A_ZIP_FILE
}

/// The name of the entry for the file `file_name`, which is the source file `identifier` of its project.
///
/// The name of the file is kept (with what is in it that a name of a file cannot have, or must not, made `_`), so that what is
/// taken out of the archive has the name it had, and so does its ending, which a program that opens it may look at.
pub fn resource_entry_name(identifier: SourceMediaIdentifier, file_name: &str) -> String {
    let mut cleaned: String = file_name
        .chars()
        .map(|character| {
            if character.is_control() || "/\\:*?\"<>|".contains(character) {
                '_'
            } else {
                character
            }
        })
        .collect();
    cleaned = cleaned
        .trim_matches(|character| character == '.' || character == ' ')
        .to_owned();
    if cleaned.is_empty() {
        cleaned = "file".to_owned();
    }
    // (If the whole name is too long, the beginning of it goes and the ending, with its dot, stays.)
    let number = identifier.value().to_string();
    let room_for_the_name = LONGEST_ENTRY_NAME_IN_BYTES - RESOURCES_FOLDER.len() - number.len() - 1;
    while cleaned.len() > room_for_the_name {
        let first_character_length = cleaned.chars().next().map_or(1, char::len_utf8);
        cleaned.drain(..first_character_length);
    }
    format!("{RESOURCES_FOLDER}{number}-{cleaned}")
}

/// Whether `name` is a name that [`resource_entry_name`] could have made: of the folder for resources, then a number, a dash, and a
/// name of a file with nothing in it that is a part of a path. (A project file that is read is not to be believed about what
/// the entry of a file is called: the name is looked up in the archive, and made the name of a file in the cache, and so it is
/// checked.)
pub fn is_a_resource_entry_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(RESOURCES_FOLDER) else {
        return false;
    };
    let Some((number, file_name)) = rest.split_once('-') else {
        return false;
    };
    name.len() <= LONGEST_ENTRY_NAME_IN_BYTES
        && !number.is_empty()
        && number.chars().all(|character| character.is_ascii_digit())
        && !file_name.is_empty()
        && file_name != "."
        && file_name != ".."
        && !file_name
            .chars()
            .any(|character| character.is_control() || "/\\:*?\"<>|".contains(character))
}

/// The name of the entry for the resource of the project that is called `name` and has the bytes whose CRC-32 is `crc32`. **The CRC-32 is in it, so a resource that is
/// changed is another entry**, and one that is not is the entry that an archive has already: a save adds only what is new.
pub fn project_resource_entry_name(crc32: u32, name: &str) -> String {
    // (The name of a resource has been checked as the name of a file, so it is a name that can be a part of an entry's; the length is what is left to see to.)
    let number = crc32.to_string();
    let room_for_the_name = LONGEST_ENTRY_NAME_IN_BYTES - PROJECT_RESOURCES_FOLDER.len() - number.len() - 1;
    let mut cleaned = name.to_owned();
    while cleaned.len() > room_for_the_name {
        let first_character_length = cleaned.chars().next().map_or(1, char::len_utf8);
        cleaned.drain(..first_character_length);
    }
    format!("{PROJECT_RESOURCES_FOLDER}{number}-{cleaned}")
}

/// Whether `name` is a name that [`project_resource_entry_name`] could have made. (A project file that is read is not to be believed about what an entry is called.)
pub fn is_a_project_resource_entry_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(PROJECT_RESOURCES_FOLDER) else {
        return false;
    };
    let Some((number, file_name)) = rest.split_once('-') else {
        return false;
    };
    name.len() <= LONGEST_ENTRY_NAME_IN_BYTES
        && number.parse::<u32>().is_ok()
        && !file_name.is_empty()
        && file_name != "."
        && file_name != ".."
        && !file_name
            .chars()
            .any(|character| character.is_control() || "/\\:*?\"<>|".contains(character))
}

/// The name that the file of a resource entry had: what is after the number and the dash. `None` if `name` is not the name
/// of an entry of a resource.
pub fn file_name_of_a_resource_entry(name: &str) -> Option<&str> {
    if !is_a_resource_entry_name(name) {
        return None;
    }
    name.strip_prefix(RESOURCES_FOLDER)?
        .split_once('-')
        .map(|(_, file_name)| file_name)
}

// --- writing ---

/// Where the bytes of an entry that is being written come from.
struct Opened {
    reader: Box<dyn Read>,
    length: u64,
    /// What the CRC-32 of the bytes must be, if it is known (a file that is copied from an archive has it).
    known_crc: Option<u32>,
}

impl Opened {
    fn text(text: &str) -> Self {
        Self {
            length: text.len() as u64,
            reader: Box::new(Cursor::new(text.as_bytes().to_vec())),
            known_crc: None,
        }
    }

    /// The bytes of `source`: all of a file, the stretch of one that it is, or bytes in memory.
    fn of(source: &FileSource) -> io::Result<Self> {
        match source {
            FileSource::Location(location) => Self::at(location),
            FileSource::Bytes(bytes) => Ok(Self {
                length: bytes.len() as u64,
                reader: Box::new(Cursor::new(bytes.clone())),
                known_crc: Some(crc32fast::hash(bytes)),
            }),
        }
    }

    /// The bytes at `location`: all of a file, or the stretch of one that it is.
    fn at(location: &MediaLocation) -> io::Result<Self> {
        let mut file = File::open(location.file())?;
        let length = match location.stretch() {
            Some(stretch) => {
                file.seek(SeekFrom::Start(stretch.start))?;
                stretch.length
            }
            None => file.metadata()?.len(),
        };
        Ok(Self {
            reader: Box::new(file.take(length)),
            length,
            known_crc: None,
        })
    }
}

/// Opens the project text and everything in `files`, before anything is written, so that one that cannot be read is said so as that.
fn open_what_is_to_be_written(
    project_text: &str,
    files: &[FileToInclude],
) -> Result<Vec<(String, Opened)>, ArchiveError> {
    let mut opened = vec![(PROJECT_ENTRY_NAME.to_owned(), Opened::text(project_text))];
    for included in files {
        let source = Opened::of(&included.source).map_err(|source| ArchiveError::CouldNotRead {
            path: match &included.source {
                FileSource::Location(location) => location.file().to_owned(),
                FileSource::Bytes(_) => PathBuf::from(&included.entry_name),
            },
            source,
        })?;
        opened.push((included.entry_name.clone(), source));
    }
    Ok(opened)
}

/// Writes all of `opened` with `writer`, telling `on_progress` how many bytes of them have been written in all.
fn write_all_of(
    writer: &mut Writer<'_>,
    opened: Vec<(String, Opened)>,
    on_progress: OnProgress<'_>,
) -> io::Result<Vec<(String, Placed)>> {
    let mut written_before = 0_u64;
    let mut placed = Vec::with_capacity(opened.len());
    for (name, mut source) in opened {
        let length = source.length;
        let at = writer.write_entry(&name, length, source.known_crc, &mut source.reader, &mut |written| {
            on_progress(written_before + written)
        })?;
        written_before += length;
        placed.push((name, at));
    }
    Ok(placed)
}

/// What an error of writing is, for the user.
fn error_of_writing(path: &Path, error: io::Error) -> ArchiveError {
    if error.kind() == io::ErrorKind::Interrupted {
        ArchiveError::Stopped
    } else {
        ArchiveError::CouldNotWrite {
            path: path.to_owned(),
            source: error,
        }
    }
}

fn error_of_the_layout(path: &Path, problem: LayoutProblem) -> ArchiveError {
    match problem {
        LayoutProblem::Io(source) => ArchiveError::CouldNotRead {
            path: path.to_owned(),
            source,
        },
        LayoutProblem::NoDirectory => ArchiveError::Unreadable {
            path: path.to_owned(),
            reason: "it has no whole list of the files in it".to_owned(),
        },
    }
}

/// Writes the archive at `destination`, replacing the file there if there is one, with the project as `project_text` and the files
/// `files` in it, all of it new: there is no garbage in it. Nothing is changed unless the whole of it is written.
pub fn write_archive(destination: &Path, project_text: &str, files: &[FileToInclude]) -> Result<(), ArchiveError> {
    write_archive_telling(destination, project_text, files, &mut |_| Ok(()))
}

/// [`write_archive`], telling `on_progress` how it goes.
pub fn write_archive_telling(
    destination: &Path,
    project_text: &str,
    files: &[FileToInclude],
    on_progress: OnProgress<'_>,
) -> Result<(), ArchiveError> {
    write_archive_using(destination, project_text, files, Zip64Thresholds::REAL, on_progress)
}

/// [`write_archive_telling`], with `thresholds` for what is too big for the ordinary fields of a zip file (the real ones are
/// too much to write in a test, so that a test brings them down).
fn write_archive_using(
    destination: &Path,
    project_text: &str,
    files: &[FileToInclude],
    thresholds: Zip64Thresholds,
    on_progress: OnProgress<'_>,
) -> Result<(), ArchiveError> {
    let opened = open_what_is_to_be_written(project_text, files)?;
    replace_file_atomically(destination, |file| {
        let mut writer = Writer::starting_at(file, 0, thresholds)?;
        write_all_of(&mut writer, opened, on_progress)?;
        writer.finish().map(|_| ())
    })
    .map_err(|error| error_of_writing(destination, error))
}

/// Adds to the archive at `archive`: the project text `project_text` (which replaces the one that is there), and the files
/// `new_files`; and of the files that are in it already, those named in `keep` stay as they are, and the others are no longer
/// in it (their bytes stay in the file as garbage until [`compact_archive`]). The files that stay are not written again, and none
/// moves: where a program is reading one in place, it goes on reading it. If it does not work, or `on_progress` stops it, the archive
/// is as it was.
pub fn append_to_archive(
    archive: &Path,
    project_text: &str,
    keep: &[String],
    new_files: &[FileToInclude],
    on_progress: OnProgress<'_>,
) -> Result<ArchiveState, ArchiveError> {
    append_using(
        archive,
        project_text,
        keep,
        new_files,
        Zip64Thresholds::REAL,
        on_progress,
    )
}

fn append_using(
    archive: &Path,
    project_text: &str,
    keep: &[String],
    new_files: &[FileToInclude],
    thresholds: Zip64Thresholds,
    on_progress: OnProgress<'_>,
) -> Result<ArchiveState, ArchiveError> {
    let opened = open_what_is_to_be_written(project_text, new_files)?;
    let could_not_write = |source: io::Error| error_of_writing(archive, source);
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(archive)
        .map_err(could_not_write)?;
    let directory = read_directory(&mut file).map_err(|problem| error_of_the_layout(archive, problem))?;
    // (What a save that did not finish left is cut off first, so that the file is as it was after the last that did.)
    cut_off_the_unfinished_save(&mut file, &directory).map_err(could_not_write)?;
    let position = file.metadata().map_err(could_not_write)?.len();
    let written = (|| {
        let mut writer = Writer::starting_at(&mut file, position, thresholds)?;
        for entry in &directory.entries {
            let is_replaced =
                entry.name == PROJECT_ENTRY_NAME || new_files.iter().any(|new| new.entry_name == entry.name);
            if keep.contains(&entry.name) && !is_replaced {
                writer.keep(entry)?;
            }
        }
        write_all_of(&mut writer, opened, on_progress)?;
        writer.finish().map(|_| ())
    })();
    if let Err(error) = written {
        // Back to what the archive was: the end record that was last whole is still the last, and what was added after it is gone.
        let _ = file.set_len(position);
        return Err(error_of_writing(archive, error));
    }
    drop(file);
    state_of_archive(archive)
}

/// Writes the archive at `archive` again without the garbage in it, as a new file that replaces it. The files in it are copied as
/// they are, and are checked as they are copied against the CRC-32 that the archive says they have: one that is damaged stops it, and
/// nothing is changed. **The files are in other places in the archive afterwards**, so nothing may be reading them in place (on Windows
/// a file that is open cannot be replaced: that fails, and nothing is changed).
pub fn compact_archive(archive: &Path, on_progress: OnProgress<'_>) -> Result<ArchiveState, ArchiveError> {
    let directory = directory_of(archive)?;
    let mut opened = Vec::with_capacity(directory.entries.len());
    for entry in &directory.entries {
        let could_not_read = |source: io::Error| ArchiveError::CouldNotRead {
            path: archive.to_owned(),
            source,
        };
        let source = if entry.is_stored() {
            let mut file = File::open(archive).map_err(could_not_read)?;
            file.seek(SeekFrom::Start(entry.data_start)).map_err(could_not_read)?;
            Opened {
                length: entry.length,
                reader: Box::new(file.take(entry.length)),
                known_crc: Some(entry.crc32),
            }
        } else {
            // (A file that another program compressed is written stored: it is taken out first, into a file that is gone when it is done.)
            let mut taken_out = tempfile::tempfile().map_err(could_not_read)?;
            extract_entry_into(archive, &entry.name, &mut taken_out)?;
            taken_out.seek(SeekFrom::Start(0)).map_err(could_not_read)?;
            Opened {
                length: entry.length,
                reader: Box::new(taken_out),
                known_crc: Some(entry.crc32),
            }
        };
        opened.push((entry.name.clone(), source));
    }
    replace_file_atomically(archive, |file| {
        let mut writer = Writer::starting_at(file, 0, Zip64Thresholds::REAL)?;
        write_all_of(&mut writer, opened, on_progress)?;
        writer.finish().map(|_| ())
    })
    .map_err(|error| error_of_writing(archive, error))?;
    state_of_archive(archive)
}

// --- reading ---

/// What is in the archive at `archive`, and how much of it is garbage.
pub fn state_of_archive(archive: &Path) -> Result<ArchiveState, ArchiveError> {
    Ok(ArchiveState::of(&directory_of(archive)?))
}

fn directory_of(archive: &Path) -> Result<Directory, ArchiveError> {
    let mut file = File::open(archive).map_err(|source| ArchiveError::CouldNotRead {
        path: archive.to_owned(),
        source,
    })?;
    read_directory(&mut file).map_err(|problem| error_of_the_layout(archive, problem))
}

/// Where the file `entry_name` is in the archive at `archive`.
pub fn entry_place(archive: &Path, entry_name: &str) -> Result<EntryPlace, ArchiveError> {
    let state = state_of_archive(archive)?;
    state
        .entries
        .get(entry_name)
        .copied()
        .ok_or_else(|| ArchiveError::NoSuchEntry {
            path: archive.to_owned(),
            entry_name: entry_name.to_owned(),
        })
}

/// What an archive says about the entry `entry_name`: how long the file is, and its CRC-32.
pub fn facts_of_entry(archive: &Path, entry_name: &str) -> Result<EntryFacts, ArchiveError> {
    let place = entry_place(archive, entry_name)?;
    Ok(EntryFacts {
        size: place.length,
        crc32: place.crc32,
    })
}

/// The archive for the zip crate to read: as far as the last end record that is whole (what a save that was cut off wrote after it is
/// not there for the crate), and the file itself is not changed.
type OpenedArchive = ZipArchive<BufReader<FirstBytesOf<File>>>;

fn open_archive(path: &Path) -> Result<OpenedArchive, ArchiveError> {
    let mut file = File::open(path).map_err(|source| ArchiveError::CouldNotRead {
        path: path.to_owned(),
        source,
    })?;
    let directory = read_directory(&mut file).map_err(|problem| error_of_the_layout(path, problem))?;
    let whole_part = FirstBytesOf::new(file, directory.end_of_the_last_end_record);
    ZipArchive::new(BufReader::new(whole_part)).map_err(|error| match error {
        ZipError::Io(source) => ArchiveError::CouldNotRead {
            path: path.to_owned(),
            source,
        },
        other => unreadable(path, &other),
    })
}

/// The text of the project in the archive at `archive`.
pub fn read_project_text(archive: &Path) -> Result<String, ArchiveError> {
    let mut zip = open_archive(archive)?;
    let entry = zip.by_name(PROJECT_ENTRY_NAME).map_err(|error| match error {
        ZipError::FileNotFound => ArchiveError::NoSuchEntry {
            path: archive.to_owned(),
            entry_name: PROJECT_ENTRY_NAME.to_owned(),
        },
        other => unreadable(archive, &other),
    })?;
    if entry.size() > MOST_BYTES_OF_PROJECT_TEXT {
        return Err(ArchiveError::Unreadable {
            path: archive.to_owned(),
            reason: format!(
                "the project in it is {} bytes long, which is more than a project is",
                entry.size()
            ),
        });
    }
    let mut text = String::new();
    entry
        .take(MOST_BYTES_OF_PROJECT_TEXT)
        .read_to_string(&mut text)
        .map_err(|error| ArchiveError::Unreadable {
            path: archive.to_owned(),
            reason: format!("the project in it could not be read: {error}"),
        })?;
    Ok(text)
}

/// The bytes of the entry `entry_name` of the archive at `archive`, which is at most `most_bytes` long (a longer one is an error: it is not read). The CRC-32 of what was
/// read is checked against the archive's.
pub fn read_entry_bytes(archive: &Path, entry_name: &str, most_bytes: usize) -> Result<Vec<u8>, ArchiveError> {
    let mut zip = open_archive(archive)?;
    let entry = zip.by_name(entry_name).map_err(|error| match error {
        ZipError::FileNotFound => ArchiveError::NoSuchEntry {
            path: archive.to_owned(),
            entry_name: entry_name.to_owned(),
        },
        other => unreadable(archive, &other),
    })?;
    if entry.size() > most_bytes as u64 {
        return Err(ArchiveError::Unreadable {
            path: archive.to_owned(),
            reason: format!(
                "\"{entry_name}\" in it is {} bytes long, which is more than {most_bytes}",
                entry.size()
            ),
        });
    }
    let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
    // (A damaged file is found out at the end of it, where the zip file's check of it is made: that is an error of reading.)
    entry
        .take(most_bytes as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| ArchiveError::Unreadable {
            path: archive.to_owned(),
            reason: format!("\"{entry_name}\" in it is damaged: {error}"),
        })?;
    Ok(bytes)
}

/// Takes the entry `entry_name` out of the archive at `archive` and puts it in a file at `destination`, which is replaced if it is
/// there, and is there whole or not at all. The CRC-32 of what was taken out is checked against the archive's.
pub fn extract_entry(archive: &Path, entry_name: &str, destination: &Path) -> Result<(), ArchiveError> {
    let mut problem_with_the_archive = None;
    let result = replace_file_atomically(destination, |file| {
        extract_entry_into(archive, entry_name, file).map_err(|error| {
            let message = error.to_string();
            problem_with_the_archive = Some(error);
            io::Error::other(message)
        })
    });
    if let Some(error) = problem_with_the_archive {
        return Err(error);
    }
    result.map_err(|source| ArchiveError::CouldNotWrite {
        path: destination.to_owned(),
        source,
    })
}

/// Writes the file `entry_name` of the archive at `archive` into `destination`.
fn extract_entry_into(archive: &Path, entry_name: &str, destination: &mut File) -> Result<(), ArchiveError> {
    let mut zip = open_archive(archive)?;
    let mut entry = zip.by_name(entry_name).map_err(|error| match error {
        ZipError::FileNotFound => ArchiveError::NoSuchEntry {
            path: archive.to_owned(),
            entry_name: entry_name.to_owned(),
        },
        other => unreadable(archive, &other),
    })?;
    let could_not_write = |source: io::Error| ArchiveError::CouldNotWrite {
        path: archive.to_owned(),
        source,
    };
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    loop {
        // (A damaged file is found out at the end of it, where the zip file's check of it is made: that is an error of reading.)
        let read = entry.read(&mut buffer).map_err(|error| ArchiveError::Unreadable {
            path: archive.to_owned(),
            reason: format!("\"{entry_name}\" in it is damaged: {error}"),
        })?;
        if read == 0 {
            return destination.flush().map_err(could_not_write);
        }
        destination.write_all(&buffer[..read]).map_err(could_not_write)?;
    }
}

fn unreadable(path: &Path, error: &ZipError) -> ArchiveError {
    ArchiveError::Unreadable {
        path: path.to_owned(),
        reason: error.to_string(),
    }
}

/// Writes all the bytes at `location` into `destination`, and says how many there were.
pub fn copy_bytes_of(location: &MediaLocation, destination: &mut impl Write) -> io::Result<u64> {
    let mut opened = Opened::at(location)?;
    io::copy(&mut opened.reader, destination)
}

/// Whether the two places have exactly the same bytes (not by their names or when they were made, but byte for byte).
pub fn have_the_same_contents(first: impl Into<MediaLocation>, second: impl Into<MediaLocation>) -> io::Result<bool> {
    let (mut first, mut second) = (Opened::at(&first.into())?, Opened::at(&second.into())?);
    if first.length != second.length {
        return Ok(false);
    }
    let mut first_buffer = vec![0_u8; COPY_BUFFER_BYTES];
    let mut second_buffer = vec![0_u8; COPY_BUFFER_BYTES];
    loop {
        let read = read_as_much_as_there_is(&mut first.reader, &mut first_buffer)?;
        let read_of_the_second = read_as_much_as_there_is(&mut second.reader, &mut second_buffer)?;
        if read != read_of_the_second || first_buffer[..read] != second_buffer[..read] {
            return Ok(false);
        }
        if read == 0 {
            return Ok(true);
        }
    }
}

/// Reads into `buffer` until it is full or there is no more, and says how many bytes that was.
fn read_as_much_as_there_is(reader: &mut dyn Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        let read = reader.read(&mut buffer[filled..])?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    Ok(filled)
}

/// Makes the folder `folder` and the ones above it that are not there, for a file that is to be put in it.
pub fn make_the_folder_for(file: &Path) -> io::Result<()> {
    match file.parent() {
        Some(folder) if !folder.as_os_str().is_empty() => fs::create_dir_all(folder),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;
