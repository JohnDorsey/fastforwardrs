//! Tests of including files in the project file and taking them out again: what the clip menu offers, the questions that are asked
//! when the project's file is not an archive, what is written when the project is saved, and what is done to put a file back.
//! (Loading a project that has files included takes them out of the archive and opens them with ffmpeg, which is
//! checked in `tests/application_journeys.rs`.)

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui_kittest::kittest::Queryable;

use super::test_window::{VIDEO, Window};
use super::{SourceFileState, SourceStatus};
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::application::file_chooser::ScriptedFileChooser;
use crate::project::{
    FrameRate, Project, SourceAudioProperties, SourceMedia, SourceMediaIdentifier, SourceMediaLink,
    SourceVideoProperties, TimelineFrameIndex,
};
use crate::project_file::archive::{self, PROJECT_ENTRY_NAME};
use crate::project_file::read_project_file;

const INCLUDE: &str = "Include in project file";
const REMOVE: &str = "Remove from project file";

/// The bytes of the video that the fixtures have, which are not a video: nothing here opens it.
const THE_VIDEO: &[u8] = b"not really a video, but some bytes that are kept as they are";

/// A folder with a video and a sound file in it, a project made from those two, and a window showing it with a chooser that
/// answers what it is told to.
struct Fixture {
    folder: tempfile::TempDir,
    chooser: ScriptedFileChooser,
    window: Window,
}

fn the_sound() -> SourceMediaIdentifier {
    SourceMediaIdentifier::new(2)
}

impl Fixture {
    fn new() -> Self {
        let folder = tempfile::tempdir().unwrap();
        fs::create_dir(folder.path().join("media")).unwrap();
        fs::write(folder.path().join("media").join("holiday.mp4"), THE_VIDEO).unwrap();
        fs::write(folder.path().join("media").join("song.mp3"), b"some sound").unwrap();
        let mut project = Project::empty();
        project
            .import_source(
                SourceMedia::video(
                    VIDEO,
                    SourceMediaLink::from_absolute_path(folder.path().join("media").join("holiday.mp4")),
                    SourceVideoProperties {
                        frame_rate: FrameRate::from_whole_frames_per_second(30).unwrap(),
                        total_frame_count: 300,
                        display_width_in_pixels: 1280,
                        display_height_in_pixels: 720,
                        has_audio: true,
                    },
                ),
                TimelineFrameIndex::new(0),
            )
            .unwrap();
        project
            .import_source(
                SourceMedia::audio(
                    the_sound(),
                    SourceMediaLink::from_absolute_path(folder.path().join("media").join("song.mp3")),
                    SourceAudioProperties {
                        duration_in_microseconds: 10_000_000,
                    },
                ),
                TimelineFrameIndex::new(0),
            )
            .unwrap();
        let chooser = ScriptedFileChooser::default();
        let mut window = Window::showing(project);
        window.use_file_chooser(&chooser);
        Self {
            folder,
            chooser,
            window,
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.folder.path().join(name)
    }

    /// Saves the project as an archive called `name` in the folder, which is its file from then on.
    fn save_as(&mut self, name: &str) -> PathBuf {
        let path = self.path(name);
        assert!(self.window.application_mut().save_project_to(path.clone()));
        path
    }

    fn the_link_of(&self, identifier: SourceMediaIdentifier) -> SourceMediaLink {
        self.window
            .application()
            .project()
            .source(identifier)
            .unwrap()
            .link
            .clone()
    }

    fn is_included(&self, identifier: SourceMediaIdentifier) -> bool {
        self.the_link_of(identifier).is_included()
    }
}

fn names_of_the_entries(archive_path: &Path) -> Vec<String> {
    let zip = zip::ZipArchive::new(fs::File::open(archive_path).unwrap()).unwrap();
    let mut names: Vec<String> = zip.file_names().map(|name| name.unwrap().into_owned()).collect();
    names.sort();
    names
}

fn dialog_showing(window: &Window) -> Option<&Dialog> {
    window.application().dialog.as_ref()
}

fn click(window: &mut Window, label: &str) {
    window.harness.get_by_label(label).click();
    window.settle();
}

fn is_offered(window: &Window, label: &str) -> bool {
    window.harness.query_by_label(label).is_some()
}

/// Whether anything is offered that has `word` in what it says, in whatever way it says it (an item that is there for no file at all
/// says "Include 0 files in project file", which is not the label that is checked for when there is one file).
fn is_anything_offered_that_says(window: &Window, word: &str) -> bool {
    window.harness.query_all_by_label_contains(word).next().is_some()
}

// --- including, in a project whose file is an archive ---

#[test]
fn including_a_file_in_a_project_whose_file_is_an_archive_says_so_and_changes_nothing_on_the_disk_yet() {
    let mut fixture = Fixture::new();
    let archive_path = fixture.save_as("trip.ffrs");
    assert_eq!(names_of_the_entries(&archive_path), vec![PROJECT_ENTRY_NAME.to_owned()]);

    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));

    let link = fixture.the_link_of(VIDEO);
    assert_eq!(link.included_as.as_deref(), Some("resources/1-holiday.mp4"));
    assert_eq!(
        link.absolute_path,
        std::path::absolute(fixture.path("media").join("holiday.mp4")).unwrap(),
        "where it is is what it is put back at"
    );
    assert_eq!(
        link.relative_path,
        Some(PathBuf::from("media/holiday.mp4")),
        "and where that is from the project"
    );
    assert!(!fixture.is_included(the_sound()), "the other file was not asked for");
    assert!(fixture.window.application().has_unsaved_changes());
    assert_eq!(dialog_showing(&fixture.window), None, "nothing is asked");
    assert_eq!(
        names_of_the_entries(&archive_path),
        vec![PROJECT_ENTRY_NAME.to_owned()],
        "the file is put in the project file when the project is saved"
    );
    assert!(fixture.chooser.questions_asked().is_empty());
}

