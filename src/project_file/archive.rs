//! The archive that a project file can be: a zip file that has the project in it, as the same JSON that a plain project file is,
//! and the files that the user chose to include in it (see [`crate::project::SourceMediaLink::included_as`]).
//!
//! Nothing in an archive is compressed: the files are the kind that are compressed already (videos, sounds, pictures), so it would
//! take time and give nothing, and as they are stored they are in the archive as they are, which any zip program can read and
//! which a program could read from where they are without taking them out first. An archive is written all at once to a
//! temporary file that replaces the old archive when it is whole, so a failure never leaves half of one.
//!
//! The entries are `project.fastforwardrs.json` and, for each file that is included, `resources/<number>-<file name>` (the
//! number is the identifier of the source file in the project, so that two files of one name are two entries). Names of entries
//! that come from a file are never used as paths: they are looked up in the archive, and a file that is taken out of one is put
//! where the program says, under a name the program makes.

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use thiserror::Error;
use zip::result::ZipError;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::atomic_file::replace_file_atomically;
use crate::project::SourceMediaIdentifier;

/// The name of the entry that has the project.
pub const PROJECT_ENTRY_NAME: &str = "project.fastforwardrs.json";

/// What the names of the entries of included files begin with.
const RESOURCES_FOLDER: &str = "resources/";

/// How long the name of an entry of an included file may be, in bytes (the most that a name of a file can be on most systems).
const LONGEST_ENTRY_NAME_IN_BYTES: usize = 255;

/// How much text a project may have in an archive, which a program that reads an archive that somebody made up should not
/// believe more of than there can be: a project of a few hundred clips is some hundreds of kilobytes.
const MOST_BYTES_OF_PROJECT_TEXT: u64 = 256 << 20;

/// How many bytes of a file are copied at a time.
const COPY_BUFFER_BYTES: usize = 1 << 20;

/// The first four bytes of a zip file that has something in it.
const MARK_OF_A_ZIP_FILE: [u8; 4] = *b"PK\x03\x04";

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
}

/// A file to put in an archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileToInclude {
    /// The name of its entry (see [`resource_entry_name`]).
    pub entry_name: String,
    /// Where the file is, which it is read from.
    pub path: PathBuf,
}

/// What an archive says about one of its entries, without taking it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryFacts {
    /// How many bytes the file has.
    pub size: u64,
    /// The CRC-32 of the file, which is what tells it from another file of the same size.
    pub crc32: u32,
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

/// How long a file has to be to be written in the zip64 form, which is the way that the zip format has for files that are too long
/// for its ordinary one (which says how long a file is in four bytes, and so cannot say it for one of four gigabytes or more): the
/// zip crate's own limit, one byte short of 4 GiB.
const LENGTH_FROM_WHICH_A_FILE_IS_IN_THE_ZIP64_FORM: u64 = zip::ZIP64_BYTES_THR;

/// Writes the archive at `destination`, replacing the file there if there is one, with the project as `project_text` and the files
/// `files` in it. Nothing is changed unless the whole of it is written.
pub fn write_archive(destination: &Path, project_text: &str, files: &[FileToInclude]) -> Result<(), ArchiveError> {
    write_archive_with_the_zip64_form_from(
        destination,
        project_text,
        files,
        LENGTH_FROM_WHICH_A_FILE_IS_IN_THE_ZIP64_FORM,
    )
}

