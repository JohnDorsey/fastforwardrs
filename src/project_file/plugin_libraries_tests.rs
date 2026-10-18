// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugin libraries of a project in a project file (format 14; phase 7, piece 7.3): linked and embedded, saved and read back, added to an archive without being written
//! again, and everything a file from elsewhere may say that is refused.

use std::sync::Arc;

use serde_json::{Value, json};
use tempfile::TempDir;

use super::archive::{self, plugin_library_entry_name};
use super::tests::{edited_project, link_to_missing_video, saved};
use super::*;
use crate::digest::Sha256Digest;
use crate::project::{
    EmbeddedBuild, LinkedLibrary, MOST_BYTES_OF_A_BUILD, MOST_BYTES_OF_ALL_BUILDS, PluginLibrary, PluginLibraryKind,
    PluginLibrarySource,
};

fn bytes(text: &str) -> Arc<[u8]> {
    Arc::from(text.as_bytes())
}

fn build(platform: &str, text: &str) -> EmbeddedBuild {
    EmbeddedBuild::new(platform, bytes(text)).unwrap()
}

fn an_embedded_library(name: &str, builds: Vec<EmbeddedBuild>) -> PluginLibrary {
    PluginLibrary::new(name, PluginLibraryKind::Ladspa, PluginLibrarySource::Embedded(builds)).unwrap()
}

fn a_linked_library(name: &str, path: &Path, relative: Option<&str>) -> PluginLibrary {
    PluginLibrary::new(
        name,
        PluginLibraryKind::Ladspa,
        PluginLibrarySource::Linked(LinkedLibrary {
            absolute_path: path.to_owned(),
            relative_path: relative.map(PathBuf::from),
        }),
    )
    .unwrap()
}

/// A project with a linked library and an embedded one that has two builds, in this order.
fn project_with_libraries(folder: &Path) -> Project {
    let mut project = edited_project(link_to_missing_video());
    let linked_file = folder.join("tap_reverb.so");
    fs::write(&linked_file, "linked bytes").unwrap();
    project
        .add_plugin_library(a_linked_library("tap_reverb.so", &linked_file, None))
        .unwrap();
    project
        .add_plugin_library(an_embedded_library(
            "amp_1181.dll",
            vec![
                build("windows-x86_64", "the windows build"),
                build("linux-x86_64", "the linux build"),
            ],
        ))
        .unwrap();
    project
}

fn entries_of(archive_path: &Path) -> Vec<String> {
    let mut names: Vec<String> = archive::state_of_archive(archive_path)
        .unwrap()
        .entries
        .keys()
        .cloned()
        .collect();
    names.sort();
    names
}

// --- the names ---

#[test]
fn the_entry_of_a_build_has_what_is_in_it_in_its_name_and_only_such_names_are_believed() {
    let digest = Sha256Digest::of_bytes(b"x").short_hex();
    let name = plugin_library_entry_name(&digest, "windows-x86_64", "amp_1181.dll");
    assert_eq!(name, format!("plugin-libraries/{digest}-windows-x86_64-amp_1181.dll"));
    assert!(archive::is_a_plugin_library_entry_name(&name));
    for bad in [
        "plugin-libraries/",
        "plugin-libraries/abc-windows-amp.dll",
        "plugin-libraries/0123456789abcdeg-windows-x86_64-amp.dll",
        "plugin-libraries/0123456789ABCDEF-windows-x86_64-amp.dll",
        "plugin-libraries/0123456789abcdef-",
        "plugin-libraries/0123456789abcdef-windows/../../amp.dll",
        "plugin-libraries/0123456789abcdef-windows\\amp.dll",
        "resources/1-amp.dll",
        "project-resources/1-amp.dll",
        "../plugin-libraries/0123456789abcdef-windows-amp.dll",
    ] {
        assert!(!archive::is_a_plugin_library_entry_name(bad), "{bad:?}");
    }
    let too_long = format!("plugin-libraries/{digest}-windows-{}", "n".repeat(300));
    assert!(!archive::is_a_plugin_library_entry_name(&too_long));
}

// --- saved and read back ---