#[test]
fn the_file_that_was_included_is_in_the_project_file_when_the_project_is_saved() {
    let mut fixture = Fixture::new();
    let archive_path = fixture.save_as("trip.ffrs");
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));

    fixture.window.do_this(Command::SaveProject);

    assert!(!fixture.window.application().has_unsaved_changes());
    assert_eq!(
        names_of_the_entries(&archive_path),
        vec![PROJECT_ENTRY_NAME.to_owned(), "resources/1-holiday.mp4".to_owned()]
    );
    let taken_out = fixture.path("taken out.mp4");
    archive::extract_entry(&archive_path, "resources/1-holiday.mp4", &taken_out).unwrap();
    assert_eq!(fs::read(&taken_out).unwrap(), THE_VIDEO);
    let loaded = read_project_file(&archive_path).unwrap();
    assert!(loaded.sources()[0].link.is_included());
    assert!(!loaded.sources()[1].link.is_included());
}

#[test]
fn including_is_not_an_edit_that_can_be_undone_and_undoing_what_came_before_keeps_the_file_included() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    fixture.window.do_this(Command::AddTrack);
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));

    fixture.window.do_this(Command::Undo);

    assert_eq!(
        fixture.window.application().project().tracks().len(),
        2,
        "the track that was added was taken back: it was the edit"
    );
    assert!(
        fixture.is_included(VIDEO),
        "where the file is kept is not an edit of the project"
    );
    fixture.window.do_this(Command::Redo);
    assert!(fixture.is_included(VIDEO));
}

#[test]
fn a_file_that_is_included_already_is_not_included_again_and_nothing_is_said() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    let before = fixture.the_link_of(VIDEO);
    fixture.window.do_this(Command::SaveProject);

    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO, VIDEO]));

    assert_eq!(fixture.the_link_of(VIDEO), before);
    assert!(!fixture.window.application().has_unsaved_changes(), "nothing changed");
    assert_eq!(dialog_showing(&fixture.window), None);
}

#[test]
fn several_files_are_included_at_once_and_each_is_in_the_project_file_as_its_own_entry() {
    let mut fixture = Fixture::new();
    let archive_path = fixture.save_as("trip.ffrs");

    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO, the_sound()]));
    fixture.window.do_this(Command::SaveProject);

    assert_eq!(
        names_of_the_entries(&archive_path),
        vec![
            PROJECT_ENTRY_NAME.to_owned(),
            "resources/1-holiday.mp4".to_owned(),
            "resources/2-song.mp3".to_owned()
        ]
    );
}

