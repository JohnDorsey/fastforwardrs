// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Switching the tables of colours between being in the project file and being linked, afterwards (RS-04): the items of the menu of the box of a look-up table effect, what
//! including needs (a `.ffrs` file, asked for if there is none), what is written, and how a table is put back when it is taken out.

use std::fs;
use std::path::PathBuf;

use egui_kittest::kittest::Queryable;

use super::included_files_tests::{Fixture, dialog_showing, names_of_the_entries};
use super::preview_layer_tests::select;
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::application::naming::what_an_effect_is_called;
use crate::application::properties_panel::{
    INCLUDE_TABLE_LABEL, TAKE_TABLE_OUT_LABEL, include_all_tables_label, take_all_tables_out_label,
};
use crate::project::{ClipId, ClipKind, EffectKind, Member, ResourceKind, Resources, SourceMediaLink};
use crate::project_file::archive::project_resource_entry_name;
use crate::project_file::read_project_file;

const A_TABLE: &str = "TITLE \"a table\"\nLUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";
const ANOTHER_TABLE: &str =
    "TITLE \"another\"\nLUT_3D_SIZE 2\n0 0 0\n0 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";

/// The video of the fixture (the sound cannot have a look-up table).
fn the_clip(fixture: &Fixture) -> ClipId {
    fixture
        .window
        .application()
        .project()
        .tracks()
        .iter()
        .flat_map(|track| track.entries())
        .find(|entry| entry.clip().kind() == ClipKind::Video)
        .expect("the fixture has a video")
        .id()
}

/// Puts a look-up table effect on the first clip with a table called `name` that is linked to a file of that name in the folder of the fixture.
fn put_a_linked_table_on_the_clip(fixture: &mut Fixture, name: &str, text: &str) -> PathBuf {
    let path = fixture.path(name);
    fs::write(&path, text).unwrap();
    let clip = the_clip(fixture);
    fixture.window.do_this(Command::UseATable {
        owner: Member::Clip(clip),
        instance: None,
        wanted_name: name.to_owned(),
        bytes: text.as_bytes().into(),
        link: SourceMediaLink::from_absolute_path(path.clone()),
    });
    path
}

fn is_included(fixture: &Fixture, name: &str) -> bool {
    fixture
        .window
        .application()
        .project()
        .resources()
        .get(name)
        .unwrap()
        .is_included()
}

fn entry_of(name: &str, text: &str) -> String {
    project_resource_entry_name(crc32fast::hash(text.as_bytes()), name)
}

fn click(fixture: &mut Fixture, label: &str) {
    fixture.window.harness.get_by_label(label).click();
    fixture.window.settle();
}

fn the_message(fixture: &Fixture) -> Option<(String, String)> {
    match dialog_showing(&fixture.window) {
        Some(Dialog::Message { title, text }) => Some((title.clone(), text.clone())),
        _ => None,
    }
}

#[test]
fn including_a_linked_table_in_a_project_whose_file_is_an_archive_writes_it_in_the_archive_now_and_it_is_read_from_there_when_the_file_is_gone()
 {
    let mut fixture = Fixture::new();
    let archive_path = fixture.save_as("trip.ffrs");
    let original = put_a_linked_table_on_the_clip(&mut fixture, "grade.cube", A_TABLE);
    assert!(
        fixture
            .window
            .application()
            .project()
            .resources()
            .get("grade.cube")
            .is_some_and(|resource| !resource.is_included()),
        "(it is linked: the file is in the folder)"
    );

    fixture
        .window
        .do_this(Command::IncludeTablesInTheProjectFile(vec!["grade.cube".to_owned()]));

    assert!(is_included(&fixture, "grade.cube"));
    assert!(
        !fixture.window.application().has_unsaved_changes(),
        "it is saved with the file, as a file of a video is"
    );
    assert!(names_of_the_entries(&archive_path).contains(&entry_of("grade.cube", A_TABLE)));
    fs::remove_file(&original).unwrap();
    let loaded = read_project_file(&archive_path).unwrap();
    let resource = loaded.resources().get("grade.cube").unwrap();
    assert!(resource.is_included());
    assert_eq!(
        resource.bytes().map(|bytes| bytes.to_vec()),
        Some(A_TABLE.as_bytes().to_vec())
    );
    assert_eq!(
        resource.link().absolute_path,
        std::path::absolute(&original).unwrap(),
        "where it came from is what it is put back at"
    );
}

#[test]
fn including_a_table_when_the_project_has_no_archive_asks_to_save_one_and_does_nothing_before_the_user_agrees() {
    let mut fixture = Fixture::new();
    put_a_linked_table_on_the_clip(&mut fixture, "grade.cube", A_TABLE);

    fixture
        .window
        .do_this(Command::IncludeTablesInTheProjectFile(vec!["grade.cube".to_owned()]));

    assert_eq!(
        dialog_showing(&fixture.window),
        Some(&Dialog::MustSaveAsAnArchiveToIncludeTables {
            names: vec!["grade.cube".to_owned()],
            has_a_plain_project_file: false,
        })
    );
    assert!(!is_included(&fixture, "grade.cube"));
    assert!(fixture.chooser.questions_asked().is_empty());

    fixture.chooser.will_answer(Some(fixture.path("trip.ffrs")));
    click(&mut fixture, "Choose where to save it");

    assert!(is_included(&fixture, "grade.cube"));
    let archive_path = fixture.path("trip.ffrs");
    assert!(names_of_the_entries(&archive_path).contains(&entry_of("grade.cube", A_TABLE)));
    assert_eq!(
        fixture.window.application().project_file_path(),
        Some(archive_path.as_path())
    );
}

