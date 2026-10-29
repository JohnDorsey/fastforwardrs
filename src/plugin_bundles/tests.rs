// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the fetching of plugins: the manifest and its policy, the download with archives made on the spot (nothing is downloaded from the network), the checksums, a download that
//! fails or is cancelled, and the system's `curl` against a server that is run here.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::test_support::{FakeFetcher, Fixture, a_fixture, a_zip};
use super::*;

// --- the manifest ---

#[test]
fn the_manifest_in_the_program_is_read_and_is_fit_and_has_a_build_for_this_system_for_every_set() {
    let manifest = Manifest::embedded();

    assert_eq!(manifest.problems(), Vec::<String>::new());
    let plugin_sets = manifest.only(SetKind::Plugins);
    let names: Vec<&str> = plugin_sets
        .sets
        .iter()
        .flat_map(|set| set.plugins.iter().map(String::as_str))
        .collect();
    assert_eq!(
        names,
        [
            "Dragonfly Hall Reverb",
            "Dragonfly Plate Reverb",
            "Dragonfly Room Reverb",
            "MVerb",
            "MaGigaverb",
            "Surge XT Effects",
            "Surge XT",
            "Kars",
            "Nekobi",
            "Dragonfly Early Reflections",
            "Dragonfly Hall Reverb",
            "Dragonfly Plate Reverb",
            "Dragonfly Room Reverb",
            "MVerb",
            "3BandEQ",
            "PingPongPan",
            "MaBitcrush",
            "MaFreeverb",
            "MaGigaverb",
            "MaPitchshift",
            "CycleShifter",
            "Kars",
            "Nekobi",
            "Wolf Shaper"
        ]
    );
    // (Every set has a build for each system, except Wolf Shaper, whose file for macOS is a zip that holds a disk image: one more layer than the program opens.)
    for set in &manifest.only(SetKind::Plugins).sets {
        let is_lv2 = set.id.ends_with("-lv2");
        for platform in ["windows-x86_64", "linux-x86_64"] {
            assert!(set.platforms.contains_key(platform), "{} for {platform}", set.id);
        }
        assert_eq!(
            set.platforms.contains_key("macos-universal"),
            set.id != "wolf-shaper-lv2",
            "{}",
            set.id
        );
        for build in set.platforms.values() {
            assert_eq!(build.members.len(), set.plugins.len(), "{}", set.id);
            for member in &build.members {
                assert_eq!(member.to.ends_with(".lv2"), is_lv2, "{}: {}", set.id, member.to);
            }
        }
    }
}

#[test]
fn the_data_of_surge_xt_in_the_manifest_is_one_folder_for_windows_and_linux_without_what_may_not_ship() {
    let manifest = Manifest::embedded();

    let data = manifest.only(SetKind::Data);

    assert_eq!(data.sets.len(), 1);
    let set = &data.sets[0];
    assert_eq!(
        (
            set.id.as_str(),
            set.folder_name(),
            set.licence.as_str(),
            set.version.as_str()
        ),
        ("surge-xt-data", "SurgeXTData", "GPL-3.0-only", "1.3.4")
    );
    // (Windows and Linux: on macOS Surge XT looks for its data only in the folders of the person's own Library, which the program does not write in.)
    assert_eq!(
        set.platforms.keys().map(String::as_str).collect::<Vec<_>>(),
        ["linux-x86_64", "windows-x86_64"]
    );
    assert_eq!(
        set.platforms["linux-x86_64"], set.platforms["windows-x86_64"],
        "one release file"
    );
    let build = &set.platforms["windows-x86_64"];
    assert!(build.url.ends_with("/surge-xt-portable-content-1.3.4.tar.gz"));
    assert_eq!(build.members.len(), 1);
    assert_eq!(
        (build.members[0].from.as_str(), build.members[0].to.as_str()),
        ("Surge Synth Team/SurgeXTData/", ".")
    );
    // (The wavetables that are named like the Waldorf Microwave's, and the skin whose fonts are under the OFL, which is not a licence that may ship here.)
    assert_eq!(
        build.members[0].leave_out,
        ["wavetables/Waldorf/", "skins/Tutorials/10 Adding Fonts.surge-skin/"]
    );
    assert_eq!(
        set.licence_texts
            .iter()
            .map(|text| text.name.as_str())
            .collect::<Vec<_>>(),
        ["LICENSE.txt", "AUTHORS.txt"]
    );
    let note = set.licence_note.as_deref().unwrap();
    for wanted in ["issue 6741", "CC0", "wavetables/Waldorf", "OFL"] {
        assert!(note.contains(wanted), "{wanted}: {note}");
    }
    // (Its folder is the one that the plugins' folders are beside, and no set of plugins has that name.)
    assert!(
        manifest
            .only(SetKind::Plugins)
            .sets
            .iter()
            .all(|set| set.folder_name() != "SurgeXTData")
    );
}

#[test]
fn the_manifest_in_the_program_is_the_one_of_the_files_of_the_project() {
    let file = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("bundled-plugins")
            .join("manifest.json"),
    )
    .unwrap();
    assert_eq!(
        EMBEDDED_MANIFEST, file,
        "the program is built from the manifest that the script reads"
    );
}

