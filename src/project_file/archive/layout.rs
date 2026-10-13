//! The layout of a zip file, as far as this program has to write it and mend it itself.
//!
//! A zip file is the entries one after another (each a small header and then the bytes of the file, which for the files in a project
//! archive are stored as they are), and after them a *directory* that says where each entry is, and an *end record* that says where the
//! directory is. A reader finds the end record at the end of the file, and the rest from that. The zip crate reads such files; what it
//! cannot do is **add to one without taking its directory away for a while**, and that is what a project that has several gigabytes of
//! video included in it needs: the video is written once and never moves, and every later save only adds the new project text, a new
//! directory and a new end record after what is there:
//!
//! ```text
//! entries... | project (old) | directory | end   | project (new) | directory | end
//!                              \_ none of this is used any more _/
//! ```
//!
//! The old project text, directory and end record are *garbage*: they stay in the file, which is as it was up to its old end, until
//! the whole archive is written again (see [`super::compact_archive`]). A save that is cut off half-way leaves bytes after the last
//! whole end record that do not belong to any directory; they are found by what they begin with, and cut off (or just not read) the next
//! time. The end record is written last, after what it points at is on the disk, so the last *whole* end record is always a state that
//! the archive was in.
//!
//! The entries that stay are not written again: their records in the directory are copied as they are, so that where each entry is
//! (which is what a program that reads a stretch of the file in place goes by) does not change.

use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};

use crc32fast::Hasher;

const LOCAL_HEADER_SIGNATURE: [u8; 4] = *b"PK\x03\x04";
const DIRECTORY_RECORD_SIGNATURE: [u8; 4] = *b"PK\x01\x02";
const END_RECORD_SIGNATURE: [u8; 4] = *b"PK\x05\x06";
const ZIP64_END_RECORD_SIGNATURE: [u8; 4] = *b"PK\x06\x06";
const ZIP64_LOCATOR_SIGNATURE: [u8; 4] = *b"PK\x06\x07";

const LOCAL_HEADER_BYTES: u64 = 30;
const DIRECTORY_RECORD_BYTES: usize = 46;
const END_RECORD_BYTES: u64 = 22;
const ZIP64_END_RECORD_BYTES: u64 = 56;
const ZIP64_LOCATOR_BYTES: u64 = 20;

/// What a field of four bytes says when the number is too big for it and is in the zip64 extra field instead (two bytes: `0xFFFF`).
const SEE_ZIP64_FIELD: u32 = 0xFFFF_FFFF;
const SEE_ZIP64_COUNT: u16 = 0xFFFF;
const ZIP64_EXTRA_FIELD_ID: u16 = 0x0001;

/// Bit 11 of the flags: the name is UTF-8.
const FLAG_NAME_IS_UTF8: u16 = 1 << 11;
/// The date and time that every entry has: 1980-01-01 00:00:00, the first that a zip file can say. (They say when a file was made,
/// which is of no use here, and that they are all alike keeps an archive of the same things the same.)
const DOS_DATE: u16 = 0x0021;
const DOS_TIME: u16 = 0;
const VERSION_MADE_BY: u16 = 45;
const VERSION_NEEDED: u16 = 20;
const VERSION_NEEDED_FOR_ZIP64: u16 = 45;

/// The most bytes a directory may have that is believed: a program that reads an archive somebody made up should not take more of
/// memory than there can be (an archive of a few thousand files has a directory of a few hundred kilobytes).
const LARGEST_DIRECTORY_BYTES: u64 = 256 << 20;

/// How many bytes of the file are looked at at a time when the end record is looked for from the end.
const SEARCH_WINDOW_BYTES: u64 = 64 << 10;

/// How many bytes of a file are copied at a time.
const COPY_BUFFER_BYTES: usize = 1 << 20;

/// What counts as too big for the ordinary fields of a zip file, so that the zip64 ones are used. The real numbers are the largest
/// that the fields can have ([`Self::REAL`]); a test brings them down, so that the zip64 forms are made without gigabytes of data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Zip64Thresholds {
    /// A file of this many bytes or more is said to be of that length in the zip64 field.
    pub length: u64,
    /// A place in the archive of this many bytes in or further (where an entry begins, or the directory) is said so in the zip64 field.
    pub offset: u64,
    /// This many entries or more are counted in the zip64 field of the end record.
    pub count: u64,
}

