// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs::{self, File, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use super::*;
use crate::test_support::check_every_case_at_once;

/// Thresholds so low that the zip64 forms are made for small things: a file of 10 bytes is long, an entry that begins 100 bytes in is
/// far, and three entries are many.
const LOW: Zip64Thresholds = Zip64Thresholds {
    length: 10,
    offset: 100,
    count: 3,
};

fn nothing_to_report(_bytes_written: u64) -> io::Result<()> {
    Ok(())
}

/// `length` bytes that look like what a video is, and that a compressor could do nothing with.
fn bytes_that_do_not_compress(length: usize, seed: u32) -> Vec<u8> {
    let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state >> 11) as u8
        })
        .collect()
}

/// Writes a new archive at `path` that has `entries` in it, in order, and says where each is.
fn write_fresh(path: &Path, entries: &[(&str, &[u8])], thresholds: Zip64Thresholds) -> Vec<Placed> {
    let mut file = File::create(path).unwrap();
    let mut writer = Writer::starting_at(&mut file, 0, thresholds).unwrap();
    let placed = entries
        .iter()
        .map(|(name, bytes)| {
            writer
                .write_entry(
                    name,
                    bytes.len() as u64,
                    None,
                    &mut Cursor::new(*bytes),
                    &mut nothing_to_report,
                )
                .unwrap()
        })
        .collect();
    writer.finish().unwrap();
    placed
}

/// Adds to the archive at `path` as a save does: the entries named in `keep` stay where they are, and `new` are written after what is
/// there. (What was cut off by an earlier save that did not finish is cut off first.)
fn append(path: &Path, keep: &[&str], new: &[(&str, &[u8])], thresholds: Zip64Thresholds) -> Vec<Placed> {
    let mut file = OpenOptions::new().read(true).write(true).open(path).unwrap();
    let directory = read_directory(&mut file).unwrap();
    cut_off_the_unfinished_save(&mut file, &directory).unwrap();
    let position = file.metadata().unwrap().len();
    let mut writer = Writer::starting_at(&mut file, position, thresholds).unwrap();
    for entry in &directory.entries {
        if keep.contains(&entry.name.as_str()) {
            writer.keep(entry).unwrap();
        }
    }
    let placed = new
        .iter()
        .map(|(name, bytes)| {
            writer
                .write_entry(
                    name,
                    bytes.len() as u64,
                    None,
                    &mut Cursor::new(*bytes),
                    &mut nothing_to_report,
                )
                .unwrap()
        })
        .collect();
    writer.finish().unwrap();
    placed
}

fn directory_of(path: &Path) -> Directory {
    read_directory(&mut File::open(path).unwrap()).unwrap()
}

fn names_in_the_directory(path: &Path) -> Vec<String> {
    directory_of(path).entries.into_iter().map(|entry| entry.name).collect()
}

/// What the zip crate reads out of the archive: for each entry its name, and its bytes.
fn what_the_zip_crate_reads(path: &Path) -> Vec<(String, Vec<u8>)> {
    let mut archive = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
    (0..archive.len())
        .map(|index| {
            let mut entry = archive.by_index(index).unwrap();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            (entry.name().unwrap().into_owned(), bytes)
        })
        .collect()
}

fn archive_in(folder: &tempfile::TempDir, name: &str) -> PathBuf {
    folder.path().join(name)
}

// --- writing, and reading what is written ---

#[test]
fn what_is_written_is_read_by_the_zip_crate_with_the_bytes_that_were_put_in() {
    let folder = tempfile::tempdir().unwrap();
    let path = archive_in(&folder, "a.ffrs");
    let video = bytes_that_do_not_compress(300_000, 1);

    write_fresh(
        &path,
        &[
            ("project.fastforwardrs.json", b"{}"),
            ("resources/1-v.mp4", &video),
            ("empty", b""),
        ],
        Zip64Thresholds::REAL,
    );

    assert_eq!(
        what_the_zip_crate_reads(&path),
        vec![
            ("project.fastforwardrs.json".to_owned(), b"{}".to_vec()),
            ("resources/1-v.mp4".to_owned(), video),
            ("empty".to_owned(), Vec::new()),
        ]
    );
}

