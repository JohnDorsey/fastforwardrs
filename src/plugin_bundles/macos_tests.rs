// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The builds for macOS: a CLAP plugin there is a folder (a bundle), taken out of a zip with everything in it and with the files that can be run still runnable, or out of an installer in a
//! disk image (which only macOS can open: here is only what is said where it cannot, and what the manifest says of it; `.github/workflows/ci.yml` fetches the real ones on a Mac of the CI).

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};

use super::test_support::{FakeFetcher, a_fixture, sha};
use super::*;

/// A zip with a bundle (a folder) of a plugin, as the builds for macOS have it, a folder whose name only begins like it, and another plugin.
fn a_zip_with_a_bundle() -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let plain = zip::write::SimpleFileOptions::default();
    let runnable = plain.unix_permissions(0o755);
    writer.add_directory("Reverb.clap/", plain).unwrap();
    writer.add_directory("Reverb.clap/Contents/", plain).unwrap();
    writer.add_directory("Reverb.clap/Contents/MacOS/", plain).unwrap();
    writer
        .start_file("Reverb.clap/Contents/MacOS/Reverb", runnable)
        .unwrap();
    writer.write_all(b"the code").unwrap();
    writer
        .start_file("Reverb.clap/Contents/Info.plist", plain.unix_permissions(0o644))
        .unwrap();
    writer.write_all(b"<plist/>").unwrap();
    writer.start_file("Reverb.clapped/Contents/Other", plain).unwrap();
    writer.write_all(b"a folder whose name only begins like it").unwrap();
    writer.start_file("Other.clap/Contents/MacOS/Other", runnable).unwrap();
    writer.write_all(b"another plugin, not taken").unwrap();
    writer.finish().unwrap().into_inner()
}

/// A manifest of one set that has one build for macOS: `archive` at `url`, taking `member_from` as `Reverb.clap`.
fn a_manifest_with_one_build(
    archive: &[u8],
    url: &str,
    member_from: &str,
    installer: Option<&str>,
) -> (Manifest, HashMap<String, Vec<u8>>) {
    let mut fixture = a_fixture();
    let mut set = fixture.manifest.sets.remove(0);
    set.platforms = BTreeMap::from([(
        "macos-universal".to_owned(),
        Build {
            url: url.to_owned(),
            sha256: sha(archive),
            size: archive.len() as u64,
            members: vec![Member {
                from: member_from.to_owned(),
                to: "Reverb.clap".to_owned(),
            }],
            installer: installer.map(str::to_owned),
        },
    )]);
    fixture.files.insert(url.to_owned(), archive.to_vec());
    (
        Manifest {
            format: 1,
            sets: vec![set],
        },
        fixture.files,
    )
}

fn fetch_for_macos(manifest: &Manifest, files: HashMap<String, Vec<u8>>, folder: &Path) -> Result<Outcome, String> {
    fetch_plugins(
        manifest,
        "macos-universal",
        folder,
        &FakeFetcher::new(files),
        &mut |_| {},
        &|| false,
    )
}

/// Every file and every folder in `folder`, at any depth.
fn walk(folder: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        found.push(path.clone());
        if path.is_dir() {
            found.extend(walk(&path));
        }
    }
    found
}