impl Zip64Thresholds {
    pub(super) const REAL: Self = Self {
        length: SEE_ZIP64_FIELD as u64,
        offset: SEE_ZIP64_FIELD as u64,
        count: SEE_ZIP64_COUNT as u64,
    };
}

#[derive(Debug, thiserror::Error)]
pub(super) enum LayoutProblem {
    #[error("{0}")]
    Io(#[from] io::Error),
    /// There is no end record that goes with a directory that is whole.
    #[error("it has no whole directory of entries in it")]
    NoDirectory,
}

/// One entry of an archive, as its directory says.
#[derive(Clone, Debug)]
pub(super) struct Entry {
    pub name: String,
    /// Where its header begins.
    pub header_start: u64,
    /// Where the bytes of the file begin.
    pub data_start: u64,
    /// How many bytes it takes up after its header (which is how many the file has, when it is stored as it is).
    pub stored_length: u64,
    /// How long the file is.
    pub length: u64,
    /// 0 for a file that is stored as it is, 8 for one that is compressed.
    pub method: u16,
    pub crc32: u32,
    /// Its record in the directory, as it is in the file.
    record: Vec<u8>,
}

impl Entry {
    pub(super) fn is_stored(&self) -> bool {
        self.method == 0
    }

    /// How many bytes of the file the entry takes up, the header too.
    fn bytes_taken_up(&self) -> u64 {
        self.data_start - self.header_start + self.stored_length
    }
}

/// What the last whole end record of an archive says, and what it means.
#[derive(Clone, Debug)]
pub(super) struct Directory {
    pub entries: Vec<Entry>,
    /// Where the directory begins.
    pub directory_start: u64,
    /// Where the last whole end record ends. What the file has after that does not belong to any directory.
    pub end_of_the_last_end_record: u64,
    /// How long the file is.
    pub file_length: u64,
}

impl Directory {
    #[cfg(test)]
    pub(super) fn entry_named(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// How many bytes of the file are in use: the entries, the directory and the end record.
    pub(super) fn bytes_in_use(&self) -> u64 {
        let entries: u64 = self.entries.iter().map(Entry::bytes_taken_up).sum();
        entries + (self.end_of_the_last_end_record - self.directory_start)
    }

    /// How many bytes of the file are of no use: what was in the directory before, and what comes after the last end record.
    pub(super) fn bytes_of_garbage(&self) -> u64 {
        self.file_length.saturating_sub(self.bytes_in_use())
    }
}

fn little_endian_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

fn little_endian_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn little_endian_u64(bytes: &[u8]) -> u64 {
    let mut eight = [0_u8; 8];
    eight.copy_from_slice(&bytes[..8]);
    u64::from_le_bytes(eight)
}

fn read_at(file: &mut File, at: u64, bytes: &mut [u8]) -> io::Result<()> {
    file.seek(SeekFrom::Start(at))?;
    file.read_exact(bytes)
}

/// Reads the directory that the last whole end record of `file` points at.
pub(super) fn read_directory(file: &mut File) -> Result<Directory, LayoutProblem> {
    let file_length = file.metadata()?.len();
    let mut window_end = file_length;
    loop {
        let window_start = window_end.saturating_sub(SEARCH_WINDOW_BYTES);
        let mut window = vec![0_u8; (window_end - window_start) as usize];
        read_at(file, window_start, &mut window)?;
        for offset in (0..window.len().saturating_sub(3)).rev() {
            if window[offset..offset + 4] != END_RECORD_SIGNATURE {
                continue;
            }
            if let Some(directory) = directory_of_the_end_record_at(file, window_start + offset as u64, file_length)? {
                return Ok(directory);
            }
        }
        if window_start == 0 {
            return Err(LayoutProblem::NoDirectory);
        }
        // (The next window overlaps this one by three bytes, for a signature that is half in one and half in the other.)
        window_end = window_start + 3;
    }
}

/// The directory that the end record that begins at `position` points at, if that is a whole one: the places that it says are inside
/// the file, and the records in the directory are all there, and the right number of them.
fn directory_of_the_end_record_at(
    file: &mut File,
    position: u64,
    file_length: u64,
) -> Result<Option<Directory>, LayoutProblem> {
    if position + END_RECORD_BYTES > file_length {
        return Ok(None);
    }
    let mut record = [0_u8; END_RECORD_BYTES as usize];
    read_at(file, position, &mut record)?;
    let end_of_the_record = position + END_RECORD_BYTES + u64::from(little_endian_u16(&record[20..22]));
    if end_of_the_record > file_length {
        return Ok(None);
    }
    let count16 = little_endian_u16(&record[10..12]);
    let size32 = little_endian_u32(&record[12..16]);
    let start32 = little_endian_u32(&record[16..20]);
    let (mut count, mut size, mut start) = (u64::from(count16), u64::from(size32), u64::from(start32));
    let mut where_the_end_records_begin = position;
    if count16 == SEE_ZIP64_COUNT || size32 == SEE_ZIP64_FIELD || start32 == SEE_ZIP64_FIELD {
        if position < ZIP64_LOCATOR_BYTES {
            return Ok(None);
        }
        let mut locator = [0_u8; ZIP64_LOCATOR_BYTES as usize];
        read_at(file, position - ZIP64_LOCATOR_BYTES, &mut locator)?;
        if locator[..4] != ZIP64_LOCATOR_SIGNATURE {
            return Ok(None);
        }
        let zip64_end_record_at = little_endian_u64(&locator[8..16]);
        if zip64_end_record_at + ZIP64_END_RECORD_BYTES > position - ZIP64_LOCATOR_BYTES {
            return Ok(None);
        }
        let mut zip64_record = [0_u8; ZIP64_END_RECORD_BYTES as usize];
        read_at(file, zip64_end_record_at, &mut zip64_record)?;
        if zip64_record[..4] != ZIP64_END_RECORD_SIGNATURE {
            return Ok(None);
        }
        count = little_endian_u64(&zip64_record[32..40]);
        size = little_endian_u64(&zip64_record[40..48]);
        start = little_endian_u64(&zip64_record[48..56]);
        where_the_end_records_begin = zip64_end_record_at;
    }
    if size > LARGEST_DIRECTORY_BYTES
        || start
            .checked_add(size)
            .is_none_or(|end| end > where_the_end_records_begin)
    {
        return Ok(None);
    }
    let mut directory_bytes = vec![0_u8; size as usize];
    read_at(file, start, &mut directory_bytes)?;
    let Some(entries) = entries_of_a_directory(&directory_bytes, count) else {
        return Ok(None);
    };
    let mut entries = entries;
    for entry in &mut entries {
        // Where the bytes of the file begin is after the header, which has a name and an extra field of its own (they may not be the
        // directory's).
        if entry.header_start + LOCAL_HEADER_BYTES > file_length {
            return Ok(None);
        }
        let mut header = [0_u8; LOCAL_HEADER_BYTES as usize];
        read_at(file, entry.header_start, &mut header)?;
        if header[..4] != LOCAL_HEADER_SIGNATURE {
            return Ok(None);
        }
        let name_bytes = u64::from(little_endian_u16(&header[26..28]));
        let extra_bytes = u64::from(little_endian_u16(&header[28..30]));
        entry.data_start = entry.header_start + LOCAL_HEADER_BYTES + name_bytes + extra_bytes;
        if entry.data_start + entry.stored_length > start {
            return Ok(None);
        }
    }
    Ok(Some(Directory {
        entries,
        directory_start: start,
        end_of_the_last_end_record: end_of_the_record,
        file_length,
    }))
}

/// The entries that `count` records, which are all of `directory_bytes`, are. `None` if they are not records of a directory.
fn entries_of_a_directory(directory_bytes: &[u8], count: u64) -> Option<Vec<Entry>> {
    let mut entries = Vec::new();
    let mut at = 0_usize;
    for _ in 0..count {
        let record = directory_bytes.get(at..)?;
        if record.len() < DIRECTORY_RECORD_BYTES || record[..4] != DIRECTORY_RECORD_SIGNATURE {
            return None;
        }
        let name_bytes = usize::from(little_endian_u16(&record[28..30]));
        let extra_bytes = usize::from(little_endian_u16(&record[30..32]));
        let comment_bytes = usize::from(little_endian_u16(&record[32..34]));
        let total = DIRECTORY_RECORD_BYTES + name_bytes + extra_bytes + comment_bytes;
        let whole = record.get(..total)?;
        let name =
            String::from_utf8_lossy(&whole[DIRECTORY_RECORD_BYTES..DIRECTORY_RECORD_BYTES + name_bytes]).into_owned();
        let extra = &whole[DIRECTORY_RECORD_BYTES + name_bytes..DIRECTORY_RECORD_BYTES + name_bytes + extra_bytes];
        let mut stored_length = u64::from(little_endian_u32(&whole[20..24]));
        let mut length = u64::from(little_endian_u32(&whole[24..28]));
        let mut header_start = u64::from(little_endian_u32(&whole[42..46]));
        if let Some(mut zip64) = zip64_extra_field(extra) {
            // (Only the fields that were too big for their places are in it, in this order.)
            if length == u64::from(SEE_ZIP64_FIELD) {
                length = take_u64(&mut zip64)?;
            }
            if stored_length == u64::from(SEE_ZIP64_FIELD) {
                stored_length = take_u64(&mut zip64)?;
            }
            if header_start == u64::from(SEE_ZIP64_FIELD) {
                header_start = take_u64(&mut zip64)?;
            }
        }
        entries.push(Entry {
            name,
            header_start,
            data_start: 0,
            stored_length,
            length,
            method: little_endian_u16(&whole[10..12]),
            crc32: little_endian_u32(&whole[16..20]),
            record: whole.to_vec(),
        });
        at += total;
    }
    (at == directory_bytes.len()).then_some(entries)
}

fn take_u64(bytes: &mut &[u8]) -> Option<u64> {
    let (first, rest) = bytes.split_at_checked(8)?;
    *bytes = rest;
    Some(little_endian_u64(first))
}

/// The bytes of the zip64 extra field among `extra` (which is a list of fields, each with an id and a length), if there is one.
fn zip64_extra_field(extra: &[u8]) -> Option<&[u8]> {
    let mut rest = extra;
    while rest.len() >= 4 {
        let id = little_endian_u16(&rest[0..2]);
        let size = usize::from(little_endian_u16(&rest[2..4]));
        let field = rest.get(4..4 + size)?;
        if id == ZIP64_EXTRA_FIELD_ID {
            return Some(field);
        }
        rest = &rest[4 + size..];
    }
    None
}

/// Cuts off what is after the last whole end record, if it is what a save that was cut off leaves (it begins as an entry does): the
/// archive is then as it was after the last save that was whole. Anything else after the last end record is left alone. Says how many
/// bytes were cut off.
pub(super) fn cut_off_the_unfinished_save(file: &mut File, directory: &Directory) -> io::Result<u64> {
    let after = directory.file_length - directory.end_of_the_last_end_record;
    if after < LOCAL_HEADER_SIGNATURE.len() as u64 {
        return Ok(0);
    }
    let mut first = [0_u8; 4];
    read_at(file, directory.end_of_the_last_end_record, &mut first)?;
    if first != LOCAL_HEADER_SIGNATURE {
        return Ok(0);
    }
    file.set_len(directory.end_of_the_last_end_record)?;
    Ok(after)
}

/// Where an entry that was written is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Placed {
    pub header_start: u64,
    pub data_start: u64,
    pub length: u64,
    pub crc32: u32,
}

/// Writes entries one after another from a place in a file, and then the directory and the end record that make them an archive.
pub(super) struct Writer<'file> {
    file: &'file mut File,
    position: u64,
    thresholds: Zip64Thresholds,
    /// The records of the directory so far, in order.
    records: Vec<Vec<u8>>,
    names: HashSet<String>,
}