#[test]
fn the_directory_reader_says_what_the_zip_crate_says_of_where_each_entry_is_and_how_long() {
    let folder = tempfile::tempdir().unwrap();
    let path = archive_in(&folder, "a.ffrs");
    let video = bytes_that_do_not_compress(70_001, 2);
    let placed = write_fresh(
        &path,
        &[
            ("project.fastforwardrs.json", b"{\"a\": 1}"),
            ("resources/1-v.mp4", &video),
        ],
        Zip64Thresholds::REAL,
    );

    let directory = directory_of(&path);
    let mut archive = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();

    assert_eq!(directory.entries.len(), 2);
    for (index, entry) in directory.entries.iter().enumerate() {
        let from_the_crate = archive.by_index(index).unwrap();
        assert_eq!(entry.name, from_the_crate.name().unwrap());
        assert_eq!(entry.data_start, from_the_crate.data_start().unwrap(), "{}", entry.name);
        assert_eq!(entry.header_start, from_the_crate.header_start(), "{}", entry.name);
        assert_eq!(entry.length, from_the_crate.size(), "{}", entry.name);
        assert_eq!(entry.stored_length, from_the_crate.compressed_size(), "{}", entry.name);
        assert_eq!(entry.crc32, from_the_crate.crc32(), "{}", entry.name);
        assert!(entry.is_stored());
    }
    assert_eq!(directory.entries[1].data_start, placed[1].data_start);
    assert_eq!(directory.entries[1].length, 70_001);
    assert_eq!(directory.entries[1].crc32, placed[1].crc32);
}

#[test]
fn an_archive_of_nothing_has_a_directory_of_nothing() {
    let folder = tempfile::tempdir().unwrap();
    let path = archive_in(&folder, "empty.ffrs");
    write_fresh(&path, &[], Zip64Thresholds::REAL);

    let directory = directory_of(&path);

    assert!(directory.entries.is_empty());
    assert_eq!(directory.bytes_of_garbage(), 0);
    assert_eq!(directory.file_length, 22);
    assert!(zip::ZipArchive::new(File::open(&path).unwrap()).unwrap().is_empty());
}

#[test]
fn nothing_is_garbage_in_an_archive_that_was_written_at_once() {
    let folder = tempfile::tempdir().unwrap();
    let path = archive_in(&folder, "a.ffrs");
    write_fresh(&path, &[("a", b"1234"), ("b", &[7; 1000])], Zip64Thresholds::REAL);

    let directory = directory_of(&path);

    assert_eq!(directory.bytes_of_garbage(), 0);
    assert_eq!(directory.bytes_in_use(), directory.file_length);
}

#[test]
fn bytes_that_are_not_the_bytes_that_were_said_to_be_there_are_refused_as_damaged() {
    let folder = tempfile::tempdir().unwrap();
    let mut file = File::create(archive_in(&folder, "a.ffrs")).unwrap();
    let mut writer = Writer::starting_at(&mut file, 0, Zip64Thresholds::REAL).unwrap();
    let crc_of_something_else = 0x1234_5678;

    let error = writer
        .write_entry(
            "v",
            4,
            Some(crc_of_something_else),
            &mut Cursor::new(b"abcd"),
            &mut nothing_to_report,
        )
        .unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("damaged"), "{error}");
}

#[test]
fn a_crc_that_is_the_crc_of_the_bytes_is_accepted() {
    let folder = tempfile::tempdir().unwrap();
    let path = archive_in(&folder, "a.ffrs");
    let placed = write_fresh(&path, &[("v", b"abcd")], Zip64Thresholds::REAL);
    let mut file = File::create(archive_in(&folder, "b.ffrs")).unwrap();
    let mut writer = Writer::starting_at(&mut file, 0, Zip64Thresholds::REAL).unwrap();

    writer
        .write_entry(
            "v",
            4,
            Some(placed[0].crc32),
            &mut Cursor::new(b"abcd"),
            &mut nothing_to_report,
        )
        .unwrap();
}

#[test]
fn a_file_that_has_fewer_bytes_than_it_was_said_to_have_is_an_error() {
    let folder = tempfile::tempdir().unwrap();
    let mut file = File::create(archive_in(&folder, "a.ffrs")).unwrap();
    let mut writer = Writer::starting_at(&mut file, 0, Zip64Thresholds::REAL).unwrap();

    let error = writer
        .write_entry("v", 10, None, &mut Cursor::new(b"abcd"), &mut nothing_to_report)
        .unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
}

#[test]
fn only_as_many_bytes_as_were_said_are_taken_from_a_file_that_has_more() {
    let folder = tempfile::tempdir().unwrap();
    let path = archive_in(&folder, "a.ffrs");
    let mut file = File::create(&path).unwrap();
    let mut writer = Writer::starting_at(&mut file, 0, Zip64Thresholds::REAL).unwrap();
    writer
        .write_entry("v", 4, None, &mut Cursor::new(b"abcdefgh"), &mut nothing_to_report)
        .unwrap();
    writer.finish().unwrap();

    assert_eq!(
        what_the_zip_crate_reads(&path),
        vec![("v".to_owned(), b"abcd".to_vec())]
    );
}

#[test]
fn two_entries_of_one_name_are_refused() {
    let folder = tempfile::tempdir().unwrap();
    let mut file = File::create(archive_in(&folder, "a.ffrs")).unwrap();
    let mut writer = Writer::starting_at(&mut file, 0, Zip64Thresholds::REAL).unwrap();
    writer
        .write_entry("v", 1, None, &mut Cursor::new(b"a"), &mut nothing_to_report)
        .unwrap();

    let error = writer
        .write_entry("v", 1, None, &mut Cursor::new(b"b"), &mut nothing_to_report)
        .unwrap_err();

    assert!(error.to_string().contains("two entries named"), "{error}");
}