#[test]
fn a_file_that_cannot_be_found_cannot_be_included_and_the_user_is_told_which() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    fixture.window.application_mut().source_files.insert(
        VIDEO,
        SourceFileState::new(SourceStatus::Unavailable {
            reason: "there is no file there".to_owned(),
        }),
    );

    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));

    assert!(!fixture.is_included(VIDEO));
    let (title, text) = fixture
        .window
        .application()
        .message_showing()
        .expect("the user is told");
    assert_eq!(title, "The file cannot be included");
    assert!(text.contains("holiday.mp4"), "{text}");
}

#[test]
fn of_several_files_those_that_are_found_are_included_and_those_that_are_not_are_said_to_be_left_out() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    fixture.window.application_mut().source_files.insert(
        the_sound(),
        SourceFileState::new(SourceStatus::Unavailable {
            reason: "gone".to_owned(),
        }),
    );

    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO, the_sound()]));

    assert!(fixture.is_included(VIDEO));
    assert!(!fixture.is_included(the_sound()));
    let (_, text) = fixture.window.application().message_showing().expect("and it is said");
    assert!(text.contains("song.mp3") && !text.contains("holiday.mp4"), "{text}");
}

#[test]
fn a_file_that_is_included_and_cannot_be_taken_out_is_listed_as_missing_with_where_it_is_said_to_be() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    for (identifier, reason) in [
        (VIDEO, "the archive is damaged"),
        (the_sound(), "there is no file there"),
    ] {
        fixture.window.application_mut().source_files.insert(
            identifier,
            SourceFileState::new(SourceStatus::Unavailable {
                reason: reason.to_owned(),
            }),
        );
    }

    let missing = fixture.window.application().missing_sources();

    assert_eq!(missing.len(), 2);
    assert!(
        missing[0]
            .last_known_path
            .ends_with("holiday.mp4 (included in the project file)"),
        "{}",
        missing[0].last_known_path
    );
    assert!(
        missing[1].last_known_path.ends_with("song.mp3"),
        "a file that is not included is where it was: {}",
        missing[1].last_known_path
    );
}

// --- including, in a project whose file is not an archive ---

#[test]
fn including_a_file_in_a_project_that_has_never_been_saved_asks_to_save_it_as_an_archive_and_does_nothing_else_yet() {
    let mut fixture = Fixture::new();

    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));

    assert_eq!(
        dialog_showing(&fixture.window),
        Some(&Dialog::MustSaveAsAnArchiveToIncludeFiles {
            files: vec![VIDEO],
            has_a_plain_project_file: false,
        })
    );
    assert!(
        !fixture.is_included(VIDEO),
        "nothing is included before the user has agreed"
    );
    assert!(
        fixture.chooser.questions_asked().is_empty(),
        "and nothing is asked yet but whether"
    );
    assert!(
        is_offered(&fixture.window, "Choose where to save it") && is_offered(&fixture.window, "Cancel"),
        "the dialog has its buttons"
    );
}

#[test]
fn agreeing_asks_where_to_save_the_archive_and_saves_the_project_there_with_the_file_included_in_one_step() {
    let mut fixture = Fixture::new();
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    let destination = fixture.path("trip.ffrs");
    fixture.chooser.will_answer(Some(destination.clone()));

    click(&mut fixture.window, "Choose where to save it");

    let asked = fixture.chooser.questions_asked();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].saving);
    assert_eq!(asked[0].title, "Save the project with the files included");
    assert_eq!(asked[0].suggested_file_name.as_deref(), Some("holiday.ffrs"));
    assert_eq!(
        asked[0].kinds,
        vec![("Project files (.ffrs)".to_owned(), vec!["ffrs".to_owned()])],
        "only the kind of project file that can have files in it"
    );
    assert_eq!(
        names_of_the_entries(&destination),
        vec![PROJECT_ENTRY_NAME.to_owned(), "resources/1-holiday.mp4".to_owned()]
    );
    assert_eq!(
        fixture.window.application().project_file_path(),
        Some(destination.as_path())
    );
    assert!(fixture.is_included(VIDEO));
    assert!(!fixture.window.application().has_unsaved_changes());
    assert_eq!(dialog_showing(&fixture.window), None);
    assert_eq!(
        fixture.the_link_of(VIDEO).relative_path,
        Some(PathBuf::from("media/holiday.mp4")),
        "the places it was at are worked out from where the archive is"
    );
}

