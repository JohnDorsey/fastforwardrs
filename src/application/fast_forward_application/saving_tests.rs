//! Tests of saving in the background: a save that copies a great deal says how far it has got, can be stopped, and leaves the project
//! file as it was when it is, and nothing else is done to the project while it goes on. (A copy that is made to wait, with a gate, so
//! that what the window does while it goes on can be looked at.)

use std::fs;
use std::time::{Duration, Instant};

use egui_kittest::kittest::Queryable;

use super::included_files_tests::{Fixture, THE_VIDEO, dialog_showing, names_of_the_entries};
use super::saving::Gate;
use super::test_window::{VIDEO, Window};
use super::{SourceFileState, SourceStatus};
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::project_file::archive::{self, PROJECT_ENTRY_NAME};
use crate::project_file::read_project_file;

const ENTRY: &str = "resources/1-holiday.mp4";

/// Draws the window until the save that is being written in the background is done.
pub(super) fn wait_until_the_save_is_done(window: &mut Window) {
    let started = Instant::now();
    while window.application().running_save.is_some() {
        assert!(started.elapsed() < Duration::from_secs(20), "the save did not finish");
        window.harness.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    window.settle();
}

/// A fixture whose project is saved as an archive, whose saves are all done in the background, and are held by a gate.
fn fixture_whose_saves_wait() -> (Fixture, std::path::PathBuf, std::sync::Arc<Gate>) {
    let mut fixture = Fixture::new();
    let archive_path = fixture.save_as("trip.ffrs");
    fixture.window.application_mut().copy_in_the_background_from(0);
    let gate = Gate::closed();
    fixture.window.application_mut().copy_gate = Some(gate.clone());
    (fixture, archive_path, gate)
}

fn entries_of_the_archive(archive_path: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = archive::state_of_archive(archive_path)
        .unwrap()
        .entries
        .into_keys()
        .collect();
    names.sort();
    names
}

#[test]
fn a_save_that_copies_little_is_not_done_in_the_background() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");

    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));

    assert!(
        !fixture.window.application().is_working_in_the_background(),
        "a few dozen bytes are copied at once"
    );
    assert!(fixture.is_included(VIDEO));
}

#[test]
fn a_save_that_copies_a_great_deal_goes_on_in_the_background_and_a_window_says_how_far_it_has_got() {
    let (mut fixture, archive_path, gate) = fixture_whose_saves_wait();

    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));

    assert!(fixture.window.application().is_working_in_the_background());
    let (heading, copied, to_copy) = fixture.window.application().progress_of_the_running_save().unwrap();
    assert_eq!(heading, "Putting files in the project file");
    assert_eq!(to_copy, THE_VIDEO.len() as u64);
    assert!(copied <= to_copy);
    assert!(
        fixture.window.harness.query_by_label("Cancel").is_some(),
        "the window with the Cancel button"
    );
    assert_eq!(
        entries_of_the_archive(&archive_path),
        vec![PROJECT_ENTRY_NAME.to_owned()],
        "the project file is as it was until the save is whole"
    );
    assert!(
        fixture.window.application().is_blocked(),
        "nothing is done to the project meanwhile"
    );

    gate.let_through();
    wait_until_the_save_is_done(&mut fixture.window);

    assert!(!fixture.window.application().is_working_in_the_background());
    assert!(
        fixture.window.harness.query_by_label("Cancel").is_none(),
        "the window is gone"
    );
    assert_eq!(
        names_of_the_entries(&archive_path),
        vec![PROJECT_ENTRY_NAME.to_owned(), ENTRY.to_owned()]
    );
    assert!(fixture.is_included(VIDEO));
    assert!(
        !fixture.window.application().has_unsaved_changes(),
        "the project is saved"
    );
    assert!(
        read_project_file(&archive_path).unwrap().sources()[0]
            .link
            .is_included()
    );
    assert_eq!(dialog_showing(&fixture.window), None);
}

#[test]
fn the_window_says_how_much_of_what_is_to_be_copied_has_been_copied_so_far() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    fs::write(fixture.path("media").join("holiday.mp4"), vec![7_u8; 3 << 20]).unwrap();
    fixture.window.application_mut().copy_in_the_background_from(0);
    let gate = Gate::closed();
    fixture.window.application_mut().copy_gate = Some(gate.clone());
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));

    // The copy waits after the first piece of the video (a megabyte at a time), which is told as soon as it is copied.
    let started = Instant::now();
    let (copied, to_copy) = loop {
        let (_, copied, to_copy) = fixture.window.application().progress_of_the_running_save().unwrap();
        if copied > 0 {
            break (copied, to_copy);
        }
        assert!(started.elapsed() < Duration::from_secs(20), "nothing was copied");
        std::thread::sleep(Duration::from_millis(2));
    };

    assert_eq!(to_copy, 3 << 20, "all of the video, and not the text of the project");
    assert!(
        copied < to_copy,
        "the copy is not done: it is waiting, with {copied} of {to_copy} bytes copied"
    );
    gate.let_through();
    wait_until_the_save_is_done(&mut fixture.window);
    assert!(fixture.is_included(VIDEO));
}

