// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the two ways to import a file: "Embed and import", which puts the file in the project file as well as on the timeline (and
//! first asks that the project file be a `.ffrs` file), and "Link and import", which leaves the file where it is.
//! (What opening a file really is, with ffmpeg, is checked in `tests/application_journeys.rs`: here the file is said to have opened.)

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui_kittest::kittest::Queryable;

use super::OpeningPurpose;
use super::included_files_tests::{Fixture, dialog_showing, names_of_the_entries};
use super::saving::Gate;
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::application::opening::OpeningOutcome;
use crate::media::MediaLocation;
use crate::project::{SourceAudioProperties, SourceMediaIdentifier, SourceMediaProperties};
use crate::project_file::archive::PROJECT_ENTRY_NAME;
use crate::project_file::read_project_file;

const THE_NEW_BYTES: &[u8] = b"a sound that is imported, which is not really one";
const THE_NEW_SOURCE: SourceMediaIdentifier = SourceMediaIdentifier::new(3);

/// The file that the fixture's folder has to be imported, and what it is said to be when it has opened.
fn the_file_to_import(fixture: &Fixture) -> PathBuf {
    let path = fixture.path("media").join("new sound.wav");
    fs::write(&path, THE_NEW_BYTES).unwrap();
    path
}

fn opened(path: PathBuf) -> OpeningOutcome {
    OpeningOutcome::Opened {
        location: MediaLocation::of_a_file(path),
        properties: SourceMediaProperties::Audio(SourceAudioProperties {
            duration_in_microseconds: 3_000_000,
        }),
    }
}

fn the_file_opened(fixture: &mut Fixture, purpose: OpeningPurpose, path: PathBuf) {
    fixture.window.application_mut().finish_opening(purpose, opened(path));
    fixture.window.settle();
}

/// How many clips there are on all the tracks. (A file that is imported goes on a track of its own in front, if the others are
/// not free at the current frame.)
fn number_of_clips(fixture: &Fixture) -> usize {
    fixture
        .window
        .application()
        .project()
        .tracks()
        .iter()
        .map(|track| track.placements().count())
        .sum()
}

fn click(fixture: &mut Fixture, label: &str) {
    fixture.window.harness.get_by_label(label).click();
    fixture.window.settle();
}

// --- the menu ---

#[test]
fn the_file_menu_offers_both_ways_to_import_and_each_asks_for_what_it_is() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    fixture.chooser.will_answer(None);
    fixture.chooser.will_answer(None);
    click(&mut fixture, "File");

    click(&mut fixture, "Embed and import");
    click(&mut fixture, "File");
    click(&mut fixture, "Link and import");

    let titles: Vec<String> = fixture
        .chooser
        .questions_asked()
        .into_iter()
        .map(|question| question.title)
        .collect();
    assert_eq!(
        titles,
        [
            "Choose a video, a sound or a picture to embed",
            "Choose a video, a sound or a picture"
        ]
    );
}

#[test]
fn both_ways_to_import_ask_for_the_same_kinds_of_file() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    fixture.chooser.will_answer(None);
    fixture.chooser.will_answer(None);

    fixture.window.do_this(Command::EmbedAndImportFile);
    fixture.window.do_this(Command::LinkAndImportFile);

    let asked = fixture.chooser.questions_asked();
    assert_eq!(asked.len(), 2);
    assert_eq!(asked[0].kinds, asked[1].kinds);
    assert!(!asked[0].saving && !asked[1].saving, "both ask which file to open");
}

// --- why the file that is chosen is opened ---

#[test]
fn the_file_chosen_after_embed_and_import_is_opened_to_be_embedded_and_after_link_and_import_to_be_linked() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    let path = the_file_to_import(&fixture);
    fixture.chooser.will_answer(Some(path.clone()));
    fixture.chooser.will_answer(Some(path));

    fixture.window.do_this(Command::EmbedAndImportFile);
    fixture.window.do_this(Command::LinkAndImportFile);

    assert_eq!(
        fixture.window.application().opening_purposes_started,
        vec![
            OpeningPurpose::ImportNewSourceAndEmbedIt,
            OpeningPurpose::ImportNewSource
        ]
    );
}

#[test]
fn a_new_project_asks_for_a_file_to_be_linked_and_not_one_to_be_embedded() {
    let mut fixture = Fixture::new();
    let path = the_file_to_import(&fixture);
    fixture.chooser.will_answer(Some(path));

    fixture.window.do_this(Command::NewProject);

    assert_eq!(
        fixture.window.application().opening_purposes_started,
        vec![OpeningPurpose::ImportNewSource]
    );
    assert!(
        !matches!(
            dialog_showing(&fixture.window),
            Some(Dialog::MustSaveAsAnArchiveToEmbedAFile { .. })
        ),
        "no question about archives (there is no ffmpeg in a test, which is what is said)"
    );
}