#[test]
fn a_name_that_does_not_end_in_ffrs_has_it_put_on_because_nothing_else_can_have_files_in_it() {
    let mut fixture = Fixture::new();
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    fixture.chooser.will_answer(Some(fixture.path("trip.json")));

    click(&mut fixture.window, "Choose where to save it");

    let destination = fixture.path("trip.json.ffrs");
    assert!(destination.exists());
    assert!(archive::is_an_archive(&destination));
    assert!(!fixture.path("trip.json").exists());
    assert!(fixture.is_included(VIDEO));
}

#[test]
fn cancelling_the_question_changes_nothing_and_asks_nothing_more() {
    let mut fixture = Fixture::new();
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));

    click(&mut fixture.window, "Cancel");

    assert_eq!(dialog_showing(&fixture.window), None);
    assert!(!fixture.is_included(VIDEO));
    assert!(fixture.chooser.questions_asked().is_empty());
    assert_eq!(fixture.window.application().project_file_path(), None);
}

#[test]
fn choosing_no_place_to_save_it_changes_nothing() {
    let mut fixture = Fixture::new();
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    fixture.chooser.will_answer(None);

    click(&mut fixture.window, "Choose where to save it");

    assert!(!fixture.is_included(VIDEO));
    assert_eq!(fixture.window.application().project_file_path(), None);
    assert_eq!(dialog_showing(&fixture.window), None);
    assert!(
        fs::read_dir(fixture.folder.path()).unwrap().all(|entry| {
            let name = entry.unwrap().file_name();
            name == "media"
        }),
        "nothing was written"
    );
}

#[test]
fn a_project_that_has_a_plain_file_is_asked_the_same_and_the_plain_file_is_left_as_it_was() {
    let mut fixture = Fixture::new();
    let plain = fixture.save_as("trip.fastforwardrs.json");
    let plain_text = fs::read_to_string(&plain).unwrap();
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    assert_eq!(
        dialog_showing(&fixture.window),
        Some(&Dialog::MustSaveAsAnArchiveToIncludeFiles {
            files: vec![VIDEO],
            has_a_plain_project_file: true,
        })
    );
    fixture.chooser.will_answer(Some(fixture.path("trip.ffrs")));

    click(&mut fixture.window, "Choose where to save it");

    assert_eq!(
        fs::read_to_string(&plain).unwrap(),
        plain_text,
        "the plain file is as it was"
    );
    assert_eq!(
        fixture.window.application().project_file_path(),
        Some(fixture.path("trip.ffrs").as_path()),
        "the archive is the project's file from now on"
    );
    assert!(fixture.is_included(VIDEO));
}

#[test]
fn if_the_archive_cannot_be_saved_nothing_is_included_and_the_user_is_told() {
    let mut fixture = Fixture::new();
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    fixture
        .chooser
        .will_answer(Some(fixture.path("no such folder").join("trip.ffrs")));

    click(&mut fixture.window, "Choose where to save it");

    assert!(!fixture.is_included(VIDEO), "the project is as it was");
    assert_eq!(fixture.window.application().project_file_path(), None);
    let (title, _) = fixture
        .window
        .application()
        .message_showing()
        .expect("the user is told");
    assert_eq!(title, "The project could not be saved");
}

// --- saving a project that has files included ---

#[test]
fn a_project_that_has_files_included_cannot_be_saved_as_plain_json_and_is_left_as_it_was() {
    let mut fixture = Fixture::new();
    let archive_path = fixture.save_as("trip.ffrs");
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    fixture.window.do_this(Command::SaveProject);
    let archive_before = fs::read(&archive_path).unwrap();
    fixture
        .chooser
        .will_answer(Some(fixture.path("plain.fastforwardrs.json")));

    fixture.window.do_this(Command::SaveProjectAs);

    let (title, text) = fixture
        .window
        .application()
        .message_showing()
        .expect("the user is told");
    assert_eq!(title, "The project could not be saved");
    assert!(text.contains(".ffrs"), "{text}");
    assert!(!fixture.path("plain.fastforwardrs.json").exists());
    assert_eq!(
        fixture.window.application().project_file_path(),
        Some(archive_path.as_path()),
        "it is still saved where it was"
    );
    assert_eq!(fs::read(&archive_path).unwrap(), archive_before);
    assert!(fixture.is_included(VIDEO));
}

