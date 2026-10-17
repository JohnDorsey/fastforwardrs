// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;

use super::*;

fn bytes(text: &str) -> Arc<[u8]> {
    Arc::from(text.as_bytes())
}

fn linked(path: &str) -> SourceMediaLink {
    SourceMediaLink::from_absolute_path(PathBuf::from(path))
}

fn included(entry: &str) -> SourceMediaLink {
    SourceMediaLink {
        included_as: Some(entry.to_owned()),
        ..linked("/tables/grade.cube")
    }
}

#[test]
fn a_resource_that_is_added_is_there_by_its_name_with_its_bytes_its_kind_and_its_link() {
    let mut resources = Resources::default();

    let name = resources
        .add(
            "grade.cube",
            ResourceKind::Cube,
            bytes("LUT_3D_SIZE 2"),
            linked("/tables/grade.cube"),
        )
        .unwrap();

    assert_eq!(name, "grade.cube");
    let resource = resources.get("grade.cube").expect("it is there");
    assert_eq!(resource.name(), "grade.cube");
    assert_eq!(resource.kind(), ResourceKind::Cube);
    assert_eq!(resource.bytes().map(|bytes| &**bytes), Some(&b"LUT_3D_SIZE 2"[..]));
    assert!(!resource.is_missing());
    assert!(!resource.is_included());
    assert_eq!(resource.link(), &linked("/tables/grade.cube"));
    assert_eq!(resource.crc32(), Some(crc32fast::hash(b"LUT_3D_SIZE 2")));
    assert_eq!((resources.len(), resources.is_empty()), (1, false));
    assert!(resources.get("another.cube").is_none());
}

#[test]
fn two_resources_with_the_same_bytes_are_one_whatever_they_were_called_and_wherever_they_are_kept() {
    let mut resources = Resources::default();
    let first = resources
        .add(
            "grade.cube",
            ResourceKind::Cube,
            bytes("the table"),
            included("resources/0-grade.cube"),
        )
        .unwrap();

    let again = resources
        .add(
            "copy of grade.cube",
            ResourceKind::Cube,
            bytes("the table"),
            linked("/elsewhere/copy.cube"),
        )
        .unwrap();

    assert_eq!(again, first, "the name of the one that is there");
    assert_eq!(resources.len(), 1);
    assert_eq!(
        resources.get("grade.cube").unwrap().link(),
        &included("resources/0-grade.cube"),
        "and it is kept where it was"
    );
}

#[test]
fn a_name_that_is_taken_by_other_bytes_gets_a_number_before_its_ending() {
    let mut resources = Resources::default();
    let mut add = |name: &str, contents: &str| {
        resources
            .add(name, ResourceKind::Cube, bytes(contents), linked("/x"))
            .unwrap()
    };

    assert_eq!(add("grade.cube", "one"), "grade.cube");
    assert_eq!(add("grade.cube", "two"), "grade (2).cube");
    assert_eq!(add("grade.cube", "three"), "grade (3).cube");
    assert_eq!(
        add("grade (2).cube", "four"),
        "grade (2) (2).cube",
        "(it is taken, so it is another name)"
    );
    assert_eq!(add("noending", "five"), "noending");
    assert_eq!(add("noending", "six"), "noending (2)");
    assert_eq!(add(".cube", "seven"), ".cube");
    assert_eq!(
        add(".cube", "eight"),
        ".cube (2)",
        "(a name that is all ending has no stem to put the number in)"
    );
    assert_eq!(
        add("grade.cube", "three"),
        "grade (3).cube",
        "(the same bytes are the same resource)"
    );
}

#[test]
fn a_name_as_long_as_there_can_be_still_gets_a_name_that_is_not_taken_and_not_too_long() {
    let mut resources = Resources::default();
    let longest: String = "n".repeat(MOST_CHARACTERS_IN_THE_NAME_OF_A_RESOURCE - 5) + ".cube";
    assert_eq!(longest.chars().count(), MOST_CHARACTERS_IN_THE_NAME_OF_A_RESOURCE);
    resources
        .add(&longest, ResourceKind::Cube, bytes("one"), linked("/x"))
        .unwrap();

    let second = resources
        .add(&longest, ResourceKind::Cube, bytes("two"), linked("/x"))
        .unwrap();

    assert_ne!(second, longest);
    assert!(
        second.chars().count() <= MOST_CHARACTERS_IN_THE_NAME_OF_A_RESOURCE,
        "{second}"
    );
    assert!(second.ends_with(" (2).cube"), "{second}");
    assert!(why_not_the_name_of_a_resource(&second).is_none());
    // And a name that has a long "ending" has the number at its end.
    let odd = format!("a.{}", "e".repeat(40));
    resources
        .add(&odd, ResourceKind::Cube, bytes("three"), linked("/x"))
        .unwrap();
    let odd_again = resources
        .add(&odd, ResourceKind::Cube, bytes("four"), linked("/x"))
        .unwrap();
    assert_eq!(odd_again, format!("{odd} (2)"));
}