impl<'file> Writer<'file> {
    /// A writer that writes from `position` on in `file`, which is where the entries that are kept end, or the start of a new file.
    pub(super) fn starting_at(file: &'file mut File, position: u64, thresholds: Zip64Thresholds) -> io::Result<Self> {
        file.seek(SeekFrom::Start(position))?;
        Ok(Self {
            file,
            position,
            thresholds,
            records: Vec::new(),
            names: HashSet::new(),
        })
    }

    /// Makes the entry `entry`, which is in the file already, one of the entries of the archive that is written. Nothing of it is written
    /// again.
    pub(super) fn keep(&mut self, entry: &Entry) -> io::Result<()> {
        self.remember_the_name(&entry.name)?;
        self.records.push(entry.record.clone());
        Ok(())
    }

    fn remember_the_name(&mut self, name: &str) -> io::Result<()> {
        if self.names.insert(name.to_owned()) {
            Ok(())
        } else {
            Err(io::Error::other(format!("there would be two entries named \"{name}\"")))
        }
    }

    /// Writes an entry named `name` that has the `length` bytes that `data` gives, stored as they are. If `known_crc` is given, it is
    /// what the CRC-32 of them must be (and an error is made if it is not: they are damaged); otherwise it is worked out. `on_progress`
    /// is told how many bytes of the file have been written, from time to time, and an error from it stops the writing.
    pub(super) fn write_entry(
        &mut self,
        name: &str,
        length: u64,
        known_crc: Option<u32>,
        data: &mut dyn Read,
        on_progress: &mut dyn FnMut(u64) -> io::Result<()>,
    ) -> io::Result<Placed> {
        self.remember_the_name(name)?;
        let header_start = self.position;
        let long = length >= self.thresholds.length;
        let far = header_start >= self.thresholds.offset;
        let header = local_header(name, length, long);
        self.file.write_all(&header)?;
        self.position += header.len() as u64;
        let data_start = self.position;

        let mut hasher = Hasher::new();
        let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
        let mut copied = 0_u64;
        while copied < length {
            let wanted = buffer.len().min((length - copied) as usize);
            let read = data.read(&mut buffer[..wanted])?;
            if read == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!("the file for \"{name}\" has {copied} bytes, and it had {length} when it was looked at"),
                ));
            }
            hasher.update(&buffer[..read]);
            self.file.write_all(&buffer[..read])?;
            copied += read as u64;
            self.position += read as u64;
            on_progress(copied)?;
        }
        let crc32 = hasher.finalize();
        if let Some(known) = known_crc
            && known != crc32
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "the bytes of \"{name}\" are damaged: they are not what they were when they were put in the project file"
                ),
            ));
        }
        // The CRC was not known when the header was written (unless it was): it is made known where it is, and the writing goes on.
        self.file.seek(SeekFrom::Start(header_start + 14))?;
        self.file.write_all(&crc32.to_le_bytes())?;
        self.file.seek(SeekFrom::Start(self.position))?;
        self.records
            .push(directory_record(name, length, crc32, header_start, long, far));
        Ok(Placed {
            header_start,
            data_start,
            length,
            crc32,
        })
    }

    /// Writes the directory and the end record, and makes sure that all of it is on the disk before the end record is (so that an end
    /// record that is there is one whose directory is). Says where the file ends.
    pub(super) fn finish(mut self) -> io::Result<u64> {
        let directory_start = self.position;
        let directory_bytes: u64 = self.records.iter().map(|record| record.len() as u64).sum();
        for record in &self.records {
            self.file.write_all(record)?;
        }
        self.position += directory_bytes;
        self.file.sync_data()?;

        let count = self.records.len() as u64;
        let zip64 = count >= self.thresholds.count
            || directory_bytes >= self.thresholds.offset
            || directory_start >= self.thresholds.offset;
        let mut end = Vec::new();
        if zip64 {
            let zip64_end_record_at = self.position;
            end.extend_from_slice(&ZIP64_END_RECORD_SIGNATURE);
            end.extend_from_slice(&(ZIP64_END_RECORD_BYTES - 12).to_le_bytes());
            end.extend_from_slice(&VERSION_MADE_BY.to_le_bytes());
            end.extend_from_slice(&VERSION_NEEDED_FOR_ZIP64.to_le_bytes());
            end.extend_from_slice(&0_u32.to_le_bytes());
            end.extend_from_slice(&0_u32.to_le_bytes());
            end.extend_from_slice(&count.to_le_bytes());
            end.extend_from_slice(&count.to_le_bytes());
            end.extend_from_slice(&directory_bytes.to_le_bytes());
            end.extend_from_slice(&directory_start.to_le_bytes());
            end.extend_from_slice(&ZIP64_LOCATOR_SIGNATURE);
            end.extend_from_slice(&0_u32.to_le_bytes());
            end.extend_from_slice(&zip64_end_record_at.to_le_bytes());
            end.extend_from_slice(&1_u32.to_le_bytes());
        }
        end.extend_from_slice(&END_RECORD_SIGNATURE);
        end.extend_from_slice(&0_u16.to_le_bytes());
        end.extend_from_slice(&0_u16.to_le_bytes());
        let count16 = if count >= self.thresholds.count {
            SEE_ZIP64_COUNT
        } else {
            count as u16
        };
        end.extend_from_slice(&count16.to_le_bytes());
        end.extend_from_slice(&count16.to_le_bytes());
        let size32 = if directory_bytes >= self.thresholds.offset {
            SEE_ZIP64_FIELD
        } else {
            directory_bytes as u32
        };
        let start32 = if directory_start >= self.thresholds.offset {
            SEE_ZIP64_FIELD
        } else {
            directory_start as u32
        };
        end.extend_from_slice(&size32.to_le_bytes());
        end.extend_from_slice(&start32.to_le_bytes());
        end.extend_from_slice(&0_u16.to_le_bytes());
        self.file.write_all(&end)?;
        self.position += end.len() as u64;
        self.file.set_len(self.position)?;
        self.file.sync_all()?;
        Ok(self.position)
    }
}

