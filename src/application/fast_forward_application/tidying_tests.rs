// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of writing the project file again without the garbage that saving adds to it leaves in it: when the user saves (and nothing is
//! read in place), and when the window is closed (and everything that is read in place is let go of first). Never by an autosave.
//! (With real files read in place by ffmpeg, which is what makes it fail on Windows when something still has the file open, it is
//! checked in `tests/application_journeys.rs`.)

use std::fs;
use std::time::{Duration, Instant};

use egui::ViewportCommand;
use egui_kittest::kittest::Queryable;

use super::included_files_tests::{Fixture, THE_VIDEO, dialog_showing, names_of_the_entries, the_sound};
use super::saving::Gate;
use super::test_window::{VIDEO, Window};
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::project_file::archive::{self, PROJECT_ENTRY_NAME};
use crate::project_file::read_project_file;

/// More than the least garbage that is worth writing a project file again to take away (4 MiB).
const MORE_THAN_WORTH_IT: usize = 5 << 20;

/// A fixture whose project is saved as an archive (`trip.ffrs`), and whose video and sound are each `MORE_THAN_WORTH_IT` bytes
/// of something, with copying always done at once.
fn fixture_with_big_files() -> (Fixture, std::path::PathBuf) {
    let mut fixture = Fixture::new();
    let archive_path = fixture.save_as("trip.ffrs");
    fs::write(
        fixture.path("media").join("holiday.mp4"),
        vec![b'v'; MORE_THAN_WORTH_IT],
    )
    .unwrap();
    fs::write(fixture.path("media").join("song.mp3"), vec![b's'; MORE_THAN_WORTH_IT]).unwrap();
    fixture.window.application_mut().copy_in_the_background_from(u64::MAX);
    (fixture, archive_path)
}

fn include(fixture: &mut Fixture, files: Vec<crate::project::SourceMediaIdentifier>) {
    fixture.window.do_this(Command::IncludeFilesInTheProjectFile(files));
    assert_eq!(dialog_showing(&fixture.window), None);
}

fn take_out(fixture: &mut Fixture, files: Vec<crate::project::SourceMediaIdentifier>) {
    fixture.window.do_this(Command::TakeFilesOutOfTheProjectFile(files));
    assert_eq!(dialog_showing(&fixture.window), None);
}

fn garbage_in(archive_path: &std::path::Path) -> u64 {
    archive::state_of_archive(archive_path).unwrap().bytes_of_garbage
}

// --- when the user saves ---

#[test]
fn saving_writes_the_project_file_again_when_files_that_were_taken_out_of_it_are_garbage_worth_taking_away_and_none_is_read_in_it()
 {
    let (mut fixture, archive_path) = fixture_with_big_files();
    include(&mut fixture, vec![VIDEO]);
    take_out(&mut fixture, vec![VIDEO]);
    assert!(fs::metadata(&archive_path).unwrap().len() > MORE_THAN_WORTH_IT as u64);

    fixture.window.do_this(Command::SaveProject);

    assert_eq!(names_of_the_entries(&archive_path), vec![PROJECT_ENTRY_NAME.to_owned()]);
    assert_eq!(garbage_in(&archive_path), 0);
    assert!(
        fs::metadata(&archive_path).unwrap().len() < 1 << 20,
        "the video's bytes are gone from the file"
    );
    assert!(!fixture.window.application().has_unsaved_changes());
    assert_eq!(dialog_showing(&fixture.window), None);
    let project = read_project_file(&archive_path).unwrap();
    assert!(
        !project.source(VIDEO).unwrap().link.is_included(),
        "and it is still a project file"
    );
}

/// How long the file is, in bytes.
fn length_of(path: &std::path::Path) -> u64 {
    fs::metadata(path).unwrap().len()
}