#[test]
fn gpl_and_lgpl_that_are_compatible_and_the_permissive_licences_may_ship_and_agpl_and_gpl_2_only_may_not() {
    for allowed in [
        "GPL-3.0-or-later",
        "GPL-3.0-only",
        "GPL-2.0-or-later",
        "LGPL-3.0-only",
        "LGPL-3.0-or-later",
        "LGPL-2.1-or-later",
        "MIT",
        "ISC",
        "BSD-3-Clause",
        "CC0-1.0",
        "MIT AND GPL-3.0-or-later",
        "GPL-3.0-or-later OR MIT",
        "AGPL-3.0-only OR GPL-3.0-or-later",
        "LGPL-3.0-only AND ISC",
    ] {
        assert!(licence_is_allowed(allowed), "{allowed}");
    }
    for refused in [
        "AGPL-3.0-only",
        "AGPL-3.0-or-later",
        "GPL-2.0-only",
        "GPL-2.0",
        "LGPL-2.1-only",
        "CC-BY-NC-4.0",
        "Proprietary",
        "",
        "MIT AND AGPL-3.0-only",
        "(MIT)",
        "GPL-3.0",
    ] {
        assert!(!licence_is_allowed(refused), "{refused}");
    }
}

fn a_manifest_that_is_wrong_in(change: impl FnOnce(&mut Manifest)) -> String {
    let mut manifest = Manifest::embedded();
    change(&mut manifest);
    manifest.problems().join("\n")
}

#[test]
fn what_is_wrong_with_a_manifest_is_said_for_each_kind_of_fault() {
    assert!(
        a_manifest_that_is_wrong_in(|m| m.sets[0].licence = "AGPL-3.0-or-later".into())
            .contains("is not one that may ship")
    );
    assert!(
        a_manifest_that_is_wrong_in(|m| m.sets[0].licence = "GPL-2.0-only".into()).contains("is not one that may ship")
    );
    let problems = a_manifest_that_is_wrong_in(|m| {
        m.sets[0].homepage = "http://example.org".into();
        m.sets[0].source.url = "ftp://example.org/source".into();
        m.sets[0].licence_texts[0].url = "http://example.org/LICENSE".into();
        m.sets[0].platforms.get_mut("windows-x86_64").unwrap().url = "http://example.org/a.zip".into();
    });
    for wanted in [
        "the homepage is not an https address",
        "the source has no https address",
        "a licence text needs",
        "needs an https address and a sha256",
    ] {
        assert!(problems.contains(wanted), "{wanted} is not in {problems}");
    }
    assert!(
        a_manifest_that_is_wrong_in(|m| m.sets[0].platforms.get_mut("linux-x86_64").unwrap().sha256 = "abc".into())
            .contains("needs an https address and a sha256")
    );
    assert!(a_manifest_that_is_wrong_in(|m| m.format = 2).contains("the format of the manifest is 2"));
    assert!(a_manifest_that_is_wrong_in(|m| m.sets.clear()).contains("no sets"));
    assert!(a_manifest_that_is_wrong_in(|m| m.sets[1].id = m.sets[0].id.clone()).contains("the id is used twice"));
    assert!(a_manifest_that_is_wrong_in(|m| m.sets[0].id = "Has Capitals".into()).contains("the id is not"));
    assert!(
        a_manifest_that_is_wrong_in(|m| {
            let build = m.sets[0].platforms["linux-x86_64"].clone();
            m.sets[0].platforms.insert("amiga".into(), build);
        })
        .contains("not a platform that is known")
    );
    assert!(a_manifest_that_is_wrong_in(|m| m.sets[0].plugins.clear()).contains("are needed"));
    assert!(a_manifest_that_is_wrong_in(|m| m.sets[0].licence_texts.clear()).contains("no licence text"));
}

#[test]
fn a_file_that_would_go_outside_the_folder_or_is_not_a_clap_file_or_is_taken_twice_is_said() {
    for name in ["../Escape.clap", "a/b.clap", "a\\b.clap", "", ".."] {
        let problems = a_manifest_that_is_wrong_in(|m| {
            m.sets[0].platforms.get_mut("windows-x86_64").unwrap().members[0].to = name.into()
        });
        assert!(
            problems.contains("is not a plain name of a .clap file"),
            "{name:?}: {problems}"
        );
    }
    for name in [
        "../secret",
        "pack/../../secret",
        "pack\\..\\secret",
        "/etc/passwd",
        "\\windows\\x",
        "",
    ] {
        let problems = a_manifest_that_is_wrong_in(|m| {
            m.sets[0].platforms.get_mut("windows-x86_64").unwrap().members[0].from = name.into()
        });
        assert!(
            problems.contains("is not a name inside the archive"),
            "{name:?}: {problems}"
        );
    }
    let problems = a_manifest_that_is_wrong_in(|m| {
        let members = &mut m.sets[0].platforms.get_mut("windows-x86_64").unwrap().members;
        members[1].to = members[0].to.clone();
        members[2].to = "library.dll".into();
    });
    assert!(problems.contains("is taken twice"), "{problems}");
    assert!(
        problems.contains("\"library.dll\" is not a plain name of a .clap file"),
        "{problems}"
    );
}

// --- what a download needs, with archives made here ---