#[test]
fn saving_as_another_archive_puts_the_included_files_in_it_too() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    fixture.window.do_this(Command::SaveProject);
    fixture.chooser.will_answer(Some(fixture.path("copy.ffrs")));

    fixture.window.do_this(Command::SaveProjectAs);

    assert_eq!(
        names_of_the_entries(&fixture.path("copy.ffrs")),
        vec![PROJECT_ENTRY_NAME.to_owned(), "resources/1-holiday.mp4".to_owned()]
    );
}

#[test]
fn a_file_that_is_included_and_cannot_be_found_stops_the_save_and_says_which() {
    let mut fixture = Fixture::new();
    let archive_path = fixture.save_as("trip.ffrs");
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    fixture.window.do_this(Command::SaveProject);
    let before = fs::read(&archive_path).unwrap();
    fixture.window.do_this(Command::AddTrack);
    fixture.window.application_mut().source_files.insert(
        VIDEO,
        SourceFileState::new(SourceStatus::Unavailable {
            reason: "it was taken out badly".to_owned(),
        }),
    );

    fixture.window.do_this(Command::SaveProject);

    let (title, text) = fixture
        .window
        .application()
        .message_showing()
        .expect("the user is told");
    assert_eq!(title, "The project could not be saved");
    assert!(
        text.contains("holiday.mp4") && text.contains("it was taken out badly"),
        "{text}"
    );
    assert_eq!(
        fs::read(&archive_path).unwrap(),
        before,
        "the project file is as it was"
    );
    assert!(fixture.window.application().has_unsaved_changes());
}

// --- taking files out ---

/// A fixture whose video is included in `trip.ffrs`, saved, as a project that has been loaded from it is: the file is not where
/// it was but in a folder of the cache (which is a copy in `cache`), and the original is gone.
fn fixture_with_the_video_included_and_the_original_gone() -> (Fixture, PathBuf, PathBuf) {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    fixture.window.do_this(Command::SaveProject);
    let original = fixture.path("media").join("holiday.mp4");
    let in_the_cache = fixture.path("cache of the video.mp4");
    fs::copy(&original, &in_the_cache).unwrap();
    fixture.window.application_mut().source_files.insert(
        VIDEO,
        SourceFileState::new(SourceStatus::Available {
            path: in_the_cache.clone(),
        }),
    );
    fs::remove_file(&original).unwrap();
    (fixture, std::path::absolute(original).unwrap(), in_the_cache)
}

#[test]
fn a_file_that_is_taken_out_is_put_back_where_it_was_and_is_no_longer_included() {
    let (mut fixture, original, _) = fixture_with_the_video_included_and_the_original_gone();

    fixture
        .window
        .do_this(Command::TakeFilesOutOfTheProjectFile(vec![VIDEO]));

    assert_eq!(fs::read(&original).unwrap(), THE_VIDEO, "it is where it was");
    let link = fixture.the_link_of(VIDEO);
    assert!(!link.is_included());
    assert_eq!(link.absolute_path, original);
    assert_eq!(link.relative_path, Some(PathBuf::from("media/holiday.mp4")));
    assert_eq!(
        fixture.window.application().source_files[&VIDEO].status,
        SourceStatus::Available { path: original },
        "it is the file that is used from now on"
    );
    assert!(
        fixture.window.application().has_unsaved_changes(),
        "the project file is to be saved without it"
    );
    assert_eq!(dialog_showing(&fixture.window), None);
}

#[test]
fn once_the_project_is_saved_again_the_file_is_no_longer_in_the_project_file() {
    let (mut fixture, _, _) = fixture_with_the_video_included_and_the_original_gone();
    fixture
        .window
        .do_this(Command::TakeFilesOutOfTheProjectFile(vec![VIDEO]));

    fixture.window.do_this(Command::SaveProject);

    assert_eq!(
        names_of_the_entries(&fixture.path("trip.ffrs")),
        vec![PROJECT_ENTRY_NAME.to_owned()]
    );
    assert!(
        !read_project_file(&fixture.path("trip.ffrs")).unwrap().sources()[0]
            .link
            .is_included()
    );
}

