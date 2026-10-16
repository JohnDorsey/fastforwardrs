// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of project files that have files included in them: saved as an archive, read back, and what is refused.

use std::collections::HashMap;
use std::fs;

use super::archive::{self, PROJECT_ENTRY_NAME, is_a_resource_entry_name, resource_entry_name};
use super::tests::{assert_same_content, edited_project, link_to_missing_video, project_with_everything, saved};
use super::*;
use crate::project::{SourceMediaIdentifier, TimelineFrameIndex};

fn identifier(number: u32) -> SourceMediaIdentifier {
    SourceMediaIdentifier::new(number)
}

/// A link of a video that was at `/videos/holiday.mp4` and is included as the entry `resources/1-holiday.mp4`.
fn link_to_an_included_video() -> SourceMediaLink {
    SourceMediaLink {
        included_as: Some(resource_entry_name(identifier(1), "holiday.mp4")),
        ..link_to_missing_video()
    }
}

/// Where the bytes of the video are, in a folder of its own: what a program that saves the project gives to read the file from.
struct FilesToInclude {
    folder: tempfile::TempDir,
    paths: IncludedFiles,
}

fn the_video_to_include(contents: &[u8]) -> FilesToInclude {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("a copy of the video.mp4");
    fs::write(&path, contents).unwrap();
    let paths = HashMap::from([(identifier(1), path.into())]);
    FilesToInclude { folder, paths }
}

fn names_of_the_entries(archive: &Path) -> Vec<String> {
    let zip = zip::ZipArchive::new(fs::File::open(archive).unwrap()).unwrap();
    let mut names: Vec<String> = zip.file_names().map(|name| name.unwrap().into_owned()).collect();
    names.sort();
    names
}

/// Says that the source file `number` of `project` is included in the project file as the entry `entry_name`.
fn include_in_the_project_as(project: &mut Project, number: u32, entry_name: &str) {
    let mut link = project.source(identifier(number)).unwrap().link.clone();
    link.included_as = Some(entry_name.to_owned());
    assert!(project.set_source_link(identifier(number), link));
}

/// Writes a zip file at `path` that has the entries `entries` (the names as they are, whatever they are, and the bytes of each), as
/// a program that is not this one could write it.
fn an_archive_of_entries_as_they_are_named(path: &Path, entries: &[(&str, &[u8])]) {
    use std::io::Write;
    let mut writer = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap();
}

fn the_included_file_of(project: &Project) -> SourceMediaLink {
    project.sources()[0].link.clone()
}

// --- what is written is what is read ---

#[test]
fn a_project_with_a_file_included_is_saved_as_an_archive_and_read_back_as_it_was() {
    let project = edited_project(link_to_an_included_video());
    let files = the_video_to_include(b"the bytes of the video");
    let destination = files.folder.path().join("holiday.ffrs");

    write_project_file_including(&project, &destination, &files.paths, HowToWrite::Anew).unwrap();

    assert!(archive::is_an_archive(&destination));
    assert_eq!(
        names_of_the_entries(&destination),
        vec![PROJECT_ENTRY_NAME.to_owned(), "resources/1-holiday.mp4".to_owned()]
    );
    let loaded = read_project_file(&destination).unwrap();
    assert_same_content(&loaded, &project);
    assert_eq!(the_included_file_of(&loaded), the_included_file_of(&project));
    let taken_out = files.folder.path().join("taken out.mp4");
    archive::extract_entry(&destination, "resources/1-holiday.mp4", &taken_out).unwrap();
    assert_eq!(fs::read(&taken_out).unwrap(), b"the bytes of the video");
}

#[test]
fn the_project_in_an_archive_is_the_text_a_plain_project_file_of_the_same_folder_would_have() {
    let project = edited_project(link_to_an_included_video());
    let files = the_video_to_include(b"x");
    let destination = files.folder.path().join("holiday.ffrs");
    write_project_file_including(&project, &destination, &files.paths, HowToWrite::Anew).unwrap();

    let text = archive::read_project_text(&destination).unwrap();

    let folder = std::path::absolute(files.folder.path()).unwrap();
    assert_eq!(text, project_to_json_text(&project, &folder).unwrap());
    let document: serde_json::Value = serde_json::from_str(&text).unwrap();
    // (Rewritten on purpose in phase 3: version 12 has the colours of the source files.)
    assert_eq!(document["file_format_version"], 12);
    let source = &document["sources"][0];
    assert_eq!(source["included_as"], "resources/1-holiday.mp4");
    assert_eq!(source["absolute_path"], "/videos/holiday.mp4", "where it was, kept");
    assert_eq!(source["relative_path"], "../videos/holiday.mp4");
}