#[test]
fn libraries_are_saved_in_an_archive_with_their_builds_and_read_back_whole_in_their_order() {
    let folder = TempDir::new().unwrap();
    let project = project_with_libraries(folder.path());
    let destination = folder.path().join("project.ffrs");

    write_project_file_including(&project, &destination, &IncludedFiles::new(), HowToWrite::Anew).unwrap();

    let windows = build("windows-x86_64", "the windows build");
    let linux = build("linux-x86_64", "the linux build");
    let entries = entries_of(&destination);
    assert!(entries.contains(&plugin_library_entry_name(
        &windows.digest().short_hex(),
        "windows-x86_64",
        "amp_1181.dll"
    )));
    assert!(entries.contains(&plugin_library_entry_name(
        &linux.digest().short_hex(),
        "linux-x86_64",
        "amp_1181.dll"
    )));

    let loaded = read_project_file(&destination).unwrap();
    let libraries: Vec<&PluginLibrary> = loaded.plugin_libraries().iter().collect();
    assert_eq!(libraries.len(), 2);
    assert_eq!(libraries[0].name(), "tap_reverb.so");
    assert!(!libraries[0].is_embedded());
    assert_eq!(libraries[1].name(), "amp_1181.dll");
    assert_eq!(libraries[1].platforms(), ["windows-x86_64", "linux-x86_64"]);
    assert_eq!(
        libraries[1].build_for("linux-x86_64").unwrap().bytes().as_ref(),
        b"the linux build"
    );
    assert!(libraries[1].build_for("windows-x86_64").unwrap().is_what_it_says());
    assert_eq!(
        loaded.plugin_libraries().bytes_of_builds(),
        "the windows build".len() + "the linux build".len()
    );
}

#[test]
fn a_project_with_an_embedded_plugin_is_the_same_text_and_the_same_builds_after_a_save_a_load_and_a_save() {
    let folder = TempDir::new().unwrap();
    let project = project_with_libraries(folder.path());
    let first = folder.path().join("first.ffrs");
    write_project_file_including(&project, &first, &IncludedFiles::new(), HowToWrite::Anew).unwrap();

    let loaded = read_project_file(&first).unwrap();
    let second = folder.path().join("second.ffrs");
    write_project_file_including(&loaded, &second, &IncludedFiles::new(), HowToWrite::Anew).unwrap();

    assert_eq!(
        archive::read_project_text(&first).unwrap(),
        archive::read_project_text(&second).unwrap(),
        "the text of the project is the same"
    );
    assert_eq!(entries_of(&first), entries_of(&second), "and so are the entries");
    for entry in entries_of(&first)
        .iter()
        .filter(|name| name.starts_with("plugin-libraries/"))
    {
        let one = archive::read_entry_bytes(&first, entry, 1 << 20).unwrap();
        let other = archive::read_entry_bytes(&second, entry, 1 << 20).unwrap();
        assert_eq!(one, other, "{entry}: byte for byte");
    }
    assert_eq!(
        loaded.plugin_libraries(),
        read_project_file(&second).unwrap().plugin_libraries()
    );
}

#[test]
fn saving_again_to_the_same_archive_adds_nothing_to_the_builds_that_are_in_it() {
    let folder = TempDir::new().unwrap();
    let project = project_with_libraries(folder.path());
    let destination = folder.path().join("project.ffrs");
    write_project_file_including(&project, &destination, &IncludedFiles::new(), HowToWrite::Anew).unwrap();
    let before = archive::state_of_archive(&destination).unwrap();

    let loaded = read_project_file(&destination).unwrap();
    write_project_file_including(
        &loaded,
        &destination,
        &IncludedFiles::new(),
        HowToWrite::AddingToWhatIsThere,
    )
    .unwrap();

    let after = archive::state_of_archive(&destination).unwrap();
    for entry in before
        .entries
        .keys()
        .filter(|name| name.starts_with("plugin-libraries/"))
    {
        assert_eq!(
            before.entries[entry], after.entries[entry],
            "{entry} stayed where it was, and was not written again"
        );
    }
    assert_eq!(
        read_project_file(&destination).unwrap().plugin_libraries(),
        loaded.plugin_libraries()
    );
}