#[test]
fn a_folder_of_a_zip_is_taken_with_everything_in_it_and_nothing_outside_it_and_what_can_be_run_can_still_be() {
    let (manifest, files) = a_manifest_with_one_build(
        &a_zip_with_a_bundle(),
        "https://example.org/mac.zip",
        "Reverb.clap/",
        None,
    );
    let folder = tempfile::tempdir().unwrap();

    fetch_for_macos(&manifest, files, folder.path()).unwrap();

    let bundle = folder.path().join("one").join("Reverb.clap");
    assert_eq!(
        std::fs::read(bundle.join("Contents").join("MacOS").join("Reverb")).unwrap(),
        b"the code"
    );
    assert_eq!(
        std::fs::read(bundle.join("Contents").join("Info.plist")).unwrap(),
        b"<plist/>"
    );
    let mut found: Vec<String> = walk(&bundle)
        .iter()
        .map(|entry| {
            entry
                .strip_prefix(&bundle)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    found.sort();
    assert_eq!(
        found,
        [
            "Contents",
            "Contents/Info.plist",
            "Contents/MacOS",
            "Contents/MacOS/Reverb"
        ],
        "only what is in the folder (not `Reverb.clapped`, not `Other.clap`), and no stray folder"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode(&bundle.join("Contents").join("MacOS").join("Reverb")),
            0o755,
            "the code can be run"
        );
        assert_eq!(
            mode(&bundle.join("Contents").join("Info.plist")),
            0o644,
            "and what is not code is not made runnable"
        );
    }
    assert!(
        is_fetched(&manifest.sets[0], "macos-universal", folder.path()),
        "a folder counts as the plugin being there"
    );
}

#[test]
fn a_folder_that_is_not_in_the_zip_is_said_and_an_entry_that_goes_out_of_the_folder_is_refused() {
    let (manifest, files) = a_manifest_with_one_build(
        &a_zip_with_a_bundle(),
        "https://example.org/mac.zip",
        "Missing.clap/",
        None,
    );
    let folder = tempfile::tempdir().unwrap();
    let error = fetch_for_macos(&manifest, files, folder.path()).unwrap_err();
    assert!(error.contains("has no folder called \"Missing.clap\""), "{error}");
    assert!(!folder.path().join("one").exists());

    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer
        .start_file("Reverb.clap/../../escaped", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"outside").unwrap();
    let bad = writer.finish().unwrap().into_inner();
    let (manifest, files) = a_manifest_with_one_build(&bad, "https://example.org/bad.zip", "Reverb.clap/", None);

    let error = fetch_for_macos(&manifest, files, folder.path()).unwrap_err();

    assert!(error.contains("goes out of the folder"), "{error}");
    assert!(!folder.path().parent().unwrap().join("escaped").exists());
}

#[test]
fn an_installer_is_only_a_plain_pkg_name_in_a_disk_image() {
    let problems = |installer: Option<&str>, url: &str| {
        let (manifest, _) = a_manifest_with_one_build(b"x", url, "Reverb.clap", installer);
        manifest.problems().join("\n")
    };
    assert_eq!(problems(Some("Plugins.pkg"), "https://example.org/a.dmg"), "");
    assert_eq!(problems(None, "https://example.org/a.zip"), "");
    for (installer, url) in [
        (Some("Plugins.pkg"), "https://example.org/a.zip"),
        (Some("../Plugins.pkg"), "https://example.org/a.dmg"),
        (Some("sub/Plugins.pkg"), "https://example.org/a.dmg"),
        (Some("Plugins.dmg"), "https://example.org/a.dmg"),
        (Some(""), "https://example.org/a.dmg"),
    ] {
        assert!(
            problems(installer, url).contains("an installer is a plain name of a .pkg file in a .dmg"),
            "{installer:?} {url}"
        );
    }
}

#[cfg(not(target_os = "macos"))]
#[test]
fn a_disk_image_where_the_system_cannot_open_it_is_said_in_words_and_nothing_is_left() {
    let (manifest, files) = a_manifest_with_one_build(
        b"not really a disk image",
        "https://example.org/a.dmg",
        "x.clap",
        Some("Plugins.pkg"),
    );
    let folder = tempfile::tempdir().unwrap();

    let error = fetch_for_macos(&manifest, files, folder.path()).unwrap_err();

    assert!(error.contains("hdiutil"), "{error}");
    assert!(!folder.path().join("one").exists() && !folder.path().join(".downloading").exists());
}

#[test]
fn macos_has_a_platform_name() {
    if cfg!(target_os = "macos") {
        assert_eq!(this_platform(), Some("macos-universal"));
    }
}
