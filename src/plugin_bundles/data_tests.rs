// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the sets of **data** that plugins use (Surge XT's patches and wavetables): the rules of the manifest for them, a download that takes a folder of a release file as the folder of the
//! set and leaves out what the manifest says may not ship, and a tar made by the system's `tar` with the names that the real file of Surge XT has (a space, `./` in front, a name that is not
//! ASCII).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::Ordering;

use super::test_support::{FakeFetcher, Fixture, a_fixture, a_fixture_with_data, a_zip, sha};
use super::*;

fn fetch_all(fixture: &Fixture, fetcher: &FakeFetcher, folder: &Path) -> Result<Outcome, String> {
    fetch_plugins(
        &fixture.manifest,
        "windows-x86_64",
        folder,
        fetcher,
        &mut |_| {},
        &|| fetcher.cancelled.load(Ordering::SeqCst),
    )
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| panic!("{path:?}: {error}"))
}

fn problems_of(fixture: Fixture) -> String {
    fixture.manifest.problems().join("\n")
}

// --- the manifest ---

#[test]
fn a_manifest_with_a_set_of_data_is_fit_and_the_sets_of_each_kind_can_be_told_apart() {
    let fixture = a_fixture_with_data();

    assert_eq!(fixture.manifest.problems(), Vec::<String>::new());
    let plugins = fixture.manifest.only(SetKind::Plugins);
    let data = fixture.manifest.only(SetKind::Data);
    assert_eq!(
        plugins.sets.iter().map(|set| set.id.as_str()).collect::<Vec<_>>(),
        ["two", "three", "elsewhere"]
    );
    assert_eq!(
        data.sets.iter().map(|set| set.id.as_str()).collect::<Vec<_>>(),
        ["data"]
    );
    assert_eq!((plugins.format, data.format), (1, 1));
    assert_eq!(data.sets[0].folder_name(), "TheData");
    assert_eq!(
        plugins.sets[0].folder_name(),
        "two",
        "a set of plugins is in a folder called after it"
    );
    assert!(data.sets[0].is_data() && !plugins.sets[0].is_data());
}

#[test]
fn a_set_in_a_manifest_that_says_no_kind_is_a_set_of_plugins() {
    let text = serde_json::json!({
        "format": 1,
        "sets": [{
            "id": "old", "name": "Old", "version": "1", "authors": ["A"], "licence": "MIT",
            "homepage": "https://example.org/", "plugins": ["P"],
            "licence_texts": [], "source": {"url": "https://example.org/s"}, "platforms": {}
        }]
    })
    .to_string();

    let manifest: Manifest = serde_json::from_str(&text).unwrap();

    assert_eq!(manifest.sets[0].kind, SetKind::Plugins);
    assert_eq!(manifest.sets[0].folder, None);
    assert!(manifest.sets[0].platforms.is_empty());
}

#[test]
fn what_is_wrong_with_a_set_of_data_is_said() {
    let mut fixture = a_fixture_with_data();
    fixture.manifest.sets[0]
        .platforms
        .get_mut("windows-x86_64")
        .unwrap()
        .members[0]
        .to = "Data".to_owned();
    let problems = problems_of(fixture);
    assert!(problems.contains("is put as the folder of the set"), "{problems}");

    let mut fixture = a_fixture_with_data();
    fixture.manifest.sets[0]
        .platforms
        .get_mut("windows-x86_64")
        .unwrap()
        .members[0]
        .from = "Pack/Data".to_owned();
    let problems = problems_of(fixture);
    assert!(problems.contains("must end with /"), "{problems}");

    let mut fixture = a_fixture_with_data();
    let members = &mut fixture.manifest.sets[0]
        .platforms
        .get_mut("windows-x86_64")
        .unwrap()
        .members;
    members.push(members[0].clone());
    let problems = problems_of(fixture);
    assert!(problems.contains("takes one folder of the archive"), "{problems}");

    for bad in ["", "/etc", "../up", "a/../../up", "a\\b"] {
        let mut fixture = a_fixture_with_data();
        fixture.manifest.sets[0]
            .platforms
            .get_mut("windows-x86_64")
            .unwrap()
            .members[0]
            .leave_out
            .push(bad.to_owned());
        let problems = problems_of(fixture);
        assert!(
            problems.contains("is not a path inside the folder"),
            "{bad:?}: {problems}"
        );
    }
}