#[test]
fn saying_no_to_saving_an_archive_leaves_the_table_linked() {
    let mut fixture = Fixture::new();
    put_a_linked_table_on_the_clip(&mut fixture, "grade.cube", A_TABLE);
    fixture
        .window
        .do_this(Command::IncludeTablesInTheProjectFile(vec!["grade.cube".to_owned()]));

    click(&mut fixture, "Cancel");

    assert!(!is_included(&fixture, "grade.cube"));
    assert_eq!(dialog_showing(&fixture.window), None);
}

#[test]
fn taking_a_table_out_puts_it_back_where_it_came_from_when_nothing_is_there_and_links_it() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    let original = put_a_linked_table_on_the_clip(&mut fixture, "grade.cube", A_TABLE);
    fixture
        .window
        .do_this(Command::IncludeTablesInTheProjectFile(vec!["grade.cube".to_owned()]));
    fs::remove_file(&original).unwrap();

    fixture
        .window
        .do_this(Command::TakeTablesOutOfTheProjectFile(vec!["grade.cube".to_owned()]));

    assert!(!is_included(&fixture, "grade.cube"));
    assert_eq!(fs::read_to_string(&original).unwrap(), A_TABLE, "the file is back");
    let link = fixture
        .window
        .application()
        .project()
        .resources()
        .get("grade.cube")
        .unwrap()
        .link()
        .clone();
    assert_eq!(link.absolute_path, std::path::absolute(&original).unwrap());
    assert_eq!(link.relative_path, Some(PathBuf::from("grade.cube")));
    assert!(
        fixture.window.application().has_unsaved_changes(),
        "the project says something else now"
    );
    assert_eq!(dialog_showing(&fixture.window), None);
}

#[test]
fn taking_a_table_out_where_the_same_file_is_already_copies_nothing_and_where_another_file_is_leaves_it_alone_and_says_so()
 {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    let original = put_a_linked_table_on_the_clip(&mut fixture, "grade.cube", A_TABLE);
    fixture
        .window
        .do_this(Command::IncludeTablesInTheProjectFile(vec!["grade.cube".to_owned()]));
    let modified = fs::metadata(&original).unwrap().modified().unwrap();

    fixture
        .window
        .do_this(Command::TakeTablesOutOfTheProjectFile(vec!["grade.cube".to_owned()]));

    assert!(!is_included(&fixture, "grade.cube"));
    assert_eq!(
        fs::metadata(&original).unwrap().modified().unwrap(),
        modified,
        "it was not written"
    );

    // Another file in its place: the table stays in the project file, the file is not touched, and the user is told.
    fixture
        .window
        .do_this(Command::IncludeTablesInTheProjectFile(vec!["grade.cube".to_owned()]));
    fs::write(&original, "something else that was put there").unwrap();
    fixture
        .window
        .do_this(Command::TakeTablesOutOfTheProjectFile(vec!["grade.cube".to_owned()]));

    assert!(is_included(&fixture, "grade.cube"), "no place to put it");
    assert_eq!(
        fs::read_to_string(&original).unwrap(),
        "something else that was put there"
    );
    let (title, text) = the_message(&fixture).expect("the user is told");
    assert_eq!(title, "The table could not be taken out of the project file");
    assert!(
        text.contains("grade.cube") && text.contains("no place to put it back"),
        "{text}"
    );
}

#[test]
fn a_table_that_is_missing_cannot_be_included_and_a_table_that_is_not_there_is_left_out_without_a_word() {
    let mut fixture = Fixture::new();
    let mut resources = Resources::default();
    resources
        .add_as_it_was_saved(
            "grade.cube",
            ResourceKind::Cube,
            None,
            SourceMediaLink::from_absolute_path(fixture.path("grade.cube")),
        )
        .unwrap();
    let project = fixture.window.application().project().clone().with_resources(resources);
    fixture.window.show_another_project(project);

    fixture.window.do_this(Command::IncludeTablesInTheProjectFile(vec![
        "grade.cube".to_owned(),
        "no such table.cube".to_owned(),
    ]));

    assert!(!is_included(&fixture, "grade.cube"));
    let (title, text) = the_message(&fixture).expect("the user is told");
    assert_eq!(title, "The table cannot be included");
    assert!(text.contains("grade.cube") && !text.contains("no such table"), "{text}");
}