#[test]
fn saving_again_and_again_a_project_that_has_no_file_included_in_it_does_not_make_its_project_file_longer() {
    // Regression test: every save added the text of the project to the file again (a project of 35 KB was 35 KB longer after each save), because
    // the garbage that saves leave was taken away only when it was some megabytes, which is no use for a file that is only text.
    let mut fixture = Fixture::new();
    let archive_path = fixture.save_as("trip.ffrs");
    let length_after_the_first_save = length_of(&archive_path);

    for _ in 0..5 {
        fixture.window.do_this(Command::SaveProject);
    }

    assert_eq!(length_of(&archive_path), length_after_the_first_save);
    assert_eq!(garbage_in(&archive_path), 0);
}

#[test]
fn loading_a_project_file_and_saving_it_without_a_change_does_not_make_it_longer_however_many_times() {
    // Regression test: the steps of the maintainer's report (load, save, with nothing changed in between) made the file 35 KB longer each time.
    let mut fixture = Fixture::new();
    let archive_path = fixture.save_as("trip.ffrs");
    let length_when_it_was_made = length_of(&archive_path);

    for _ in 0..3 {
        fixture.window.application_mut().load_project(archive_path.clone());
        fixture.window.do_this(Command::SaveProject);
    }

    assert_eq!(length_of(&archive_path), length_when_it_was_made);
    assert!(
        read_project_file(&archive_path).is_ok(),
        "and it is still a project file"
    );
}

#[test]
fn the_garbage_that_autosaves_leave_in_a_small_project_file_is_taken_away_when_the_user_saves_and_when_the_window_is_closed()
 {
    let mut fixture = Fixture::new();
    let archive_path = fixture.save_as("trip.ffrs");
    fixture.window.application_mut().environment.global_settings.autosave = true;
    let context = egui::Context::default();
    for _ in 0..3 {
        fixture.window.do_this(Command::AddTrack);
        fixture
            .window
            .application_mut()
            .autosave_when_quiet_for_long_enough(&context, Instant::now() + Duration::from_secs(5));
    }
    assert!(garbage_in(&archive_path) > 0, "the autosaves added to the file");

    fixture.window.do_this(Command::SaveProject);
    assert_eq!(garbage_in(&archive_path), 0, "the save of the user took it away");

    fixture.window.do_this(Command::AddTrack);
    fixture
        .window
        .application_mut()
        .autosave_when_quiet_for_long_enough(&context, Instant::now() + Duration::from_secs(5));
    assert!(garbage_in(&archive_path) > 0);
    let commands = fixture.window.ask_to_close();
    assert!(!commands.contains(&ViewportCommand::CancelClose));
    assert_eq!(garbage_in(&archive_path), 0, "and so did the closing of the window");
}

#[test]
fn saving_writes_the_project_file_again_for_any_garbage_when_it_has_little_in_use() {
    // (This was the opposite once: "a few dozen bytes are not worth writing the file again for". They are, when writing it again takes a moment:
    // see the tests above, and `is_worth_compacting`.)
    let mut fixture = Fixture::new();
    let archive_path = fixture.save_as("trip.ffrs");
    include(&mut fixture, vec![VIDEO]);
    take_out(&mut fixture, vec![VIDEO]);
    assert!(
        garbage_in(&archive_path) >= THE_VIDEO.len() as u64,
        "the video that was taken out is garbage until then"
    );

    fixture.window.do_this(Command::SaveProject);

    assert_eq!(garbage_in(&archive_path), 0);
    assert!(!fixture.window.application().has_unsaved_changes());
}

#[test]
fn saving_does_not_write_the_project_file_again_while_a_file_is_read_in_it_but_does_when_none_is() {
    let (mut fixture, archive_path) = fixture_with_big_files();
    include(&mut fixture, vec![VIDEO, the_sound()]);
    take_out(&mut fixture, vec![the_sound()]);
    let bytes_before = archive::state_of_archive(&archive_path).unwrap();

    fixture.window.do_this(Command::SaveProject);

    let state = archive::state_of_archive(&archive_path).unwrap();
    assert!(
        state.bytes_of_garbage >= MORE_THAN_WORTH_IT as u64,
        "the sound that was taken out is still in the file: the video is read in it"
    );
    assert_eq!(
        state.entries["resources/1-holiday.mp4"], bytes_before.entries["resources/1-holiday.mp4"],
        "and the video is where it was, as it is read there"
    );
    assert!(
        fixture
            .window
            .application()
            .location_of_the_source(VIDEO)
            .is_some_and(|location| location.stretch().is_some())
    );

    take_out(&mut fixture, vec![VIDEO]);
    fixture.window.do_this(Command::SaveProject);

    assert_eq!(garbage_in(&archive_path), 0, "now nothing is read in it");
}