#[test]
fn a_file_that_is_not_included_has_no_mention_of_it_in_the_project() {
    let text = saved(&edited_project(link_to_missing_video()));

    assert!(!text.contains("included_as"), "{text}");
}

#[test]
fn a_project_with_nothing_included_can_be_an_archive_too_and_is_read_back_as_it_was() {
    let project = project_with_everything();
    let folder = tempfile::tempdir().unwrap();
    let destination = folder.path().join("plain.ffrs");

    write_project_file(&project, &destination).unwrap();

    assert!(archive::is_an_archive(&destination));
    assert_eq!(names_of_the_entries(&destination), vec![PROJECT_ENTRY_NAME.to_owned()]);
    assert_same_content(&read_project_file(&destination).unwrap(), &project);
}

#[test]
fn what_is_written_to_a_name_that_does_not_end_in_ffrs_is_plain_json_as_before() {
    let project = edited_project(link_to_missing_video());
    let folder = tempfile::tempdir().unwrap();
    for name in [
        "holiday.fastforwardrs.json",
        "holiday.json",
        "holiday",
        "holiday.ffrs.json",
        "holiday.ffrs.bak",
    ] {
        let destination = folder.path().join(name);

        write_project_file(&project, &destination).unwrap();

        assert!(!archive::is_an_archive(&destination), "{name}");
        assert!(fs::read_to_string(&destination).unwrap().starts_with('{'), "{name}");
        assert_same_content(&read_project_file(&destination).unwrap(), &project);
    }
}

#[test]
fn the_ending_ffrs_is_known_whatever_the_case_of_the_letters() {
    let project = edited_project(link_to_missing_video());
    let folder = tempfile::tempdir().unwrap();
    for name in ["a.FFRS", "b.Ffrs", "c.ffrs"] {
        let destination = folder.path().join(name);
        write_project_file(&project, &destination).unwrap();
        assert!(archive::is_an_archive(&destination), "{name}");
    }
    assert!(is_the_name_of_an_archive(Path::new("x/y.FFRS")));
    assert!(!is_the_name_of_an_archive(Path::new("x/y.json")));
    assert!(!is_the_name_of_an_archive(Path::new("x/ffrs")));
    assert!(!is_the_name_of_an_archive(Path::new("x/y.ffrs.json")));
}

#[test]
fn an_archive_is_read_whatever_its_name_is_because_it_is_known_by_what_is_in_it() {
    let project = edited_project(link_to_missing_video());
    let folder = tempfile::tempdir().unwrap();
    let archive_path = folder.path().join("holiday.ffrs");
    write_project_file(&project, &archive_path).unwrap();
    let renamed = folder.path().join("holiday.fastforwardrs.json");
    fs::rename(&archive_path, &renamed).unwrap();

    assert_same_content(&read_project_file(&renamed).unwrap(), &project);
}

#[test]
fn the_places_a_file_was_at_stay_as_they_were_when_the_project_is_saved_in_another_folder_but_those_of_other_files_are_worked_out_again()
 {
    let folder = tempfile::tempdir().unwrap();
    let on_disk = folder.path().join("pictures").join("logo.png");
    fs::create_dir_all(on_disk.parent().unwrap()).unwrap();
    fs::write(&on_disk, "png").unwrap();
    let mut project = edited_project(link_to_an_included_video());
    project
        .import_source(
            SourceMedia::image(
                identifier(2),
                SourceMediaLink::from_absolute_path(std::path::absolute(&on_disk).unwrap()),
                SourceImageProperties {
                    display_width_in_pixels: 4,
                    display_height_in_pixels: 4,
                },
            ),
            TimelineFrameIndex::new(0),
        )
        .unwrap();
    let files = the_video_to_include(b"v");
    let somewhere_else = folder.path().join("a").join("b");
    fs::create_dir_all(&somewhere_else).unwrap();
    let destination = somewhere_else.join("holiday.ffrs");

    write_project_file_including(&project, &destination, &files.paths, HowToWrite::Anew).unwrap();

    let loaded = read_project_file(&destination).unwrap();
    let (video, picture) = (&loaded.sources()[0].link, &loaded.sources()[1].link);
    assert_eq!(
        video.relative_path,
        Some(PathBuf::from("../videos/holiday.mp4")),
        "kept as it was"
    );
    assert_eq!(video.absolute_path, PathBuf::from("/videos/holiday.mp4"));
    assert_eq!(
        picture.relative_path,
        Some(PathBuf::from("../../pictures/logo.png")),
        "this one is where it is, so it is worked out from where the project is"
    );
}