#[test]
fn the_menu_of_the_box_of_a_look_up_table_offers_to_include_a_linked_table_and_to_take_an_included_one_out() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    put_a_linked_table_on_the_clip(&mut fixture, "grade.cube", A_TABLE);
    let clip = the_clip(&fixture);
    select(&mut fixture.window, clip);
    let heading = what_an_effect_is_called(EffectKind::LUT);

    let at = fixture.window.rectangle_of(&heading).center();
    fixture.window.right_click(at);
    assert!(fixture.window.harness.query_by_label(INCLUDE_TABLE_LABEL).is_some());
    assert!(fixture.window.harness.query_by_label(TAKE_TABLE_OUT_LABEL).is_none());
    let item = fixture.window.rectangle_of(INCLUDE_TABLE_LABEL).center();
    fixture.window.click(item);
    assert!(is_included(&fixture, "grade.cube"), "the item does it");

    let at = fixture.window.rectangle_of(&heading).center();
    fixture.window.right_click(at);
    assert!(fixture.window.harness.query_by_label(TAKE_TABLE_OUT_LABEL).is_some());
    assert!(fixture.window.harness.query_by_label(INCLUDE_TABLE_LABEL).is_none());
}

#[test]
fn with_several_tables_that_could_be_included_or_taken_out_the_menu_offers_to_do_it_to_all_of_them() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    put_a_linked_table_on_the_clip(&mut fixture, "grade.cube", A_TABLE);
    put_a_linked_table_on_the_clip(&mut fixture, "other.cube", ANOTHER_TABLE);
    let clip = the_clip(&fixture);
    select(&mut fixture.window, clip);
    let heading = fixture
        .window
        .rectangle_of(&format!("{} 1", what_an_effect_is_called(EffectKind::LUT)))
        .center();

    fixture.window.right_click(heading);
    assert!(
        fixture
            .window
            .harness
            .query_by_label(&include_all_tables_label(2))
            .is_some()
    );
    let item = fixture.window.rectangle_of(&include_all_tables_label(2)).center();
    fixture.window.click(item);

    assert!(is_included(&fixture, "grade.cube") && is_included(&fixture, "other.cube"));
    let heading = fixture
        .window
        .rectangle_of(&format!("{} 1", what_an_effect_is_called(EffectKind::LUT)))
        .center();
    fixture.window.right_click(heading);
    assert!(
        fixture
            .window
            .harness
            .query_by_label(&take_all_tables_out_label(2))
            .is_some()
    );
    assert!(
        fixture
            .window
            .harness
            .query_by_label(&include_all_tables_label(2))
            .is_none()
    );
}

#[test]
fn taking_out_a_table_that_is_not_in_the_project_file_changes_nothing_and_the_project_stays_as_saved() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    put_a_linked_table_on_the_clip(&mut fixture, "grade.cube", A_TABLE);
    fixture
        .window
        .do_this(Command::IncludeTablesInTheProjectFile(vec!["other.cube".to_owned()]));
    fixture.save_as("trip.ffrs");
    assert!(!fixture.window.application().has_unsaved_changes());

    fixture.window.do_this(Command::TakeTablesOutOfTheProjectFile(vec![
        "grade.cube".to_owned(),
        "no such table.cube".to_owned(),
    ]));

    assert!(!is_included(&fixture, "grade.cube"));
    assert!(
        !fixture.window.application().has_unsaved_changes(),
        "nothing was taken out, so nothing changed"
    );
    assert_eq!(the_message(&fixture), None);
}

#[test]
fn with_one_table_the_menu_does_not_offer_to_do_it_to_all_of_them() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    put_a_linked_table_on_the_clip(&mut fixture, "grade.cube", A_TABLE);
    let clip = the_clip(&fixture);
    select(&mut fixture.window, clip);
    let heading = what_an_effect_is_called(EffectKind::LUT);
    let at = fixture.window.rectangle_of(&heading).center();

    fixture.window.right_click(at);

    assert!(fixture.window.harness.query_by_label(INCLUDE_TABLE_LABEL).is_some());
    assert!(
        fixture
            .window
            .harness
            .query_by_label(&include_all_tables_label(1))
            .is_none()
    );
    assert!(fixture.window.harness.query_by_label_contains("Include all").is_none());
    assert!(fixture.window.harness.query_by_label_contains("Take all").is_none());
}

#[test]
fn with_one_table_in_the_project_file_the_menu_offers_to_take_it_out_and_not_all_of_them() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    put_a_linked_table_on_the_clip(&mut fixture, "grade.cube", A_TABLE);
    fixture
        .window
        .do_this(Command::IncludeTablesInTheProjectFile(vec!["grade.cube".to_owned()]));
    let clip = the_clip(&fixture);
    select(&mut fixture.window, clip);
    let at = fixture
        .window
        .rectangle_of(&what_an_effect_is_called(EffectKind::LUT))
        .center();

    fixture.window.right_click(at);

    assert!(fixture.window.harness.query_by_label(TAKE_TABLE_OUT_LABEL).is_some());
    assert!(fixture.window.harness.query_by_label_contains("Take all").is_none());
    assert!(fixture.window.harness.query_by_label_contains("Include all").is_none());
}