#[test]
fn the_folder_of_a_set_of_data_is_a_plain_name_that_no_other_set_has_and_plugins_have_none_of_their_own() {
    for bad in ["", ".", "..", "a/b", "a\\b", ".hidden"] {
        let mut fixture = a_fixture_with_data();
        fixture.manifest.sets[0].folder = Some(bad.to_owned());
        let problems = problems_of(fixture);
        assert!(
            problems.contains("the folder is not a plain name of a folder"),
            "{bad:?}: {problems}"
        );
    }
    // (The folder of the data is the folder of a set of plugins.)
    let mut fixture = a_fixture_with_data();
    fixture.manifest.sets[0].folder = Some("two".to_owned());
    let problems = problems_of(fixture);
    assert!(problems.contains("is the folder of another set too"), "{problems}");
    // (Two sets of data in one folder.)
    let mut fixture = a_fixture_with_data();
    let mut second = fixture.manifest.sets[0].clone();
    second.id = "more-data".to_owned();
    fixture.manifest.sets.push(second);
    let problems = problems_of(fixture);
    assert!(problems.contains("is the folder of another set too"), "{problems}");
    // (A set of plugins that says it has a folder, or leaves something out.)
    let mut fixture = a_fixture_with_data();
    fixture.manifest.sets[1].folder = Some("Elsewhere".to_owned());
    let problems = problems_of(fixture);
    assert!(problems.contains("only data has a folder of its own"), "{problems}");
    let mut fixture = a_fixture_with_data();
    fixture.manifest.sets[1]
        .platforms
        .get_mut("windows-x86_64")
        .unwrap()
        .members[0]
        .leave_out
        .push("x".to_owned());
    let problems = problems_of(fixture);
    assert!(problems.contains("only data leaves anything out"), "{problems}");
}

#[test]
fn a_set_of_data_still_needs_a_licence_that_may_ship() {
    let mut fixture = a_fixture_with_data();
    fixture.manifest.sets[0].licence = "AGPL-3.0-only".to_owned();

    let problems = problems_of(fixture);

    assert!(problems.contains("is not one that may ship"), "{problems}");
}

// --- the download ---

#[test]
fn a_set_of_data_is_put_in_its_own_folder_as_what_is_in_the_folder_it_takes_without_what_is_left_out() {
    let fixture = a_fixture_with_data();
    let fetcher = FakeFetcher::new(fixture.files.clone());
    let folder = tempfile::tempdir().unwrap();

    let outcome = fetch_all(&fixture, &fetcher, folder.path()).unwrap();

    let data = folder.path().join("TheData");
    assert_eq!(read(&data.join("patches").join("a.fxp")), b"a patch");
    assert_eq!(
        read(&data.join("patches").join("Factory").join("b.fxp")),
        b"another patch"
    );
    assert_eq!(
        read(&data.join("wavetables").join("Free").join("x.wt")),
        b"a free wavetable"
    );
    assert_eq!(read(&data.join("skins").join("plain").join("skin.xml")), b"a skin");
    assert_eq!(read(&data.join("skins").join("readme.txt")), b"kept");
    assert!(
        !data.join("wavetables").join("Maybe").exists(),
        "a folder that is left out is not there"
    );
    assert!(
        !data.join("skins").join("plain").join("font.ttf").exists(),
        "a file that is left out is not there"
    );
    assert!(!data.join("not-taken.txt").exists() && !data.join("Pack").exists());
    // (The licence texts and the notice are in the folder of the data, where they go with it.)
    assert_eq!(read(&data.join("LICENSE.txt")), b"GNU GENERAL PUBLIC LICENSE, a copy");
    let notice = String::from_utf8(read(&data.join("NOTICE.txt"))).unwrap();
    assert!(
        notice.contains("THE DATA 1.0") && notice.contains("what the manifest says is not kept was left out"),
        "{notice}"
    );
    assert!(
        !notice.contains("the plugins as their authors released them"),
        "{notice}"
    );
    // (Nothing that the download used is left, the plugins are where they were, and what was made is not called a plugin.)
    assert_eq!(super::tests::names_in(folder.path()), ["TheData", "three", "two"]);
    assert_eq!(outcome.sets, ["THE DATA", "TWO", "THREE"]);
    assert_eq!(outcome.plugin_files.len(), 3, "{:?}", outcome.plugin_files);
    assert!(outcome.plugin_files.iter().all(|file| !file.starts_with(&data)));
    assert!(is_fetched(&fixture.manifest.sets[0], "windows-x86_64", folder.path()));
}