#[test]
fn the_file_chosen_after_the_project_was_saved_as_an_archive_to_embed_it_is_opened_to_be_embedded() {
    let mut fixture = Fixture::new();
    fixture.window.do_this(Command::EmbedAndImportFile);
    fixture.chooser.will_answer(Some(fixture.path("trip.ffrs")));
    let path = the_file_to_import(&fixture);
    fixture.chooser.will_answer(Some(path));

    click(&mut fixture, "Choose where to save it");

    assert_eq!(
        fixture.window.application().opening_purposes_started,
        vec![OpeningPurpose::ImportNewSourceAndEmbedIt]
    );
}

// --- embedding, when the project file is an archive ---

#[test]
fn embedding_in_a_project_whose_file_is_an_archive_asks_for_the_file_at_once() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    fixture.chooser.will_answer(None);

    fixture.window.do_this(Command::EmbedAndImportFile);

    assert_eq!(fixture.chooser.questions_asked().len(), 1);
    assert_eq!(dialog_showing(&fixture.window), None);
    assert!(!fixture.window.application().can_undo(), "nothing was imported");
}

#[test]
fn an_embedded_file_is_put_on_the_timeline_and_in_the_archive_and_the_project_is_saved_with_it() {
    let mut fixture = Fixture::new();
    let archive_path = fixture.save_as("trip.ffrs");
    let path = the_file_to_import(&fixture);
    let clips_before = number_of_clips(&fixture);

    the_file_opened(&mut fixture, OpeningPurpose::ImportNewSourceAndEmbedIt, path.clone());

    assert_eq!(number_of_clips(&fixture), clips_before + 1, "it is on the timeline");
    assert!(
        fixture.is_included(THE_NEW_SOURCE),
        "and the project says it is included"
    );
    assert_eq!(
        fixture.the_link_of(THE_NEW_SOURCE).absolute_path,
        std::path::absolute(&path).unwrap(),
        "where it came from is remembered, for taking it out again"
    );
    assert_eq!(
        names_of_the_entries(&archive_path),
        vec![PROJECT_ENTRY_NAME.to_owned(), "resources/3-new sound.wav".to_owned()],
        "the archive has it"
    );
    assert!(
        !fixture.window.application().has_unsaved_changes(),
        "the project is saved with it"
    );
    let location = fixture
        .window
        .application()
        .location_of_the_source(THE_NEW_SOURCE)
        .expect("the file is available");
    assert_eq!(
        location.file(),
        std::path::absolute(&archive_path).unwrap(),
        "it is read where it is in the archive from now on"
    );
    assert!(location.stretch().is_some());
    assert!(
        read_project_file(&archive_path)
            .unwrap()
            .source(THE_NEW_SOURCE)
            .unwrap()
            .link
            .is_included()
    );
    assert_eq!(dialog_showing(&fixture.window), None);
}

#[test]
fn embedding_does_not_include_the_other_files_of_the_project() {
    let mut fixture = Fixture::new();
    let archive_path = fixture.save_as("trip.ffrs");
    let path = the_file_to_import(&fixture);

    the_file_opened(&mut fixture, OpeningPurpose::ImportNewSourceAndEmbedIt, path);

    assert!(!fixture.is_included(super::test_window::VIDEO));
    assert!(
        !names_of_the_entries(&archive_path)
            .iter()
            .any(|name| name.contains("holiday"))
    );
}

#[test]
fn an_embedded_import_can_be_undone_and_the_file_stays_in_the_archive_as_what_the_next_compaction_drops() {
    let mut fixture = Fixture::new();
    let archive_path = fixture.save_as("trip.ffrs");
    let path = the_file_to_import(&fixture);
    let clips_before = fixture.window.clips();
    the_file_opened(&mut fixture, OpeningPurpose::ImportNewSourceAndEmbedIt, path);

    fixture.window.do_this(Command::Undo);

    assert_eq!(fixture.window.clips(), clips_before, "the clip is off the timeline");
    assert!(
        names_of_the_entries(&archive_path).contains(&"resources/3-new sound.wav".to_owned()),
        "the file is still in the archive: undoing does not rewrite it"
    );
}