#[test]
fn a_library_that_is_taken_away_is_not_saved_again_and_a_new_one_is_added() {
    let folder = TempDir::new().unwrap();
    let mut project = project_with_libraries(folder.path());
    let destination = folder.path().join("project.ffrs");
    write_project_file_including(&project, &destination, &IncludedFiles::new(), HowToWrite::Anew).unwrap();

    assert!(project.remove_plugin_library("amp_1181.dll"));
    project
        .add_plugin_library(an_embedded_library(
            "tap_reverb2.dll",
            vec![build("windows-x86_64", "another library")],
        ))
        .unwrap();
    write_project_file_including(&project, &destination, &IncludedFiles::new(), HowToWrite::Anew).unwrap();

    let entries = entries_of(&destination);
    assert_eq!(
        entries
            .iter()
            .filter(|name| name.starts_with("plugin-libraries/"))
            .count(),
        1,
        "{entries:?}"
    );
    let loaded = read_project_file(&destination).unwrap();
    assert_eq!(
        loaded
            .plugin_libraries()
            .iter()
            .map(PluginLibrary::name)
            .collect::<Vec<_>>(),
        ["tap_reverb.so", "tap_reverb2.dll"]
    );
}

#[test]
fn a_build_whose_bytes_are_not_what_the_file_says_is_read_and_kept_as_it_is_and_is_not_what_it_says() {
    let folder = TempDir::new().unwrap();
    let mut project = edited_project(link_to_missing_video());
    let lying = EmbeddedBuild::as_recorded(
        "windows-x86_64",
        Sha256Digest::of_bytes(b"what the project says"),
        bytes("what is in it"),
    )
    .unwrap();
    project
        .add_plugin_library(an_embedded_library("amp.dll", vec![lying]))
        .unwrap();
    let destination = folder.path().join("project.ffrs");
    write_project_file_including(&project, &destination, &IncludedFiles::new(), HowToWrite::Anew).unwrap();

    let loaded = read_project_file(&destination).unwrap();

    let build = loaded
        .plugin_libraries()
        .get("amp.dll")
        .unwrap()
        .build_for("windows-x86_64")
        .unwrap();
    assert!(!build.is_what_it_says());
    assert_eq!(build.digest(), Sha256Digest::of_bytes(b"what the project says"));
    assert_eq!(build.bytes().as_ref(), b"what is in it");
}

// --- plain JSON ---

#[test]
fn a_plain_json_file_has_linked_libraries_and_cannot_have_a_plugin_inside_it() {
    let folder = TempDir::new().unwrap();
    let linked_file = folder.path().join("tap_reverb.so");
    fs::write(&linked_file, "bytes").unwrap();
    let mut project = edited_project(link_to_missing_video());
    project
        .add_plugin_library(a_linked_library("tap_reverb.so", &linked_file, None))
        .unwrap();
    let plain = folder.path().join("project.json");
    write_project_file(&project, &plain).unwrap();
    let loaded = read_project_file(&plain).unwrap();
    assert_eq!(loaded.plugin_libraries().len(), 1);
    let PluginLibrarySource::Linked(link) = loaded.plugin_libraries().get("tap_reverb.so").unwrap().source() else {
        panic!("it is linked")
    };
    assert_eq!(link.absolute_path, linked_file);
    assert_eq!(
        link.relative_path,
        Some(PathBuf::from("tap_reverb.so")),
        "the file is there, so where it is from the project is worked out when it is saved"
    );
    assert!(fs::read_to_string(&plain).unwrap().contains("\"linked\""));

    project
        .add_plugin_library(an_embedded_library("amp.dll", vec![build("windows-x86_64", "x")]))
        .unwrap();
    let refused = write_project_file(&project, &folder.path().join("with a plugin inside.json"));
    assert!(
        matches!(refused, Err(ProjectFileError::IncludedFilesNeedAnArchive { .. })),
        "{refused:?}"
    );

    // And a plain file that says a plugin is inside it is refused when it is read.
    let mut text: Value = serde_json::from_str(&saved(&edited_project(link_to_missing_video()))).unwrap();
    text["plugin_libraries"] = json!([{"name": "amp.dll", "kind": "ladspa", "embedded": {"builds": []}}]);
    let reason = match project_from_json_text(&serde_json::to_string(&text).unwrap()) {
        Err(ProjectFileError::InvalidProject { reason }) => reason,
        other => panic!("{other:?}"),
    };
    assert!(
        reason.contains("only a project file that is a .ffrs file can have a plugin inside it"),
        "{reason}"
    );
}