#[test]
fn a_set_of_data_is_fetched_only_if_the_folder_has_the_notice_that_a_download_writes() {
    let fixture = a_fixture_with_data();
    let folder = tempfile::tempdir().unwrap();
    let data = &fixture.manifest.sets[0];

    assert!(!is_fetched(data, "windows-x86_64", folder.path()), "nothing there");
    std::fs::create_dir_all(folder.path().join("TheData")).unwrap();
    assert!(
        !is_fetched(data, "windows-x86_64", folder.path()),
        "a folder that a person made with the name is not the data"
    );
    std::fs::write(folder.path().join("TheData").join("NOTICE.txt"), b"x").unwrap();
    assert!(is_fetched(data, "windows-x86_64", folder.path()));
    assert!(
        !is_fetched(data, "linux-x86_64", folder.path()),
        "a system that has no build of it"
    );
}

#[test]
fn something_that_is_left_out_and_is_not_in_the_release_file_is_said_and_nothing_of_the_set_is_kept() {
    let mut fixture = a_fixture_with_data();
    fixture.manifest.sets[0]
        .platforms
        .get_mut("windows-x86_64")
        .unwrap()
        .members[0]
        .leave_out
        .push("wavetables/Moved/".to_owned());
    let fetcher = FakeFetcher::new(fixture.files.clone());
    let folder = tempfile::tempdir().unwrap();
    // (What was there before is kept when the new set is not made.)
    std::fs::create_dir_all(folder.path().join("TheData")).unwrap();
    std::fs::write(folder.path().join("TheData").join("old.txt"), b"old").unwrap();

    let error = fetch_all(&fixture, &fetcher, folder.path()).unwrap_err();

    assert!(
        error.contains("\"wavetables/Moved/\" is not in \"Pack/Data/\""),
        "{error}"
    );
    assert!(error.contains("nothing of it was used"), "{error}");
    assert_eq!(
        super::tests::names_in(folder.path()),
        ["TheData"],
        "no folder that a download made is left"
    );
    assert_eq!(read(&folder.path().join("TheData").join("old.txt")), b"old");
    assert!(!folder.path().join("TheData").join("NOTICE.txt").exists());
}

#[test]
fn a_file_is_not_left_out_by_a_name_that_says_folder_and_a_new_fetch_does_not_keep_what_was_in_the_old_folder() {
    let mut fixture = a_fixture_with_data();
    fixture.manifest.sets[0]
        .platforms
        .get_mut("windows-x86_64")
        .unwrap()
        .members[0]
        .leave_out = vec!["skins/readme.txt/".to_owned()];
    let fetcher = FakeFetcher::new(fixture.files.clone());
    let folder = tempfile::tempdir().unwrap();
    let error = fetch_all(&fixture, &fetcher, folder.path()).unwrap_err();
    assert!(error.contains("is not in"), "a file is not a folder: {error}");

    let fixture = a_fixture_with_data();
    let fetcher = FakeFetcher::new(fixture.files.clone());
    let folder = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(folder.path().join("TheData").join("patches")).unwrap();
    std::fs::write(
        folder.path().join("TheData").join("patches").join("stale.fxp"),
        b"stale",
    )
    .unwrap();
    fetch_all(&fixture, &fetcher, folder.path()).unwrap();
    assert!(!folder.path().join("TheData").join("patches").join("stale.fxp").exists());
    assert!(folder.path().join("TheData").join("patches").join("a.fxp").is_file());
}