/// The header of an entry, with the CRC-32 left to be filled in (it is not known until the file has been read).
fn local_header(name: &str, length: u64, long: bool) -> Vec<u8> {
    let extra = if long {
        zip64_extra_for_header(length)
    } else {
        Vec::new()
    };
    let mut header = Vec::with_capacity(LOCAL_HEADER_BYTES as usize + name.len() + extra.len());
    header.extend_from_slice(&LOCAL_HEADER_SIGNATURE);
    header.extend_from_slice(&(if long { VERSION_NEEDED_FOR_ZIP64 } else { VERSION_NEEDED }).to_le_bytes());
    header.extend_from_slice(&FLAG_NAME_IS_UTF8.to_le_bytes());
    header.extend_from_slice(&0_u16.to_le_bytes());
    header.extend_from_slice(&DOS_TIME.to_le_bytes());
    header.extend_from_slice(&DOS_DATE.to_le_bytes());
    header.extend_from_slice(&0_u32.to_le_bytes());
    let size32 = if long { SEE_ZIP64_FIELD } else { length as u32 };
    header.extend_from_slice(&size32.to_le_bytes());
    header.extend_from_slice(&size32.to_le_bytes());
    header.extend_from_slice(&(name.len() as u16).to_le_bytes());
    header.extend_from_slice(&(extra.len() as u16).to_le_bytes());
    header.extend_from_slice(name.as_bytes());
    header.extend_from_slice(&extra);
    header
}