#[test]
fn the_places_of_an_included_file_stay_as_they_were_even_when_the_file_is_still_where_it_was_and_could_be_worked_out_again()
 {
    let folder = tempfile::tempdir().unwrap();
    let original = std::path::absolute(folder.path().join("original").join("holiday.mp4")).unwrap();
    fs::create_dir_all(original.parent().unwrap()).unwrap();
    fs::write(&original, "the video").unwrap();
    let project = edited_project(SourceMediaLink {
        absolute_path: original.clone(),
        // (Where the file was from the folder that the project file was in when the file was included.)
        relative_path: Some(PathBuf::from("footage/holiday.mp4")),
        included_as: Some(resource_entry_name(identifier(1), "holiday.mp4")),
    });
    let files = the_video_to_include(b"v");
    let somewhere_else = folder.path().join("a").join("b");
    fs::create_dir_all(&somewhere_else).unwrap();
    let destination = somewhere_else.join("holiday.ffrs");

    write_project_file_including(&project, &destination, &files.paths, HowToWrite::Anew).unwrap();

    let link = read_project_file(&destination).unwrap().sources()[0].link.clone();
    assert_eq!(
        link.relative_path,
        Some(PathBuf::from("footage/holiday.mp4")),
        "as it was when the file was included, and not worked out from where the project is now (which would be ../../original/holiday.mp4)"
    );
    assert_eq!(link.absolute_path, original);
}

#[test]
fn every_kind_of_source_can_be_included_at_once_and_each_has_its_own_entry() {
    let mut project = project_with_everything();
    // The video, the sound and (added here) a picture, each included.
    for (number, name) in [(1, "holiday.mp4"), (2, "song.mp3")] {
        include_in_the_project_as(&mut project, number, &resource_entry_name(identifier(number), name));
    }
    let folder = tempfile::tempdir().unwrap();
    let mut paths = IncludedFiles::new();
    for (number, name, bytes) in [
        (1, "holiday.mp4", b"video".as_slice()),
        (2, "song.mp3", b"sound".as_slice()),
    ] {
        let path = folder.path().join(name);
        fs::write(&path, bytes).unwrap();
        paths.insert(identifier(number), path.into());
    }
    let destination = folder.path().join("all.ffrs");

    write_project_file_including(&project, &destination, &paths, HowToWrite::Anew).unwrap();

    assert_eq!(
        names_of_the_entries(&destination),
        vec![
            PROJECT_ENTRY_NAME.to_owned(),
            "resources/1-holiday.mp4".to_owned(),
            "resources/2-song.mp3".to_owned(),
        ]
    );
    let loaded = read_project_file(&destination).unwrap();
    assert_eq!(
        loaded
            .sources()
            .iter()
            .filter(|source| source.link.is_included())
            .count(),
        2
    );
    assert_same_content(&loaded, &project);
}

// --- what is refused ---

#[test]
fn a_plain_project_file_cannot_have_files_included_and_nothing_is_written() {
    let project = edited_project(link_to_an_included_video());
    let files = the_video_to_include(b"x");
    let destination = files.folder.path().join("holiday.fastforwardrs.json");

    let error = write_project_file_including(&project, &destination, &files.paths, HowToWrite::Anew).unwrap_err();

    assert!(
        matches!(error, ProjectFileError::IncludedFilesNeedAnArchive { .. }),
        "{error:?}"
    );
    assert!(error.to_string().contains(".ffrs"), "{error}");
    assert!(!destination.exists());
}

#[test]
fn an_included_file_that_there_is_nowhere_to_read_from_is_refused_and_the_old_project_file_is_left_alone() {
    let project = edited_project(link_to_an_included_video());
    let folder = tempfile::tempdir().unwrap();
    let destination = folder.path().join("holiday.ffrs");
    write_project_file(&edited_project(link_to_missing_video()), &destination).unwrap();
    let before = fs::read(&destination).unwrap();

    let error = write_project_file(&project, &destination).unwrap_err();

    assert!(
        matches!(&error, ProjectFileError::IncludedFileNotAvailable { file_name } if file_name == "holiday.mp4"),
        "{error:?}"
    );
    assert_eq!(fs::read(&destination).unwrap(), before);
}

#[test]
fn an_included_file_that_cannot_be_read_stops_the_save_and_leaves_the_old_project_file_alone() {
    let project = edited_project(link_to_an_included_video());
    let folder = tempfile::tempdir().unwrap();
    let destination = folder.path().join("holiday.ffrs");
    write_project_file(&edited_project(link_to_missing_video()), &destination).unwrap();
    let before = fs::read(&destination).unwrap();
    let paths = HashMap::from([(identifier(1), folder.path().join("gone.mp4").into())]);

    let error = write_project_file_including(&project, &destination, &paths, HowToWrite::Anew).unwrap_err();

    assert!(matches!(error, ProjectFileError::CouldNotRead { .. }), "{error:?}");
    assert_eq!(fs::read(&destination).unwrap(), before);
}