#[test]
fn where_the_same_file_is_already_nothing_is_written_over_it() {
    let (mut fixture, original, in_the_cache) = fixture_with_the_video_included_and_the_original_gone();
    fs::copy(&in_the_cache, &original).unwrap();
    let modified = fs::metadata(&original).unwrap().modified().unwrap();

    fixture
        .window
        .do_this(Command::TakeFilesOutOfTheProjectFile(vec![VIDEO]));

    assert_eq!(fs::metadata(&original).unwrap().modified().unwrap(), modified);
    assert!(!fixture.is_included(VIDEO));
    assert_eq!(fixture.the_link_of(VIDEO).absolute_path, original);
}

#[test]
fn where_another_file_is_it_is_left_alone_and_with_nowhere_else_the_file_stays_in_the_project_file_and_the_user_is_told()
 {
    let (mut fixture, original, _) = fixture_with_the_video_included_and_the_original_gone();
    fs::write(&original, "another file that has been put there since").unwrap();

    fixture
        .window
        .do_this(Command::TakeFilesOutOfTheProjectFile(vec![VIDEO]));

    assert_eq!(
        fs::read_to_string(&original).unwrap(),
        "another file that has been put there since"
    );
    assert!(fixture.is_included(VIDEO), "it is in the project file still");
    let (title, text) = fixture
        .window
        .application()
        .message_showing()
        .expect("the user is told");
    assert_eq!(title, "The file could not be taken out of the project file");
    assert!(
        text.contains("holiday.mp4") && text.contains("no place to put it back"),
        "{text}"
    );
    assert!(!fixture.window.application().has_unsaved_changes(), "nothing changed");
}

#[test]
fn when_the_project_has_been_moved_the_file_goes_to_the_same_place_from_the_folder_it_is_in_now() {
    let (mut fixture, original, _) = fixture_with_the_video_included_and_the_original_gone();
    // The project is saved in another folder, in which the folder for the media is (and the first folder is not there).
    let moved = fixture.path("moved");
    fs::create_dir_all(moved.join("media")).unwrap();
    fixture.window.application_mut().project_file_path = Some(moved.join("trip.ffrs"));
    fs::remove_dir_all(fixture.path("media")).unwrap();
    assert!(!original.exists());

    fixture
        .window
        .do_this(Command::TakeFilesOutOfTheProjectFile(vec![VIDEO]));

    let put_here = std::path::absolute(moved.join("media").join("holiday.mp4")).unwrap();
    assert_eq!(fs::read(&put_here).unwrap(), THE_VIDEO);
    assert_eq!(fixture.the_link_of(VIDEO).absolute_path, put_here);
    assert!(!fixture.is_included(VIDEO));
}

#[test]
fn a_file_that_is_not_included_is_left_as_it_is_by_taking_it_out() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    let before = fixture.the_link_of(VIDEO);

    fixture
        .window
        .do_this(Command::TakeFilesOutOfTheProjectFile(vec![VIDEO]));

    assert_eq!(fixture.the_link_of(VIDEO), before);
    assert!(!fixture.window.application().has_unsaved_changes());
    assert_eq!(dialog_showing(&fixture.window), None);
}

#[test]
fn a_file_that_was_included_a_moment_ago_and_is_still_where_it_was_is_just_no_longer_included_by_taking_it_out() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    let original = std::path::absolute(fixture.path("media").join("holiday.mp4")).unwrap();

    fixture
        .window
        .do_this(Command::TakeFilesOutOfTheProjectFile(vec![VIDEO]));

    assert!(!fixture.is_included(VIDEO));
    assert_eq!(fs::read(&original).unwrap(), THE_VIDEO);
    assert_eq!(dialog_showing(&fixture.window), None);
}

// --- the menu of a clip ---

fn the_video_clip(fixture: &Fixture) -> crate::project::ClipId {
    fixture
        .window
        .application()
        .project()
        .tracks()
        .iter()
        .flat_map(|track| track.placements())
        .find(|placement| placement.clip.source() == Some(VIDEO))
        .expect("the video has a clip")
        .id
}