#[test]
fn a_save_that_is_not_the_save_of_the_menu_does_not_write_the_project_file_again() {
    let (mut fixture, archive_path) = fixture_with_big_files();
    include(&mut fixture, vec![VIDEO]);
    take_out(&mut fixture, vec![VIDEO]);

    // (The programmatic save is the save of the menu without what the user's save does after it.)
    assert!(fixture.window.application_mut().save_project_to(archive_path.clone()));

    assert!(garbage_in(&archive_path) >= MORE_THAN_WORTH_IT as u64);
}

#[test]
fn an_autosave_never_writes_the_project_file_again_however_much_garbage_there_is() {
    let (mut fixture, archive_path) = fixture_with_big_files();
    include(&mut fixture, vec![VIDEO]);
    take_out(&mut fixture, vec![VIDEO]);
    fixture.window.application_mut().environment.global_settings.autosave = true;
    let context = egui::Context::default();
    fixture.window.do_this(Command::AddTrack);

    fixture
        .window
        .application_mut()
        .autosave_when_quiet_for_long_enough(&context, Instant::now() + Duration::from_secs(5));

    assert!(!fixture.window.application().has_unsaved_changes(), "it was saved");
    assert!(
        garbage_in(&archive_path) >= MORE_THAN_WORTH_IT as u64,
        "but only added to"
    );
}

#[test]
fn a_damaged_file_stops_the_tidying_that_follows_a_save_and_says_so_and_leaves_the_project_file_as_it_was() {
    let (mut fixture, archive_path) = fixture_with_big_files();
    include(&mut fixture, vec![VIDEO]);
    take_out(&mut fixture, vec![VIDEO]);
    assert!(fixture.window.application_mut().save_project_to(archive_path.clone()));
    // The text of the project is what is left in the archive, and a byte of it is damaged.
    let text = archive::state_of_archive(&archive_path).unwrap().entries[PROJECT_ENTRY_NAME];
    let mut bytes = fs::read(&archive_path).unwrap();
    bytes[text.data_start as usize + 3] ^= 0xff;
    fs::write(&archive_path, &bytes).unwrap();

    fixture.window.application_mut().tidy_the_project_file_after_saving();

    assert_eq!(fs::read(&archive_path).unwrap(), bytes, "nothing was changed");
    assert!(
        matches!(
            dialog_showing(&fixture.window),
            Some(Dialog::Message { title, .. }) if title == "The project was saved, but its file could not be tidied up"
        ),
        "{:?}",
        dialog_showing(&fixture.window)
    );
}

// --- when the window is closed ---