/// The zip64 extra field of the header of an entry: both the lengths, as numbers of eight bytes.
fn zip64_extra_for_header(length: u64) -> Vec<u8> {
    let mut extra = Vec::with_capacity(20);
    extra.extend_from_slice(&ZIP64_EXTRA_FIELD_ID.to_le_bytes());
    extra.extend_from_slice(&16_u16.to_le_bytes());
    extra.extend_from_slice(&length.to_le_bytes());
    extra.extend_from_slice(&length.to_le_bytes());
    extra
}

/// The record of the directory for an entry of `length` bytes that begins at `header_start`.
fn directory_record(name: &str, length: u64, crc32: u32, header_start: u64, long: bool, far: bool) -> Vec<u8> {
    // (Only what is too big for its place is in the zip64 extra field, in this order: both lengths, then where the entry is.)
    let mut extra = Vec::new();
    if long {
        extra.extend_from_slice(&length.to_le_bytes());
        extra.extend_from_slice(&length.to_le_bytes());
    }
    if far {
        extra.extend_from_slice(&header_start.to_le_bytes());
    }
    let extra = if extra.is_empty() {
        extra
    } else {
        let mut field = Vec::with_capacity(4 + extra.len());
        field.extend_from_slice(&ZIP64_EXTRA_FIELD_ID.to_le_bytes());
        field.extend_from_slice(&(extra.len() as u16).to_le_bytes());
        field.extend_from_slice(&extra);
        field
    };
    let size32 = if long { SEE_ZIP64_FIELD } else { length as u32 };
    let start32 = if far { SEE_ZIP64_FIELD } else { header_start as u32 };
    let mut record = Vec::with_capacity(DIRECTORY_RECORD_BYTES + name.len() + extra.len());
    record.extend_from_slice(&DIRECTORY_RECORD_SIGNATURE);
    record.extend_from_slice(&VERSION_MADE_BY.to_le_bytes());
    let needed = if long || far {
        VERSION_NEEDED_FOR_ZIP64
    } else {
        VERSION_NEEDED
    };
    record.extend_from_slice(&needed.to_le_bytes());
    record.extend_from_slice(&FLAG_NAME_IS_UTF8.to_le_bytes());
    record.extend_from_slice(&0_u16.to_le_bytes());
    record.extend_from_slice(&DOS_TIME.to_le_bytes());
    record.extend_from_slice(&DOS_DATE.to_le_bytes());
    record.extend_from_slice(&crc32.to_le_bytes());
    record.extend_from_slice(&size32.to_le_bytes());
    record.extend_from_slice(&size32.to_le_bytes());
    record.extend_from_slice(&(name.len() as u16).to_le_bytes());
    record.extend_from_slice(&(extra.len() as u16).to_le_bytes());
    record.extend_from_slice(&0_u16.to_le_bytes());
    record.extend_from_slice(&0_u16.to_le_bytes());
    record.extend_from_slice(&0_u16.to_le_bytes());
    record.extend_from_slice(&0_u32.to_le_bytes());
    record.extend_from_slice(&start32.to_le_bytes());
    record.extend_from_slice(name.as_bytes());
    record.extend_from_slice(&extra);
    record
}