#[test]
fn a_name_that_a_file_in_an_archive_could_not_have_is_refused_with_the_reason() {
    let mut resources = Resources::default();
    let long = "n".repeat(MOST_CHARACTERS_IN_THE_NAME_OF_A_RESOURCE + 1);
    let control = "a control character or one that a name of a file cannot have";
    for (name, reason) in [
        ("", "it is empty"),
        ("..", "it is the name of a folder"),
        (".", "it is the name of a folder"),
        ("folder/grade.cube", control),
        ("folder\\grade.cube", control),
        ("grade?.cube", control),
        ("grade\n.cube", control),
        ("grade\u{7f}.cube", control),
        (" grade.cube", "begins or ends with a space"),
        ("grade.cube ", "begins or ends with a space"),
        ("grade.", "begins or ends with a space"),
        (long.as_str(), "it is too long"),
    ] {
        match resources.add(name, ResourceKind::Cube, bytes("x"), linked("/x")) {
            Err(ResourceProblem::BadName { reason: found, .. }) => {
                assert!(found.contains(reason), "{name:?}: {found}");
            }
            other => panic!("{name:?}: {other:?}"),
        }
    }
    assert!(resources.is_empty());
    // And names that are fine: spaces inside, other scripts, a dot or two.
    for name in [
        "my grade.cube",
        "Kodak 2383 D55.cube",
        "grade.v2.cube",
        "écran.cube",
        "ルック.cube",
        "a",
    ] {
        assert!(why_not_the_name_of_a_resource(name).is_none(), "{name}");
    }
}

#[test]
fn an_empty_file_and_a_file_of_more_than_the_most_are_refused_and_a_file_of_exactly_the_most_is_not() {
    let mut resources = Resources::default();
    assert_eq!(
        resources.add("empty.cube", ResourceKind::Cube, bytes(""), linked("/x")),
        Err(ResourceProblem::Empty)
    );
    let too_big: Arc<[u8]> = vec![7_u8; MOST_BYTES_OF_A_RESOURCE + 1].into();
    assert_eq!(
        resources.add("big.cube", ResourceKind::Cube, too_big, linked("/x")),
        Err(ResourceProblem::TooBig {
            bytes: MOST_BYTES_OF_A_RESOURCE + 1,
            limit: MOST_BYTES_OF_A_RESOURCE
        })
    );
    assert!(resources.is_empty());
    let exactly: Arc<[u8]> = vec![7_u8; MOST_BYTES_OF_A_RESOURCE].into();
    assert!(
        resources
            .add("exact.cube", ResourceKind::Cube, exactly, linked("/x"))
            .is_ok()
    );
    let message = ResourceProblem::TooBig {
        bytes: 70_000_000,
        limit: MOST_BYTES_OF_A_RESOURCE,
    }
    .to_string();
    assert!(
        message.contains("70000000") && message.contains("67108864"),
        "{message}"
    );
}

#[test]
fn a_project_has_at_most_so_many_resources_but_a_file_that_is_there_already_is_still_found() {
    let mut resources = Resources::default();
    for number in 0..MOST_RESOURCES {
        resources
            .add(
                &format!("table {number}.cube"),
                ResourceKind::Cube,
                bytes(&format!("contents {number}")),
                linked("/x"),
            )
            .unwrap();
    }
    assert_eq!(
        resources.add("one more.cube", ResourceKind::Cube, bytes("new contents"), linked("/x")),
        Err(ResourceProblem::TooMany { limit: MOST_RESOURCES })
    );
    assert_eq!(
        resources.add("other name.cube", ResourceKind::Cube, bytes("contents 7"), linked("/x")),
        Ok("table 7.cube".to_owned()),
        "(it adds nothing, so it fits)"
    );
    assert_eq!(resources.len(), MOST_RESOURCES);
}

