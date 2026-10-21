// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn bytes(text: &str) -> Arc<[u8]> {
    Arc::from(text.as_bytes())
}

fn build(platform: &str, text: &str) -> EmbeddedBuild {
    EmbeddedBuild::new(platform, bytes(text)).unwrap()
}

fn linked(path: &str) -> PluginLibrarySource {
    PluginLibrarySource::Linked(LinkedLibrary {
        absolute_path: PathBuf::from(path),
        relative_path: None,
    })
}

fn embedded(builds: Vec<EmbeddedBuild>) -> PluginLibrarySource {
    PluginLibrarySource::Embedded(builds)
}

#[test]
fn a_library_has_a_name_a_kind_and_where_it_is_and_an_embedded_one_is_found_by_platform() {
    let library = PluginLibrary::new(
        "amp_1181.dll",
        PluginLibraryKind::Ladspa,
        embedded(vec![build("windows-x86_64", "windows"), build("linux-x86_64", "linux")]),
    )
    .unwrap();
    assert_eq!(library.name(), "amp_1181.dll");
    assert_eq!(library.kind(), PluginLibraryKind::Ladspa);
    assert!(library.is_embedded());
    assert_eq!(library.platforms(), ["windows-x86_64", "linux-x86_64"]);
    assert_eq!(library.build_for("linux-x86_64").unwrap().bytes().as_ref(), b"linux");
    assert!(library.build_for("macos-aarch64").is_none());
    assert_eq!(library.bytes_of_builds(), "windows".len() + "linux".len());

    let by_a_path = PluginLibrary::new("amp_1181", PluginLibraryKind::Ladspa, linked("/plugins/amp_1181.so")).unwrap();
    assert!(!by_a_path.is_embedded());
    assert!(by_a_path.build_for("linux-x86_64").is_none());
    assert!(by_a_path.platforms().is_empty());
    assert_eq!(by_a_path.bytes_of_builds(), 0);
}

// (Rewritten on purpose on 2026-10-21: it said that "clap" is a kind this version does not know, which was true until CLAP plugins could be hosted and a project could carry one.)
#[test]
fn the_kinds_of_library_are_known_by_their_keys() {
    assert_eq!(PluginLibraryKind::Ladspa.key(), "ladspa");
    assert_eq!(PluginLibraryKind::from_key("ladspa"), Some(PluginLibraryKind::Ladspa));
    assert_eq!(PluginLibraryKind::Clap.key(), "clap");
    assert_eq!(PluginLibraryKind::from_key("clap"), Some(PluginLibraryKind::Clap));
    assert_eq!(PluginLibraryKind::from_key("vst3"), None);
    assert_eq!(PluginLibraryKind::from_key("LADSPA"), None);
    assert_eq!(PluginLibraryKind::from_key("CLAP"), None);
}

#[test]
fn a_name_that_a_file_in_an_archive_could_not_have_is_not_a_name_for_a_library() {
    for name in [
        "",
        "..",
        "a/b.dll",
        "a\\b.dll",
        "bad?.dll",
        " x.dll",
        "x.dll ",
        "x\n.dll",
        &"n".repeat(121),
    ] {
        let problem = PluginLibrary::new(name, PluginLibraryKind::Ladspa, linked("/x")).unwrap_err();
        assert!(
            matches!(problem, PluginLibraryProblem::BadName { .. }),
            "{name:?}: {problem:?}"
        );
    }
    assert!(PluginLibrary::new("tap_reverb.dll", PluginLibraryKind::Ladspa, linked("/x")).is_ok());
}

#[test]
fn a_platform_is_named_in_lower_case_letters_digits_dashes_and_underscores_and_not_longer_than_forty() {
    for good in ["windows-x86_64", "linux-aarch64", "macos-aarch64", "a", &"a".repeat(40)] {
        assert!(is_a_platform_name(good), "{good}");
    }
    for bad in [
        "",
        "Windows-x86_64",
        "windows x86",
        "windows/x86",
        "é",
        &"a".repeat(41),
        "a.b",
    ] {
        assert!(!is_a_platform_name(bad), "{bad:?}");
    }
}

#[test]
fn a_build_that_is_not_one_is_refused_with_the_reason() {
    assert_eq!(
        EmbeddedBuild::new("Windows", bytes("x")),
        Err(PluginLibraryProblem::BadPlatform {
            platform: "Windows".to_owned()
        })
    );
    assert_eq!(
        EmbeddedBuild::new("windows-x86_64", bytes("")),
        Err(PluginLibraryProblem::EmptyBuild {
            platform: "windows-x86_64".to_owned()
        })
    );
    let too_big: Arc<[u8]> = vec![1_u8; MOST_BYTES_OF_A_BUILD + 1].into();
    assert!(matches!(
        EmbeddedBuild::new("windows-x86_64", too_big),
        Err(PluginLibraryProblem::BuildTooBig { .. })
    ));
    let exactly: Arc<[u8]> = vec![1_u8; MOST_BYTES_OF_A_BUILD].into();
    assert!(EmbeddedBuild::new("windows-x86_64", exactly).is_ok());
}