fn fetch(fixture: &Fixture, fetcher: &FakeFetcher, folder: &Path) -> Result<Outcome, String> {
    fetch_plugins(
        &fixture.manifest,
        "windows-x86_64",
        folder,
        fetcher,
        &mut |_| {},
        &|| fetcher.cancelled.load(Ordering::SeqCst),
    )
}

pub(super) fn names_in(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .map(|entries| {
            entries
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn only_the_files_the_manifest_names_are_taken_with_the_licence_and_a_notice_and_nothing_else_is_left() {
    let fixture = a_fixture();
    let fetcher = FakeFetcher::new(fixture.files.clone());
    let folder = tempfile::tempdir().unwrap();

    let outcome = fetch(&fixture, &fetcher, folder.path()).unwrap();

    assert_eq!(
        outcome.sets,
        ["ONE", "TWO", "THREE"],
        "the set that has no build for this system is left out"
    );
    assert_eq!(
        outcome.plugin_files,
        [
            folder.path().join("one").join("Reverb.clap"),
            folder.path().join("two").join("First.clap"),
            folder.path().join("two").join("Second.clap"),
            folder.path().join("three").join("Third.clap"),
        ]
    );
    assert_eq!(
        names_in(folder.path()),
        ["one", "three", "two"],
        "no temporary folder and no half made one is left"
    );
    assert_eq!(
        names_in(&folder.path().join("one")),
        ["LICENSE.txt", "NOTICE.txt", "Reverb.clap"]
    );
    assert_eq!(
        std::fs::read(folder.path().join("one").join("Reverb.clap")).unwrap(),
        b"the reverb"
    );
    assert_eq!(
        std::fs::read(folder.path().join("two").join("Second.clap")).unwrap(),
        b"second"
    );
    assert_eq!(
        std::fs::read(folder.path().join("one").join("LICENSE.txt")).unwrap(),
        b"GNU GENERAL PUBLIC LICENSE, a copy"
    );
    let notice = std::fs::read_to_string(folder.path().join("one").join("NOTICE.txt")).unwrap();
    for wanted in [
        "ONE 1.0",
        "By Someone.",
        "Licence: GPL-3.0-or-later",
        "https://example.org/",
        "unaltered",
        "The corresponding source of this version is at: https://example.org/one-source.tar.gz",
        "a note about the source",
    ] {
        assert!(notice.contains(wanted), "{wanted} is not in {notice}");
    }
    assert!(is_fetched(&fixture.manifest.sets[0], "windows-x86_64", folder.path()));
    assert!(
        !is_fetched(&fixture.manifest.sets[3], "windows-x86_64", folder.path()),
        "no build for it"
    );
}

#[test]
fn a_release_file_that_two_sets_are_in_is_downloaded_once_and_the_licence_texts_are_downloaded_for_each_set() {
    let fixture = a_fixture();
    let fetcher = FakeFetcher::new(fixture.files.clone());
    let folder = tempfile::tempdir().unwrap();

    fetch(&fixture, &fetcher, folder.path()).unwrap();

    let asked = fetcher.asked();
    assert_eq!(
        asked.iter().filter(|url| url.ends_with("two.zip")).count(),
        1,
        "{asked:?}"
    );
    assert_eq!(
        asked.iter().filter(|url| url.ends_with("one.zip")).count(),
        1,
        "{asked:?}"
    );
    assert_eq!(
        asked.iter().filter(|url| url.ends_with("LICENSE")).count(),
        3,
        "{asked:?}"
    );
    assert_eq!(
        bytes_to_download(&fixture.manifest, "windows-x86_64"),
        fixture.manifest.sets[0].platforms["windows-x86_64"].size
            + fixture.manifest.sets[1].platforms["windows-x86_64"].size,
        "the file of two sets counts once"
    );
    assert_eq!(
        bytes_to_download(&fixture.manifest, "linux-x86_64"),
        fixture.manifest.sets[3].platforms["linux-x86_64"].size
    );
    assert_eq!(bytes_to_download(&fixture.manifest, "amiga"), 0);
}

#[test]
fn the_progress_names_each_step_in_order_and_ends_at_the_whole() {
    let fixture = a_fixture();
    let fetcher = FakeFetcher::new(fixture.files.clone());
    let folder = tempfile::tempdir().unwrap();
    let mut reports: Vec<Progress> = Vec::new();

    fetch_plugins(
        &fixture.manifest,
        "windows-x86_64",
        folder.path(),
        &fetcher,
        &mut |progress| reports.push(progress.clone()),
        &|| false,
    )
    .unwrap();

    let steps: Vec<&str> = reports.iter().map(|progress| progress.step.as_str()).collect();
    let position = |text: &str| {
        steps
            .iter()
            .position(|step| *step == text)
            .unwrap_or_else(|| panic!("{text} is not in {steps:?}"))
    };
    assert!(position("Downloading ONE (1 of 3)") < position("Checking ONE (1 of 3)"));
    assert!(position("Checking ONE (1 of 3)") < position("Unpacking ONE (1 of 3)"));
    assert!(position("Unpacking ONE (1 of 3)") < position("Downloading TWO (2 of 3)"));
    assert!(steps.contains(&"Unpacking THREE (3 of 3)"));
    assert!(
        !steps.contains(&"Downloading THREE (3 of 3)"),
        "its file was downloaded for the set before"
    );
    let last = reports.last().unwrap();
    assert_eq!((last.step.as_str(), last.bytes_done), ("Done", last.bytes_total));
    assert_eq!(last.bytes_total, bytes_to_download(&fixture.manifest, "windows-x86_64"));
    assert!(
        reports.windows(2).all(|pair| pair[0].bytes_done <= pair[1].bytes_done),
        "the count never goes back"
    );
}

#[test]
fn a_release_file_whose_checksum_is_not_the_one_said_is_refused_and_nothing_of_it_is_used() {
    let mut fixture = a_fixture();
    fixture.files.insert(
        "https://example.org/two.zip".to_owned(),
        a_zip(&[("pack/First.clap", b"a different file")]),
    );
    let fetcher = FakeFetcher::new(fixture.files.clone());
    let folder = tempfile::tempdir().unwrap();

    let error = fetch(&fixture, &fetcher, folder.path()).unwrap_err();

    assert!(error.contains("is not the file that was expected"), "{error}");
    assert!(error.contains("two.zip"), "{error}");
    assert!(!folder.path().join("two").exists());
    assert!(!folder.path().join("two.building").exists());
    assert!(
        !folder.path().join(".downloading").exists(),
        "the temporary folder goes in every case"
    );
}

#[test]
fn a_licence_text_whose_checksum_is_wrong_refuses_the_set_and_the_set_that_was_there_before_is_kept() {
    let fixture = a_fixture();
    let folder = tempfile::tempdir().unwrap();
    fetch(&fixture, &FakeFetcher::new(fixture.files.clone()), folder.path()).unwrap();
    std::fs::write(folder.path().join("one").join("Reverb.clap"), b"as it was before").unwrap();
    let mut files = fixture.files.clone();
    files.insert("https://example.org/LICENSE".to_owned(), b"another text".to_vec());

    let error = fetch(&fixture, &FakeFetcher::new(files), folder.path()).unwrap_err();

    assert!(error.contains("LICENSE"), "{error}");
    assert_eq!(
        std::fs::read(folder.path().join("one").join("Reverb.clap")).unwrap(),
        b"as it was before",
        "what was there is as it was"
    );
    assert!(!folder.path().join("one.building").exists());
}

#[test]
fn a_file_that_is_not_in_the_archive_is_said() {
    let mut fixture = a_fixture();
    fixture.manifest.sets[0]
        .platforms
        .get_mut("windows-x86_64")
        .unwrap()
        .members[0]
        .from = "one-1.0/Missing.clap".to_owned();
    let folder = tempfile::tempdir().unwrap();

    let error = fetch(&fixture, &FakeFetcher::new(fixture.files.clone()), folder.path()).unwrap_err();

    assert!(error.contains("has no file called \"one-1.0/Missing.clap\""), "{error}");
    assert!(!folder.path().join("one").exists());
}

#[test]
fn a_download_that_is_cancelled_stops_and_leaves_what_was_there_and_no_temporary_folder() {
    let fixture = a_fixture();
    let folder = tempfile::tempdir().unwrap();
    fetch(&fixture, &FakeFetcher::new(fixture.files.clone()), folder.path()).unwrap();
    std::fs::write(folder.path().join("two").join("First.clap"), b"as it was before").unwrap();
    let mut cancelling = FakeFetcher::new(fixture.files.clone());
    cancelling.cancel_after = Some(2);

    let error = fetch(&fixture, &cancelling, folder.path()).unwrap_err();

    assert_eq!(error, CANCELLED);
    assert_eq!(
        std::fs::read(folder.path().join("two").join("First.clap")).unwrap(),
        b"as it was before"
    );
    assert_eq!(names_in(folder.path()), ["one", "three", "two"]);
}

#[test]
fn a_cancel_that_is_said_before_a_set_is_begun_stops_it_there() {
    let fixture = a_fixture();
    let fetcher = FakeFetcher::new(fixture.files.clone());
    let folder = tempfile::tempdir().unwrap();

    let result = fetch_plugins(
        &fixture.manifest,
        "windows-x86_64",
        folder.path(),
        &fetcher,
        &mut |_| {},
        &|| true,
    );

    assert_eq!(result.unwrap_err(), CANCELLED);
    assert!(fetcher.asked().is_empty(), "nothing was asked for");
}

#[test]
fn a_manifest_that_is_not_fit_is_refused_before_anything_is_downloaded_and_so_is_a_system_with_no_build() {
    let mut fixture = a_fixture();
    fixture.manifest.sets[0].licence = "AGPL-3.0-only".to_owned();
    let fetcher = FakeFetcher::new(fixture.files.clone());
    let folder = tempfile::tempdir().unwrap();

    let error = fetch(&fixture, &fetcher, folder.path()).unwrap_err();

    assert!(
        error.contains("is not fit to be used") && error.contains("may ship"),
        "{error}"
    );
    assert!(fetcher.asked().is_empty());
    let fixture = a_fixture();
    let error = fetch_plugins(
        &fixture.manifest,
        "amiga",
        folder.path(),
        &fetcher,
        &mut |_| {},
        &|| false,
    )
    .unwrap_err();
    assert!(error.contains("no plugins to fetch for amiga"), "{error}");
}

#[test]
fn a_set_is_replaced_by_a_new_fetch_and_a_file_that_is_left_of_the_old_one_does_not_stay() {
    let fixture = a_fixture();
    let folder = tempfile::tempdir().unwrap();
    fetch(&fixture, &FakeFetcher::new(fixture.files.clone()), folder.path()).unwrap();
    std::fs::write(folder.path().join("one").join("Stale.clap"), b"left from before").unwrap();

    fetch(&fixture, &FakeFetcher::new(fixture.files.clone()), folder.path()).unwrap();

    assert!(!folder.path().join("one").join("Stale.clap").exists());
    assert!(folder.path().join("one").join("Reverb.clap").exists());
}

#[test]
fn the_notice_names_the_authors_the_licence_with_its_note_the_home_page_and_where_the_source_is() {
    let mut set = a_fixture().manifest.sets.remove(0);
    set.authors = vec!["A".to_owned(), "B".to_owned()];
    set.licence_note = Some("the wrapper is ISC".to_owned());
    set.source.note = None;

    let notice = notice_of(&set);

    assert!(notice.contains("By A, B."), "{notice}");
    assert!(
        notice.contains("Licence: GPL-3.0-or-later (the wrapper is ISC)"),
        "{notice}"
    );
    assert!(notice.contains("Home page: https://example.org/"), "{notice}");
    assert!(notice.ends_with("one-source.tar.gz\n"), "{notice}");
}

#[test]
fn the_folder_of_fetched_plugins_is_in_the_data_folder_of_the_person() {
    if let Some(folder) = fetched_plugins_folder() {
        assert!(
            folder.ends_with(Path::new("fastforwardrs").join("plugins")),
            "{folder:?}"
        );
    }
}

// --- tar archives (the builds for Linux) ---

#[cfg(unix)]
#[test]
fn a_tar_archive_gives_its_files_with_the_names_inside_it_with_or_without_dot_slash() {
    if std::process::Command::new("tar").arg("--version").output().is_err() {
        eprintln!("SKIPPED: there is no tar");
        return;
    }
    let work = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(work.path().join("src").join("pack")).unwrap();
    std::fs::write(work.path().join("src").join("pack").join("A.clap"), b"the first").unwrap();
    std::fs::write(work.path().join("src").join("Top.clap"), b"on the top").unwrap();
    let archive = work.path().join("things.tar.gz");
    let made = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(work.path().join("src"))
        .arg("./pack/A.clap")
        .arg("./Top.clap")
        .status()
        .unwrap();
    assert!(made.success());
    let release_file = std::fs::read(&archive).unwrap();
    let mut set = a_fixture().manifest.sets.remove(0);
    set.platforms = std::collections::BTreeMap::from([(
        "linux-x86_64".to_owned(),
        Build {
            url: "https://example.org/things.tar.gz".to_owned(),
            sha256: super::test_support::sha(&release_file),
            size: release_file.len() as u64,
            members: vec![
                Member {
                    from: "pack/A.clap".to_owned(),
                    to: "A.clap".to_owned(),
                    leave_out: Vec::new(),
                },
                Member {
                    from: "Top.clap".to_owned(),
                    to: "Top.clap".to_owned(),
                    leave_out: Vec::new(),
                },
            ],
            installer: None,
        },
    )]);
    let mut files = a_fixture().files;
    files.insert("https://example.org/things.tar.gz".to_owned(), release_file);
    let folder = tempfile::tempdir().unwrap();

    let outcome = fetch_plugins(
        &Manifest {
            format: 1,
            sets: vec![set],
        },
        "linux-x86_64",
        folder.path(),
        &FakeFetcher::new(files),
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    assert_eq!(outcome.plugin_files.len(), 2);
    assert_eq!(
        std::fs::read(folder.path().join("one").join("A.clap")).unwrap(),
        b"the first"
    );
    assert_eq!(
        std::fs::read(folder.path().join("one").join("Top.clap")).unwrap(),
        b"on the top"
    );
}

// --- the system's curl ---

fn curl_is_there() -> bool {
    std::process::Command::new("curl")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Serves `/ok` (with `body`), `/slow` (headers, then nothing for a long time) and a 404 for the rest, to as many as ask, from a thread. Gives the address.
fn a_server_here(body: &'static [u8]) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for connection in listener.incoming() {
            let Ok(mut stream) = connection else { continue };
            std::thread::spawn(move || {
                let mut request = [0_u8; 2048];
                let count = stream.read(&mut request).unwrap_or(0);
                let first_line = String::from_utf8_lossy(&request[..count])
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_owned();
                if first_line.contains("/ok") {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(body);
                } else if first_line.contains("/slow") {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\nConnection: close\r\n\r\n"
                    );
                    let _ = stream.flush();
                    std::thread::sleep(Duration::from_secs(30));
                } else {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    );
                }
            });
        }
    });
    format!("http://{address}")
}