/// Draws frames until the job that is going on in the background is done, and a few more; says what the window told its viewport to
/// do in all of them.
fn commands_until_the_job_is_done(window: &mut Window) -> Vec<ViewportCommand> {
    let started = Instant::now();
    while window.application().jobs.running_save.is_some() {
        assert!(started.elapsed() < Duration::from_secs(20), "the job did not finish");
        window.harness.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    let mut commands = Vec::new();
    for _ in 0..3 {
        window.harness.step();
        commands.extend(window.viewport_commands_of_the_last_frame());
    }
    commands
}

/// A fixture whose project file has a video read in it and as much garbage as a sound that was taken out of it, saved with the
/// menu (which could not write the file again, with the video read in it).
fn fixture_with_garbage_and_a_video_read_in_the_project_file() -> (Fixture, std::path::PathBuf) {
    let (mut fixture, archive_path) = fixture_with_big_files();
    include(&mut fixture, vec![VIDEO, the_sound()]);
    take_out(&mut fixture, vec![the_sound()]);
    fixture.window.do_this(Command::SaveProject);
    assert!(garbage_in(&archive_path) >= MORE_THAN_WORTH_IT as u64);
    (fixture, archive_path)
}

fn click_ok(window: &mut Window) {
    window.harness.get_by_label("OK").click();
    window.harness.step();
}

#[test]
fn closing_writes_the_project_file_again_after_letting_go_of_the_files_that_are_read_in_it() {
    let (mut fixture, archive_path) = fixture_with_garbage_and_a_video_read_in_the_project_file();

    let commands = fixture.window.ask_to_close();

    assert!(
        !commands.contains(&ViewportCommand::CancelClose),
        "it was little to copy: it was done at once, and the window closes: {commands:?}"
    );
    assert_eq!(garbage_in(&archive_path), 0);
    assert_eq!(
        names_of_the_entries(&archive_path),
        vec![PROJECT_ENTRY_NAME.to_owned(), "resources/1-holiday.mp4".to_owned()]
    );
    let state = archive::state_of_archive(&archive_path).unwrap();
    let place = state.entries["resources/1-holiday.mp4"];
    assert_eq!(place.length, MORE_THAN_WORTH_IT as u64);
    assert!(
        archive::have_the_same_contents(
            place.as_a_location_in(&archive_path).unwrap(),
            fixture.path("media").join("holiday.mp4").as_path()
        )
        .unwrap(),
        "the video is whole, where it is now"
    );
    assert_eq!(
        fixture.window.application().location_of_the_source(VIDEO),
        None,
        "nothing reads it any more: it was let go of, to be opened again where it is now by whoever loads the project"
    );
    assert_eq!(dialog_showing(&fixture.window), None);
    assert!(!fixture.window.application().is_working_in_the_background());
    fixture.window.harness.step();
    assert!(
        !fixture
            .window
            .viewport_commands_of_the_last_frame()
            .contains(&ViewportCommand::Close),
        "nothing keeps the window from closing, so it is not asked to close again by something that waited"
    );
}

#[test]
fn closing_writes_the_project_file_again_only_if_it_is_worth_it() {
    // More than a little is in use (two files of 5 MiB are included in it), and the garbage is the text of the project that the saves left.
    let (mut fixture, archive_path) = fixture_with_big_files();
    include(&mut fixture, vec![VIDEO, the_sound()]);
    fixture.window.do_this(Command::SaveProject);
    let state = archive::state_of_archive(&archive_path).unwrap();
    assert!(state.bytes_of_garbage > 0, "the saves left text");
    assert!(
        !state.is_worth_compacting(),
        "but it is a few kilobytes beside the files that are in it"
    );
    let bytes = fs::read(&archive_path).unwrap();

    let commands = fixture.window.ask_to_close();

    assert!(!commands.contains(&ViewportCommand::CancelClose));
    assert_eq!(
        fs::read(&archive_path).unwrap(),
        bytes,
        "so little garbage is not worth it"
    );
    assert!(
        fixture.window.application().location_of_the_source(VIDEO).is_some(),
        "and nothing is let go of when nothing is to be written"
    );
}

#[test]
fn closing_does_not_write_the_project_file_again_when_it_is_not_an_archive() {
    let mut fixture = Fixture::new();
    let plain = fixture.path("trip.json");
    assert!(fixture.window.application_mut().save_project_to(plain.clone()));
    let bytes = fs::read(&plain).unwrap();

    let commands = fixture.window.ask_to_close();

    assert!(!commands.contains(&ViewportCommand::CancelClose));
    assert_eq!(fs::read(&plain).unwrap(), bytes);
}

#[test]
fn closing_does_not_write_the_project_file_again_while_a_save_is_going_on() {
    let (mut fixture, _archive_path) = fixture_with_garbage_and_a_video_read_in_the_project_file();
    fixture.window.application_mut().copy_in_the_background_from(0);
    let gate = Gate::closed();
    fixture.window.application_mut().jobs.copy_gate = Some(gate.clone());
    // The sound is put in the project file again, which is a copy that goes on in the background (and is being waited for).
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![the_sound()]));
    assert!(fixture.window.application().is_working_in_the_background());
    // (What is not saved yet is a question of its own, answered before this one: it is as if it was answered.)
    fixture.window.application_mut().interface.close_confirmed = true;

    let commands = fixture.window.ask_to_close();

    assert!(
        !commands.contains(&ViewportCommand::CancelClose),
        "nothing is written again, and the window closes (and the save that is going on stops): {commands:?}"
    );
    assert!(
        fixture.window.application().location_of_the_source(VIDEO).is_some(),
        "nothing was let go of"
    );
    assert!(
        fixture.window.application().is_working_in_the_background(),
        "and what was going on is going on"
    );
    gate.let_through();
}

