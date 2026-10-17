// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The resources of a project (the files its effects use) in project files, on a real disk: linked and included, saved and read back, missing, too big, damaged, and what
//! is refused.

use std::sync::Arc;

use serde_json::{Value, json};
use tempfile::TempDir;

use super::archive::{self, project_resource_entry_name};
use super::tests::{edited_project, link_to_missing_video};
use super::*;
use crate::project::{MOST_BYTES_OF_A_RESOURCE, ResourceKind, ResourceProblem, TimelineFrameIndex};
use crate::test_support::a_kind_of_effect_that_uses_a_resource;

/// A project (see `edited_project`) whose first clip has an effect that uses the resource `table_name` (which the project may or may not have).
fn project_using(table_name: &str) -> Project {
    let mut project = edited_project(link_to_missing_video());
    let clip = AnimationTarget::Clip(project.tracks()[0].entries()[0].id());
    let kind = a_kind_of_effect_that_uses_a_resource();
    let instance = project.add_effect(clip, kind).unwrap();
    project
        .set_property_at(
            clip,
            crate::project::PropertyId::effect_value(kind, instance, 0),
            PropertyValue::Words(table_name.to_owned()),
            TimelineFrameIndex::new(0),
        )
        .unwrap();
    project
}

fn bytes(text: &str) -> Arc<[u8]> {
    Arc::from(text.as_bytes())
}