#[test]
fn curl_downloads_the_file_and_says_how_many_bytes_there_are_and_a_missing_one_is_an_error_with_the_address() {
    if !curl_is_there() {
        eprintln!("SKIPPED: there is no curl");
        return;
    }
    let server = a_server_here(b"the plugin, in bytes");
    let folder = tempfile::tempdir().unwrap();
    let to = folder.path().join("got");
    let mut last = 0;

    Curl::for_a_test_on_this_computer()
        .fetch(&format!("{server}/ok"), &to, &mut |bytes| last = bytes, &|| false)
        .unwrap();

    assert_eq!(std::fs::read(&to).unwrap(), b"the plugin, in bytes");
    assert_eq!(last, 20);
    let error = Curl::for_a_test_on_this_computer()
        .fetch(
            &format!("{server}/nothing"),
            &folder.path().join("other"),
            &mut |_| {},
            &|| false,
        )
        .unwrap_err();
    assert!(
        error.contains("/nothing") && error.contains("could not be downloaded"),
        "{error}"
    );
    assert!(
        !folder.path().join("other").exists(),
        "no file is left of a download that failed"
    );
}

#[test]
fn the_curl_that_is_used_for_real_refuses_an_address_that_is_not_https() {
    if !curl_is_there() {
        eprintln!("SKIPPED: there is no curl");
        return;
    }
    let server = a_server_here(b"never given");
    let folder = tempfile::tempdir().unwrap();

    let error = Curl::https_only()
        .fetch(
            &format!("{server}/ok"),
            &folder.path().join("got"),
            &mut |_| {},
            &|| false,
        )
        .unwrap_err();

    assert!(error.contains("could not be downloaded"), "{error}");
    assert!(
        !folder.path().join("got").exists(),
        "nothing came of an address that is not https"
    );
}