#[test]
fn closing_does_not_write_a_file_again_whose_name_is_not_the_name_of_an_archive_though_it_is_one() {
    let (mut fixture, archive_path) = fixture_with_garbage_and_a_video_read_in_the_project_file();
    let not_by_its_name = fixture.path("trip.json");
    fs::copy(&archive_path, &not_by_its_name).unwrap();
    fixture
        .window
        .application_mut()
        .load_project_from(not_by_its_name.clone());
    fixture.window.settle();
    let bytes = fs::read(&not_by_its_name).unwrap();
    assert_eq!(
        fixture.window.application().project_file_path(),
        Some(not_by_its_name.as_path())
    );

    let commands = fixture.window.ask_to_close();

    assert!(!commands.contains(&ViewportCommand::CancelClose), "{commands:?}");
    assert_eq!(
        fs::read(&not_by_its_name).unwrap(),
        bytes,
        "it is saved as plain JSON when it is saved: it is not one to write as an archive"
    );
}

#[test]
fn closing_with_changes_that_are_not_saved_asks_about_them_before_the_project_file_is_written_again() {
    let (mut fixture, archive_path) = fixture_with_garbage_and_a_video_read_in_the_project_file();
    fixture.window.do_this(Command::AddTrack);
    let bytes = fs::read(&archive_path).unwrap();

    let commands = fixture.window.ask_to_close();

    assert!(commands.contains(&ViewportCommand::CancelClose));
    assert!(matches!(
        dialog_showing(&fixture.window),
        Some(Dialog::ConfirmDiscardChanges { .. })
    ));
    assert_eq!(
        fs::read(&archive_path).unwrap(),
        bytes,
        "nothing is written before the question is answered"
    );
    assert!(fixture.window.application().location_of_the_source(VIDEO).is_some());

    // Agreeing to lose them closes the window, which is asked to close again, and then the file is written.
    fixture.window.settle();
    fixture.window.harness.get_by_label("Discard changes").click();
    fixture.window.settle();
    let commands = fixture.window.ask_to_close();
    eprintln!("DEBUG {commands:?} dialog {:?}", dialog_showing(&fixture.window));

    assert!(!commands.contains(&ViewportCommand::CancelClose));
    assert_eq!(garbage_in(&archive_path), 0);
}

#[test]
fn a_damaged_file_stops_the_tidying_before_closing_and_says_so_and_the_window_closes_when_it_has_been_read() {
    let (mut fixture, archive_path) = fixture_with_garbage_and_a_video_read_in_the_project_file();
    let video = archive::state_of_archive(&archive_path).unwrap().entries["resources/1-holiday.mp4"];
    let mut bytes = fs::read(&archive_path).unwrap();
    bytes[video.data_start as usize + 1000] ^= 0xff;
    fs::write(&archive_path, &bytes).unwrap();

    let commands = fixture.window.ask_to_close();

    assert!(
        commands.contains(&ViewportCommand::CancelClose),
        "the message is to be read first"
    );
    assert_eq!(fs::read(&archive_path).unwrap(), bytes, "the project file is as it was");
    assert!(
        matches!(
            dialog_showing(&fixture.window),
            Some(Dialog::Message { title, .. }) if title == "The project file could not be tidied up"
        ),
        "{:?}",
        dialog_showing(&fixture.window)
    );
    fixture.window.harness.step();
    assert!(
        !fixture
            .window
            .viewport_commands_of_the_last_frame()
            .contains(&ViewportCommand::Close),
        "not before the message has been read"
    );

    click_ok(&mut fixture.window);
    let commands = commands_until_the_job_is_done(&mut fixture.window);

    assert!(
        commands.contains(&ViewportCommand::Close),
        "the window closes: {commands:?}"
    );
    let commands = fixture.window.ask_to_close();
    assert!(
        !commands.contains(&ViewportCommand::CancelClose),
        "and it is not tried again"
    );
    assert_eq!(dialog_showing(&fixture.window), None);
}

