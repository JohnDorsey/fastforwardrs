// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs;
use std::io::{Read, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::*;

fn identifier(number: u32) -> SourceMediaIdentifier {
    SourceMediaIdentifier::new(number)
}

/// `length` bytes that look like what a video is: no two stretches of them alike, and nothing that a compressor could use.
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

/// The CRC-32 of `bytes`, worked out the plain way (one bit at a time), to check the archive's against.
fn crc32_of(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn a_file(folder: &Path, name: &str, contents: &[u8]) -> FileToInclude {
    let path = folder.join(name);
    fs::write(&path, contents).unwrap();
    FileToInclude {
        entry_name: resource_entry_name(identifier(1), name),
        source: path.into(),
    }
}

fn names_of_the_entries(archive: &Path) -> Vec<String> {
    let zip = ZipArchive::new(File::open(archive).unwrap()).unwrap();
    let mut names: Vec<String> = zip.file_names().map(|name| name.unwrap().into_owned()).collect();
    names.sort();
    names
}

fn names_in(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

// --- what is written is what is read ---

#[test]
fn an_archive_has_the_project_and_the_files_in_it_as_they_were_written() {
    let folder = tempfile::tempdir().unwrap();
    let video = bytes_that_do_not_compress(300_000, 1);
    let sound = bytes_that_do_not_compress(5_000, 2);
    let files = [
        FileToInclude {
            entry_name: resource_entry_name(identifier(1), "holiday.mp4"),
            ..a_file(folder.path(), "holiday.mp4", &video)
        },
        FileToInclude {
            entry_name: resource_entry_name(identifier(2), "song.mp3"),
            ..a_file(folder.path(), "song.mp3", &sound)
        },
    ];
    let archive = folder.path().join("project.ffrs");

    write_archive(&archive, "{\"the project\": true}\n", &files).unwrap();

    assert_eq!(read_project_text(&archive).unwrap(), "{\"the project\": true}\n");
    assert_eq!(
        names_of_the_entries(&archive),
        vec![
            "project.fastforwardrs.json".to_owned(),
            "resources/1-holiday.mp4".to_owned(),
            "resources/2-song.mp3".to_owned(),
        ]
    );
    let taken_out = folder.path().join("taken out.mp4");
    extract_entry(&archive, "resources/1-holiday.mp4", &taken_out).unwrap();
    assert_eq!(fs::read(&taken_out).unwrap(), video);
    extract_entry(&archive, "resources/2-song.mp3", &taken_out).unwrap();
    assert_eq!(fs::read(&taken_out).unwrap(), sound, "and what is there is replaced");
}

#[test]
fn an_archive_of_a_project_with_no_files_is_just_the_project() {
    let folder = tempfile::tempdir().unwrap();
    let archive = folder.path().join("empty.ffrs");

    write_archive(&archive, "text", &[]).unwrap();

    assert_eq!(names_of_the_entries(&archive), vec![PROJECT_ENTRY_NAME.to_owned()]);
    assert_eq!(read_project_text(&archive).unwrap(), "text");
    assert!(is_an_archive(&archive));
}

#[test]
fn nothing_in_an_archive_is_compressed_so_the_files_are_in_it_as_they_are() {
    let folder = tempfile::tempdir().unwrap();
    // (Text, which a compressor shrinks a great deal: it is stored as it is all the same.)
    let text = "a project that says the same thing over and over, ".repeat(2000);
    let video = bytes_that_do_not_compress(100_000, 3);
    let included = a_file(folder.path(), "clip.mov", &video);
    let archive = folder.path().join("stored.ffrs");

    write_archive(&archive, &text, std::slice::from_ref(&included)).unwrap();

    let mut zip = ZipArchive::new(File::open(&archive).unwrap()).unwrap();
    for index in 0..zip.len() {
        let entry = zip.by_index(index).unwrap();
        assert_eq!(entry.compression(), CompressionMethod::Stored, "{:?}", entry.name());
        assert_eq!(entry.compressed_size(), entry.size(), "{:?}", entry.name());
    }
    // The bytes of the file are in the archive, one after another, which a program could read where they are.
    let whole_archive = fs::read(&archive).unwrap();
    assert!(
        find(&whole_archive, &video).is_some(),
        "the file is in the archive as it is"
    );
    assert!(find(&whole_archive, text.as_bytes()).is_some(), "so is the project");
}

#[test]
fn an_archive_that_another_zip_program_compressed_is_read_as_well_as_one_that_was_stored() {
    // (A zip program that is used to change an archive may compress what it writes, the project and the files that were in it
    // before: what is read is what was written, and this program writes everything stored, as it always did.)
    let folder = tempfile::tempdir().unwrap();
    let archive = folder.path().join("recompressed.ffrs");
    let project_text = "{\"a project\": \"that says the same thing over and over\"}
"
    .repeat(100);
    let video = vec![7_u8; 50_000];
    {
        let mut writer = ZipWriter::new(File::create(&archive).unwrap());
        let compressed = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer.start_file(PROJECT_ENTRY_NAME, compressed).unwrap();
        writer.write_all(project_text.as_bytes()).unwrap();
        writer.start_file("resources/1-v.mp4", compressed).unwrap();
        writer.write_all(&video).unwrap();
        writer.finish().unwrap();
    }
    let zip = ZipArchive::new(File::open(&archive).unwrap()).unwrap();
    assert!(
        zip.len() == 2 && fs::metadata(&archive).unwrap().len() < 5_000,
        "the setup: what is in it is much shorter than it is when it is stored"
    );

    assert_eq!(read_project_text(&archive).unwrap(), project_text);
    let facts = facts_of_entry(&archive, "resources/1-v.mp4").unwrap();
    assert_eq!((facts.size, facts.crc32), (50_000, crc32_of(&video)));
    let taken_out = folder.path().join("out");
    extract_entry(&archive, "resources/1-v.mp4", &taken_out).unwrap();
    assert_eq!(fs::read(&taken_out).unwrap(), video);
}

#[test]
fn a_file_of_nothing_and_a_file_of_several_megabytes_are_both_kept_whole() {
    let folder = tempfile::tempdir().unwrap();
    let empty = FileToInclude {
        entry_name: resource_entry_name(identifier(1), "empty.wav"),
        ..a_file(folder.path(), "empty.wav", b"")
    };
    let big_bytes = bytes_that_do_not_compress(5 * COPY_BUFFER_BYTES + 12_345, 4);
    let big = FileToInclude {
        entry_name: resource_entry_name(identifier(2), "big.mkv"),
        ..a_file(folder.path(), "big.mkv", &big_bytes)
    };
    let archive = folder.path().join("sizes.ffrs");

    write_archive(&archive, "p", &[empty, big]).unwrap();

    let taken_out = folder.path().join("out");
    extract_entry(&archive, "resources/1-empty.wav", &taken_out).unwrap();
    assert_eq!(fs::read(&taken_out).unwrap(), b"");
    extract_entry(&archive, "resources/2-big.mkv", &taken_out).unwrap();
    assert_eq!(fs::read(&taken_out).unwrap(), big_bytes);
}

#[test]
fn the_facts_of_an_entry_are_its_length_and_its_crc_which_were_worked_out_apart_from_the_archive() {
    let folder = tempfile::tempdir().unwrap();
    let bytes = bytes_that_do_not_compress(70_001, 5);
    let included = a_file(folder.path(), "x.mp4", &bytes);
    let archive = folder.path().join("a.ffrs");
    write_archive(&archive, "p", &[included]).unwrap();

    let facts = facts_of_entry(&archive, "resources/1-x.mp4").unwrap();

    assert_eq!(facts.size, 70_001);
    assert_eq!(facts.crc32, crc32_of(&bytes));
}

#[test]
fn a_file_with_a_name_that_is_not_plain_is_kept_and_found_by_the_name_that_was_made_for_it() {
    let folder = tempfile::tempdir().unwrap();
    let name = "Ferien in Österreich — Tag 1 (ワイド).mp4";
    let bytes = bytes_that_do_not_compress(3_000, 6);
    let included = FileToInclude {
        entry_name: resource_entry_name(identifier(14), name),
        ..a_file(folder.path(), "plain.mp4", &bytes)
    };
    let archive = folder.path().join("names.ffrs");
    write_archive(&archive, "p", std::slice::from_ref(&included)).unwrap();

    let taken_out = folder.path().join("out");
    extract_entry(&archive, &included.entry_name, &taken_out).unwrap();

    assert_eq!(fs::read(&taken_out).unwrap(), bytes);
    assert_eq!(
        file_name_of_a_resource_entry(&included.entry_name),
        Some("Ferien in Österreich — Tag 1 (ワイド).mp4")
    );
}

// --- telling an archive from other files, and a damaged archive from a whole one ---

#[test]
fn a_file_is_an_archive_if_it_begins_like_one_and_not_otherwise() {
    let folder = tempfile::tempdir().unwrap();
    let archive = folder.path().join("a.ffrs");
    write_archive(&archive, "p", &[]).unwrap();
    let json = folder.path().join("p.fastforwardrs.json");
    fs::write(&json, "{ \"file_format\": \"fastforwardrs-project\" }").unwrap();
    let empty = folder.path().join("empty");
    fs::write(&empty, "").unwrap();
    let short = folder.path().join("short");
    fs::write(&short, "PK").unwrap();

    assert!(is_an_archive(&archive));
    assert!(!is_an_archive(&json));
    assert!(!is_an_archive(&empty));
    assert!(!is_an_archive(&short));
    assert!(!is_an_archive(&folder.path().join("no such file")));
    assert!(!is_an_archive(folder.path()), "a folder is not one");
}

#[test]
fn something_that_is_not_an_archive_is_said_to_be_unreadable_and_a_missing_file_could_not_be_read() {
    let folder = tempfile::tempdir().unwrap();
    let json = folder.path().join("p.json");
    fs::write(&json, "{ }").unwrap();

    let error = read_project_text(&json).unwrap_err();
    assert!(matches!(error, ArchiveError::Unreadable { .. }), "{error:?}");
    assert!(error.to_string().contains("p.json"), "{error}");

    let error = read_project_text(&folder.path().join("missing.ffrs")).unwrap_err();
    assert!(matches!(error, ArchiveError::CouldNotRead { .. }), "{error:?}");
}

#[test]
fn an_archive_with_no_project_in_it_says_that_and_so_does_a_missing_entry() {
    let folder = tempfile::tempdir().unwrap();
    let archive = folder.path().join("odd.zip");
    {
        let mut writer = ZipWriter::new(File::create(&archive).unwrap());
        writer
            .start_file("something else.txt", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"hello").unwrap();
        writer.finish().unwrap();
    }

    let error = read_project_text(&archive).unwrap_err();
    assert!(
        matches!(&error, ArchiveError::NoSuchEntry { entry_name, .. } if entry_name == PROJECT_ENTRY_NAME),
        "{error:?}"
    );
    let error = facts_of_entry(&archive, "resources/1-a.mp4").unwrap_err();
    assert!(matches!(error, ArchiveError::NoSuchEntry { .. }), "{error:?}");
    let error = extract_entry(&archive, "resources/1-a.mp4", &folder.path().join("out")).unwrap_err();
    assert!(matches!(error, ArchiveError::NoSuchEntry { .. }), "{error:?}");
    assert!(!folder.path().join("out").exists(), "nothing was made for it");
}

#[test]
fn an_archive_that_is_cut_short_is_unreadable() {
    let folder = tempfile::tempdir().unwrap();
    let included = a_file(folder.path(), "v.mp4", &bytes_that_do_not_compress(50_000, 7));
    let archive = folder.path().join("a.ffrs");
    write_archive(&archive, "p", &[included]).unwrap();
    let whole = fs::read(&archive).unwrap();
    fs::write(&archive, &whole[..whole.len() - 40]).unwrap();

    let error = read_project_text(&archive).unwrap_err();

    assert!(matches!(error, ArchiveError::Unreadable { .. }), "{error:?}");
}

#[test]
fn a_file_that_is_damaged_in_the_archive_is_found_out_when_it_is_taken_out_and_leaves_nothing_behind() {
    let folder = tempfile::tempdir().unwrap();
    let bytes = bytes_that_do_not_compress(40_000, 8);
    let included = a_file(folder.path(), "v.mp4", &bytes);
    let archive = folder.path().join("a.ffrs");
    write_archive(&archive, "p", &[included]).unwrap();
    // One byte of the middle of the file is changed (it is in the archive as it is, so it is found there).
    let mut whole = fs::read(&archive).unwrap();
    let at = find(&whole, &bytes[10_000..10_064]).unwrap();
    whole[at + 20] ^= 0xFF;
    fs::write(&archive, &whole).unwrap();
    let taken_out = folder.path().join("out");

    let error = extract_entry(&archive, "resources/1-v.mp4", &taken_out).unwrap_err();

    assert!(matches!(error, ArchiveError::Unreadable { .. }), "{error:?}");
    assert!(error.to_string().contains("damaged"), "{error}");
    assert!(!taken_out.exists(), "no half of a file");
    assert!(
        names_in(folder.path()).iter().all(|name| !name.ends_with(".saving")),
        "{:?}",
        names_in(folder.path())
    );
}

#[test]
fn what_an_extraction_replaces_is_left_as_it_was_if_the_extraction_fails() {
    let folder = tempfile::tempdir().unwrap();
    let bytes = bytes_that_do_not_compress(40_000, 9);
    let included = a_file(folder.path(), "v.mp4", &bytes);
    let archive = folder.path().join("a.ffrs");
    write_archive(&archive, "p", &[included]).unwrap();
    let mut whole = fs::read(&archive).unwrap();
    let at = find(&whole, &bytes[30_000..30_064]).unwrap();
    whole[at + 3] ^= 0x01;
    fs::write(&archive, &whole).unwrap();
    let taken_out = folder.path().join("out");
    fs::write(&taken_out, "what was there").unwrap();

    assert!(extract_entry(&archive, "resources/1-v.mp4", &taken_out).is_err());

    assert_eq!(fs::read_to_string(&taken_out).unwrap(), "what was there");
}

// --- writing is all or nothing ---

#[test]
fn an_archive_that_replaces_another_replaces_it_whole() {
    let folder = tempfile::tempdir().unwrap();
    let archive = folder.path().join("a.ffrs");
    let first = a_file(folder.path(), "one.mp4", &bytes_that_do_not_compress(10_000, 10));
    write_archive(&archive, "first", std::slice::from_ref(&first)).unwrap();

    write_archive(&archive, "second", &[]).unwrap();

    assert_eq!(read_project_text(&archive).unwrap(), "second");
    assert_eq!(names_of_the_entries(&archive), vec![PROJECT_ENTRY_NAME.to_owned()]);
    assert!(names_in(folder.path()).iter().all(|name| !name.ends_with(".saving")));
}

#[test]
fn a_file_that_cannot_be_read_stops_the_archive_before_anything_of_the_old_one_is_touched() {
    let folder = tempfile::tempdir().unwrap();
    let archive = folder.path().join("a.ffrs");
    write_archive(&archive, "the old one", &[]).unwrap();
    let missing = FileToInclude {
        entry_name: resource_entry_name(identifier(1), "gone.mp4"),
        source: folder.path().join("gone.mp4").into(),
    };

    let error = write_archive(&archive, "the new one", &[missing]).unwrap_err();

    assert!(
        matches!(&error, ArchiveError::CouldNotRead { path, .. } if path.ends_with("gone.mp4")),
        "{error:?}"
    );
    assert_eq!(read_project_text(&archive).unwrap(), "the old one");
    assert_eq!(names_in(folder.path()), vec!["a.ffrs".to_owned()]);
}

#[test]
fn an_archive_cannot_be_written_into_a_folder_that_is_not_there_and_nothing_is_made() {
    let folder = tempfile::tempdir().unwrap();
    let archive = folder.path().join("no such folder").join("a.ffrs");

    let error = write_archive(&archive, "p", &[]).unwrap_err();

    assert!(matches!(error, ArchiveError::CouldNotWrite { .. }), "{error:?}");
    assert!(names_in(folder.path()).is_empty());
}

// --- the names of entries ---

#[test]
fn the_name_of_the_entry_of_a_file_has_the_number_of_the_source_and_the_name_of_the_file() {
    assert_eq!(
        resource_entry_name(identifier(1), "holiday.mp4"),
        "resources/1-holiday.mp4"
    );
    assert_eq!(resource_entry_name(identifier(42), "a b.MOV"), "resources/42-a b.MOV");
    assert_ne!(
        resource_entry_name(identifier(1), "clip.mp4"),
        resource_entry_name(identifier(2), "clip.mp4"),
        "two files of one name are two entries"
    );
}

#[test]
fn what_a_name_cannot_have_in_it_is_made_into_a_dash_that_is_low() {
    assert_eq!(
        resource_entry_name(identifier(3), "a/b\\c:d*e?f\"g<h>i|j.mp4"),
        "resources/3-a_b_c_d_e_f_g_h_i_j.mp4"
    );
    assert_eq!(
        resource_entry_name(identifier(3), "tab\there\u{7}.mp4"),
        "resources/3-tab_here_.mp4"
    );
    assert_eq!(
        resource_entry_name(identifier(3), "  ..hidden.mp4. "),
        "resources/3-hidden.mp4"
    );
    assert_eq!(resource_entry_name(identifier(3), ""), "resources/3-file");
    assert_eq!(resource_entry_name(identifier(3), "..."), "resources/3-file");
    assert_eq!(
        resource_entry_name(identifier(3), "../../etc/passwd"),
        "resources/3-_.._etc_passwd"
    );
}

#[test]
fn a_name_that_is_too_long_loses_the_beginning_of_it_and_keeps_the_ending() {
    let long_name = format!("{}.mp4", "w".repeat(400));

    let name = resource_entry_name(identifier(123), &long_name);

    assert!(name.len() <= 255, "{} bytes", name.len());
    assert!(name.starts_with("resources/123-"), "{name}");
    assert!(name.ends_with("w.mp4"), "{name}");
    assert!(is_a_resource_entry_name(&name));

    let wide = format!("{}.mp4", "ワ".repeat(200));
    let name = resource_entry_name(identifier(1), &wide);
    assert!(name.len() <= 255, "{} bytes", name.len());
    assert!(
        name.ends_with(".mp4") && name.contains('ワ'),
        "no character was cut in two: {name}"
    );
}

#[test]
fn every_name_that_is_made_for_a_file_is_a_name_of_a_resource_and_gives_the_name_it_was_made_from_back() {
    for (number, file_name) in [
        (0, "a.mp4"),
        (7, "with space.wav"),
        (1_000_000, "ünï.png"),
        (5, ".hidden"),
        (5, "-leading dash.mp4"),
        (8, "1-2-3.mp4"),
    ] {
        let name = resource_entry_name(identifier(number), file_name);
        assert!(is_a_resource_entry_name(&name), "{name}");
        let given_back = file_name_of_a_resource_entry(&name).unwrap();
        // (What is given back is the file name with what it cannot have taken out, and not the dots and spaces at its ends.)
        assert_eq!(
            given_back,
            file_name.trim_matches(|character| character == '.' || character == ' '),
            "{name}"
        );
    }
}

#[test]
fn a_name_that_a_file_says_is_a_resource_name_only_if_it_is_one_that_cannot_lead_out_of_the_cache() {
    for good in [
        "resources/1-a.mp4",
        "resources/12-b c.wav",
        "resources/0-.hidden",
        "resources/3-1-2.mp4",
    ] {
        assert!(is_a_resource_entry_name(good), "{good}");
    }
    for bad in [
        "",
        "project.fastforwardrs.json",
        "resources/",
        "resources/-a.mp4",
        "resources/a-b.mp4",
        "resources/1-",
        "resources/1-.",
        "resources/1-..",
        "resources/1-../../x",
        "resources/1-a/b.mp4",
        "resources/1-a\\b.mp4",
        "resources/1-c:\\x.mp4",
        "resources/../1-a.mp4",
        "/resources/1-a.mp4",
        "resources/1-a\u{0}.mp4",
        "Resources/1-a.mp4",
        "resources/1 -a.mp4",
        "resources/-1-a.mp4",
    ] {
        assert!(!is_a_resource_entry_name(bad), "{bad:?}");
        assert_eq!(file_name_of_a_resource_entry(bad), None, "{bad:?}");
    }
    assert!(
        !is_a_resource_entry_name(&format!("resources/1-{}", "x".repeat(300))),
        "too long"
    );
}

// --- comparing files ---

#[test]
fn files_have_the_same_contents_when_every_byte_is_the_same() {
    let folder = tempfile::tempdir().unwrap();
    let bytes = bytes_that_do_not_compress(3 * COPY_BUFFER_BYTES + 17, 11);
    let (first, second, third, shorter, empty, other_empty) = (
        folder.path().join("first"),
        folder.path().join("second"),
        folder.path().join("third"),
        folder.path().join("shorter"),
        folder.path().join("empty"),
        folder.path().join("other empty"),
    );
    fs::write(&first, &bytes).unwrap();
    fs::write(&second, &bytes).unwrap();
    let mut changed_at_the_end = bytes.clone();
    *changed_at_the_end.last_mut().unwrap() ^= 1;
    fs::write(&third, &changed_at_the_end).unwrap();
    fs::write(&shorter, &bytes[..bytes.len() - 1]).unwrap();
    fs::write(&empty, "").unwrap();
    fs::write(&other_empty, "").unwrap();

    assert!(have_the_same_contents(&first, &second).unwrap());
    assert!(
        !have_the_same_contents(&first, &third).unwrap(),
        "one byte at the very end"
    );
    assert!(!have_the_same_contents(&first, &shorter).unwrap(), "one byte shorter");
    assert!(have_the_same_contents(&empty, &other_empty).unwrap());
    assert!(!have_the_same_contents(&first, &empty).unwrap());
    assert!(have_the_same_contents(&first, folder.path().join("missing")).is_err());
}

#[test]
fn the_folder_for_a_file_is_made_with_the_folders_above_it_that_are_not_there() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("a").join("b").join("c.mp4");

    make_the_folder_for(&file).unwrap();

    assert!(folder.path().join("a").join("b").is_dir());
    assert!(!file.exists());
    make_the_folder_for(&file).unwrap();
    make_the_folder_for(Path::new("just a name.mp4")).unwrap();
}

#[test]
fn an_archive_is_read_by_other_programs_because_it_is_a_zip_file_of_the_usual_kind() {
    // (There is nothing here to read it with but the crate that wrote it; that another program reads it was checked with
    // Python's zipfile by hand, which this cannot say. What is checked here is what a reader of any kind relies on: the end of
    // the file is the central directory of an ordinary zip file, and each entry begins with the mark of one.)
    let folder = tempfile::tempdir().unwrap();
    let included = a_file(folder.path(), "v.mp4", &bytes_that_do_not_compress(2_000, 12));
    let archive = folder.path().join("a.ffrs");
    write_archive(&archive, "p", &[included]).unwrap();

    let whole = fs::read(&archive).unwrap();

    assert_eq!(&whole[..4], b"PK\x03\x04", "the first entry begins as one does");
    assert!(
        find(&whole, b"PK\x05\x06").is_some(),
        "the end of the central directory is there"
    );
    let mut zip = ZipArchive::new(File::open(&archive).unwrap()).unwrap();
    let mut project = String::new();
    zip.by_name(PROJECT_ENTRY_NAME)
        .unwrap()
        .read_to_string(&mut project)
        .unwrap();
    assert_eq!(project, "p");
}

/// How long the extra field of the local header of each entry of the archive is, in the order of the entries: it is where the
/// zip64 form of an entry says how long the file is.
fn lengths_of_the_extra_fields_of_the_local_headers(archive: &Path) -> Vec<u16> {
    let bytes = fs::read(archive).unwrap();
    let mut zip = ZipArchive::new(File::open(archive).unwrap()).unwrap();
    (0..zip.len())
        .map(|index| {
            let start = zip.by_index(index).unwrap().header_start() as usize;
            assert_eq!(&bytes[start..start + 4], b"PK", "the entry begins as one does");
            u16::from_le_bytes([bytes[start + 28], bytes[start + 29]])
        })
        .collect()
}

#[test]
fn a_file_that_is_too_long_for_the_ordinary_form_of_a_zip_file_is_written_in_the_zip64_form_and_one_that_is_not_is_not()
{
    let folder = tempfile::tempdir().unwrap();
    let shorter_bytes = bytes_that_do_not_compress(999, 20);
    let exactly_bytes = bytes_that_do_not_compress(1_000, 21);
    let longer_bytes = bytes_that_do_not_compress(5_000, 22);
    let named = |number: u32, name: &str, bytes: &[u8]| FileToInclude {
        entry_name: resource_entry_name(identifier(number), name),
        ..a_file(folder.path(), name, bytes)
    };
    let files = [
        named(1, "shorter.mp4", &shorter_bytes),
        named(2, "exactly.mp4", &exactly_bytes),
        named(3, "longer.mp4", &longer_bytes),
    ];
    let archive = folder.path().join("a.ffrs");

    // (The real limit is a file of 4 GiB, which is too much to write in a test: it is brought down to 1000 bytes.)
    write_archive_using(
        &archive,
        "p",
        &files,
        Zip64Thresholds {
            length: 1_000,
            ..Zip64Thresholds::REAL
        },
        &mut |_| Ok(()),
    )
    .unwrap();

    assert_eq!(
        lengths_of_the_extra_fields_of_the_local_headers(&archive),
        vec![0, 0, 20, 20],
        "the project and the shorter file are in the ordinary form, and the files of the length and longer have the zip64 field,          which is 4 bytes of heading and the 8 bytes of each of the two lengths"
    );
    for (number, name, bytes) in [
        (1, "shorter.mp4", &shorter_bytes),
        (2, "exactly.mp4", &exactly_bytes),
        (3, "longer.mp4", &longer_bytes),
    ] {
        let taken_out = folder.path().join("out");
        extract_entry(&archive, &resource_entry_name(identifier(number), name), &taken_out).unwrap();
        assert_eq!(
            &fs::read(&taken_out).unwrap(),
            bytes,
            "{name} is read as it was written"
        );
    }
}

#[test]
fn files_of_the_usual_length_are_not_written_in_the_zip64_form_by_write_archive() {
    let folder = tempfile::tempdir().unwrap();
    let included = a_file(folder.path(), "v.mp4", &bytes_that_do_not_compress(50_000, 23));
    let archive = folder.path().join("a.ffrs");

    write_archive(&archive, "p", &[included]).unwrap();

    assert_eq!(lengths_of_the_extra_fields_of_the_local_headers(&archive), vec![0, 0]);
}

#[test]
fn a_file_is_in_the_zip64_form_from_the_length_at_which_the_ordinary_form_cannot_say_how_long_it_is() {
    assert_eq!(
        Zip64Thresholds::REAL.length,
        u64::from(u32::MAX),
        "the largest number of four bytes: one byte short of 4 GiB"
    );
}

/// A file of more than four gigabytes goes in an archive whole and comes out whole: a zip file holds one that large only in its
/// zip64 form, which the writer asks for itself. This writes about 13 gigabytes into the temporary folder (the file, the archive
/// and the file taken out again) and takes a while, so it is not run with the others: run it when the archive writer or the zip
/// crate is changed, on its own.
///
/// ```text
/// cargo test --lib a_file_of_more_than_four_gigabytes -- --ignored
/// ```
#[test]
#[ignore = "writes about 13 GB and takes a while; run it on its own when the archive writer or the zip crate changes"]
fn a_file_of_more_than_four_gigabytes_is_kept_whole_and_found_whole() {
    use std::io::{Seek, SeekFrom};

    const FOUR_GIGABYTES: u64 = 4 * 1024 * 1024 * 1024;
    let length = FOUR_GIGABYTES + 123_456;
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("huge.mkv");
    // Zeros, with something in them at the start, across the place where four gigabytes end (where a zip file that is not in its
    // zip64 form cannot say how far it has gone) and at the end, to tell where each part went.
    let marks = [
        (0, *b"the very start!!"),
        (FOUR_GIGABYTES - 8, *b"over four giga!!"),
        (length - 16, *b"the very end!!!!"),
    ];
    {
        let mut file = File::create(&path).unwrap();
        file.set_len(length).unwrap();
        for (at, mark) in marks {
            file.seek(SeekFrom::Start(at)).unwrap();
            file.write_all(&mark).unwrap();
        }
    }
    let included = FileToInclude {
        entry_name: resource_entry_name(identifier(1), "huge.mkv"),
        source: path.clone().into(),
    };
    let archive = folder.path().join("huge.ffrs");

    write_archive(&archive, "p", &[included]).unwrap();

    assert_eq!(read_project_text(&archive).unwrap(), "p");
    let facts = facts_of_entry(&archive, "resources/1-huge.mkv").unwrap();
    assert_eq!(facts.size, length, "its whole length, which does not fit in 32 bits");
    let taken_out = folder.path().join("taken out.mkv");
    extract_entry(&archive, "resources/1-huge.mkv", &taken_out).unwrap();
    assert_eq!(fs::metadata(&taken_out).unwrap().len(), length);
    for (at, mark) in marks {
        let mut file = File::open(&taken_out).unwrap();
        file.seek(SeekFrom::Start(at)).unwrap();
        let mut found = [0_u8; 16];
        file.read_exact(&mut found).unwrap();
        assert_eq!(found, mark, "at {at}");
    }
    assert!(have_the_same_contents(&path, &taken_out).unwrap(), "every byte of it");
}

// --- what an archive says about itself ---

#[test]
fn only_a_file_that_is_stored_as_it_is_can_be_read_where_it_is_in_the_archive() {
    let folder = tempfile::tempdir().unwrap();
    let video = bytes_that_do_not_compress(3_000, 11);
    let stored = folder.path().join("stored.ffrs");
    write_archive(&stored, "{}", &[a_file(folder.path(), "clip.mov", &video)]).unwrap();

    let place = state_of_archive(&stored).unwrap().entries["resources/1-clip.mov"];
    let location = place.as_a_location_in(&stored).expect("it is stored as it is");

    assert_eq!(location.file(), stored.as_path());
    let stretch = location.stretch().expect("a stretch of the archive");
    assert_eq!((stretch.start, stretch.length), (place.data_start, 3_000));
    let whole_archive = fs::read(&stored).unwrap();
    assert_eq!(
        &whole_archive[stretch.start as usize..(stretch.start + stretch.length) as usize],
        video.as_slice(),
        "the bytes there are the file"
    );

    // (A file that another zip program compressed is not there as it is: there is nothing to read where it is.)
    let compressed = folder.path().join("compressed.ffrs");
    {
        let mut writer = ZipWriter::new(File::create(&compressed).unwrap());
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer.start_file(PROJECT_ENTRY_NAME, options).unwrap();
        writer.write_all(b"{}").unwrap();
        writer.start_file("resources/1-clip.mov", options).unwrap();
        writer.write_all(&vec![7_u8; 40_000]).unwrap();
        writer.finish().unwrap();
    }
    let place = state_of_archive(&compressed).unwrap().entries["resources/1-clip.mov"];
    assert!(!place.is_stored);
    assert_eq!(place.as_a_location_in(&compressed), None);
}

#[test]
fn the_bytes_of_the_included_files_are_those_of_the_files_and_not_those_of_the_project() {
    let folder = tempfile::tempdir().unwrap();
    let archive = folder.path().join("a.ffrs");
    let project_text = "a project that has a good deal to say, ".repeat(300);
    write_archive(&archive, &project_text, &[]).unwrap();
    assert_eq!(state_of_archive(&archive).unwrap().bytes_of_included_files(), 0);

    let first = FileToInclude {
        entry_name: resource_entry_name(identifier(1), "one.mp4"),
        ..a_file(folder.path(), "one.mp4", &bytes_that_do_not_compress(1_000, 1))
    };
    let second = FileToInclude {
        entry_name: resource_entry_name(identifier(2), "two.mp4"),
        ..a_file(folder.path(), "two.mp4", &bytes_that_do_not_compress(2_500, 2))
    };
    write_archive(&archive, &project_text, &[first, second]).unwrap();

    assert_eq!(state_of_archive(&archive).unwrap().bytes_of_included_files(), 3_500);
}

#[test]
fn garbage_is_worth_taking_away_when_there_is_any_and_little_is_in_use_and_when_it_is_half_of_what_is_in_use_if_more_is()
 {
    let state = |bytes_in_use: u64, bytes_of_garbage: u64| ArchiveState {
        entries: HashMap::new(),
        bytes_in_use,
        bytes_of_garbage,
    };
    let megabyte = 1_u64 << 20;

    // Of a file that has little in use (8 MiB at most), any garbage is worth it (it was some megabytes before: a project that is only text grew by
    // its text at every save, for ever), and no garbage is not.
    assert!(!state(0, 0).is_worth_compacting());
    assert!(!state(10_000, 0).is_worth_compacting());
    assert!(state(10_000, 1).is_worth_compacting());
    assert!(
        state(35_000, 35_000).is_worth_compacting(),
        "a save of a project that is only text"
    );
    assert!(state(8 * megabyte, 1).is_worth_compacting(), "8 MiB is still little");
    assert!(!state(8 * megabyte, 0).is_worth_compacting());
    // Of a bigger file, it must be at least half of what is in use.
    assert!(!state(8 * megabyte + 1, 1).is_worth_compacting());
    assert!(
        !state(8 * megabyte + 2, 4 * megabyte).is_worth_compacting(),
        "half of it is 4 MiB and 1 byte"
    );
    assert!(state(8 * megabyte + 2, 4 * megabyte + 1).is_worth_compacting());
    assert!(!state(100 * megabyte, 20 * megabyte).is_worth_compacting());
    assert!(!state(100 * megabyte, 50 * megabyte - 1).is_worth_compacting());
    assert!(state(100 * megabyte, 50 * megabyte).is_worth_compacting());
    assert!(state(100 * megabyte, 500 * megabyte).is_worth_compacting());
}

// --- adding to an archive ---

fn a_file_named(folder: &Path, entry_name: &str, contents: &[u8]) -> FileToInclude {
    let path = folder.join(format!("source for {}", entry_name.replace('/', "_")));
    fs::write(&path, contents).unwrap();
    FileToInclude {
        entry_name: entry_name.to_owned(),
        source: path.into(),
    }
}

#[test]
fn adding_to_an_archive_keeps_the_files_that_are_named_leaves_the_others_as_garbage_and_replaces_the_text() {
    let folder = tempfile::tempdir().unwrap();
    let archive = folder.path().join("a.ffrs");
    let first = bytes_that_do_not_compress(40_000, 1);
    let second = bytes_that_do_not_compress(30_000, 2);
    let third = bytes_that_do_not_compress(20_000, 3);
    write_archive(
        &archive,
        "the first text",
        &[
            a_file_named(folder.path(), "resources/1-one.mp4", &first),
            a_file_named(folder.path(), "resources/2-two.mp4", &second),
        ],
    )
    .unwrap();
    let before = state_of_archive(&archive).unwrap();
    assert_eq!(before.bytes_of_garbage, 0);

    let after = append_to_archive(
        &archive,
        "the second text",
        &["resources/1-one.mp4".to_owned()],
        &[a_file_named(folder.path(), "resources/3-three.mp4", &third)],
        &mut |_| Ok(()),
    )
    .unwrap();

    assert_eq!(
        names_of_the_entries(&archive),
        vec![
            PROJECT_ENTRY_NAME.to_owned(),
            "resources/1-one.mp4".to_owned(),
            "resources/3-three.mp4".to_owned()
        ]
    );
    assert_eq!(read_project_text(&archive).unwrap(), "the second text");
    assert_eq!(
        after.entries["resources/1-one.mp4"], before.entries["resources/1-one.mp4"],
        "the file that stays is where it was: not written again, and not moved"
    );
    assert_eq!(
        after,
        state_of_archive(&archive).unwrap(),
        "what is told is what the archive says"
    );
    assert!(
        after.bytes_of_garbage >= second.len() as u64,
        "the file that was dropped is still in it, as garbage: {} bytes",
        after.bytes_of_garbage
    );
    let taken_out = folder.path().join("out");
    extract_entry(&archive, "resources/3-three.mp4", &taken_out).unwrap();
    assert_eq!(fs::read(&taken_out).unwrap(), third);
    extract_entry(&archive, "resources/1-one.mp4", &taken_out).unwrap();
    assert_eq!(fs::read(&taken_out).unwrap(), first);
}

#[test]
fn a_file_that_is_new_replaces_the_one_of_the_same_name_that_is_named_to_stay() {
    let folder = tempfile::tempdir().unwrap();
    let archive = folder.path().join("a.ffrs");
    write_archive(
        &archive,
        "text",
        &[a_file_named(folder.path(), "resources/1-one.mp4", b"the old bytes")],
    )
    .unwrap();

    append_to_archive(
        &archive,
        "text",
        &["resources/1-one.mp4".to_owned()],
        &[a_file_named(folder.path(), "resources/1-one.mp4", b"the new bytes")],
        &mut |_| Ok(()),
    )
    .unwrap();

    assert_eq!(
        names_of_the_entries(&archive),
        vec![PROJECT_ENTRY_NAME.to_owned(), "resources/1-one.mp4".to_owned()],
        "one file of that name"
    );
    let taken_out = folder.path().join("out");
    extract_entry(&archive, "resources/1-one.mp4", &taken_out).unwrap();
    assert_eq!(fs::read(&taken_out).unwrap(), b"the new bytes");
}

#[test]
fn files_that_are_not_named_to_stay_are_not_kept_even_if_nothing_new_is_added() {
    let folder = tempfile::tempdir().unwrap();
    let archive = folder.path().join("a.ffrs");
    write_archive(
        &archive,
        "text",
        &[a_file_named(folder.path(), "resources/1-one.mp4", b"some bytes")],
    )
    .unwrap();

    append_to_archive(&archive, "text again", &[], &[], &mut |_| Ok(())).unwrap();

    assert_eq!(names_of_the_entries(&archive), vec![PROJECT_ENTRY_NAME.to_owned()]);
    assert_eq!(read_project_text(&archive).unwrap(), "text again");
}

#[test]
fn a_save_that_is_stopped_leaves_the_archive_as_it_was_byte_for_byte() {
    let folder = tempfile::tempdir().unwrap();
    let archive = folder.path().join("a.ffrs");
    write_archive(
        &archive,
        "text",
        &[a_file_named(
            folder.path(),
            "resources/1-one.mp4",
            &bytes_that_do_not_compress(5_000, 1),
        )],
    )
    .unwrap();
    let bytes_before = fs::read(&archive).unwrap();

    let stopped = append_to_archive(
        &archive,
        "text that is never kept",
        &["resources/1-one.mp4".to_owned()],
        &[a_file_named(
            folder.path(),
            "resources/2-two.mp4",
            &bytes_that_do_not_compress(3 * COPY_BUFFER_BYTES, 2),
        )],
        &mut |copied| {
            if copied > COPY_BUFFER_BYTES as u64 {
                Err(io::Error::new(io::ErrorKind::Interrupted, "stopped"))
            } else {
                Ok(())
            }
        },
    );

    assert!(matches!(stopped, Err(ArchiveError::Stopped)), "{stopped:?}");
    assert_eq!(fs::read(&archive).unwrap(), bytes_before);
}

#[test]
fn the_old_text_of_the_project_is_never_kept_even_if_it_is_named_to_stay() {
    let folder = tempfile::tempdir().unwrap();
    let archive = folder.path().join("a.ffrs");
    write_archive(&archive, "the first text", &[]).unwrap();

    append_to_archive(
        &archive,
        "the second text",
        &[PROJECT_ENTRY_NAME.to_owned()],
        &[],
        &mut |_| Ok(()),
    )
    .unwrap();

    assert_eq!(names_of_the_entries(&archive), vec![PROJECT_ENTRY_NAME.to_owned()]);
    assert_eq!(read_project_text(&archive).unwrap(), "the second text");
}

#[test]
fn what_a_save_that_was_cut_off_left_after_the_last_whole_end_record_is_cut_off_before_the_next_is_added() {
    let folder = tempfile::tempdir().unwrap();
    let archive = folder.path().join("a.ffrs");
    let untouched = folder.path().join("b.ffrs");
    let file = a_file_named(
        folder.path(),
        "resources/1-one.mp4",
        &bytes_that_do_not_compress(20_000, 1),
    );
    write_archive(&archive, "text", std::slice::from_ref(&file)).unwrap();
    fs::copy(&archive, &untouched).unwrap();
    // A save that was cut off: it began a new entry, and the power went.
    let mut cut_off = fs::OpenOptions::new().append(true).open(&archive).unwrap();
    cut_off.write_all(b"PK\x03\x04").unwrap();
    cut_off.write_all(&bytes_that_do_not_compress(10_000, 2)).unwrap();
    drop(cut_off);
    assert!(fs::metadata(&archive).unwrap().len() > fs::metadata(&untouched).unwrap().len() + 10_000);

    for path in [&archive, &untouched] {
        append_to_archive(
            path,
            "newer text",
            &["resources/1-one.mp4".to_owned()],
            &[],
            &mut |_| Ok(()),
        )
        .unwrap();
    }

    assert_eq!(
        fs::read(&archive).unwrap(),
        fs::read(&untouched).unwrap(),
        "what was left was cut off, and the file is as if it had never been there"
    );
    assert_eq!(read_project_text(&archive).unwrap(), "newer text");
}

#[test]
fn writing_an_archive_again_stores_a_file_that_another_zip_program_compressed_and_keeps_all_of_it() {
    let folder = tempfile::tempdir().unwrap();
    let archive = folder.path().join("recompressed.ffrs");
    let video = bytes_that_do_not_compress(60_000, 9)
        .into_iter()
        .map(|byte| byte & 0x0f) // (something that a compressor does shrink)
        .collect::<Vec<u8>>();
    {
        let mut writer = ZipWriter::new(File::create(&archive).unwrap());
        let compressed = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer.start_file(PROJECT_ENTRY_NAME, compressed).unwrap();
        writer.write_all(b"{\"a project\": 1}").unwrap();
        writer.start_file("resources/1-v.mp4", compressed).unwrap();
        writer.write_all(&video).unwrap();
        writer.finish().unwrap();
    }
    assert!(
        !state_of_archive(&archive).unwrap().entries["resources/1-v.mp4"].is_stored,
        "the setup: it is compressed"
    );

    let state = compact_archive(&archive, &mut |_| Ok(())).unwrap();

    let place = state.entries["resources/1-v.mp4"];
    assert!(
        place.is_stored,
        "it is stored as it is now, so it can be read where it is"
    );
    assert_eq!(place.length, video.len() as u64);
    assert_eq!(place.crc32, crc32_of(&video));
    let taken_out = folder.path().join("out");
    extract_entry(&archive, "resources/1-v.mp4", &taken_out).unwrap();
    assert_eq!(fs::read(&taken_out).unwrap(), video);
    assert_eq!(read_project_text(&archive).unwrap(), "{\"a project\": 1}");
    assert_eq!(state.bytes_of_garbage, 0);
}
