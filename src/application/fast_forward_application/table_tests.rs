// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tables of colours in the window (phase 4, piece 4.7): *Add effect from file...*, the table of a look-up table effect chosen again, how the file is kept (embedded, linked,
//! asked about), what is refused, and that it is all one step of the history.

use std::path::{Path, PathBuf};

use egui_kittest::kittest::Queryable;

use super::keyframe_tests::window_with_a_video;
use super::preview_layer_tests::select;
use super::test_window::Window;
use super::undo_tests::{redo, undo};
use crate::application::commands::Command;
use crate::application::dialogs::{Dialog, HOW_TO_KEEP_A_TABLE_EMBED, HOW_TO_KEEP_A_TABLE_LINK};
use crate::application::fast_forward_application::table_flow::{
    LINKED_BECAUSE_THE_FILE_IS_PLAIN_JSON, NOT_A_FILE_TITLE, NOT_A_TABLE_TITLE,
};
use crate::application::file_chooser::ScriptedFileChooser;
use crate::project::{
    AnimationTarget, ClipId, EffectKind, GroupKind, MOST_BYTES_OF_A_RESOURCE, Member, PropertyValue, ResourceStorage,
    Settings,
};

const A_TABLE: &str = "TITLE \"a table\"\nLUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";

/// A window with a video that is selected, nothing unsaved, a chooser that will be asked for files, and the clip.
fn window_that_asks() -> (Window, ClipId, ScriptedFileChooser) {
    let (mut window, clip) = window_with_a_video();
    select(&mut window, clip);
    let chooser = ScriptedFileChooser::default();
    window.use_file_chooser(&chooser);
    (window, clip, chooser)
}

fn a_table_in(folder: &Path, name: &str, text: &str) -> PathBuf {
    let path = folder.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

fn the_effects_of(window: &Window, clip: ClipId) -> Vec<(EffectKind, Vec<PropertyValue>)> {
    window
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .map(|effect| (effect.kind(), effect.values().to_vec()))
        .collect()
}

fn set_how_to_keep(window: &mut Window, storage: ResourceStorage) {
    window.do_this(Command::SetGlobalSettings(Settings {
        resource_storage: storage,
        ..Settings::DEFAULT
    }));
}

fn add_from_a_file(window: &mut Window, chooser: &ScriptedFileChooser, clip: ClipId, path: &Path) {
    chooser.will_answer(Some(path.to_owned()));
    window.do_this(Command::AddEffectFromFile {
        owner: Member::Clip(clip),
    });
}

fn the_message(window: &Window) -> Option<(String, String)> {
    match &window.application().interface.dialog {
        Some(Dialog::Message { title, text }) => Some((title.clone(), text.clone())),
        _ => None,
    }
}

#[test]
fn adding_an_effect_from_a_file_asks_for_a_cube_file_and_puts_a_look_up_table_with_the_table_embedded_on_the_clip() {
    let folder = tempfile::tempdir().unwrap();
    let table = a_table_in(folder.path(), "grade.cube", A_TABLE);
    let (mut window, clip, chooser) = window_that_asks();

    add_from_a_file(&mut window, &chooser, clip, &table);

    let asked = chooser.questions_asked();
    assert_eq!(asked.len(), 1);
    assert!(!asked[0].saving);
    assert_eq!(
        asked[0].kinds[0],
        ("Tables of colours (.cube)".to_owned(), vec!["cube".to_owned()])
    );
    let effects = the_effects_of(&window, clip);
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].0, EffectKind::LUT);
    assert_eq!(
        effects[0].1[0],
        PropertyValue::Words("grade.cube".to_owned()),
        "the table, by its name"
    );
    assert_eq!(
        effects[0].1[1],
        PropertyValue::Number(1.0),
        "and the rest as the effect begins"
    );
    let resource = window.application().project().resources().get("grade.cube").unwrap();
    assert!(resource.is_included(), "embedded: the setting begins as embed");
    assert_eq!(
        resource.link().absolute_path,
        table,
        "(and it remembers where it came from)"
    );
    assert!(resource.cube_table().is_ok());
    assert!(window.application().has_unsaved_changes());
    assert_eq!(the_message(&window), None);
}

#[test]
fn undoing_it_takes_the_effect_and_the_table_back_and_redoing_brings_both_back_in_one_step() {
    let folder = tempfile::tempdir().unwrap();
    let table = a_table_in(folder.path(), "grade.cube", A_TABLE);
    let (mut window, clip, chooser) = window_that_asks();
    add_from_a_file(&mut window, &chooser, clip, &table);

    undo(&mut window);

    assert!(the_effects_of(&window, clip).is_empty());
    assert!(window.application().project().resources().is_empty());
    redo(&mut window);
    assert_eq!(the_effects_of(&window, clip).len(), 1);
    assert!(window.application().project().resources().get("grade.cube").is_some());
}