#[test]
fn closing_with_a_great_deal_to_copy_keeps_the_window_open_with_a_window_that_says_how_far_it_has_got_and_closes_it_when_done()
 {
    let (mut fixture, archive_path) = fixture_with_garbage_and_a_video_read_in_the_project_file();
    fixture.window.application_mut().copy_in_the_background_from(0);
    let gate = Gate::closed();
    fixture.window.application_mut().jobs.copy_gate = Some(gate.clone());

    let commands = fixture.window.ask_to_close();
    fixture.window.settle();

    assert!(
        commands.contains(&ViewportCommand::CancelClose),
        "the window stays open: {commands:?}"
    );
    let (heading, _, to_copy) = fixture.window.application().progress_of_the_running_save().unwrap();
    assert_eq!(heading, "Tidying up the project file");
    assert!(to_copy >= MORE_THAN_WORTH_IT as u64, "the video is what is copied");
    assert!(fixture.window.harness.query_by_label("Cancel").is_some());
    assert!(
        garbage_in(&archive_path) >= MORE_THAN_WORTH_IT as u64,
        "nothing is changed before it is whole"
    );
    assert!(
        fixture.window.application().location_of_the_source(VIDEO).is_none(),
        "and what was reading it is gone, so that it can be replaced"
    );

    gate.let_through();
    let commands = commands_until_the_job_is_done(&mut fixture.window);

    assert_eq!(garbage_in(&archive_path), 0);
    assert_eq!(
        commands
            .iter()
            .filter(|command| **command == ViewportCommand::Close)
            .count(),
        1,
        "the window was asked to close, once: {commands:?}"
    );
    assert_eq!(dialog_showing(&fixture.window), None);
    let commands = fixture.window.ask_to_close();
    assert!(!commands.contains(&ViewportCommand::CancelClose), "and it closes now");
}

#[test]
fn giving_up_the_tidying_before_closing_leaves_the_project_file_as_it_was_and_the_window_closes_without_a_message() {
    let (mut fixture, archive_path) = fixture_with_garbage_and_a_video_read_in_the_project_file();
    let bytes_before = fs::read(&archive_path).unwrap();
    fixture.window.application_mut().copy_in_the_background_from(0);
    let gate = Gate::closed();
    fixture.window.application_mut().jobs.copy_gate = Some(gate.clone());
    fixture.window.ask_to_close();
    fixture.window.settle();

    let cancel = fixture.window.harness.get_by_label("Cancel").rect().center();
    let mut commands = Vec::new();
    fixture.window.move_pointer_to(cancel);
    fixture.window.press(cancel, egui::PointerButton::Primary);
    commands.extend(fixture.window.viewport_commands_of_the_last_frame());
    fixture.window.release(cancel, egui::PointerButton::Primary);
    commands.extend(fixture.window.viewport_commands_of_the_last_frame());
    gate.let_through();
    commands.extend(commands_until_the_job_is_done(&mut fixture.window));

    assert_eq!(fs::read(&archive_path).unwrap(), bytes_before);
    assert_eq!(dialog_showing(&fixture.window), None, "giving it up is not a failure");
    assert!(
        commands.contains(&ViewportCommand::Close),
        "the window closes: {commands:?}"
    );
}