/// [`write_archive`], with the files that are `zip64_from` bytes long or longer written in the zip64 form. (The length that the real
/// limit is, is too much to write in a test, so that a test brings it down.)
fn write_archive_with_the_zip64_form_from(
    destination: &Path,
    project_text: &str,
    files: &[FileToInclude],
    zip64_from: u64,
) -> Result<(), ArchiveError> {
    // The files are opened first, so that one that cannot be read is said so, and not as a failure to write.
    let mut sources = Vec::with_capacity(files.len());
    for included in files {
        let could_not_read = |source| ArchiveError::CouldNotRead {
            path: included.path.clone(),
            source,
        };
        let source = File::open(&included.path).map_err(could_not_read)?;
        let length = source.metadata().map_err(could_not_read)?.len();
        sources.push((&included.entry_name, source, length));
    }
    let could_not_write = |source: io::Error| ArchiveError::CouldNotWrite {
        path: destination.to_owned(),
        source,
    };
    replace_file_atomically(destination, |file| {
        let mut writer = ZipWriter::new(BufWriter::new(file));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        writer.start_file(PROJECT_ENTRY_NAME, options).map_err(io_error_of)?;
        writer.write_all(project_text.as_bytes())?;
        let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
        for (entry_name, mut source, length) in sources {
            // (The zip crate cannot go back and say that a file is a long one once it has begun to write it: it says so before, and
            // stops with an error when a file that was not said to be goes on past four gigabytes.)
            let options = options.large_file(length >= zip64_from);
            writer.start_file(entry_name.as_str(), options).map_err(io_error_of)?;
            loop {
                let read = source.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                writer.write_all(&buffer[..read])?;
            }
        }
        writer.finish().map_err(io_error_of)?.flush()
    })
    .map_err(could_not_write)
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

/// What the archive at `archive` says about the entry `entry_name`: how long the file is, and its CRC-32.
pub fn facts_of_entry(archive: &Path, entry_name: &str) -> Result<EntryFacts, ArchiveError> {
    let mut zip = open_archive(archive)?;
    let entry = entry_of(&mut zip, archive, entry_name)?;
    Ok(EntryFacts {
        size: entry.size(),
        crc32: entry.crc32(),
    })
}

/// Takes the entry `entry_name` out of the archive at `archive` and puts it in a file at `destination`, which is replaced if it is
/// there, and is there whole or not at all. The CRC-32 of what was taken out is checked against the archive's.
pub fn extract_entry(archive: &Path, entry_name: &str, destination: &Path) -> Result<(), ArchiveError> {
    let mut zip = open_archive(archive)?;
    let mut entry = entry_of(&mut zip, archive, entry_name)?;
    let mut problem_with_the_archive = None;
    let result = replace_file_atomically(destination, |file| {
        let mut writer = BufWriter::new(file);
        let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
        loop {
            // (A damaged file is found out at the end of it, where the zip file's check of it is made: that is an error of reading.)
            let read = match entry.read(&mut buffer) {
                Ok(read) => read,
                Err(error) => {
                    problem_with_the_archive = Some(error);
                    return Err(io::Error::other("the archive could not be read"));
                }
            };
            if read == 0 {
                break;
            }
            writer.write_all(&buffer[..read])?;
        }
        writer.flush()
    });
    if let Some(error) = problem_with_the_archive {
        return Err(ArchiveError::Unreadable {
            path: archive.to_owned(),
            reason: format!("\"{entry_name}\" in it is damaged: {error}"),
        });
    }
    result.map_err(|source| ArchiveError::CouldNotWrite {
        path: destination.to_owned(),
        source,
    })
}

fn open_archive(path: &Path) -> Result<ZipArchive<BufReader<File>>, ArchiveError> {
    let file = File::open(path).map_err(|source| ArchiveError::CouldNotRead {
        path: path.to_owned(),
        source,
    })?;
    ZipArchive::new(BufReader::new(file)).map_err(|error| match error {
        ZipError::Io(source) => ArchiveError::CouldNotRead {
            path: path.to_owned(),
            source,
        },
        other => unreadable(path, &other),
    })
}

fn entry_of<'a>(
    zip: &'a mut ZipArchive<BufReader<File>>,
    archive: &Path,
    entry_name: &str,
) -> Result<zip::read::ZipFile<'a, BufReader<File>>, ArchiveError> {
    zip.by_name(entry_name).map_err(|error| match error {
        ZipError::FileNotFound => ArchiveError::NoSuchEntry {
            path: archive.to_owned(),
            entry_name: entry_name.to_owned(),
        },
        other => unreadable(archive, &other),
    })
}

fn unreadable(path: &Path, error: &ZipError) -> ArchiveError {
    ArchiveError::Unreadable {
        path: path.to_owned(),
        reason: error.to_string(),
    }
}

fn io_error_of(error: ZipError) -> io::Error {
    match error {
        ZipError::Io(error) => error,
        other => io::Error::other(other.to_string()),
    }
}

/// Whether the two files have exactly the same contents (not by their names or when they were made, but byte for byte).
pub fn have_the_same_contents(first: &Path, second: &Path) -> io::Result<bool> {
    let (mut first_file, mut second_file) = (File::open(first)?, File::open(second)?);
    if first_file.metadata()?.len() != second_file.metadata()?.len() {
        return Ok(false);
    }
    let mut first_buffer = vec![0_u8; COPY_BUFFER_BYTES];
    let mut second_buffer = vec![0_u8; COPY_BUFFER_BYTES];
    loop {
        let read = read_as_much_as_there_is(&mut first_file, &mut first_buffer)?;
        let read_of_the_second = read_as_much_as_there_is(&mut second_file, &mut second_buffer)?;
        if read != read_of_the_second || first_buffer[..read] != second_buffer[..read] {
            return Ok(false);
        }
        if read == 0 {
            return Ok(true);
        }
    }
}

/// Reads into `buffer` until it is full or the file has no more, and says how many bytes that was.
fn read_as_much_as_there_is(file: &mut File, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        let read = file.read(&mut buffer[filled..])?;
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