#[test]
fn a_release_file_that_a_set_of_data_and_a_set_of_plugins_are_both_in_is_downloaded_once() {
    let mut fixture = a_fixture_with_data();
    // (The plugin set `two` takes its plugins from the zip of the data, which has them too.)
    let zip = a_zip(&[
        ("Pack/Data/patches/a.fxp", b"a patch"),
        ("Pack/Plugin.clap", b"the plugin"),
    ]);
    fixture
        .files
        .insert("https://example.org/data.zip".to_owned(), zip.clone());
    for set in &mut fixture.manifest.sets {
        if set.id == "data" || set.id == "two" {
            let build = set.platforms.get_mut("windows-x86_64").unwrap();
            build.url = "https://example.org/data.zip".to_owned();
            build.sha256 = sha(&zip);
            build.size = zip.len() as u64;
            if set.id == "data" {
                build.members[0].leave_out.clear();
            } else {
                build.members = vec![Member {
                    from: "Pack/Plugin.clap".to_owned(),
                    to: "First.clap".to_owned(),
                    leave_out: Vec::new(),
                }];
                set.plugins = vec!["First".to_owned()];
            }
        }
    }
    let fetcher = FakeFetcher::new(fixture.files.clone());
    let folder = tempfile::tempdir().unwrap();

    fetch_all(&fixture, &fetcher, folder.path()).unwrap();

    let asked = fetcher.asked();
    assert_eq!(
        asked
            .iter()
            .filter(|url| url.as_str() == "https://example.org/data.zip")
            .count(),
        1,
        "{asked:?}"
    );
    assert_eq!(read(&folder.path().join("two").join("First.clap")), b"the plugin");
    assert_eq!(
        read(&folder.path().join("TheData").join("patches").join("a.fxp")),
        b"a patch"
    );
}

#[test]
fn the_bytes_of_a_download_of_data_alone_are_the_size_of_its_release_file() {
    let fixture = a_fixture_with_data();
    let data = fixture.manifest.only(SetKind::Data);
    let plugins = fixture.manifest.only(SetKind::Plugins);

    let size_of_the_data = data.sets[0].platforms["windows-x86_64"].size;

    assert_eq!(bytes_to_download(&data, "windows-x86_64"), size_of_the_data);
    assert_eq!(
        bytes_to_download(&fixture.manifest, "windows-x86_64"),
        size_of_the_data + bytes_to_download(&plugins, "windows-x86_64")
    );
    assert_eq!(bytes_to_download(&data, "linux-x86_64"), 0, "no build for that system");
}

// --- a tar, as the real release file of Surge XT is ---

/// A tar of `source` made by the system's `tar` with every name beginning with `./` (as the real file's are), or `None` where there is no `tar`.
fn a_tar_of_everything_in(work: &Path, source: &Path) -> Option<Vec<u8>> {
    if std::process::Command::new("tar").arg("--version").output().is_err() {
        eprintln!("SKIPPED: there is no tar");
        return None;
    }
    let archive = work.join("content.tar.gz");
    let made = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(source)
        .arg(".")
        .status()
        .unwrap();
    assert!(made.success());
    Some(std::fs::read(&archive).unwrap())
}

#[test]
fn a_folder_of_a_tar_that_the_system_made_with_dot_slash_in_front_of_the_names_and_a_space_in_them_is_taken_whole() {
    let work = tempfile::tempdir().unwrap();
    let source = work.path().join("src");
    let data = source.join("Some Team").join("TheData");
    std::fs::create_dir_all(data.join("patches").join("Factory")).unwrap();
    std::fs::create_dir_all(data.join("wavetables").join("Waldorf")).unwrap();
    std::fs::write(source.join("README.txt"), b"not in the folder").unwrap();
    // (Only ASCII names: the system's `tar` of Windows writes and reads the others by the code page of the console, which is why a gzipped tar is read by the program, and tested above with
    // names that are written as UTF-8.)
    for name in ["computer.fxp", "Cafe.fxp", "Wavetextures Pro Kit.fxp", "plain name.fxp"] {
        std::fs::write(data.join("patches").join("Factory").join(name), name.as_bytes()).unwrap();
    }
    std::fs::write(data.join("wavetables").join("Waldorf").join("MW.wt"), b"may not ship").unwrap();
    std::fs::write(data.join("wavetables").join("Plain.wt"), b"may ship").unwrap();
    let Some(release_file) = a_tar_of_everything_in(work.path(), &source) else {
        return;
    };
    let mut manifest = a_fixture_with_data().manifest;
    {
        let set = &mut manifest.sets[0];
        set.platforms = BTreeMap::from([(
            "linux-x86_64".to_owned(),
            Build {
                url: "https://example.org/content.tar.gz".to_owned(),
                sha256: sha(&release_file),
                size: release_file.len() as u64,
                members: vec![Member {
                    from: "./Some Team/TheData/".to_owned(),
                    to: ".".to_owned(),
                    leave_out: vec!["wavetables/Waldorf/".to_owned()],
                }],
                installer: None,
            },
        )]);
    }
    let mut files = a_fixture().files;
    files.insert("https://example.org/content.tar.gz".to_owned(), release_file);
    let folder = tempfile::tempdir().unwrap();
    manifest.sets.truncate(1);

    let outcome = fetch_plugins(
        &manifest,
        "linux-x86_64",
        folder.path(),
        &FakeFetcher::new(files),
        &mut |_| {},
        &|| false,
    )
    .unwrap_or_else(|error| panic!("{error}"));

    let made = folder.path().join("TheData");
    for name in ["computer.fxp", "Cafe.fxp", "Wavetextures Pro Kit.fxp", "plain name.fxp"] {
        assert_eq!(
            read(&made.join("patches").join("Factory").join(name)),
            name.as_bytes(),
            "{name}"
        );
    }
    assert_eq!(read(&made.join("wavetables").join("Plain.wt")), b"may ship");
    assert!(
        !made.join("wavetables").join("Waldorf").exists(),
        "what is left out is not there"
    );
    assert!(
        !made.join("README.txt").exists(),
        "what is outside the folder is not taken"
    );
    assert!(made.join("NOTICE.txt").is_file() && made.join("LICENSE.txt").is_file());
    assert!(outcome.plugin_files.is_empty());
    assert_eq!(super::tests::names_in(folder.path()), ["TheData"]);
}