#[test]
fn a_plain_project_file_that_says_a_file_is_included_is_refused_when_it_is_read() {
    let project = edited_project(link_to_an_included_video());
    let text = project_to_json_text(&project, Path::new("/projects")).unwrap();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("holiday.fastforwardrs.json");
    fs::write(&path, text).unwrap();

    let error = read_project_file(&path).unwrap_err();

    assert!(matches!(error, ProjectFileError::InvalidProject { .. }), "{error:?}");
    assert!(error.to_string().contains(".ffrs"), "{error}");
}

#[test]
fn an_archive_that_names_an_entry_it_has_not_got_is_refused_by_what_it_says() {
    let project = edited_project(link_to_an_included_video());
    let text = project_to_json_text(&project, Path::new("/projects")).unwrap();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("holiday.ffrs");
    // An archive that has the project, but not the file that it says is included.
    archive::write_archive(&path, &text, &[]).unwrap();

    let error = read_project_file(&path).unwrap_err();

    assert!(matches!(error, ProjectFileError::InvalidProject { .. }), "{error:?}");
    assert!(error.to_string().contains("resources/1-holiday.mp4"), "{error}");
}

#[test]
fn an_archive_that_names_an_entry_in_a_way_that_could_lead_out_of_the_cache_is_refused() {
    for bad_name in [
        "../../x.mp4",
        "resources/../x.mp4",
        "resources/1-../../x.mp4",
        "/etc/passwd",
        "resources/1-a\\b.mp4",
        "",
    ] {
        let mut project = edited_project(link_to_missing_video());
        include_in_the_project_as(&mut project, 1, bad_name);
        let text = project_to_json_text(&project, Path::new("/projects")).unwrap();
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("holiday.ffrs");
        // The archive has an entry of the name that the project says (as one made by hand can have): so that it is the name that is
        // refused, and not an entry that is not there.
        let mut entries = vec![(PROJECT_ENTRY_NAME, text.as_bytes())];
        if !bad_name.is_empty() {
            entries.push((bad_name, b"not a video"));
        }
        an_archive_of_entries_as_they_are_named(&path, &entries);

        let error = read_project_file(&path).unwrap_err();

        assert!(
            matches!(error, ProjectFileError::InvalidProject { .. }),
            "{bad_name:?}: {error:?}"
        );
        assert!(
            error
                .to_string()
                .contains("which is not a name that a file included in it has"),
            "{bad_name:?}: {error}"
        );
        assert!(!is_a_resource_entry_name(bad_name));
    }
}

#[test]
fn two_files_that_are_included_as_one_entry_are_refused_when_saving() {
    let mut project = project_with_everything();
    include_in_the_project_as(&mut project, 1, "resources/1-holiday.mp4");
    include_in_the_project_as(&mut project, 2, "resources/1-holiday.mp4");
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("x");
    fs::write(&path, "x").unwrap();
    let paths = HashMap::from([(identifier(1), path.clone().into()), (identifier(2), path.into())]);

    let error =
        write_project_file_including(&project, &folder.path().join("two.ffrs"), &paths, HowToWrite::Anew).unwrap_err();

    assert!(matches!(error, ProjectFileError::InvalidProject { .. }), "{error:?}");
}

#[test]
fn a_file_that_is_neither_an_archive_nor_a_project_says_so() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("notes.ffrs");
    fs::write(&path, "just some words").unwrap();

    let error = read_project_file(&path).unwrap_err();

    assert!(matches!(error, ProjectFileError::NotAProjectFile { .. }), "{error:?}");
}

#[test]
fn an_archive_whose_project_is_of_a_newer_version_says_so_as_a_plain_file_does() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("future.ffrs");
    let text = saved(&edited_project(link_to_missing_video())).replace(
        &super::tests::current_version_line(),
        &super::tests::version_line(FILE_FORMAT_VERSION + 1),
    );
    archive::write_archive(&path, &text, &[]).unwrap();

    let error = read_project_file(&path).unwrap_err();

    assert!(matches!(error, ProjectFileError::NewerFileFormat { .. }), "{error:?}");
}

#[test]
fn a_project_file_of_version_eight_is_read_still_and_has_no_files_included() {
    let eight = saved(&edited_project(link_to_missing_video()))
        .replace(&super::tests::current_version_line(), &super::tests::version_line(8));

    let project = project_from_json_text(&eight).unwrap();

    assert!(!project.sources()[0].link.is_included());
}