/// The first `length` bytes of a file, as if that were all of it: for a reader of zip files to read an archive whose last save was cut
/// off, as it was before it, without the file being changed.
pub(super) struct FirstBytesOf<R> {
    inner: R,
    length: u64,
    position: u64,
}

impl<R> FirstBytesOf<R> {
    pub(super) fn new(inner: R, length: u64) -> Self {
        Self {
            inner,
            length,
            position: 0,
        }
    }
}

impl<R: Read + Seek> Read for FirstBytesOf<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let left = self.length.saturating_sub(self.position);
        let wanted = buffer.len().min(left.min(usize::MAX as u64) as usize);
        if wanted == 0 {
            return Ok(0);
        }
        self.inner.seek(SeekFrom::Start(self.position))?;
        let read = self.inner.read(&mut buffer[..wanted])?;
        self.position += read as u64;
        Ok(read)
    }
}

impl<R: Read + Seek> Seek for FirstBytesOf<R> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let new_position = match to {
            SeekFrom::Start(position) => Some(position),
            SeekFrom::Current(change) => self.position.checked_add_signed(change),
            SeekFrom::End(change) => self.length.checked_add_signed(change),
        };
        let Some(new_position) = new_position else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "a place before the start"));
        };
        self.position = new_position;
        Ok(new_position)
    }
}

#[cfg(test)]
mod tests;