// --- a gzipped tar, read by the program itself (the system's `tar` of Windows reads the names of a tar that was made on Linux with the code page of the console) ---

/// One 512-byte header of a tar in the GNU format (the format of the real file of Surge XT): `name` (at most 100 bytes), the kind (`0` a file, `5` a folder, `L` a long name) and the size.
fn a_tar_header(name: &[u8], kind: u8, size: usize) -> [u8; 512] {
    a_tar_header_of_a_format(name, b"", b"ustar  \0", kind, size)
}

/// The same with the bytes that say the format (`ustar  \0` is GNU's, `ustar\000` is POSIX's) and, for a POSIX header, the `prefix` that is the first part of the name.
fn a_tar_header_of_a_format(name: &[u8], prefix: &[u8], magic: &[u8; 8], kind: u8, size: usize) -> [u8; 512] {
    let mut header = [0_u8; 512];
    header[..name.len()].copy_from_slice(name);
    header[345..345 + prefix.len()].copy_from_slice(prefix);
    header[100..108].copy_from_slice(b"0000644\0");
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    header[124..136].copy_from_slice(format!("{size:011o}\0").as_bytes());
    header[136..148].copy_from_slice(b"00000000000\0");
    header[148..156].copy_from_slice(b"        ");
    header[156] = kind;
    header[257..265].copy_from_slice(magic);
    let sum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
    header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    header
}

/// A gzipped tar of the entries `(name, content)`: a name that ends with `/` is a folder (and has no content), and a name longer than 100 bytes is given as GNU tar gives it, in an entry of its
/// own before the file's. **The names are written as the bytes of UTF-8**, as the real file's are.
pub(super) fn a_gnu_tar_gz(entries: &[(&str, Option<&[u8]>)]) -> Vec<u8> {
    use std::io::Write as _;
    let mut tar: Vec<u8> = Vec::new();
    let put = |tar: &mut Vec<u8>, header: [u8; 512], content: &[u8]| {
        tar.extend_from_slice(&header);
        tar.extend_from_slice(content);
        tar.extend(std::iter::repeat_n(0_u8, (512 - content.len() % 512) % 512));
    };
    for (name, content) in entries {
        let bytes = name.as_bytes();
        let kept = &bytes[..bytes.len().min(100)];
        if bytes.len() > 100 {
            let mut long = bytes.to_vec();
            long.push(0);
            put(&mut tar, a_tar_header(b"././@LongLink", b'L', long.len()), &long);
        }
        match content {
            Some(content) => put(&mut tar, a_tar_header(kept, b'0', content.len()), content),
            None => put(&mut tar, a_tar_header(kept, b'5', 0), &[]),
        }
    }
    tar.extend(std::iter::repeat_n(0_u8, 1024));
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gzip.write_all(&tar).unwrap();
    gzip.finish().unwrap()
}