#[test]
fn a_file_that_the_person_does_not_choose_changes_nothing() {
    let (mut window, clip, chooser) = window_that_asks();
    chooser.will_answer(None);

    window.do_this(Command::AddEffectFromFile {
        owner: Member::Clip(clip),
    });

    assert!(the_effects_of(&window, clip).is_empty());
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn the_setting_to_link_keeps_the_table_where_it_is() {
    let folder = tempfile::tempdir().unwrap();
    let table = a_table_in(folder.path(), "grade.cube", A_TABLE);
    let (mut window, clip, chooser) = window_that_asks();
    set_how_to_keep(&mut window, ResourceStorage::Link);

    add_from_a_file(&mut window, &chooser, clip, &table);

    let resource = window.application().project().resources().get("grade.cube").unwrap();
    assert!(!resource.is_included());
    assert_eq!(resource.link().absolute_path, table);
}

#[test]
fn the_setting_of_the_project_beats_the_global_one() {
    let folder = tempfile::tempdir().unwrap();
    let table = a_table_in(folder.path(), "grade.cube", A_TABLE);
    let (mut window, clip, chooser) = window_that_asks();
    set_how_to_keep(&mut window, ResourceStorage::Embed);
    window.do_this(Command::SetProjectSettings(crate::project::ProjectSettings {
        resource_storage: Some(ResourceStorage::Link),
        ..crate::project::ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS
    }));

    add_from_a_file(&mut window, &chooser, clip, &table);

    assert!(
        !window
            .application()
            .project()
            .resources()
            .get("grade.cube")
            .unwrap()
            .is_included()
    );
}

#[test]
fn the_setting_to_ask_asks_and_each_answer_does_what_it_says() {
    let folder = tempfile::tempdir().unwrap();
    for (button, embedded) in [(HOW_TO_KEEP_A_TABLE_EMBED, true), (HOW_TO_KEEP_A_TABLE_LINK, false)] {
        let table = a_table_in(folder.path(), "grade.cube", A_TABLE);
        let (mut window, clip, chooser) = window_that_asks();
        set_how_to_keep(&mut window, ResourceStorage::Ask);
        add_from_a_file(&mut window, &chooser, clip, &table);
        assert!(
            matches!(
                window.application().interface.dialog,
                Some(Dialog::HowToKeepATable { can_embed: true, .. })
            ),
            "it asks"
        );
        assert!(
            the_effects_of(&window, clip).is_empty(),
            "and nothing is done until it is answered"
        );

        let button = window.rectangle_of(button).center();
        window.click(button);

        assert_eq!(the_effects_of(&window, clip).len(), 1, "{button:?}");
        let resource = window.application().project().resources().get("grade.cube").unwrap();
        assert_eq!(resource.is_included(), embedded);
        assert!(window.application().interface.dialog.is_none());
    }
}

#[test]
fn cancelling_the_question_about_how_to_keep_the_table_changes_nothing() {
    let folder = tempfile::tempdir().unwrap();
    let table = a_table_in(folder.path(), "grade.cube", A_TABLE);
    let (mut window, clip, chooser) = window_that_asks();
    set_how_to_keep(&mut window, ResourceStorage::Ask);
    add_from_a_file(&mut window, &chooser, clip, &table);

    let cancel = window.rectangle_of("Cancel").center();
    window.click(cancel);

    assert!(the_effects_of(&window, clip).is_empty());
    assert!(window.application().project().resources().is_empty());
    assert!(window.application().interface.dialog.is_none());
}

#[test]
fn a_project_that_is_plain_json_links_the_table_and_says_so_once() {
    let folder = tempfile::tempdir().unwrap();
    let first = a_table_in(folder.path(), "first.cube", A_TABLE);
    let second = a_table_in(
        folder.path(),
        "second.cube",
        &A_TABLE.replace("1 1 1\n", "0.5 0.5 0.5\n"),
    );
    let (mut window, clip, chooser) = window_that_asks();
    window.application_mut().document.project_file_path = Some(folder.path().join("project.json"));

    add_from_a_file(&mut window, &chooser, clip, &first);

    assert!(
        !window
            .application()
            .project()
            .resources()
            .get("first.cube")
            .unwrap()
            .is_included()
    );
    assert_eq!(
        the_message(&window).map(|(_, text)| text),
        Some(LINKED_BECAUSE_THE_FILE_IS_PLAIN_JSON.to_owned())
    );
    // The message is closed, and the next table is linked without it.
    let ok = window.rectangle_of("OK").center();
    window.click(ok);
    add_from_a_file(&mut window, &chooser, clip, &second);
    assert!(
        !window
            .application()
            .project()
            .resources()
            .get("second.cube")
            .unwrap()
            .is_included()
    );
    assert_eq!(the_message(&window), None, "(it was said once)");
    assert_eq!(the_effects_of(&window, clip).len(), 2);
}

#[test]
fn a_project_that_is_an_archive_embeds_the_table() {
    let folder = tempfile::tempdir().unwrap();
    let table = a_table_in(folder.path(), "grade.cube", A_TABLE);
    let (mut window, clip, chooser) = window_that_asks();
    window.application_mut().document.project_file_path = Some(folder.path().join("project.ffrs"));

    add_from_a_file(&mut window, &chooser, clip, &table);

    assert!(
        window
            .application()
            .project()
            .resources()
            .get("grade.cube")
            .unwrap()
            .is_included()
    );
    assert_eq!(the_message(&window), None);
}

#[test]
fn when_asked_in_a_project_that_is_plain_json_only_linking_is_offered() {
    let folder = tempfile::tempdir().unwrap();
    let table = a_table_in(folder.path(), "grade.cube", A_TABLE);
    let (mut window, clip, chooser) = window_that_asks();
    window.application_mut().document.project_file_path = Some(folder.path().join("project.json"));
    set_how_to_keep(&mut window, ResourceStorage::Ask);

    add_from_a_file(&mut window, &chooser, clip, &table);

    assert!(matches!(
        window.application().interface.dialog,
        Some(Dialog::HowToKeepATable { can_embed: false, .. })
    ));
    assert!(window.harness.query_by_label(HOW_TO_KEEP_A_TABLE_EMBED).is_none());
    assert!(window.harness.query_by_label(HOW_TO_KEEP_A_TABLE_LINK).is_some());
}

#[test]
fn a_file_that_is_not_a_table_is_refused_with_the_line_that_is_wrong_and_the_project_is_left_as_it_was() {
    let folder = tempfile::tempdir().unwrap();
    let (mut window, clip, chooser) = window_that_asks();
    for (name, text, wanted) in [
        (
            "size.cube",
            "LUT_3D_SIZE two\n",
            "line 1: LUT_3D_SIZE needs a whole number",
        ),
        (
            "rows.cube",
            "LUT_1D_SIZE 4\n0 0 0\n",
            "LUT_1D_SIZE 4 needs 4 rows, and the file has 1",
        ),
        ("empty.cube", "", "does not say how big"),
    ] {
        let path = a_table_in(folder.path(), name, text);
        add_from_a_file(&mut window, &chooser, clip, &path);
        let (title, message) = the_message(&window).expect("the user is told");
        assert_eq!(
            title,
            if text.is_empty() {
                NOT_A_FILE_TITLE
            } else {
                NOT_A_TABLE_TITLE
            },
            "{name}"
        );
        assert!(
            message.contains(wanted) || message.contains("is empty"),
            "{name}: {message}"
        );
        let ok = window.rectangle_of("OK").center();
        window.click(ok);
    }
    assert!(the_effects_of(&window, clip).is_empty());
    assert!(window.application().project().resources().is_empty());
    assert!(!window.application().has_unsaved_changes());
    // And a file that is not text.
    let binary = folder.path().join("binary.cube");
    std::fs::write(&binary, [0xff_u8, 0xfe, 0x00]).unwrap();
    add_from_a_file(&mut window, &chooser, clip, &binary);
    assert!(the_message(&window).unwrap().1.contains("not text"));
}

#[test]
fn a_file_that_is_missing_or_far_too_big_is_refused_and_says_why() {
    let folder = tempfile::tempdir().unwrap();
    let (mut window, clip, chooser) = window_that_asks();
    add_from_a_file(&mut window, &chooser, clip, &folder.path().join("gone.cube"));
    let (title, text) = the_message(&window).unwrap();
    assert_eq!(title, NOT_A_FILE_TITLE);
    assert!(text.contains("gone.cube"), "{text}");
    let ok = window.rectangle_of("OK").center();
    window.click(ok);

    let big = folder.path().join("big.cube");
    std::fs::write(&big, vec![b'x'; MOST_BYTES_OF_A_RESOURCE + 1]).unwrap();
    add_from_a_file(&mut window, &chooser, clip, &big);
    let text = the_message(&window).unwrap().1;
    assert!(text.contains("67108865") && text.contains("big.cube"), "{text}");
    assert!(the_effects_of(&window, clip).is_empty());
}

#[test]
fn the_same_table_added_twice_is_one_resource_with_two_effects_using_it() {
    let folder = tempfile::tempdir().unwrap();
    let table = a_table_in(folder.path(), "grade.cube", A_TABLE);
    let copy = a_table_in(folder.path(), "copy of grade.cube", A_TABLE);
    let (mut window, clip, chooser) = window_that_asks();

    add_from_a_file(&mut window, &chooser, clip, &table);
    add_from_a_file(&mut window, &chooser, clip, &copy);

    assert_eq!(window.application().project().resources().len(), 1);
    let effects = the_effects_of(&window, clip);
    assert_eq!(effects.len(), 2);
    assert!(
        effects
            .iter()
            .all(|(_, values)| values[0] == PropertyValue::Words("grade.cube".to_owned()))
    );
}

#[test]
fn choosing_a_file_for_the_table_of_an_effect_that_is_on_changes_its_table_and_nothing_else() {
    let folder = tempfile::tempdir().unwrap();
    let first = a_table_in(folder.path(), "first.cube", A_TABLE);
    let second = a_table_in(
        folder.path(),
        "second.cube",
        &A_TABLE.replace("0 0 1\n", "0.2 0.2 0.9\n"),
    );
    let (mut window, clip, chooser) = window_that_asks();
    add_from_a_file(&mut window, &chooser, clip, &first);
    window.do_this(Command::SetEffectValue {
        target: AnimationTarget::Clip(clip),
        property: crate::project::PropertyId::effect_value(EffectKind::LUT, crate::project::EffectInstanceId::FIRST, 1),
        value: PropertyValue::Number(0.5),
    });

    chooser.will_answer(Some(second));
    window.do_this(Command::ChooseTableFile {
        owner: Member::Clip(clip),
        instance: crate::project::EffectInstanceId::FIRST,
    });

    let effects = the_effects_of(&window, clip);
    assert_eq!(effects.len(), 1, "no new effect");
    assert_eq!(effects[0].1[0], PropertyValue::Words("second.cube".to_owned()));
    assert_eq!(
        effects[0].1[1],
        PropertyValue::Number(0.5),
        "the intensity is as it was"
    );
    assert_eq!(
        window.application().project().resources().len(),
        2,
        "(the first stays until the project is saved)"
    );
}

#[test]
fn a_table_cannot_be_chosen_for_an_effect_that_is_not_a_look_up_table_or_is_not_there() {
    let folder = tempfile::tempdir().unwrap();
    let table = a_table_in(folder.path(), "grade.cube", A_TABLE);
    let (mut window, clip, chooser) = window_that_asks();
    window.do_this(Command::AddEffect {
        target: AnimationTarget::Clip(clip),
        kind: EffectKind::HUE_SHIFT,
    });
    for instance in [
        crate::project::EffectInstanceId::FIRST,
        crate::project::EffectInstanceId::new(9),
    ] {
        chooser.will_answer(Some(table.clone()));
        window.do_this(Command::ChooseTableFile {
            owner: Member::Clip(clip),
            instance,
        });
    }
    assert!(window.application().project().resources().is_empty());
    assert_eq!(the_effects_of(&window, clip).len(), 1);
}

#[test]
fn a_tag_and_a_track_can_have_a_look_up_table_effect_added_from_a_file() {
    let folder = tempfile::tempdir().unwrap();
    let table = a_table_in(folder.path(), "grade.cube", A_TABLE);
    let (mut window, clip, chooser) = window_that_asks();
    let track = window.application().project().tracks()[0].id();
    add_from_a_file_for(&mut window, &chooser, Member::Track(track), &table);
    assert_eq!(
        window
            .application()
            .project()
            .effects_of(AnimationTarget::Track(track))
            .unwrap()
            .len(),
        1
    );
    let tag = {
        let project = &mut window.application_mut().document.project;
        project.create_group(GroupKind::Tag, &[Member::Clip(clip)]).unwrap()
    };
    add_from_a_file_for(&mut window, &chooser, Member::Group(tag), &table);
    let group = window.application().project().group(tag).unwrap();
    assert_eq!(group.effects().len(), 1);
    assert_eq!(
        group.effects().iter().next().unwrap().values()[0],
        PropertyValue::Words("grade.cube".to_owned())
    );
}

fn add_from_a_file_for(window: &mut Window, chooser: &ScriptedFileChooser, owner: Member, path: &Path) {
    chooser.will_answer(Some(path.to_owned()));
    window.do_this(Command::AddEffectFromFile { owner });
}