#[test]
fn stopping_a_save_in_the_background_leaves_the_project_file_and_the_project_as_they_were() {
    let (mut fixture, archive_path, gate) = fixture_whose_saves_wait();
    let bytes_before = fs::read(&archive_path).unwrap();
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    assert!(fixture.window.application().is_working_in_the_background());

    let cancel = fixture.window.harness.get_by_label("Cancel").rect().center();
    fixture.window.click(cancel);
    gate.let_through();
    wait_until_the_save_is_done(&mut fixture.window);

    assert_eq!(
        fs::read(&archive_path).unwrap(),
        bytes_before,
        "the project file is as it was, byte for byte: what was added to it is gone"
    );
    assert!(
        !fixture.is_included(VIDEO),
        "the project is as it was before the file was said to be included"
    );
    assert_eq!(
        dialog_showing(&fixture.window),
        None,
        "stopping it on purpose is not a failure to be told of"
    );
    assert!(!fixture.window.application().is_working_in_the_background());

    // And it can be done again.
    fixture.window.application_mut().copy_in_the_background_from(u64::MAX);
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    assert!(fixture.is_included(VIDEO));
    assert_eq!(
        entries_of_the_archive(&archive_path),
        vec![PROJECT_ENTRY_NAME.to_owned(), ENTRY.to_owned()]
    );
}

/// Saves the project of a fixture that has its video included, with that video in the state `status`; says what the user was told.
fn what_saving_says_when_the_included_video_is_in_the_state(
    status: Option<SourceStatus>,
) -> (bool, String, Vec<u8>, Vec<u8>) {
    let mut fixture = Fixture::new();
    let archive_path = fixture.save_as("trip.ffrs");
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    assert_eq!(dialog_showing(&fixture.window), None);
    let bytes_before = fs::read(&archive_path).unwrap();
    match status {
        Some(status) => {
            fixture
                .window
                .application_mut()
                .source_files
                .insert(VIDEO, SourceFileState::new(status));
        }
        None => {
            fixture.window.application_mut().source_files.remove(&VIDEO);
        }
    }
    fixture.window.do_this(Command::AddTrack);

    let saved = fixture.window.application_mut().save_project_to(archive_path.clone());

    let Some(Dialog::Message { title, text }) = dialog_showing(&fixture.window) else {
        panic!("the user was told nothing: {:?}", dialog_showing(&fixture.window));
    };
    assert_eq!(title, "The project could not be saved");
    (saved, text.clone(), bytes_before, fs::read(&archive_path).unwrap())
}

#[test]
fn a_project_whose_included_file_is_still_being_opened_is_not_saved_and_the_user_is_told_why() {
    let (saved, text, before, after) =
        what_saving_says_when_the_included_video_is_in_the_state(Some(SourceStatus::Opening));

    assert!(!saved);
    assert!(
        text.contains("holiday.mp4") && text.contains("still being opened"),
        "{text}"
    );
    assert_eq!(before, after, "the project file is as it was");
}

#[test]
fn a_project_whose_included_file_cannot_be_used_just_now_is_not_saved_and_the_user_is_told_why() {
    let (saved, text, before, after) =
        what_saving_says_when_the_included_video_is_in_the_state(Some(SourceStatus::Unavailable {
            reason: "it is damaged".to_owned(),
        }));

    assert!(!saved);
    assert!(
        text.contains("holiday.mp4") && text.contains("cannot be used just now (it is damaged)"),
        "{text}"
    );
    assert_eq!(before, after, "the project file is as it was");
}

#[test]
fn a_project_whose_included_file_has_not_been_opened_yet_is_not_saved_and_the_user_is_told_why() {
    let (saved, text, before, after) = what_saving_says_when_the_included_video_is_in_the_state(None);

    assert!(!saved);
    assert!(
        text.contains("holiday.mp4") && text.contains("has not been opened yet"),
        "{text}"
    );
    assert_eq!(before, after, "the project file is as it was");
}