#[test]
fn an_embedded_file_that_cannot_be_written_is_left_where_it_is_and_the_user_is_told() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    let gone = fixture.path("media").join("gone.wav");

    the_file_opened(&mut fixture, OpeningPurpose::ImportNewSourceAndEmbedIt, gone);

    assert!(!fixture.is_included(THE_NEW_SOURCE), "it stays linked");
    assert!(fixture.window.application().project().source(THE_NEW_SOURCE).is_some());
    assert!(
        matches!(dialog_showing(&fixture.window), Some(Dialog::Message { title, .. }) if title == "The project could not be saved"),
        "{:?}",
        dialog_showing(&fixture.window)
    );
    assert!(
        fixture.window.application().has_unsaved_changes(),
        "and the import is not saved, as it was not before"
    );
}

#[test]
fn a_file_that_could_not_be_opened_is_not_embedded_and_the_user_is_told() {
    let mut fixture = Fixture::new();
    let archive_path = fixture.save_as("trip.ffrs");
    let length_before = fs::metadata(&archive_path).unwrap().len();
    let clips_before = fixture.window.clips();

    fixture.window.application_mut().finish_opening(
        OpeningPurpose::ImportNewSourceAndEmbedIt,
        OpeningOutcome::Failed {
            attempts: vec![(MediaLocation::of_a_file("a.txt"), "it is not a media file".to_owned())],
        },
    );
    fixture.window.settle();

    assert_eq!(fixture.window.clips(), clips_before);
    assert_eq!(fs::metadata(&archive_path).unwrap().len(), length_before);
    assert!(matches!(
        dialog_showing(&fixture.window),
        Some(Dialog::Message { title, .. }) if title == "The file could not be opened"
    ));
}

#[test]
fn a_file_that_is_a_great_deal_to_copy_is_embedded_in_the_background_with_the_window_that_says_how_far_it_has_got() {
    let mut fixture = Fixture::new();
    let archive_path = fixture.save_as("trip.ffrs");
    let path = the_file_to_import(&fixture);
    fixture.window.application_mut().copy_in_the_background_from(0);
    let gate = Gate::closed();
    fixture.window.application_mut().copy_gate = Some(gate.clone());

    the_file_opened(&mut fixture, OpeningPurpose::ImportNewSourceAndEmbedIt, path);

    assert!(fixture.window.application().is_working_in_the_background());
    assert_eq!(
        fixture
            .window
            .application()
            .progress_of_the_running_save()
            .map(|(heading, _, to_copy)| (heading, to_copy)),
        Some(("Putting files in the project file", THE_NEW_BYTES.len() as u64))
    );
    gate.let_through();
    let started = Instant::now();
    while fixture.window.application().running_save.is_some() {
        assert!(started.elapsed() < Duration::from_secs(20), "the save did not finish");
        fixture.window.harness.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    fixture.window.settle();
    assert!(fixture.is_included(THE_NEW_SOURCE));
    assert!(names_of_the_entries(&archive_path).contains(&"resources/3-new sound.wav".to_owned()));
}

// --- embedding, when the project file is not an archive ---

#[test]
fn embedding_in_a_project_that_has_never_been_saved_asks_to_save_it_as_an_archive_before_asking_for_any_file() {
    let mut fixture = Fixture::new();

    fixture.window.do_this(Command::EmbedAndImportFile);

    assert_eq!(
        dialog_showing(&fixture.window),
        Some(&Dialog::MustSaveAsAnArchiveToEmbedAFile {
            has_a_plain_project_file: false
        })
    );
    assert!(
        fixture.chooser.questions_asked().is_empty(),
        "no question about a file is asked before the user has agreed"
    );
    assert!(
        fixture
            .window
            .harness
            .query_by_label("Choose where to save it")
            .is_some()
            && fixture.window.harness.query_by_label("Cancel").is_some(),
        "the dialog has its buttons"
    );
}

#[test]
fn embedding_in_a_project_saved_as_plain_json_says_so_in_the_dialog() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.json");

    fixture.window.do_this(Command::EmbedAndImportFile);

    assert_eq!(
        dialog_showing(&fixture.window),
        Some(&Dialog::MustSaveAsAnArchiveToEmbedAFile {
            has_a_plain_project_file: true
        })
    );
}

#[test]
fn cancelling_the_question_about_saving_as_an_archive_does_nothing_at_all() {
    let mut fixture = Fixture::new();
    fixture.window.do_this(Command::EmbedAndImportFile);

    click(&mut fixture, "Cancel");

    assert_eq!(dialog_showing(&fixture.window), None);
    assert!(fixture.chooser.questions_asked().is_empty());
    assert_eq!(fixture.window.application().project_file_path(), None);
}