fn json_of(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

/// A resource linked to a file `name` that is in `folder`, with `contents`: the file is written, and the project has the resource.
fn link_a_table(project: &mut Project, folder: &Path, name: &str, contents: &str) -> PathBuf {
    let path = folder.join(name);
    fs::write(&path, contents).unwrap();
    project
        .add_resource(
            name,
            ResourceKind::Cube,
            bytes(contents),
            SourceMediaLink::from_absolute_path(path.clone()),
        )
        .unwrap();
    path
}

/// A resource that is included, as the program includes it: its entry is made of the CRC-32 of its bytes and its name.
fn include_a_table(project: &mut Project, name: &str, contents: &str) -> String {
    let entry = project_resource_entry_name(crc32fast::hash(contents.as_bytes()), name);
    project
        .add_resource(
            name,
            ResourceKind::Cube,
            bytes(contents),
            SourceMediaLink {
                absolute_path: PathBuf::from("/where/it/was").join(name),
                relative_path: Some(PathBuf::from("../where/it/was").join(name)),
                included_as: Some(entry.clone()),
            },
        )
        .unwrap();
    entry
}

fn bytes_of(project: &Project, name: &str) -> Option<Vec<u8>> {
    project
        .resources()
        .get(name)
        .and_then(|resource| resource.bytes())
        .map(|bytes| bytes.to_vec())
}

// --- linked ---

#[test]
fn a_linked_resource_is_saved_with_its_paths_and_its_size_and_read_back_from_the_file_it_is_linked_to() {
    let folder = TempDir::new().unwrap();
    let mut project = project_using("grade.cube");
    let table = link_a_table(&mut project, folder.path(), "grade.cube", "LUT_3D_SIZE 2\n");
    let destination = folder.path().join("project.json");

    write_project_file(&project, &destination).unwrap();

    let file = json_of(&destination);
    assert_eq!(
        file["resources"],
        json!([{
            "name": "grade.cube",
            "kind": "cube",
            "size": 14,
            "crc32": crc32fast::hash(b"LUT_3D_SIZE 2\n"),
            "absolute_path": table.to_str().unwrap(),
            "relative_path": "grade.cube"
        }])
    );
    let loaded = read_project_file(&destination).unwrap();
    assert_eq!(bytes_of(&loaded, "grade.cube"), Some(b"LUT_3D_SIZE 2\n".to_vec()));
    let resource = loaded.resources().get("grade.cube").unwrap();
    assert_eq!(resource.kind(), ResourceKind::Cube);
    assert!(!resource.is_included());
    assert_eq!(resource.link().relative_path, Some(PathBuf::from("grade.cube")));
}

#[test]
fn a_linked_file_that_changed_since_is_read_as_it_is_now_and_one_that_moved_is_found_by_its_relative_path() {
    let folder = TempDir::new().unwrap();
    let mut project = project_using("grade.cube");
    let table = link_a_table(&mut project, folder.path(), "grade.cube", "the first table");
    let destination = folder.path().join("project.json");
    write_project_file(&project, &destination).unwrap();

    fs::write(&table, "the second table").unwrap();
    let changed = read_project_file(&destination).unwrap();
    assert_eq!(bytes_of(&changed, "grade.cube"), Some(b"the second table".to_vec()));

    // The folder is moved: the absolute path is gone, the relative path still leads to the file.
    let moved = TempDir::new().unwrap();
    fs::rename(&table, moved.path().join("grade.cube")).unwrap();
    fs::rename(&destination, moved.path().join("project.json")).unwrap();
    let found = read_project_file(&moved.path().join("project.json")).unwrap();
    assert_eq!(bytes_of(&found, "grade.cube"), Some(b"the second table".to_vec()));
}

#[test]
fn a_linked_file_that_is_gone_leaves_the_resource_missing_and_saving_again_keeps_its_places() {
    let folder = TempDir::new().unwrap();
    let mut project = project_using("grade.cube");
    let table = link_a_table(&mut project, folder.path(), "grade.cube", "table");
    let destination = folder.path().join("project.json");
    write_project_file(&project, &destination).unwrap();
    fs::remove_file(&table).unwrap();

    let loaded = read_project_file(&destination).expect("a project whose table is gone is still opened");

    let resource = loaded.resources().get("grade.cube").unwrap();
    assert!(resource.is_missing());
    assert_eq!(resource.link().absolute_path, table, "its places are kept");
    // Saved again (somewhere else), it says what it said, and not a size or a checksum that nobody has.
    let again = folder.path().join("again.json");
    write_project_file(&loaded, &again).unwrap();
    let entry = &json_of(&again)["resources"][0];
    assert_eq!(entry["name"], "grade.cube");
    assert_eq!(entry["absolute_path"], table.to_str().unwrap());
    assert!(entry.get("size").is_none() && entry.get("crc32").is_none(), "{entry}");
}

#[test]
fn a_linked_file_of_more_than_the_most_is_left_missing_and_one_of_exactly_the_most_is_read() {
    let folder = TempDir::new().unwrap();
    let mut project = project_using("big.cube");
    let path = folder.path().join("big.cube");
    fs::write(&path, vec![b'x'; MOST_BYTES_OF_A_RESOURCE]).unwrap();
    project
        .add_resource(
            "big.cube",
            ResourceKind::Cube,
            bytes("stand-in"),
            SourceMediaLink::from_absolute_path(path.clone()),
        )
        .unwrap();
    let destination = folder.path().join("project.json");
    write_project_file(&project, &destination).unwrap();

    let loaded = read_project_file(&destination).unwrap();
    assert_eq!(
        loaded
            .resources()
            .get("big.cube")
            .unwrap()
            .bytes()
            .map(|bytes| bytes.len()),
        Some(MOST_BYTES_OF_A_RESOURCE)
    );

    fs::write(&path, vec![b'x'; MOST_BYTES_OF_A_RESOURCE + 1]).unwrap();
    let loaded = read_project_file(&destination).unwrap();
    assert!(loaded.resources().get("big.cube").unwrap().is_missing());
}

#[test]
fn a_resource_that_no_effect_uses_is_not_saved_and_a_project_with_none_has_no_list_at_all() {
    let folder = TempDir::new().unwrap();
    let mut project = project_using("used.cube");
    link_a_table(&mut project, folder.path(), "used.cube", "used");
    link_a_table(&mut project, folder.path(), "unused.cube", "unused");
    let destination = folder.path().join("project.json");

    write_project_file(&project, &destination).unwrap();

    let names: Vec<String> = json_of(&destination)["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|resource| resource["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["used.cube"]);
    assert_eq!(
        project.resources().len(),
        2,
        "(the project keeps both, for as long as it is open)"
    );

    let plain = edited_project(link_to_missing_video());
    let other = folder.path().join("plain.json");
    write_project_file(&plain, &other).unwrap();
    assert!(json_of(&other).get("resources").is_none());
}

// --- included ---

#[test]
fn an_included_resource_is_in_the_archive_by_the_checksum_and_the_name_and_read_back_from_it() {
    let folder = TempDir::new().unwrap();
    let mut project = project_using("grade.cube");
    let entry = include_a_table(&mut project, "grade.cube", "LUT_3D_SIZE 2\n");
    assert_eq!(
        entry,
        format!("project-resources/{}-grade.cube", crc32fast::hash(b"LUT_3D_SIZE 2\n"))
    );
    let destination = folder.path().join("project.ffrs");

    let written =
        write_project_file_including(&project, &destination, &IncludedFiles::new(), HowToWrite::Anew).unwrap();

    let state = written.archive.expect("it is an archive");
    assert!(state.entries.contains_key(&entry), "{:?}", state.entries.keys());
    let file: Value = serde_json::from_str(&archive::read_project_text(&destination).unwrap()).unwrap();
    assert_eq!(file["resources"][0]["included_as"], entry.as_str());
    let where_it_was = PathBuf::from("/where/it/was").join("grade.cube");
    assert_eq!(file["resources"][0]["absolute_path"], where_it_was.to_str().unwrap());

    let loaded = read_project_file(&destination).unwrap();
    assert_eq!(bytes_of(&loaded, "grade.cube"), Some(b"LUT_3D_SIZE 2\n".to_vec()));
    let resource = loaded.resources().get("grade.cube").unwrap();
    assert!(resource.is_included());
    assert_eq!(resource.link().absolute_path, where_it_was);
}

#[test]
fn saving_an_archive_again_does_not_write_the_included_resource_again_and_a_changed_one_is_a_new_entry() {
    let folder = TempDir::new().unwrap();
    let mut project = project_using("grade.cube");
    let first_entry = include_a_table(&mut project, "grade.cube", "the first table");
    let destination = folder.path().join("project.ffrs");
    let first = write_project_file_including(
        &project,
        &destination,
        &IncludedFiles::new(),
        HowToWrite::AddingToWhatIsThere,
    )
    .unwrap()
    .archive
    .unwrap();
    let place_before = first.entries[&first_entry];

    // Saved again with nothing changed: the entry is where it was, and the archive has no garbage of a table.
    let again = write_project_file_including(
        &project,
        &destination,
        &IncludedFiles::new(),
        HowToWrite::AddingToWhatIsThere,
    )
    .unwrap()
    .archive
    .unwrap();
    assert_eq!(again.entries[&first_entry], place_before);

    // A table by the same name with other bytes: another entry (another checksum). The old one is no longer in the project, and is garbage.
    let mut changed = project_using("grade.cube");
    let second_entry = include_a_table(&mut changed, "grade.cube", "the second table");
    assert_ne!(first_entry, second_entry);
    let after = write_project_file_including(
        &changed,
        &destination,
        &IncludedFiles::new(),
        HowToWrite::AddingToWhatIsThere,
    )
    .unwrap()
    .archive
    .unwrap();
    assert!(after.entries.contains_key(&second_entry));
    assert!(!after.entries.contains_key(&first_entry), "(it is no longer listed)");
    assert!(after.bytes_of_garbage > 0);
    let loaded = read_project_file(&destination).unwrap();
    assert_eq!(bytes_of(&loaded, "grade.cube"), Some(b"the second table".to_vec()));
}

#[test]
fn two_resources_included_and_linked_together_in_one_archive_are_each_read_back_by_the_way_they_are_kept() {
    let folder = TempDir::new().unwrap();
    let mut project = project_using("embedded.cube");
    include_a_table(&mut project, "embedded.cube", "embedded table");
    link_a_table(&mut project, folder.path(), "linked.cube", "linked table");
    // (A second effect uses the linked one.)
    let clip = AnimationTarget::Clip(project.tracks()[0].entries()[0].id());
    let kind = a_kind_of_effect_that_uses_a_resource();
    let instance = project.add_effect(clip, kind).unwrap();
    project
        .set_property_at(
            clip,
            crate::project::PropertyId::effect_value(kind, instance, 0),
            PropertyValue::Words("linked.cube".to_owned()),
            TimelineFrameIndex::new(0),
        )
        .unwrap();
    let destination = folder.path().join("project.ffrs");
    write_project_file_including(&project, &destination, &IncludedFiles::new(), HowToWrite::Anew).unwrap();

    let loaded = read_project_file(&destination).unwrap();

    assert_eq!(bytes_of(&loaded, "embedded.cube"), Some(b"embedded table".to_vec()));
    assert_eq!(bytes_of(&loaded, "linked.cube"), Some(b"linked table".to_vec()));
    assert!(loaded.resources().get("embedded.cube").unwrap().is_included());
    assert!(!loaded.resources().get("linked.cube").unwrap().is_included());
}

#[test]
fn a_plain_json_file_cannot_have_a_resource_included_and_says_so_when_it_is_written_and_when_it_is_read() {
    let folder = TempDir::new().unwrap();
    let mut project = project_using("grade.cube");
    include_a_table(&mut project, "grade.cube", "table");
    let destination = folder.path().join("project.json");

    assert!(matches!(
        write_project_file(&project, &destination),
        Err(ProjectFileError::IncludedFilesNeedAnArchive { .. })
    ));
    assert!(!destination.exists(), "nothing is written");

    // The text of a project that says a resource is included, given as plain JSON.
    let text = project_to_json_text(&project, folder.path()).unwrap();
    let reason = match project_from_json_text(&text) {
        Err(ProjectFileError::InvalidProject { reason }) => reason,
        other => panic!("{other:?}"),
    };
    assert!(reason.contains("grade.cube") && reason.contains(".ffrs"), "{reason}");
}

#[test]
fn an_included_resource_that_has_no_bytes_cannot_be_written() {
    let folder = TempDir::new().unwrap();
    let mut project = project_using("grade.cube");
    let mut resources = crate::project::Resources::default();
    resources
        .add_as_it_was_saved(
            "grade.cube",
            ResourceKind::Cube,
            None,
            SourceMediaLink {
                included_as: Some("project-resources/1-grade.cube".to_owned()),
                ..SourceMediaLink::from_absolute_path(PathBuf::from("/x/grade.cube"))
            },
        )
        .unwrap();
    project = project.with_resources(resources);

    let result = write_project_file_including(
        &project,
        &folder.path().join("project.ffrs"),
        &IncludedFiles::new(),
        HowToWrite::Anew,
    );

    assert!(matches!(
        result,
        Err(ProjectFileError::IncludedFileNotAvailable { file_name }) if file_name == "grade.cube"
    ));
}

// --- what a file may say ---

/// An archive with the text of a project (edited by `change`) and no files, at `folder`/`project.ffrs`.
fn archive_with_the_resources(folder: &Path, resources: Value, files: &[(&str, &[u8])]) -> PathBuf {
    let mut file: Value =
        serde_json::from_str(&project_to_json_text(&project_using("grade.cube"), folder).unwrap()).unwrap();
    file["resources"] = resources;
    let text = serde_json::to_string_pretty(&file).unwrap();
    let destination = folder.join("project.ffrs");
    let to_include: Vec<_> = files
        .iter()
        .map(|(entry_name, contents)| archive::FileToInclude {
            entry_name: (*entry_name).to_owned(),
            source: archive::FileSource::Bytes(Arc::from(*contents)),
        })
        .collect();
    archive::write_archive(&destination, &text, &to_include).unwrap();
    destination
}

fn reason_for_unreadable(path: &Path) -> String {
    match read_project_file(path) {
        Err(ProjectFileError::InvalidProject { reason }) => reason,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_file_that_says_a_resource_is_included_where_the_archive_has_nothing_is_refused_with_its_name() {
    let folder = TempDir::new().unwrap();
    let archive = archive_with_the_resources(
        folder.path(),
        json!([{"name": "grade.cube", "kind": "cube", "included_as": "project-resources/5-grade.cube", "absolute_path": "/x/grade.cube"}]),
        &[],
    );

    let reason = reason_for_unreadable(&archive);

    assert!(
        reason.contains("grade.cube") && reason.contains("project-resources/5-grade.cube"),
        "{reason}"
    );
}

#[test]
fn an_entry_name_that_is_not_one_a_resource_could_have_is_refused_before_the_archive_is_looked_in() {
    let folder = TempDir::new().unwrap();
    for entry in [
        "../outside.cube",
        "project-resources/../x",
        "project-resources/abc-grade.cube",
        "project-resources/5-",
        "project-resources/5-a/b",
        "resources/5-grade.cube",
        "project.fastforwardrs.json",
        "project-resources/99999999999-grade.cube",
    ] {
        let archive = archive_with_the_resources(
            folder.path(),
            json!([{"name": "grade.cube", "kind": "cube", "included_as": entry, "absolute_path": "/x/grade.cube"}]),
            &[],
        );
        let reason = reason_for_unreadable(&archive);
        assert!(
            reason.contains("not a name that a resource included in it has"),
            "{entry}: {reason}"
        );
    }
}

#[test]
fn a_file_with_a_resource_of_a_kind_this_version_does_not_use_or_a_name_taken_twice_or_a_name_that_is_no_name_is_refused()
 {
    let folder = TempDir::new().unwrap();
    let one = |extra: Value| {
        let mut resource = json!({"name": "grade.cube", "kind": "cube", "absolute_path": "/x/grade.cube"});
        resource
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        resource
    };
    for (resources, wanted) in [
        (
            json!([one(json!({"kind": "png"}))]),
            "a kind of file that this version does not use",
        ),
        (json!([one(json!({})), one(json!({}))]), "another resource has the name"),
        (
            json!([one(json!({"name": "../grade.cube"}))]),
            "is not a name for a resource",
        ),
        (json!([one(json!({"name": ""}))]), "it is empty"),
    ] {
        let archive = archive_with_the_resources(folder.path(), resources, &[]);
        let reason = reason_for_unreadable(&archive);
        assert!(
            reason.contains("resource number 1") || reason.contains("resource number 2"),
            "{reason}"
        );
        assert!(reason.contains(wanted), "{wanted}: {reason}");
    }
}

#[test]
fn a_resource_that_the_project_file_has_but_no_effect_uses_is_read_and_dropped_when_it_is_saved_next() {
    let folder = TempDir::new().unwrap();
    let entry = project_resource_entry_name(crc32fast::hash(b"table"), "other.cube");
    let archive = archive_with_the_resources(
        folder.path(),
        json!([{"name": "other.cube", "kind": "cube", "included_as": entry, "absolute_path": "/x/other.cube"}]),
        &[(&entry, b"table")],
    );

    let loaded = read_project_file(&archive).unwrap();
    assert_eq!(bytes_of(&loaded, "other.cube"), Some(b"table".to_vec()));
    let again = folder.path().join("again.ffrs");
    write_project_file(&loaded, &again).unwrap();

    let file: Value = serde_json::from_str(&archive::read_project_text(&again).unwrap()).unwrap();
    assert!(file.get("resources").is_none(), "{file}");
}

#[test]
fn the_problems_a_resource_can_have_are_told_in_words_a_person_can_read() {
    for (problem, wanted) in [
        (
            ResourceProblem::TooBig {
                bytes: 70_000_000,
                limit: MOST_BYTES_OF_A_RESOURCE,
            },
            "more than 67108864",
        ),
        (ResourceProblem::Empty, "empty"),
        (ResourceProblem::TooMany { limit: 256 }, "at most 256"),
    ] {
        assert!(problem.to_string().contains(wanted), "{problem}");
    }
}

// --- adding a resource from a file ---

#[test]
fn a_file_is_read_whole_to_be_a_resource_and_the_reasons_it_cannot_be_are_each_told() {
    let folder = TempDir::new().unwrap();
    let table = folder.path().join("grade.cube");
    fs::write(&table, "LUT_3D_SIZE 2\n").unwrap();
    assert_eq!(&*read_a_resource_file(&table).unwrap(), b"LUT_3D_SIZE 2\n");

    let empty = folder.path().join("empty.cube");
    fs::write(&empty, "").unwrap();
    assert!(matches!(
        read_a_resource_file(&empty),
        Err(ResourceFileProblem::Empty { .. })
    ));

    assert!(matches!(
        read_a_resource_file(&folder.path().join("gone.cube")),
        Err(ResourceFileProblem::CouldNotRead { .. })
    ));
    assert!(
        matches!(
            read_a_resource_file(folder.path()),
            Err(ResourceFileProblem::CouldNotRead { .. })
        ),
        "(a folder is not a file)"
    );

    let big = folder.path().join("big.cube");
    fs::write(&big, vec![b'x'; MOST_BYTES_OF_A_RESOURCE + 1]).unwrap();
    let problem = read_a_resource_file(&big).unwrap_err();
    assert!(matches!(
        problem,
        ResourceFileProblem::TooBig { bytes, limit, .. } if bytes == MOST_BYTES_OF_A_RESOURCE as u64 + 1 && limit == MOST_BYTES_OF_A_RESOURCE
    ));
    assert!(
        problem.to_string().contains("67108865") && problem.to_string().contains("big.cube"),
        "{problem}"
    );
    fs::write(&big, vec![b'x'; MOST_BYTES_OF_A_RESOURCE]).unwrap();
    assert_eq!(read_a_resource_file(&big).unwrap().len(), MOST_BYTES_OF_A_RESOURCE);
}

#[test]
fn the_link_of_a_new_resource_is_the_path_of_its_file_and_when_it_is_embedded_the_entry_it_will_have() {
    let folder = TempDir::new().unwrap();
    let path = folder.path().join("grade.cube");

    let linked = link_of_a_new_resource(&path, "grade.cube", b"table", false);
    assert_eq!(linked.absolute_path, path);
    assert_eq!((linked.relative_path.clone(), linked.included_as.clone()), (None, None));

    let embedded = link_of_a_new_resource(&path, "grade.cube", b"table", true);
    assert_eq!(embedded.absolute_path, path, "(where it was, kept)");
    assert_eq!(
        embedded.included_as,
        Some(project_resource_entry_name(crc32fast::hash(b"table"), "grade.cube"))
    );
    assert!(archive::is_a_project_resource_entry_name(
        embedded.included_as.as_deref().unwrap()
    ));
    // A path that is not absolute is made so.
    let made_absolute = link_of_a_new_resource(Path::new("grade.cube"), "grade.cube", b"table", false);
    assert!(made_absolute.absolute_path.is_absolute());
}

#[test]
fn an_archive_with_an_included_resource_still_has_it_after_it_is_written_again_without_its_garbage() {
    let folder = TempDir::new().unwrap();
    let mut project = project_using("grade.cube");
    let first_entry = include_a_table(&mut project, "grade.cube", &"first table ".repeat(100_000));
    let destination = folder.path().join("project.ffrs");
    write_project_file_including(
        &project,
        &destination,
        &IncludedFiles::new(),
        HowToWrite::AddingToWhatIsThere,
    )
    .unwrap();
    // The table is changed: the old entry is garbage now, as the first save left it.
    let mut changed = project_using("grade.cube");
    let second_table = "second table ".repeat(100_000);
    let second_entry = include_a_table(&mut changed, "grade.cube", &second_table);
    let written = write_project_file_including(
        &changed,
        &destination,
        &IncludedFiles::new(),
        HowToWrite::AddingToWhatIsThere,
    )
    .unwrap();
    assert!(written.archive.unwrap().bytes_of_garbage > 1_000_000);
    assert_ne!(first_entry, second_entry);

    let compacted = archive::compact_archive(&destination, &mut |_| Ok(())).unwrap();

    assert_eq!(compacted.bytes_of_garbage, 0);
    assert!(compacted.entries.contains_key(&second_entry));
    assert!(!compacted.entries.contains_key(&first_entry));
    let loaded = read_project_file(&destination).unwrap();
    assert_eq!(bytes_of(&loaded, "grade.cube"), Some(second_table.into_bytes()));
}