#[test]
fn a_download_that_is_cancelled_while_it_goes_on_is_stopped_at_once_and_leaves_no_file() {
    if !curl_is_there() {
        eprintln!("SKIPPED: there is no curl");
        return;
    }
    let server = a_server_here(b"");
    let folder = tempfile::tempdir().unwrap();
    let to = folder.path().join("got");
    let began = Instant::now();
    let polls = AtomicUsize::new(0);

    let error = Curl::for_a_test_on_this_computer()
        .fetch(&format!("{server}/slow"), &to, &mut |_| {}, &|| {
            polls.fetch_add(1, Ordering::SeqCst) >= 3
        })
        .unwrap_err();

    assert_eq!(error, CANCELLED);
    assert!(began.elapsed() < Duration::from_secs(10), "{:?}", began.elapsed());
    assert!(!to.exists());
}

// --- the real network (by hand) ---

/// **Fetches the plugins of the manifest from the network, with the system's `curl`**, into a folder of its own: about 140 MB on Windows, so it is ignored unless asked for
/// (`cargo test --lib fetch_the_real_plugins -- --ignored --nocapture`). Every checksum of the manifest is checked by the download itself.
#[test]
#[ignore = "downloads about 140 MB from the network"]
fn fetch_the_real_plugins_from_the_network_and_find_every_file_the_manifest_names() {
    let Some(platform) = this_platform() else {
        eprintln!("SKIPPED: no builds for this system");
        return;
    };
    let manifest = Manifest::embedded();
    // (`FFRS_FETCH_INTO` names a folder to fetch into and keep, for the step of the CI that then scans what was fetched; without it a folder is made and is not removed either.)
    let kept = std::env::var_os("FFRS_FETCH_INTO").map(PathBuf::from);
    let made = kept.is_none().then(|| tempfile::tempdir().unwrap());
    let folder: &Path = kept
        .as_deref()
        .or_else(|| made.as_ref().map(tempfile::TempDir::path))
        .unwrap();
    let began = Instant::now();
    let mut last_step = String::new();

    let outcome = fetch_plugins(
        &manifest,
        platform,
        folder,
        &Curl::https_only(),
        &mut |progress| {
            if progress.step != last_step {
                eprintln!(
                    "{:>6.1?}  {} ({} of {} bytes)",
                    began.elapsed(),
                    progress.step,
                    progress.bytes_done,
                    progress.bytes_total
                );
                last_step = progress.step.clone();
            }
        },
        &|| false,
    )
    .expect("the plugins are fetched");

    // (A set that has no build for this system is not fetched: the LV2 sets have none for macOS yet.)
    let sets_with_a_build: Vec<&BundleSet> = manifest
        .sets
        .iter()
        .filter(|set| set.platforms.contains_key(platform))
        .collect();
    assert_eq!(outcome.sets.len(), sets_with_a_build.len());
    for set in sets_with_a_build {
        for member in &set.platforms[platform].members {
            let file = folder.join(set.folder_name()).join(&member.to);
            if set.is_data() {
                // (The data of Surge XT: the patches and the wavetables are most of its 378 MB.)
                assert!(size_of_a_file_or_a_folder(&file) > 300_000_000, "{file:?}");
            } else if member.to.ends_with(".lv2") {
                assert!(file.join("manifest.ttl").is_file(), "{file:?} is a bundle");
                assert!(size_of_a_file_or_a_folder(&file) > 10_000, "{file:?}");
            } else {
                assert!(size_of_a_file_or_a_folder(&file) > 100_000, "{file:?}");
            }
        }
        assert!(folder.join(set.folder_name()).join("NOTICE.txt").is_file());
        for text in &set.licence_texts {
            assert!(folder.join(set.folder_name()).join(&text.name).is_file());
        }
        assert!(is_fetched(set, platform, folder));
    }
    // The folder that was made is the one a release makes with the script: the plugins are found by the scan program and start steady (see `tests/bundled_plugins_real.rs`).
    eprintln!("fetched into {folder:?} in {:?}", began.elapsed());
    std::mem::forget(made);
}