#[test]
fn agreeing_saves_the_project_as_an_archive_and_then_asks_for_the_file() {
    let mut fixture = Fixture::new();
    fixture.window.do_this(Command::EmbedAndImportFile);
    let destination = fixture.path("trip.ffrs");
    fixture.chooser.will_answer(Some(destination.clone()));
    fixture.chooser.will_answer(None);

    click(&mut fixture, "Choose where to save it");

    let asked = fixture.chooser.questions_asked();
    assert_eq!(asked.len(), 2, "where to save it, and then which file");
    assert!(asked[0].saving);
    assert_eq!(
        asked[0].kinds,
        vec![("Project files (.ffrs)".to_owned(), vec!["ffrs".to_owned()])],
        "only the kind of project file that can have files in it"
    );
    assert!(!asked[1].saving);
    assert_eq!(asked[1].title, "Choose a video, a sound or a picture to embed");
    assert_eq!(
        fixture.window.application().project_file_path(),
        Some(destination.as_path())
    );
    assert_eq!(
        names_of_the_entries(&destination),
        vec![PROJECT_ENTRY_NAME.to_owned()],
        "the project is saved, with nothing included yet"
    );
    assert!(!fixture.window.application().has_unsaved_changes());
}

#[test]
fn a_name_that_does_not_end_as_an_archive_does_is_given_the_ending_so_that_the_file_can_hold_files() {
    let mut fixture = Fixture::new();
    fixture.window.do_this(Command::EmbedAndImportFile);
    fixture.chooser.will_answer(Some(fixture.path("trip")));
    fixture.chooser.will_answer(None);

    click(&mut fixture, "Choose where to save it");

    assert_eq!(
        fixture.window.application().project_file_path(),
        Some(fixture.path("trip.ffrs").as_path())
    );
}

#[test]
fn choosing_no_place_to_save_the_archive_asks_for_no_file_and_changes_nothing() {
    let mut fixture = Fixture::new();
    fixture.window.do_this(Command::EmbedAndImportFile);
    fixture.chooser.will_answer(None);

    click(&mut fixture, "Choose where to save it");

    assert_eq!(fixture.chooser.questions_asked().len(), 1, "only where to save it");
    assert_eq!(fixture.window.application().project_file_path(), None);
    assert_eq!(dialog_showing(&fixture.window), None);
}

#[test]
fn a_project_that_cannot_be_saved_as_an_archive_is_told_so_and_no_file_is_asked_for() {
    let mut fixture = Fixture::new();
    fixture.window.do_this(Command::EmbedAndImportFile);
    fixture
        .chooser
        .will_answer(Some(fixture.path("no such folder").join("trip.ffrs")));

    click(&mut fixture, "Choose where to save it");

    assert_eq!(fixture.chooser.questions_asked().len(), 1, "no file is asked for");
    assert!(matches!(
        dialog_showing(&fixture.window),
        Some(Dialog::Message { title, .. }) if title == "The project could not be saved"
    ));
    assert_eq!(fixture.window.application().project_file_path(), None);
}

// --- linking ---

#[test]
fn a_linked_file_is_put_on_the_timeline_and_is_not_put_in_the_archive() {
    let mut fixture = Fixture::new();
    let archive_path = fixture.save_as("trip.ffrs");
    let length_before = fs::metadata(&archive_path).unwrap().len();
    let path = the_file_to_import(&fixture);
    let clips_before = number_of_clips(&fixture);

    the_file_opened(&mut fixture, OpeningPurpose::ImportNewSource, path.clone());

    assert_eq!(number_of_clips(&fixture), clips_before + 1);
    assert!(!fixture.is_included(THE_NEW_SOURCE));
    assert_eq!(
        fs::metadata(&archive_path).unwrap().len(),
        length_before,
        "the project file was not written"
    );
    assert!(
        fixture.window.application().has_unsaved_changes(),
        "it is an edit like another, saved when the project is"
    );
    assert_eq!(
        fixture
            .window
            .application()
            .location_of_the_source(THE_NEW_SOURCE)
            .unwrap()
            .file(),
        path.as_path()
    );
}

#[test]
fn linking_in_a_project_that_has_never_been_saved_asks_nothing_about_archives() {
    let mut fixture = Fixture::new();
    fixture.chooser.will_answer(None);

    fixture.window.do_this(Command::LinkAndImportFile);

    assert_eq!(dialog_showing(&fixture.window), None);
    assert_eq!(fixture.chooser.questions_asked().len(), 1);
}