// --- what a file from elsewhere may say ---

/// An archive made by hand whose project says `libraries` and that has the `files`.
fn archive_with_the_libraries(folder: &Path, libraries: Value, files: &[(&str, &[u8])]) -> PathBuf {
    let mut file: Value = serde_json::from_str(&saved(&edited_project(link_to_missing_video()))).unwrap();
    file["plugin_libraries"] = libraries;
    let text = serde_json::to_string_pretty(&file).unwrap();
    let destination = folder.join("by hand.ffrs");
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

fn a_library_of(builds: Value) -> Value {
    json!([{"name": "amp.dll", "kind": "ladspa", "embedded": {"builds": builds}}])
}

fn entry_for(text: &[u8]) -> (String, String) {
    let digest = Sha256Digest::of_bytes(text);
    (
        plugin_library_entry_name(&digest.short_hex(), "windows-x86_64", "amp.dll"),
        digest.to_hex(),
    )
}

#[test]
fn a_file_by_hand_with_a_plugin_that_is_as_it_says_is_read() {
    let folder = TempDir::new().unwrap();
    let (entry, sha) = entry_for(b"the bytes");
    let path = archive_with_the_libraries(
        folder.path(),
        a_library_of(json!([{"platform": "windows-x86_64", "sha256": sha, "size": 9, "entry": entry}])),
        &[(&entry, b"the bytes")],
    );
    let project = read_project_file(&path).unwrap();
    let library = project.plugin_libraries().get("amp.dll").unwrap();
    assert_eq!(
        library.build_for("windows-x86_64").unwrap().bytes().as_ref(),
        b"the bytes"
    );
}

#[test]
fn a_file_that_says_a_build_is_inside_where_the_archive_has_nothing_or_another_size_is_refused_before_it_is_read() {
    let folder = TempDir::new().unwrap();
    let (entry, sha) = entry_for(b"the bytes");
    let build = json!([{"platform": "windows-x86_64", "sha256": sha, "size": 9, "entry": entry}]);

    let nothing = archive_with_the_libraries(folder.path(), a_library_of(build.clone()), &[]);
    let reason = reason_for_unreadable(&nothing);
    assert!(
        reason.contains("plugin library number 1") && reason.contains("which the project file does not have"),
        "{reason}"
    );

    let other_size = archive_with_the_libraries(folder.path(), a_library_of(build), &[(&entry, b"the bytes and more")]);
    let reason = reason_for_unreadable(&other_size);
    assert!(
        reason.contains("is said to have 9 bytes, and the project file has 18"),
        "{reason}"
    );
}

#[test]
fn a_file_that_asks_for_more_than_the_limits_is_refused_by_what_it_says_and_nothing_is_read() {
    let folder = TempDir::new().unwrap();
    let (entry, sha) = entry_for(b"x");
    let too_big_one = json!([{"platform": "windows-x86_64", "sha256": sha, "size": MOST_BYTES_OF_A_BUILD as u64 + 1, "entry": entry}]);
    let path = archive_with_the_libraries(folder.path(), a_library_of(too_big_one), &[]);
    assert!(reason_for_unreadable(&path).contains("a build of more than 256 MiB is not used"));

    // Two that are each allowed and are too much together.
    let half_and_a_bit = MOST_BYTES_OF_ALL_BUILDS as u64 / 2 + 1;
    let digest = Sha256Digest::of_bytes(b"y");
    let first = plugin_library_entry_name(&digest.short_hex(), "linux-x86_64", "amp.dll");
    let two = json!([
        {"platform": "windows-x86_64", "sha256": sha, "size": half_and_a_bit, "entry": entry},
        {"platform": "linux-x86_64", "sha256": digest.to_hex(), "size": half_and_a_bit, "entry": first},
    ]);
    let path = archive_with_the_libraries(folder.path(), a_library_of(two), &[]);
    assert!(reason_for_unreadable(&path).contains("more than 256 MiB together"));
}

#[test]
fn a_file_with_a_build_that_is_not_well_formed_is_refused_with_the_number_of_the_library() {
    let folder = TempDir::new().unwrap();
    let (entry, sha) = entry_for(b"the bytes");
    let files: [(&str, &[u8]); 1] = [(&entry, b"the bytes")];
    let refuse = |libraries: Value, files: &[(&str, &[u8])]| {
        reason_for_unreadable(&archive_with_the_libraries(folder.path(), libraries, files))
    };

    let reason = refuse(
        a_library_of(
            json!([{"platform": "windows-x86_64", "sha256": sha, "size": 9, "entry": "plugin-libraries/../../outside"}]),
        ),
        &files,
    );
    assert!(
        reason.contains("which is not a name that a plugin inside it has"),
        "{reason}"
    );

    let reason = refuse(
        a_library_of(json!([{"platform": "windows-x86_64", "sha256": "not a digest", "size": 9, "entry": entry}])),
        &files,
    );
    assert!(reason.contains("is not 64 hexadecimal digits"), "{reason}");

    let reason = refuse(
        a_library_of(json!([{"platform": "Windows X86", "sha256": sha, "size": 9, "entry": entry}])),
        &files,
    );
    assert!(reason.contains("is not the name of a platform"), "{reason}");

    let reason = refuse(
        a_library_of(json!([
            {"platform": "windows-x86_64", "sha256": sha, "size": 9, "entry": entry},
            {"platform": "windows-x86_64", "sha256": sha, "size": 9, "entry": entry},
        ])),
        &files,
    );
    assert!(
        reason.contains("more than one build is said to be inside the project file as"),
        "{reason}"
    );

    let reason = refuse(
        json!([{"name": "amp.dll", "kind": "vst3", "linked": {"absolute_path": "/x"}}]),
        &[],
    );
    assert!(
        reason.contains("plugin library number 1") && reason.contains("kind of plugin that this version does not use"),
        "{reason}"
    );

    let reason = refuse(json!([{"name": "amp.dll", "kind": "ladspa"}]), &[]);
    assert!(
        reason.contains("must say either where its file is (linked) or what its builds are (embedded)"),
        "{reason}"
    );

    let reason = refuse(
        json!([{"name": "amp.dll", "kind": "ladspa", "linked": {"absolute_path": "/x"}, "embedded": {"builds": []}}]),
        &[],
    );
    assert!(reason.contains("one of the two"), "{reason}");

    let reason = refuse(
        json!([{"name": "a/b.dll", "kind": "ladspa", "linked": {"absolute_path": "/x"}}]),
        &[],
    );
    assert!(reason.contains("is not a name for a plugin library"), "{reason}");

    let reason = refuse(
        json!([
            {"name": "amp.dll", "kind": "ladspa", "linked": {"absolute_path": "/x"}},
            {"name": "amp.dll", "kind": "ladspa", "linked": {"absolute_path": "/y"}},
        ]),
        &[],
    );
    assert!(
        reason.contains("plugin library number 2") && reason.contains("a plugin library called \"amp.dll\" already"),
        "{reason}"
    );
}

// --- the version ---

#[test]
fn a_file_of_version_13_has_no_libraries_and_is_saved_as_version_14() {
    let folder = TempDir::new().unwrap();
    let project = edited_project(link_to_missing_video());
    let text = saved(&project);
    assert!(text.contains("\"file_format_version\": 14"));
    assert!(
        !text.contains("plugin_libraries"),
        "a project with none says nothing of them"
    );
    let thirteen = text.replace("\"file_format_version\": 14", "\"file_format_version\": 13");
    let loaded = project_from_json_text(&thirteen).unwrap();
    assert!(loaded.plugin_libraries().is_empty());
    assert!(saved(&loaded).contains("\"file_format_version\": 14"));
    // A file of a version after this one is refused, as before.
    let fifteen = text.replace("\"file_format_version\": 14", "\"file_format_version\": 15");
    assert!(matches!(
        project_from_json_text(&fifteen),
        Err(ProjectFileError::NewerFileFormat {
            found: 15,
            supported: 14
        })
    ));
    let _ = folder;
}