/// The bytes of a file, or of every file in a folder (a plugin of macOS is a folder).
fn size_of_a_file_or_a_folder(path: &Path) -> u64 {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => std::fs::read_dir(path)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| size_of_a_file_or_a_folder(&entry.path()))
                    .sum()
            })
            .unwrap_or(0),
        Ok(metadata) => metadata.len(),
        Err(_) => 0,
    }
}

#[test]
fn a_set_with_only_some_of_its_plugins_in_its_folder_is_not_fetched() {
    let fixture = a_fixture();
    let folder = tempfile::tempdir().unwrap();
    let two = &fixture.manifest.sets[1];
    assert!(!is_fetched(two, "windows-x86_64", folder.path()), "nothing is there");
    std::fs::create_dir_all(folder.path().join("two")).unwrap();
    std::fs::write(folder.path().join("two").join("First.clap"), b"x").unwrap();

    assert!(
        !is_fetched(two, "windows-x86_64", folder.path()),
        "one of two is not the set"
    );

    std::fs::write(folder.path().join("two").join("Second.clap"), b"x").unwrap();
    assert!(is_fetched(two, "windows-x86_64", folder.path()));
}

// --- LV2 bundles: a member that is a folder ---

/// A fixture of one set whose plugin is an LV2 bundle in a zip (with files beside it that are not taken).
fn a_fixture_with_an_lv2_bundle() -> Fixture {
    let mut fixture = a_fixture();
    let zip = a_zip(&[
        ("pack/Thing.lv2/manifest.ttl", b"the manifest"),
        ("pack/Thing.lv2/Thing_dsp.dll", b"the code"),
        ("pack/Thing.lv2/resources/data.bin", b"inside a folder of the bundle"),
        (
            "pack/Thing.lv2x/other.txt",
            b"a folder with a name that begins the same way",
        ),
        ("pack/Thing.clap", b"not taken"),
    ]);
    fixture
        .files
        .insert("https://example.org/thing.zip".to_owned(), zip.clone());
    let mut set = fixture.manifest.sets.remove(0);
    set.id = "thing".to_owned();
    set.name = "THING".to_owned();
    set.plugins = vec!["Thing".to_owned()];
    let build = set.platforms.get_mut("windows-x86_64").unwrap();
    build.url = "https://example.org/thing.zip".to_owned();
    build.sha256 = super::test_support::sha(&zip);
    build.size = zip.len() as u64;
    build.members = vec![Member {
        from: "pack/Thing.lv2/".to_owned(),
        to: "Thing.lv2".to_owned(),
        leave_out: Vec::new(),
    }];
    fixture.manifest.sets = vec![set];
    fixture
}