#[test]
fn a_resource_that_a_file_had_is_put_back_as_it_was_even_when_its_bytes_are_missing() {
    let mut resources = Resources::default();

    resources
        .add_as_it_was_saved(
            "a.cube",
            ResourceKind::Cube,
            Some(bytes("table")),
            included("resources/0-a.cube"),
        )
        .unwrap();
    resources
        .add_as_it_was_saved("b.cube", ResourceKind::Cube, None, linked("/gone/b.cube"))
        .unwrap();

    assert!(!resources.get("a.cube").unwrap().is_missing());
    let missing = resources.get("b.cube").unwrap();
    assert!(missing.is_missing());
    assert_eq!(missing.crc32(), None);
    assert_eq!(
        missing.link(),
        &linked("/gone/b.cube"),
        "its places are kept, to be looked at"
    );
    // (Two with the same bytes are not made one: a file says what it says. Only a name taken twice is a mistake.)
    resources
        .add_as_it_was_saved("c.cube", ResourceKind::Cube, Some(bytes("table")), linked("/c"))
        .unwrap();
    assert!(matches!(
        resources.add_as_it_was_saved("a.cube", ResourceKind::Cube, None, linked("/x")),
        Err(ResourceProblem::BadName {
            reason: "another resource has the name",
            ..
        })
    ));
    assert!(matches!(
        resources.add_as_it_was_saved("../a.cube", ResourceKind::Cube, None, linked("/x")),
        Err(ResourceProblem::BadName { .. })
    ));
}

#[test]
fn the_bytes_of_a_missing_resource_can_be_found_later_and_nothing_else_is_changed_by_that() {
    let mut resources = Resources::default();
    resources
        .add_as_it_was_saved("b.cube", ResourceKind::Cube, None, linked("/b"))
        .unwrap();
    resources
        .add("a.cube", ResourceKind::Cube, bytes("have"), linked("/a"))
        .unwrap();

    assert!(
        !resources.found_the_bytes_of("a.cube", bytes("other")),
        "it has bytes already"
    );
    assert!(
        !resources.found_the_bytes_of("nothing.cube", bytes("other")),
        "no such resource"
    );
    let too_big: Arc<[u8]> = vec![1_u8; MOST_BYTES_OF_A_RESOURCE + 1].into();
    assert!(!resources.found_the_bytes_of("b.cube", too_big), "too many bytes");
    assert!(resources.get("b.cube").unwrap().is_missing());
    assert!(resources.found_the_bytes_of("b.cube", bytes("found")));

    assert!(!resources.get("b.cube").unwrap().is_missing());
    assert_eq!(&**resources.get("a.cube").unwrap().bytes().unwrap(), b"have");
    assert_eq!(resources.get("b.cube").unwrap().link(), &linked("/b"));
}

#[test]
fn a_resource_can_be_kept_somewhere_else_without_being_changed() {
    let mut resources = Resources::default();
    resources
        .add("a.cube", ResourceKind::Cube, bytes("table"), linked("/a"))
        .unwrap();

    assert!(resources.keep_at("a.cube", included("resources/0-a.cube")));
    assert!(!resources.keep_at("b.cube", linked("/b")), "no such resource");

    let resource = resources.get("a.cube").unwrap();
    assert!(resource.is_included());
    assert_eq!(&**resource.bytes().unwrap(), b"table");
}

#[test]
fn the_resources_that_are_used_are_kept_in_their_order_and_the_others_go() {
    let mut resources = Resources::default();
    for name in ["a.cube", "b.cube", "c.cube", "d.cube"] {
        resources
            .add(name, ResourceKind::Cube, bytes(name), linked("/x"))
            .unwrap();
    }
    let used: BTreeSet<String> = ["d.cube", "b.cube", "e.cube"].into_iter().map(str::to_owned).collect();

    let kept = resources.only_those_named_in(&used);

    assert_eq!(
        kept.iter().map(Resource::name).collect::<Vec<_>>(),
        ["b.cube", "d.cube"]
    );
    assert_eq!(resources.len(), 4, "(the project keeps them all)");
    assert!(resources.only_those_named_in(&BTreeSet::new()).is_empty());
}

#[test]
fn a_kind_of_resource_is_known_by_its_key() {
    for kind in ResourceKind::ALL {
        assert_eq!(ResourceKind::from_key(kind.key()), Some(kind));
        assert!(!kind.label().is_empty());
    }
    assert_eq!(ResourceKind::from_key("cube"), Some(ResourceKind::Cube));
    assert_eq!(ResourceKind::from_key("Cube"), None);
    assert_eq!(ResourceKind::from_key(""), None);
    assert_eq!(ResourceKind::from_key("png"), None);
}