/// A gzipped tar from the bytes of its blocks, then two blocks of nothing.
fn a_gzipped_tar_of(blocks: &[u8]) -> Vec<u8> {
    use std::io::Write as _;
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gzip.write_all(blocks).unwrap();
    gzip.write_all(&[0_u8; 1024]).unwrap();
    gzip.finish().unwrap()
}

/// An entry of a tar: its header, its content, and the padding to a whole block.
fn an_entry(header: [u8; 512], content: &[u8]) -> Vec<u8> {
    let mut entry = header.to_vec();
    entry.extend_from_slice(content);
    entry.extend(std::iter::repeat_n(0_u8, (512 - content.len() % 512) % 512));
    entry
}

/// A manifest of the data set of `a_fixture_with_data` that takes `from` of `release_file` (a gzipped tar, at `https://example.org/content.tar.gz`) for `linux-x86_64`, leaving out `leave_out`.
fn a_manifest_of_data_in_a_tar(release_file: &[u8], from: &str, leave_out: &[&str]) -> Manifest {
    let mut manifest = a_fixture_with_data().manifest;
    manifest.sets.truncate(1);
    manifest.sets[0].platforms = BTreeMap::from([(
        "linux-x86_64".to_owned(),
        Build {
            url: "https://example.org/content.tar.gz".to_owned(),
            sha256: sha(release_file),
            size: release_file.len() as u64,
            members: vec![Member {
                from: from.to_owned(),
                to: ".".to_owned(),
                leave_out: leave_out.iter().map(|path| (*path).to_owned()).collect(),
            }],
            installer: None,
        },
    )]);
    manifest
}

fn fetch_the_tar(manifest: &Manifest, release_file: Vec<u8>, folder: &Path) -> Result<Outcome, String> {
    let mut files = a_fixture().files;
    files.insert("https://example.org/content.tar.gz".to_owned(), release_file);
    fetch_plugins(
        manifest,
        "linux-x86_64",
        folder,
        &FakeFetcher::new(files),
        &mut |_| {},
        &|| false,
    )
}

/// The names of the files below `folder`, each as the path from it with `/` between the parts, in order.
fn files_below(folder: &Path) -> Vec<String> {
    fn walk(folder: &Path, prefix: &str, names: &mut Vec<String>) {
        let mut entries: Vec<_> = std::fs::read_dir(folder).unwrap().flatten().collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
            if entry.path().is_dir() {
                walk(&entry.path(), &format!("{name}/"), names);
            } else {
                names.push(name);
            }
        }
    }
    let mut names = Vec::new();
    walk(folder, "", &mut names);
    names
}

#[test]
fn a_gzipped_tar_is_read_by_the_program_and_the_names_that_are_not_ascii_come_out_as_they_were_written() {
    // (The three names of the real file: a micro sign, an e with an accent, and a trade mark sign. The long one is over 100 bytes, as GNU tar writes the ones that are.)
    let long_name = format!(
        "Some Team/TheData/patches/{}/deep.fxp",
        "a-folder-with-a-long-name".repeat(3)
    );
    assert!(long_name.len() > 100);
    let release_file = a_gnu_tar_gz(&[
        ("./", None),
        ("./README.txt", Some(b"outside the folder")),
        ("./Some Team/", None),
        ("./Some Team/TheData/", None),
        ("./Some Team/TheData/patches/", None),
        ("./Some Team/TheData/patches/Factory/", None),
        ("./Some Team/TheData/patches/Factory/\u{b5}computer.fxp", Some(b"micro")),
        ("./Some Team/TheData/patches/Factory/Caf\u{e9}.fxp", Some(b"accent")),
        (
            "./Some Team/TheData/patches/Factory/Wavetextures\u{2122} Pro Kit.fxp",
            Some(b"trade mark"),
        ),
        ("./Some Team/TheData/patches/Factory/plain name.fxp", Some(b"plain")),
        (&format!("./{long_name}"), Some(b"long")),
        ("./Some Team/TheData/wavetables/Waldorf/MW.wt", Some(b"may not ship")),
        (
            "./Some Team/TheDataMore/other.txt",
            Some(b"a folder that only begins with the name"),
        ),
    ]);
    let manifest = a_manifest_of_data_in_a_tar(&release_file, "Some Team/TheData/", &["wavetables/Waldorf/"]);
    let folder = tempfile::tempdir().unwrap();

    let outcome = fetch_the_tar(&manifest, release_file, folder.path()).unwrap_or_else(|error| panic!("{error}"));

    let made = folder.path().join("TheData");
    let long_relative = long_name.strip_prefix("Some Team/TheData/").unwrap();
    assert_eq!(
        files_below(&made),
        [
            "LICENSE.txt".to_owned(),
            "NOTICE.txt".to_owned(),
            "patches/Factory/Caf\u{e9}.fxp".to_owned(),
            "patches/Factory/Wavetextures\u{2122} Pro Kit.fxp".to_owned(),
            "patches/Factory/plain name.fxp".to_owned(),
            "patches/Factory/\u{b5}computer.fxp".to_owned(),
            long_relative.to_owned(),
        ]
    );
    assert_eq!(
        read(&made.join("patches").join("Factory").join("\u{b5}computer.fxp")),
        b"micro"
    );
    assert_eq!(read(&made.join(long_relative)), b"long");
    assert!(outcome.plugin_files.is_empty());
}