#[test]
fn the_progress_is_told_as_the_bytes_are_written_and_an_error_from_it_stops_the_writing() {
    let folder = tempfile::tempdir().unwrap();
    let mut file = File::create(archive_in(&folder, "a.ffrs")).unwrap();
    let mut writer = Writer::starting_at(&mut file, 0, Zip64Thresholds::REAL).unwrap();
    let big = bytes_that_do_not_compress(COPY_BUFFER_BYTES * 3 + 5, 3);
    let mut told = Vec::new();

    writer
        .write_entry("v", big.len() as u64, None, &mut Cursor::new(&big), &mut |written| {
            told.push(written);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        told,
        vec![
            COPY_BUFFER_BYTES as u64,
            2 * COPY_BUFFER_BYTES as u64,
            3 * COPY_BUFFER_BYTES as u64,
            big.len() as u64
        ]
    );

    let mut file = File::create(archive_in(&folder, "b.ffrs")).unwrap();
    let mut writer = Writer::starting_at(&mut file, 0, Zip64Thresholds::REAL).unwrap();
    let error = writer
        .write_entry("v", big.len() as u64, None, &mut Cursor::new(&big), &mut |written| {
            if written > COPY_BUFFER_BYTES as u64 {
                Err(io::Error::new(io::ErrorKind::Interrupted, "stopped"))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
}

// --- adding to an archive ---

/// An archive of a video, a sound and a project, as a save has left it.
fn an_archive_with_two_files_and_a_project(folder: &tempfile::TempDir) -> (PathBuf, Vec<u8>, Vec<u8>) {
    let path = archive_in(folder, "trip.ffrs");
    let video = bytes_that_do_not_compress(400_000, 10);
    let sound = bytes_that_do_not_compress(50_000, 11);
    write_fresh(
        &path,
        &[
            ("project.fastforwardrs.json", br#"{"version": 1}"#),
            ("resources/1-v.mp4", &video),
            ("resources/2-s.wav", &sound),
        ],
        Zip64Thresholds::REAL,
    );
    (path, video, sound)
}

#[test]
fn adding_a_project_text_leaves_the_files_where_they_are_and_the_new_text_is_the_one_that_is_read() {
    let folder = tempfile::tempdir().unwrap();
    let (path, video, sound) = an_archive_with_two_files_and_a_project(&folder);
    let before = directory_of(&path);
    let length_before = before.file_length;

    append(
        &path,
        &["resources/1-v.mp4", "resources/2-s.wav"],
        &[("project.fastforwardrs.json", br#"{"version": 2, "more": true}"#)],
        Zip64Thresholds::REAL,
    );

    let after = directory_of(&path);
    for name in ["resources/1-v.mp4", "resources/2-s.wav"] {
        let (was, is) = (before.entry_named(name).unwrap(), after.entry_named(name).unwrap());
        assert_eq!(
            (was.header_start, was.data_start),
            (is.header_start, is.data_start),
            "{name}"
        );
    }
    assert!(after.file_length > length_before, "it was added to");
    let read = what_the_zip_crate_reads(&path);
    let by_name = |name: &str| read.iter().find(|(entry, _)| entry == name).unwrap().1.clone();
    assert_eq!(
        by_name("project.fastforwardrs.json"),
        br#"{"version": 2, "more": true}"#.to_vec()
    );
    assert_eq!(by_name("resources/1-v.mp4"), video);
    assert_eq!(by_name("resources/2-s.wav"), sound);
    assert_eq!(read.len(), 3, "the old project text is not one of the entries any more");
}

#[test]
fn what_the_old_project_text_and_directory_were_is_garbage_and_it_is_counted() {
    let folder = tempfile::tempdir().unwrap();
    let (path, _, _) = an_archive_with_two_files_and_a_project(&folder);
    let before = directory_of(&path);
    let old_project = before.entry_named("project.fastforwardrs.json").unwrap();
    let old_project_bytes = old_project.data_start - old_project.header_start + old_project.stored_length;
    let old_directory_and_end = before.end_of_the_last_end_record - before.directory_start;

    append(
        &path,
        &["resources/1-v.mp4", "resources/2-s.wav"],
        &[("project.fastforwardrs.json", b"{}")],
        Zip64Thresholds::REAL,
    );

    assert_eq!(
        directory_of(&path).bytes_of_garbage(),
        old_project_bytes + old_directory_and_end
    );
}

#[test]
fn an_entry_that_is_not_kept_is_not_in_the_directory_and_its_bytes_are_garbage() {
    let folder = tempfile::tempdir().unwrap();
    let (path, _, sound) = an_archive_with_two_files_and_a_project(&folder);
    let video_bytes = {
        let entry = directory_of(&path).entry_named("resources/1-v.mp4").unwrap().clone();
        entry.data_start - entry.header_start + entry.stored_length
    };

    append(
        &path,
        &["resources/2-s.wav"],
        &[("project.fastforwardrs.json", b"{}")],
        Zip64Thresholds::REAL,
    );

    assert_eq!(
        names_in_the_directory_sorted(&path),
        vec!["project.fastforwardrs.json".to_owned(), "resources/2-s.wav".to_owned()]
    );
    assert!(directory_of(&path).bytes_of_garbage() >= video_bytes);
    let read = what_the_zip_crate_reads(&path);
    assert_eq!(
        read.iter().find(|(name, _)| name == "resources/2-s.wav").unwrap().1,
        sound
    );
}

fn names_in_the_directory_sorted(path: &Path) -> Vec<String> {
    let mut names = names_in_the_directory(path);
    names.sort();
    names
}

#[test]
fn a_file_can_be_added_and_is_found_where_it_was_put() {
    let folder = tempfile::tempdir().unwrap();
    let (path, video, _) = an_archive_with_two_files_and_a_project(&folder);
    let picture = bytes_that_do_not_compress(5_000, 12);

    let placed = append(
        &path,
        &["resources/1-v.mp4", "resources/2-s.wav"],
        &[("resources/3-p.png", &picture), ("project.fastforwardrs.json", b"{}")],
        Zip64Thresholds::REAL,
    );

    let directory = directory_of(&path);
    let entry = directory.entry_named("resources/3-p.png").unwrap();
    assert_eq!(
        (entry.header_start, entry.data_start),
        (placed[0].header_start, placed[0].data_start)
    );
    let mut archive_bytes = Vec::new();
    File::open(&path).unwrap().read_to_end(&mut archive_bytes).unwrap();
    assert_eq!(
        &archive_bytes[entry.data_start as usize..(entry.data_start + entry.length) as usize],
        picture.as_slice()
    );
    assert_eq!(
        what_the_zip_crate_reads(&path)
            .iter()
            .find(|(name, _)| name == "resources/1-v.mp4")
            .unwrap()
            .1,
        video
    );
}

#[test]
fn adding_again_and_again_keeps_the_files_where_they_are_and_only_the_last_text_counts() {
    let folder = tempfile::tempdir().unwrap();
    let (path, video, _) = an_archive_with_two_files_and_a_project(&folder);
    let place_of_the_video = {
        let entry = directory_of(&path).entry_named("resources/1-v.mp4").unwrap().clone();
        (entry.header_start, entry.data_start)
    };
    let mut garbage_so_far = 0;

    for round in 1..=20_u32 {
        let text = format!("{{\"round\": {round}}}");
        append(
            &path,
            &["resources/1-v.mp4", "resources/2-s.wav"],
            &[("project.fastforwardrs.json", text.as_bytes())],
            Zip64Thresholds::REAL,
        );
        let garbage = directory_of(&path).bytes_of_garbage();
        assert!(
            garbage > garbage_so_far,
            "round {round}: more garbage ({garbage}) than before ({garbage_so_far})"
        );
        garbage_so_far = garbage;
    }

    let directory = directory_of(&path);
    let entry = directory.entry_named("resources/1-v.mp4").unwrap();
    assert_eq!((entry.header_start, entry.data_start), place_of_the_video);
    let read = what_the_zip_crate_reads(&path);
    assert_eq!(
        read.iter()
            .find(|(name, _)| name == "project.fastforwardrs.json")
            .unwrap()
            .1,
        br#"{"round": 20}"#.to_vec()
    );
    assert_eq!(
        read.iter().find(|(name, _)| name == "resources/1-v.mp4").unwrap().1,
        video
    );
}

#[test]
fn a_file_that_is_appended_to_while_a_reader_has_it_open_is_not_disturbed_for_the_reader() {
    let folder = tempfile::tempdir().unwrap();
    let (path, video, _) = an_archive_with_two_files_and_a_project(&folder);
    let entry = directory_of(&path).entry_named("resources/1-v.mp4").unwrap().clone();
    let mut reader = File::open(&path).unwrap();

    append(
        &path,
        &["resources/1-v.mp4", "resources/2-s.wav"],
        &[("project.fastforwardrs.json", b"{}")],
        Zip64Thresholds::REAL,
    );

    use std::io::{Seek, SeekFrom};
    reader.seek(SeekFrom::Start(entry.data_start)).unwrap();
    let mut read = vec![0_u8; entry.length as usize];
    reader.read_exact(&mut read).unwrap();
    assert_eq!(read, video);
}

// --- a save that did not finish ---

#[test]
fn whatever_part_of_a_save_was_written_the_archive_is_as_it_was_before_it() {
    let folder = tempfile::tempdir().unwrap();
    let (path, _, _) = an_archive_with_two_files_and_a_project(&folder);
    let before_bytes = fs::read(&path).unwrap();
    let before = directory_of(&path);
    // A save that adds a picture and a new project text, kept as the whole of what it wrote.
    let picture = bytes_that_do_not_compress(3_000, 13);
    append(
        &path,
        &["resources/1-v.mp4", "resources/2-s.wav"],
        &[
            ("resources/3-p.png", &picture),
            ("project.fastforwardrs.json", br#"{"saved": "again"}"#),
        ],
        Zip64Thresholds::REAL,
    );
    let after_bytes = fs::read(&path).unwrap();
    assert!(after_bytes.len() > before_bytes.len());
    assert_eq!(
        &after_bytes[..before_bytes.len()],
        &before_bytes[..],
        "what was there is not changed"
    );

    // Every length that the file could have been left at (several at once: each takes a file to be written and read again, and there are
    // thousands, which took half a minute one after another; each thread has a file of its own).
    let lengths: Vec<usize> = (before_bytes.len() + 1..after_bytes.len()).collect();
    check_every_case_at_once(&lengths, |&length| {
        let thread = format!("{:?}", std::thread::current().id()).replace(['(', ')'], "");
        let cut = archive_in(&folder, &format!("cut-{thread}.ffrs"));
        fs::write(&cut, &after_bytes[..length]).unwrap();

        let directory = directory_of(&cut);

        assert_eq!(
            directory.end_of_the_last_end_record,
            before_bytes.len() as u64,
            "left at {length} of {}",
            after_bytes.len()
        );
        assert_eq!(
            names_in_the_directory(&cut),
            before
                .entries
                .iter()
                .map(|entry| entry.name.clone())
                .collect::<Vec<_>>(),
            "left at {length}"
        );
        // And it is mended: what is cut off is what the save wrote, and the archive is exactly as it was.
        let mut file = OpenOptions::new().read(true).write(true).open(&cut).unwrap();
        let cut_off = cut_off_the_unfinished_save(&mut file, &directory).unwrap();
        let wrote = (length - before_bytes.len()) as u64;
        if wrote >= 4 {
            assert_eq!(cut_off, wrote, "left at {length}");
            assert_eq!(fs::read(&cut).unwrap(), before_bytes, "left at {length}");
        } else {
            assert_eq!(cut_off, 0, "a few bytes may be anything: left at {length}");
        }
    });
}

#[test]
fn a_save_that_is_made_after_one_that_did_not_finish_is_whole_and_is_the_one_that_counts() {
    let folder = tempfile::tempdir().unwrap();
    let (path, video, _) = an_archive_with_two_files_and_a_project(&folder);
    let before_bytes = fs::read(&path).unwrap();
    append(
        &path,
        &["resources/1-v.mp4", "resources/2-s.wav"],
        &[("project.fastforwardrs.json", br#"{"unfinished": true}"#)],
        Zip64Thresholds::REAL,
    );
    let after_bytes = fs::read(&path).unwrap();
    for length in [
        before_bytes.len() + 1,
        before_bytes.len() + 3,
        before_bytes.len() + 40,
        after_bytes.len() - 1,
    ] {
        fs::write(&path, &after_bytes[..length]).unwrap();

        append(
            &path,
            &["resources/1-v.mp4", "resources/2-s.wav"],
            &[("project.fastforwardrs.json", br#"{"second": true}"#)],
            Zip64Thresholds::REAL,
        );

        let read = what_the_zip_crate_reads(&path);
        assert_eq!(
            read.iter()
                .find(|(name, _)| name == "project.fastforwardrs.json")
                .unwrap()
                .1,
            br#"{"second": true}"#.to_vec(),
            "after one left at {length}"
        );
        assert_eq!(
            read.iter().find(|(name, _)| name == "resources/1-v.mp4").unwrap().1,
            video
        );
        assert_eq!(read.len(), 3);
    }
}

#[test]
fn bytes_after_the_last_end_record_that_are_not_the_beginning_of_an_entry_are_left_alone() {
    let folder = tempfile::tempdir().unwrap();
    let (path, _, _) = an_archive_with_two_files_and_a_project(&folder);
    let length = fs::metadata(&path).unwrap().len();
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"something else entirely")
        .unwrap();
    let mut file = OpenOptions::new().read(true).write(true).open(&path).unwrap();
    let directory = read_directory(&mut file).unwrap();

    let cut_off = cut_off_the_unfinished_save(&mut file, &directory).unwrap();

    assert_eq!(cut_off, 0);
    assert_eq!(fs::metadata(&path).unwrap().len(), length + 23);
    assert_eq!(directory.end_of_the_last_end_record, length);
    assert_eq!(directory.bytes_of_garbage(), 23);
}

#[test]
fn the_end_record_is_found_however_far_from_the_end_of_the_file_it_is_and_wherever_the_windows_of_the_search_fall() {
    let folder = tempfile::tempdir().unwrap();
    let (path, _, _) = an_archive_with_two_files_and_a_project(&folder);
    let whole = fs::read(&path).unwrap();
    // What is after the end record is what an entry that was begun looks like, of lengths that put the signature of the end record
    // across the edge of a window of the search (which is 64 KiB from the end) in every way, and further than several windows.
    let mut lengths: Vec<usize> = (SEARCH_WINDOW_BYTES as usize - 40..SEARCH_WINDOW_BYTES as usize + 40).collect();
    lengths.extend([
        3 * SEARCH_WINDOW_BYTES as usize - 20,
        3 * SEARCH_WINDOW_BYTES as usize + 5,
        200_000,
    ]);
    for tail in lengths {
        let mut bytes = whole.clone();
        bytes.extend_from_slice(&LOCAL_HEADER_SIGNATURE);
        bytes.extend(bytes_that_do_not_compress(tail, 5));
        let longer = archive_in(&folder, "longer.ffrs");
        fs::write(&longer, &bytes).unwrap();

        let directory = directory_of(&longer);

        assert_eq!(
            directory.end_of_the_last_end_record,
            whole.len() as u64,
            "a tail of {tail}"
        );
        assert_eq!(directory.entries.len(), 3, "a tail of {tail}");
    }
}

#[test]
fn bytes_that_look_like_an_end_record_inside_what_was_cut_off_are_not_taken_for_one() {
    let folder = tempfile::tempdir().unwrap();
    let (path, _, _) = an_archive_with_two_files_and_a_project(&folder);
    let whole = fs::read(&path).unwrap();
    // An entry that was begun, whose bytes have in them the signature of an end record and numbers that make it look whole.
    let mut fake = Vec::new();
    fake.extend_from_slice(&LOCAL_HEADER_SIGNATURE);
    fake.extend_from_slice(&[0; 40]);
    fake.extend_from_slice(&END_RECORD_SIGNATURE);
    fake.extend_from_slice(&[0, 0, 0, 0, 1, 0, 1, 0]);
    fake.extend_from_slice(&46_u32.to_le_bytes());
    fake.extend_from_slice(&0_u32.to_le_bytes());
    fake.extend_from_slice(&[0, 0]);
    let mut bytes = whole.clone();
    bytes.extend_from_slice(&fake);
    let cut = archive_in(&folder, "fake.ffrs");
    fs::write(&cut, &bytes).unwrap();

    let directory = directory_of(&cut);

    assert_eq!(directory.end_of_the_last_end_record, whole.len() as u64);
    assert_eq!(directory.entries.len(), 3);
}

#[test]
fn a_file_with_no_directory_in_it_is_said_so() {
    let folder = tempfile::tempdir().unwrap();
    let path = archive_in(&folder, "text.ffrs");
    fs::write(&path, b"this is only text, and not very long").unwrap();
    let error = read_directory(&mut File::open(&path).unwrap()).unwrap_err();
    assert!(matches!(error, LayoutProblem::NoDirectory), "{error:?}");

    let empty = archive_in(&folder, "empty");
    fs::write(&empty, b"").unwrap();
    let error = read_directory(&mut File::open(&empty).unwrap()).unwrap_err();
    assert!(matches!(error, LayoutProblem::NoDirectory), "{error:?}");
}

#[test]
fn an_entry_whose_header_is_not_where_the_directory_says_makes_the_directory_not_a_whole_one() {
    let folder = tempfile::tempdir().unwrap();
    let (path, _, _) = an_archive_with_two_files_and_a_project(&folder);
    let mut bytes = fs::read(&path).unwrap();
    let start = directory_of(&path)
        .entry_named("resources/1-v.mp4")
        .unwrap()
        .header_start as usize;
    bytes[start] = b'X';
    fs::write(&path, &bytes).unwrap();

    let error = read_directory(&mut File::open(&path).unwrap()).unwrap_err();

    assert!(matches!(error, LayoutProblem::NoDirectory), "{error:?}");
}

#[test]
fn an_archive_that_has_a_comment_after_its_end_record_is_read_and_the_comment_is_not_taken_for_a_save_that_was_cut_off()
{
    let folder = tempfile::tempdir().unwrap();
    let path = archive_in(&folder, "commented.ffrs");
    let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
    writer
        .start_file(
            "a",
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
    writer.write_all(b"hello").unwrap();
    writer
        .set_raw_comment(b"made by another program".to_vec().into())
        .unwrap();
    writer.finish().unwrap();

    let mut file = OpenOptions::new().read(true).write(true).open(&path).unwrap();
    let directory = read_directory(&mut file).unwrap();

    assert_eq!(directory.entries.len(), 1);
    assert_eq!(directory.end_of_the_last_end_record, directory.file_length);
    assert_eq!(cut_off_the_unfinished_save(&mut file, &directory).unwrap(), 0);
}

#[test]
fn the_zip_crate_reads_the_first_bytes_of_a_file_as_an_archive_with_the_rest_not_there() {
    let folder = tempfile::tempdir().unwrap();
    let (path, _, _) = an_archive_with_two_files_and_a_project(&folder);
    let before = fs::metadata(&path).unwrap().len();
    append(
        &path,
        &["resources/1-v.mp4", "resources/2-s.wav"],
        &[("project.fastforwardrs.json", br#"{"newer": true}"#)],
        Zip64Thresholds::REAL,
    );
    let longer = fs::read(&path).unwrap();
    let cut = archive_in(&folder, "cut.ffrs");
    fs::write(&cut, &longer[..longer.len() - 10]).unwrap();

    // The file is a cut-off save; in the first bytes of it, up to the last whole end record, the crate finds the archive as it was
    // (and the file is not changed to make it).
    let prefix = FirstBytesOf::new(File::open(&cut).unwrap(), before);
    let mut archive = zip::ZipArchive::new(prefix).unwrap();
    let mut text = String::new();
    archive
        .by_name("project.fastforwardrs.json")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert_eq!(text, r#"{"version": 1}"#);
}

// --- what other programs write ---

#[test]
fn an_archive_that_the_zip_crate_wrote_is_read_with_the_places_and_the_methods_it_has() {
    let folder = tempfile::tempdir().unwrap();
    let path = archive_in(&folder, "other.zip");
    let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    writer.start_file("stored.txt", stored).unwrap();
    writer.write_all(b"some text that is stored").unwrap();
    writer.start_file("deflated.txt", deflated).unwrap();
    writer.write_all(&b"a b c ".repeat(500)).unwrap();
    writer.finish().unwrap();

    let directory = directory_of(&path);
    let mut archive = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();

    assert_eq!(directory.entries.len(), 2);
    for (index, entry) in directory.entries.iter().enumerate() {
        let from_the_crate = archive.by_index(index).unwrap();
        assert_eq!(entry.name, from_the_crate.name().unwrap());
        assert_eq!(entry.data_start, from_the_crate.data_start().unwrap());
        assert_eq!(entry.length, from_the_crate.size());
        assert_eq!(entry.stored_length, from_the_crate.compressed_size());
        assert_eq!(entry.crc32, from_the_crate.crc32());
    }
    assert!(directory.entries[0].is_stored());
    assert!(!directory.entries[1].is_stored());
    assert_eq!(directory.bytes_of_garbage(), 0);
}

// --- zip64 ---

#[test]
fn the_zip64_forms_of_an_entry_and_its_record_are_read_by_the_zip_crate_and_by_the_directory_reader() {
    let folder = tempfile::tempdir().unwrap();
    let path = archive_in(&folder, "low.ffrs");
    let long = bytes_that_do_not_compress(500, 20);
    let short = bytes_that_do_not_compress(9, 21);
    // (The second entry begins past 100 bytes in, which is far; the third is the third, and the third is many.)
    write_fresh(&path, &[("short", &short), ("long", &long), ("project", b"{}")], LOW);

    let directory = directory_of(&path);
    let mut archive = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();

    assert_eq!(directory.entries.len(), 3);
    for (index, entry) in directory.entries.iter().enumerate() {
        let from_the_crate = archive.by_index(index).unwrap();
        assert_eq!(entry.name, from_the_crate.name().unwrap());
        assert_eq!(entry.header_start, from_the_crate.header_start(), "{}", entry.name);
        assert_eq!(entry.data_start, from_the_crate.data_start().unwrap(), "{}", entry.name);
        assert_eq!(entry.length, from_the_crate.size(), "{}", entry.name);
        assert_eq!(entry.crc32, from_the_crate.crc32(), "{}", entry.name);
    }
    assert_eq!(
        what_the_zip_crate_reads(&path),
        vec![
            ("short".to_owned(), short),
            ("long".to_owned(), long),
            ("project".to_owned(), b"{}".to_vec())
        ]
    );
}

#[test]
fn a_zip64_end_record_is_written_where_the_numbers_are_too_big_for_the_ordinary_one_and_found_again() {
    let folder = tempfile::tempdir().unwrap();
    let path = archive_in(&folder, "low.ffrs");
    write_fresh(&path, &[("a", b"1"), ("b", b"2"), ("c", b"3"), ("d", b"4")], LOW);
    let bytes = fs::read(&path).unwrap();

    assert!(
        bytes.windows(4).any(|window| window == ZIP64_END_RECORD_SIGNATURE),
        "the zip64 end record is there"
    );
    assert!(
        bytes.windows(4).any(|window| window == ZIP64_LOCATOR_SIGNATURE),
        "and the locator"
    );
    assert_eq!(names_in_the_directory(&path), vec!["a", "b", "c", "d"]);
    assert_eq!(what_the_zip_crate_reads(&path).len(), 4);
}

#[test]
fn the_real_thresholds_are_the_largest_numbers_that_the_ordinary_fields_hold() {
    assert_eq!(Zip64Thresholds::REAL.length, u64::from(u32::MAX));
    assert_eq!(Zip64Thresholds::REAL.offset, u64::from(u32::MAX));
    assert_eq!(Zip64Thresholds::REAL.count, u64::from(u16::MAX));
}

#[test]
fn a_save_in_the_zip64_form_that_was_cut_off_at_any_byte_leaves_the_archive_as_it_was() {
    let folder = tempfile::tempdir().unwrap();
    let path = archive_in(&folder, "low.ffrs");
    write_fresh(&path, &[("a", b"1234567890123"), ("project", b"{}")], LOW);
    let before_bytes = fs::read(&path).unwrap();
    append(
        &path,
        &["a"],
        &[("b", b"abcdefghijklmnop"), ("c", b"xy"), ("project", b"{\"n\": 2}")],
        LOW,
    );
    let after_bytes = fs::read(&path).unwrap();

    for length in before_bytes.len() + 1..after_bytes.len() {
        let cut = archive_in(&folder, "cut.ffrs");
        fs::write(&cut, &after_bytes[..length]).unwrap();
        let directory = directory_of(&cut);
        assert_eq!(
            directory.end_of_the_last_end_record,
            before_bytes.len() as u64,
            "left at {length}"
        );
    }
    // And the whole of it is read, with its zip64 end record.
    assert_eq!(names_in_the_directory(&path), vec!["a", "b", "c", "project"]);
}

#[test]
fn bytes_are_copied_whole_whatever_size_the_buffer_makes_the_pieces() {
    let folder = tempfile::tempdir().unwrap();
    let path = archive_in(&folder, "big.ffrs");
    let big = bytes_that_do_not_compress(COPY_BUFFER_BYTES * 2 + 12_345, 30);

    write_fresh(&path, &[("big", &big)], Zip64Thresholds::REAL);

    assert_eq!(what_the_zip_crate_reads(&path), vec![("big".to_owned(), big)]);
}

// --- what is not believed ---

#[test]
fn an_end_record_whose_comment_goes_past_the_end_of_the_file_is_not_believed() {
    let folder = tempfile::tempdir().unwrap();
    let (path, _, _) = an_archive_with_two_files_and_a_project(&folder);
    let mut bytes = fs::read(&path).unwrap();
    let length = bytes.len();
    // (The last two bytes of an end record that has no comment say how long its comment is.)
    bytes[length - 2] = 0xff;
    bytes[length - 1] = 0xff;
    fs::write(&path, &bytes).unwrap();

    let error = read_directory(&mut File::open(&path).unwrap()).unwrap_err();

    assert!(matches!(error, LayoutProblem::NoDirectory), "{error:?}");
}

#[test]
fn an_entry_that_says_it_has_more_bytes_than_there_is_room_for_before_the_directory_is_not_believed() {
    let folder = tempfile::tempdir().unwrap();
    let (path, _, _) = an_archive_with_two_files_and_a_project(&folder);
    let mut bytes = fs::read(&path).unwrap();
    let directory_start = directory_of(&path).directory_start as usize;
    // The first record of the directory: its compressed size is the four bytes at 20 (it is not 0xFFFFFFFF, which would send the
    // reader to a zip64 field).
    bytes[directory_start + 20..directory_start + 24].copy_from_slice(&0x7fff_0000_u32.to_le_bytes());
    fs::write(&path, &bytes).unwrap();

    let error = read_directory(&mut File::open(&path).unwrap()).unwrap_err();

    assert!(matches!(error, LayoutProblem::NoDirectory), "{error:?}");
}

#[test]
fn a_zip64_locator_that_is_not_one_makes_the_end_record_not_a_whole_one() {
    let folder = tempfile::tempdir().unwrap();
    let path = archive_in(&folder, "low.ffrs");
    write_fresh(&path, &[("a", b"1"), ("b", b"2"), ("c", b"3"), ("d", b"4")], LOW);
    assert_eq!(
        names_in_the_directory(&path),
        vec!["a", "b", "c", "d"],
        "the setup: it is read"
    );
    let mut bytes = fs::read(&path).unwrap();
    let at = bytes
        .windows(4)
        .rposition(|window| window == ZIP64_LOCATOR_SIGNATURE)
        .expect("the locator is there");
    bytes[at + 3] = 0x08;
    fs::write(&path, &bytes).unwrap();

    let error = read_directory(&mut File::open(&path).unwrap()).unwrap_err();

    assert!(matches!(error, LayoutProblem::NoDirectory), "{error:?}");
}