#[test]
fn a_save_asked_for_while_another_is_going_on_is_refused_and_the_one_that_is_going_on_goes_on() {
    let (mut fixture, archive_path, gate) = fixture_whose_saves_wait();
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    assert!(fixture.window.application().is_working_in_the_background());
    let other = fixture.path("other.ffrs");

    let saved = fixture.window.application_mut().save_project_to(other.clone());

    assert!(!saved);
    assert!(
        matches!(
            dialog_showing(&fixture.window),
            Some(Dialog::Message { title, text })
                if title == "The project could not be saved" && text == "It is being saved already."
        ),
        "{:?}",
        dialog_showing(&fixture.window)
    );
    assert!(
        fixture.window.application().is_working_in_the_background(),
        "the save that was going on goes on"
    );
    gate.let_through();
    wait_until_the_save_is_done(&mut fixture.window);
    assert!(!other.exists(), "nothing was written to the other file");
    assert!(fixture.is_included(VIDEO));
    assert_eq!(
        names_of_the_entries(&archive_path),
        vec![PROJECT_ENTRY_NAME.to_owned(), ENTRY.to_owned()]
    );
}

#[test]
fn autosave_waits_while_a_save_is_going_on_and_works_when_it_is_done() {
    let (mut fixture, archive_path, gate) = fixture_whose_saves_wait();
    fixture.window.application_mut().global_settings.autosave = true;
    let context = egui::Context::default();
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    let tracks_at_the_start = fixture.window.application().project().tracks().len();
    fixture.window.do_this(Command::AddTrack);

    fixture
        .window
        .application_mut()
        .autosave_when_quiet_for_long_enough(&context, Instant::now() + Duration::from_secs(5));
    assert!(
        fixture.window.application().has_unsaved_changes(),
        "it does not save over what is being saved"
    );
    assert_eq!(
        dialog_showing(&fixture.window),
        None,
        "and it does not even try, which would be said"
    );

    gate.let_through();
    wait_until_the_save_is_done(&mut fixture.window);
    fixture.window.do_this(Command::AddTrack);
    fixture
        .window
        .application_mut()
        .autosave_when_quiet_for_long_enough(&context, Instant::now() + Duration::from_secs(5));
    assert!(!fixture.window.application().has_unsaved_changes());
    assert_eq!(
        read_project_file(&archive_path).unwrap().tracks().len(),
        tracks_at_the_start + 2
    );
}

#[test]
fn a_project_that_is_saved_as_another_file_is_written_with_its_files_in_the_background_and_reads_them_from_the_new_file()
 {
    let mut fixture = Fixture::new();
    let first_archive = fixture.save_as("first.ffrs");
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    fixture.window.application_mut().copy_in_the_background_from(0);
    let gate = Gate::closed();
    fixture.window.application_mut().copy_gate = Some(gate.clone());
    let second_archive = fixture.path("second.ffrs");

    assert!(fixture.window.application_mut().save_project_to(second_archive.clone()));
    assert!(fixture.window.application().is_working_in_the_background());
    assert_eq!(
        fixture.window.application().project_file_path(),
        Some(first_archive.as_path()),
        "the project's file is the new one only when it is whole"
    );
    gate.let_through();
    wait_until_the_save_is_done(&mut fixture.window);

    assert_eq!(
        fixture.window.application().project_file_path(),
        Some(second_archive.as_path())
    );
    assert_eq!(
        names_of_the_entries(&second_archive),
        vec![PROJECT_ENTRY_NAME.to_owned(), ENTRY.to_owned()]
    );
    let location = fixture
        .window
        .application()
        .location_of_the_source(VIDEO)
        .expect("the video is there");
    assert_eq!(location.file(), std::path::absolute(&second_archive).unwrap());
    assert!(location.stretch().is_some());
    assert_eq!(
        names_of_the_entries(&first_archive),
        vec![PROJECT_ENTRY_NAME.to_owned(), ENTRY.to_owned()],
        "the first file is as it was"
    );
}

#[test]
fn closing_the_window_while_a_save_is_going_on_stops_it_and_leaves_the_project_file_as_it_was() {
    let (fixture, archive_path, _gate) = fixture_whose_saves_wait();
    let Fixture {
        mut window,
        folder,
        chooser,
    } = fixture;
    let bytes_before = fs::read(&archive_path).unwrap();
    window.do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    assert!(window.application().is_working_in_the_background());

    // (The gate is never opened: the copy is waiting when the window goes.)
    drop(window);

    assert_eq!(fs::read(&archive_path).unwrap(), bytes_before);
    drop((folder, chooser));
}