#[test]
fn a_name_that_a_posix_tar_splits_in_a_prefix_and_a_name_and_one_that_a_pax_header_gives_are_read_whole() {
    let mut blocks = Vec::new();
    // (A POSIX header: the folder is in the prefix field and the file in the name field.)
    blocks.extend(an_entry(
        a_tar_header_of_a_format(b"split.fxp", b"Some Team/TheData/patches", b"ustar\x0000", b'0', 5),
        b"split",
    ));
    // (A pax header gives the name of the entry after it, which is as long as the person who wrote the tar wanted: here with a letter that is not ASCII.)
    let name = "Some Team/TheData/patches/Caf\u{e9} by pax.fxp";
    let mut record = format!(" path={name}\n");
    let mut length = record.len() + 1;
    while format!("{length}{record}").len() != length {
        length += 1;
    }
    record = format!("{length}{record}");
    blocks.extend(an_entry(
        a_tar_header_of_a_format(b"./PaxHeaders/x", b"", b"ustar\x0000", b'x', record.len()),
        record.as_bytes(),
    ));
    blocks.extend(an_entry(
        a_tar_header_of_a_format(b"cut short by the writer", b"", b"ustar\x0000", b'0', 3),
        b"pax",
    ));
    // (A global pax header, which says nothing that is wanted, is let by.)
    blocks.extend(an_entry(a_tar_header(b"pax_global_header", b'g', 6), b"6 x=y\n"));
    blocks.extend(an_entry(
        a_tar_header_of_a_format(b"Some Team/TheData/patches/plain.fxp", b"", b"ustar\x0000", b'0', 5),
        b"plain",
    ));
    let release_file = a_gzipped_tar_of(&blocks);
    let manifest = a_manifest_of_data_in_a_tar(&release_file, "Some Team/TheData/", &[]);
    let folder = tempfile::tempdir().unwrap();

    fetch_the_tar(&manifest, release_file, folder.path()).unwrap_or_else(|error| panic!("{error}"));

    let made = folder.path().join("TheData");
    assert_eq!(
        files_below(&made),
        [
            "LICENSE.txt".to_owned(),
            "NOTICE.txt".to_owned(),
            "patches/Caf\u{e9} by pax.fxp".to_owned(),
            "patches/plain.fxp".to_owned(),
            "patches/split.fxp".to_owned(),
        ]
    );
    assert_eq!(read(&made.join("patches").join("split.fxp")), b"split");
    assert_eq!(read(&made.join("patches").join("Caf\u{e9} by pax.fxp")), b"pax");
}

#[test]
fn a_gzip_file_that_is_not_a_tar_is_said_and_so_is_a_file_that_is_not_gzip_at_all_and_nothing_is_left() {
    // (Text, gzipped: the first block does not say `ustar`.)
    let release_file = a_gzipped_tar_of(&[b'x'; 512]);
    let manifest = a_manifest_of_data_in_a_tar(&release_file, "Some Team/TheData/", &[]);
    let folder = tempfile::tempdir().unwrap();

    let error = fetch_the_tar(&manifest, release_file, folder.path()).unwrap_err();

    assert!(error.contains("is not a tar archive"), "{error}");
    assert_eq!(super::tests::names_in(folder.path()), Vec::<String>::new());
}