#[test]
fn a_build_is_what_it_says_only_if_its_bytes_have_the_digest_that_it_says() {
    let honest = build("linux-x86_64", "the library");
    assert!(honest.is_what_it_says());
    assert_eq!(honest.digest(), Sha256Digest::of_bytes(b"the library"));
    let lying =
        EmbeddedBuild::as_recorded("linux-x86_64", Sha256Digest::of_bytes(b"another"), bytes("the library")).unwrap();
    assert!(!lying.is_what_it_says());
    assert_eq!(
        lying.digest(),
        Sha256Digest::of_bytes(b"another"),
        "what it says is kept as it was said"
    );
}

#[test]
fn an_embedded_library_has_builds_but_not_none_not_too_many_and_not_two_for_one_platform() {
    let name = "x.dll";
    assert_eq!(
        PluginLibrary::new(name, PluginLibraryKind::Ladspa, embedded(Vec::new())),
        Err(PluginLibraryProblem::NoBuilds { name: name.to_owned() })
    );
    assert_eq!(
        PluginLibrary::new(
            name,
            PluginLibraryKind::Ladspa,
            embedded(vec![build("linux-x86_64", "a"), build("linux-x86_64", "b")])
        ),
        Err(PluginLibraryProblem::TwoBuildsForOnePlatform {
            name: name.to_owned(),
            platform: "linux-x86_64".to_owned()
        })
    );
    let many: Vec<EmbeddedBuild> = (0..=MOST_BUILDS_OF_A_LIBRARY)
        .map(|number| build(&format!("platform-{number}"), "x"))
        .collect();
    assert!(matches!(
        PluginLibrary::new(name, PluginLibraryKind::Ladspa, embedded(many.clone())),
        Err(PluginLibraryProblem::TooManyBuilds { limit: 16, .. })
    ));
    assert!(
        PluginLibrary::new(
            name,
            PluginLibraryKind::Ladspa,
            embedded(many[..MOST_BUILDS_OF_A_LIBRARY].to_vec())
        )
        .is_ok()
    );
}

#[test]
fn libraries_are_added_in_order_each_name_once_and_taken_away_by_name() {
    let mut libraries = PluginLibraries::default();
    assert!(libraries.is_empty());
    for name in ["b.dll", "a.dll", "c.dll"] {
        libraries
            .add(PluginLibrary::new(name, PluginLibraryKind::Ladspa, linked("/x")).unwrap())
            .unwrap();
    }
    assert_eq!(
        libraries.iter().map(PluginLibrary::name).collect::<Vec<_>>(),
        ["b.dll", "a.dll", "c.dll"]
    );
    assert_eq!(libraries.len(), 3);
    assert_eq!(
        libraries.add(PluginLibrary::new("a.dll", PluginLibraryKind::Ladspa, linked("/y")).unwrap()),
        Err(PluginLibraryProblem::NameTaken {
            name: "a.dll".to_owned()
        })
    );
    assert!(libraries.remove("a.dll"));
    assert!(!libraries.remove("a.dll"));
    assert_eq!((&libraries).into_iter().count(), 2);
    assert!(libraries.get("a.dll").is_none());
    assert!(libraries.get("c.dll").is_some());
}

#[test]
fn a_project_has_at_most_so_many_libraries_and_so_many_bytes_of_builds() {
    assert_eq!(MOST_PLUGIN_LIBRARIES, 64);
    assert_eq!(MOST_BUILDS_OF_A_LIBRARY, 16);
    assert_eq!(MOST_BYTES_OF_A_BUILD, 256 * 1024 * 1024);
    assert_eq!(MOST_BYTES_OF_ALL_BUILDS, 256 * 1024 * 1024);
    let mut libraries = PluginLibraries::default();
    for number in 0..MOST_PLUGIN_LIBRARIES {
        libraries
            .add(
                PluginLibrary::new(
                    &format!("library {number}.dll"),
                    PluginLibraryKind::Ladspa,
                    linked("/x"),
                )
                .unwrap(),
            )
            .unwrap();
    }
    assert_eq!(
        libraries.add(PluginLibrary::new("one more.dll", PluginLibraryKind::Ladspa, linked("/x")).unwrap()),
        Err(PluginLibraryProblem::TooMany { limit: 64 })
    );

    let mut heavy = PluginLibraries::default();
    let big: Arc<[u8]> = vec![7_u8; MOST_BYTES_OF_ALL_BUILDS / 2 + 1].into();
    let first = PluginLibrary::new(
        "first.dll",
        PluginLibraryKind::Ladspa,
        embedded(vec![EmbeddedBuild::new("linux-x86_64", big.clone()).unwrap()]),
    )
    .unwrap();
    heavy.add(first).unwrap();
    let second = PluginLibrary::new(
        "second.dll",
        PluginLibraryKind::Ladspa,
        embedded(vec![EmbeddedBuild::new("linux-x86_64", big).unwrap()]),
    )
    .unwrap();
    assert_eq!(heavy.add(second), Err(PluginLibraryProblem::TooMuchInAll));
    assert_eq!(heavy.len(), 1, "nothing was added");
}