fn right_click_on_the_video_clip(fixture: &mut Fixture) {
    let clip = the_video_clip(fixture);
    let track = fixture
        .window
        .application()
        .project()
        .track_position_holding(clip)
        .unwrap();
    let point = fixture.window.middle_of_the_strip_at(track, 100);
    fixture.window.right_click(point);
}

#[test]
fn the_menu_of_a_clip_made_from_a_file_that_is_not_included_offers_to_include_it_and_not_to_take_it_out() {
    let mut fixture = Fixture::new();

    right_click_on_the_video_clip(&mut fixture);

    assert!(is_offered(&fixture.window, INCLUDE));
    assert!(!is_anything_offered_that_says(&fixture.window, "Remove"));
}

#[test]
fn choosing_include_in_the_menu_of_a_clip_includes_its_file() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    right_click_on_the_video_clip(&mut fixture);

    click(&mut fixture.window, INCLUDE);

    assert!(fixture.is_included(VIDEO));
    assert!(
        !fixture.is_included(the_sound()),
        "only the file of the clip that was clicked"
    );
}

#[test]
fn the_menu_of_a_clip_whose_file_is_included_offers_to_take_it_out_and_not_to_include_it() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));

    right_click_on_the_video_clip(&mut fixture);

    assert!(is_offered(&fixture.window, REMOVE));
    assert!(!is_anything_offered_that_says(&fixture.window, "Include"));
}

#[test]
fn choosing_remove_in_the_menu_of_a_clip_takes_its_file_out_of_the_project_file() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    right_click_on_the_video_clip(&mut fixture);

    click(&mut fixture.window, REMOVE);

    assert!(!fixture.is_included(VIDEO));
}

// --- autosave ---

#[test]
fn a_project_that_has_a_great_deal_included_is_not_autosaved_but_one_that_has_a_little_is() {
    let mut fixture = Fixture::new();
    let archive_path = fixture.save_as("trip.ffrs");
    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO]));
    fixture.window.do_this(Command::SaveProject);
    fixture.window.application_mut().global_settings.autosave = true;
    let context = egui::Context::default();
    let tracks_at_the_start = fixture.window.application().project().tracks().len();

    // A little: autosaved, with the file in it.
    fixture.window.do_this(Command::AddTrack);
    fixture
        .window
        .application_mut()
        .autosave_when_quiet_for_long_enough(&context, Instant::now() + Duration::from_secs(5));
    assert!(!fixture.window.application().has_unsaved_changes(), "saved by itself");
    assert_eq!(
        read_project_file(&archive_path).unwrap().tracks().len(),
        tracks_at_the_start + 1
    );

    // A great deal: the file is made bigger than the most that is autosaved.
    let big = fixture.path("media").join("holiday.mp4");
    let file = fs::OpenOptions::new().write(true).open(&big).unwrap();
    file.set_len(super::autosave::MOST_BYTES_INCLUDED_FOR_AUTOSAVE + 1)
        .unwrap();
    drop(file);
    fixture.window.do_this(Command::AddTrack);
    fixture
        .window
        .application_mut()
        .autosave_when_quiet_for_long_enough(&context, Instant::now() + Duration::from_secs(5));
    assert!(
        fixture.window.application().has_unsaved_changes(),
        "not saved by itself: it would write all of that again"
    );
    assert_eq!(
        read_project_file(&archive_path).unwrap().tracks().len(),
        tracks_at_the_start + 1,
        "the archive is as it was"
    );
    assert_eq!(dialog_showing(&fixture.window), None, "and nothing is said about it");
}

#[test]
fn the_bytes_that_are_included_are_counted_from_the_files_that_are_found() {
    let mut fixture = Fixture::new();
    fixture.save_as("trip.ffrs");
    assert_eq!(fixture.window.application().bytes_included_in_the_project_file(), 0);

    fixture
        .window
        .do_this(Command::IncludeFilesInTheProjectFile(vec![VIDEO, the_sound()]));

    assert_eq!(
        fixture.window.application().bytes_included_in_the_project_file(),
        THE_VIDEO.len() as u64 + b"some sound".len() as u64
    );
}