#[test]
fn a_file_of_a_gzipped_tar_is_taken_by_its_name_with_or_without_dot_slash_and_one_that_is_not_there_is_said() {
    let release_file = a_gnu_tar_gz(&[
        ("./pack/Plugin.clap", Some(b"the plugin")),
        ("pack/Bare.clap", Some(b"bare name")),
        ("./pack/Plugin.clap.old", Some(b"only begins with the name")),
    ]);
    let mut manifest = a_fixture().manifest;
    manifest.sets.truncate(1);
    for (from, to, wanted) in [
        ("pack/Plugin.clap", "Plugin.clap", &b"the plugin"[..]),
        ("pack/Bare.clap", "Bare.clap", &b"bare name"[..]),
    ] {
        manifest.sets[0].platforms = BTreeMap::from([(
            "linux-x86_64".to_owned(),
            Build {
                url: "https://example.org/content.tar.gz".to_owned(),
                sha256: sha(&release_file),
                size: release_file.len() as u64,
                members: vec![Member {
                    from: from.to_owned(),
                    to: to.to_owned(),
                    leave_out: Vec::new(),
                }],
                installer: None,
            },
        )]);
        let folder = tempfile::tempdir().unwrap();

        fetch_the_tar(&manifest, release_file.clone(), folder.path()).unwrap_or_else(|error| panic!("{from}: {error}"));

        assert_eq!(read(&folder.path().join("one").join(to)), wanted, "{from}");
    }
    manifest.sets[0].platforms.get_mut("linux-x86_64").unwrap().members[0].from = "pack/Missing.clap".to_owned();
    let folder = tempfile::tempdir().unwrap();

    let error = fetch_the_tar(&manifest, release_file, folder.path()).unwrap_err();

    assert!(error.contains("has no file called \"pack/Missing.clap\""), "{error}");
    assert_eq!(super::tests::names_in(folder.path()), Vec::<String>::new());
}

#[test]
fn a_folder_that_is_not_in_a_gzipped_tar_is_said_and_so_is_an_entry_that_goes_out_of_the_folder_and_a_tar_that_is_cut_short()
 {
    let release_file = a_gnu_tar_gz(&[("./Some Team/TheData/a.fxp", Some(b"a"))]);
    let manifest = a_manifest_of_data_in_a_tar(&release_file, "Some Team/Missing/", &[]);
    let folder = tempfile::tempdir().unwrap();
    let error = fetch_the_tar(&manifest, release_file, folder.path()).unwrap_err();
    assert!(error.contains("has no folder called \"Some Team/Missing\""), "{error}");
    assert_eq!(super::tests::names_in(folder.path()), Vec::<String>::new());

    let release_file = a_gnu_tar_gz(&[
        ("./Some Team/TheData/a.fxp", Some(b"a")),
        ("./Some Team/TheData/../../escaped.txt", Some(b"must not be written")),
    ]);
    let manifest = a_manifest_of_data_in_a_tar(&release_file, "Some Team/TheData/", &[]);
    // (A folder of its own to look in for what escaped: it would be written two folders up from where the data is being made, which is one above the folder of the fetch.)
    let above = tempfile::tempdir().unwrap();
    let folder = above.path().join("fetched");
    std::fs::create_dir(&folder).unwrap();
    let error = fetch_the_tar(&manifest, release_file, &folder).unwrap_err();
    assert!(error.contains("goes out of the folder"), "{error}");
    assert_eq!(super::tests::names_in(&folder), Vec::<String>::new());
    assert_eq!(
        super::tests::names_in(above.path()),
        ["fetched"],
        "nothing was written above the folder of the fetch"
    );

    let whole = a_gnu_tar_gz(&[("./Some Team/TheData/a.fxp", Some(&[7_u8; 5000][..]))]);
    let cut = whole[..whole.len() / 2].to_vec();
    let manifest = a_manifest_of_data_in_a_tar(&cut, "Some Team/TheData/", &[]);
    let folder = tempfile::tempdir().unwrap();
    let error = fetch_the_tar(&manifest, cut, folder.path()).unwrap_err();
    assert!(error.contains("could not be read"), "{error}");
    assert_eq!(super::tests::names_in(folder.path()), Vec::<String>::new());
}