#[test]
fn an_lv2_bundle_is_taken_as_a_folder_with_everything_in_it_and_nothing_that_only_begins_with_its_name() {
    let fixture = a_fixture_with_an_lv2_bundle();
    let fetcher = FakeFetcher::new(fixture.files.clone());
    let folder = tempfile::tempdir().unwrap();

    let outcome = fetch(&fixture, &fetcher, folder.path()).unwrap();

    let set = folder.path().join("thing");
    assert_eq!(outcome.plugin_files, [set.join("Thing.lv2")]);
    assert_eq!(names_in(&set), ["LICENSE.txt", "NOTICE.txt", "Thing.lv2"]);
    assert_eq!(
        names_in(&set.join("Thing.lv2")),
        ["Thing_dsp.dll", "manifest.ttl", "resources"]
    );
    assert_eq!(
        std::fs::read(set.join("Thing.lv2").join("Thing_dsp.dll")).unwrap(),
        b"the code"
    );
    assert_eq!(
        std::fs::read(set.join("Thing.lv2").join("resources").join("data.bin")).unwrap(),
        b"inside a folder of the bundle"
    );
    assert!(is_fetched(&fixture.manifest.sets[0], "windows-x86_64", folder.path()));
}

/// A tar archive made by the system's `tar` of `source` (the names with `prefix` in front of them), or `None` where there is no `tar`.
fn a_tar_of(work: &Path, source: &Path, prefix: &str, file: &str) -> Option<Vec<u8>> {
    if std::process::Command::new("tar").arg("--version").output().is_err() {
        eprintln!("SKIPPED: there is no tar");
        return None;
    }
    let archive = work.join(file);
    let made = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(source)
        .arg(format!("{prefix}pack"))
        .status()
        .unwrap();
    assert!(made.success());
    Some(std::fs::read(&archive).unwrap())
}

/// A manifest of one set with a build for `linux-x86_64` that is `release_file` at `https://example.org/<file>`, from which the folder `from` is taken as `to`.
fn a_manifest_that_takes_a_folder_of(release_file: &[u8], file: &str, from: &str, to: &str) -> Manifest {
    let mut set = a_fixture().manifest.sets.remove(0);
    set.platforms = std::collections::BTreeMap::from([(
        "linux-x86_64".to_owned(),
        Build {
            url: format!("https://example.org/{file}"),
            sha256: super::test_support::sha(release_file),
            size: release_file.len() as u64,
            members: vec![Member {
                from: from.to_owned(),
                to: to.to_owned(),
                leave_out: Vec::new(),
            }],
            installer: None,
        },
    )]);
    Manifest {
        format: 1,
        sets: vec![set],
    }
}

#[test]
fn a_tar_archive_gives_a_folder_with_everything_in_it_with_or_without_dot_slash() {
    let work = tempfile::tempdir().unwrap();
    let source = work.path().join("src");
    let thing = source.join("pack").join("Thing.lv2");
    std::fs::create_dir_all(thing.join("resources")).unwrap();
    std::fs::create_dir_all(source.join("pack").join("Thing.lv2x")).unwrap();
    std::fs::write(thing.join("manifest.ttl"), b"the manifest").unwrap();
    std::fs::write(thing.join("Thing_dsp.so"), b"the code").unwrap();
    std::fs::write(thing.join("resources").join("data.bin"), b"inside").unwrap();
    std::fs::write(source.join("pack").join("Thing.lv2x").join("other.txt"), b"not taken").unwrap();
    for (prefix, file) in [("./", "dot.tar.gz"), ("", "plain.tar.gz")] {
        let Some(release_file) = a_tar_of(work.path(), &source, prefix, file) else {
            return;
        };
        let mut files = a_fixture().files;
        files.insert(format!("https://example.org/{file}"), release_file.clone());
        let folder = tempfile::tempdir().unwrap();

        let outcome = fetch_plugins(
            &a_manifest_that_takes_a_folder_of(&release_file, file, "pack/Thing.lv2/", "Thing.lv2"),
            "linux-x86_64",
            folder.path(),
            &FakeFetcher::new(files),
            &mut |_| {},
            &|| false,
        )
        .unwrap();

        let bundle = folder.path().join("one").join("Thing.lv2");
        assert_eq!(outcome.plugin_files, std::slice::from_ref(&bundle), "{file}");
        assert_eq!(
            names_in(&bundle),
            ["Thing_dsp.so", "manifest.ttl", "resources"],
            "{file}"
        );
        assert_eq!(
            std::fs::read(bundle.join("Thing_dsp.so")).unwrap(),
            b"the code",
            "{file}"
        );
        assert_eq!(
            std::fs::read(bundle.join("resources").join("data.bin")).unwrap(),
            b"inside",
            "{file}"
        );
        assert_eq!(
            names_in(folder.path()),
            ["one"],
            "{file}: no folder that the unpacking used is left"
        );
    }
}

#[test]
fn a_folder_that_is_not_in_a_tar_is_said_and_nothing_of_the_set_is_left() {
    let work = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(work.path().join("src").join("pack")).unwrap();
    std::fs::write(work.path().join("src").join("pack").join("A.txt"), b"a").unwrap();
    let Some(release_file) = a_tar_of(work.path(), &work.path().join("src"), "", "things.tar.gz") else {
        return;
    };
    let mut files = a_fixture().files;
    files.insert("https://example.org/things.tar.gz".to_owned(), release_file.clone());
    let folder = tempfile::tempdir().unwrap();

    let error = fetch_plugins(
        &a_manifest_that_takes_a_folder_of(&release_file, "things.tar.gz", "pack/Missing.lv2/", "Missing.lv2"),
        "linux-x86_64",
        folder.path(),
        &FakeFetcher::new(files),
        &mut |_| {},
        &|| false,
    )
    .unwrap_err();

    assert!(error.contains("no folder called \"pack/Missing.lv2\""), "{error}");
    assert_eq!(names_in(folder.path()), Vec::<String>::new());
}

#[test]
fn an_lv2_member_is_a_name_of_a_folder_and_what_is_not_is_said() {
    let with_the_member = |platform: &'static str, from: &'static str, to: &'static str| {
        a_manifest_that_is_wrong_in(move |m| {
            m.sets[0].platforms.get_mut(platform).unwrap().members[0] = Member {
                from: from.to_owned(),
                to: to.to_owned(),
                leave_out: Vec::new(),
            };
        })
    };
    let problems = with_the_member("windows-x86_64", "pack/Thing.lv2", "Thing.lv2");
    assert!(
        problems.contains("is a folder (an .lv2 bundle) and its name inside the archive must end with /"),
        "{problems}"
    );
    assert_eq!(
        with_the_member("windows-x86_64", "pack/Thing.lv2/", "Thing.LV2"),
        "",
        "a folder with its name ending in / and any case of .lv2 is fit"
    );
    let problems = with_the_member("windows-x86_64", "pack/Thing.lv2/", "Thing.vst3");
    assert!(
        problems.contains("is not a plain name of a .clap file or an .lv2 folder"),
        "{problems}"
    );
    assert_eq!(
        with_the_member("macos-universal", "dpf-x-lv2bundles.pkg/Payload/Thing.lv2", "Thing.lv2"),
        "",
        "out of the installer of a disk image the member is a path in what the installer holds, with no / at the end"
    );
}
