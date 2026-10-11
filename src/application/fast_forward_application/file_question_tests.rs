//! What the editor asks when it needs a file, checked with a chooser that gives the answers a test
//! tells it to (a real file dialog would wait for someone to answer it).

use super::test_window::{Window, project_of};
use crate::application::commands::Command;
use crate::application::dialogs::{AfterConfirmation, Dialog};
use crate::application::file_chooser::{AskedQuestion, ScriptedFileChooser};

/// A window showing a project made from `holiday.mp4`, with a chooser that will be asked for files.
fn window_that_asks(chooser: &ScriptedFileChooser) -> Window {
    let mut window = Window::showing(project_of(300));
    window.use_file_chooser(chooser);
    window
}

fn names_of_the_kinds(question: &AskedQuestion) -> Vec<&str> {
    question.kinds.iter().map(|(name, _)| name.as_str()).collect()
}

fn extensions_of<'a>(question: &'a AskedQuestion, kind: &str) -> &'a [String] {
    &question
        .kinds
        .iter()
        .find(|(name, _)| name == kind)
        .unwrap_or_else(|| panic!("no kind of file is called {kind:?}"))
        .1
}

#[test]
fn importing_asks_for_a_video_a_sound_or_a_picture_with_all_of_those_offered_first() {
    let chooser = ScriptedFileChooser::default();
    chooser.will_answer(None);
    let mut window = window_that_asks(&chooser);

    window.do_this(Command::ImportFile);

    let asked = chooser.questions_asked();
    assert_eq!(asked.len(), 1, "one question");
    assert!(!asked[0].saving, "which file to open");
    assert_eq!(asked[0].title, "Choose a video, a sound or a picture");
    assert_eq!(
        names_of_the_kinds(&asked[0]),
        [
            "Video, sound and picture files",
            "Video files",
            "Sound files",
            "Picture files",
            "All files"
        ]
    );
    let videos = extensions_of(&asked[0], "Video files");
    let sounds = extensions_of(&asked[0], "Sound files");
    let pictures = extensions_of(&asked[0], "Picture files");
    assert!(videos.contains(&"mp4".to_owned()) && sounds.contains(&"mp3".to_owned()));
    assert!(pictures.contains(&"png".to_owned()) && pictures.contains(&"jpg".to_owned()));
    let everything = extensions_of(&asked[0], "Video, sound and picture files");
    assert!(
        videos
            .iter()
            .chain(sounds)
            .chain(pictures)
            .all(|extension| everything.contains(extension)),
        "the first kind has every ending that the next three have"
    );
    assert_eq!(extensions_of(&asked[0], "All files"), ["*"]);
}

#[test]
fn choosing_no_file_to_import_changes_nothing() {
    let chooser = ScriptedFileChooser::default();
    chooser.will_answer(None);
    let mut window = window_that_asks(&chooser);
    let clips_before = window.clips();

    window.do_this(Command::ImportFile);

    assert_eq!(window.clips(), clips_before);
    assert!(
        !window.application().can_undo(),
        "nothing was imported, so there is nothing to undo"
    );
    assert!(!window.application().is_working_in_the_background());
    assert_eq!(window.application().message_showing(), None);
}

#[test]
fn saving_for_the_first_time_asks_where_and_suggests_the_name_of_the_first_file_of_the_project() {
    let chooser = ScriptedFileChooser::default();
    chooser.will_answer(None);
    let mut window = window_that_asks(&chooser);

    window.do_this(Command::SaveProject);

    let asked = chooser.questions_asked();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].saving, "where to save");
    assert_eq!(asked[0].title, "Save the project");
    assert_eq!(
        asked[0].suggested_file_name.as_deref(),
        Some("holiday.fastforwardrs.json")
    );
    assert_eq!(names_of_the_kinds(&asked[0]), ["Project files"]);
    assert_eq!(extensions_of(&asked[0], "Project files"), ["json"]);
    assert_eq!(
        window.application().project_file_path(),
        None,
        "it was not saved anywhere"
    );
}

#[test]
fn exporting_asks_where_and_suggests_a_name_that_is_not_the_name_of_a_source_file() {
    let chooser = ScriptedFileChooser::default();
    chooser.will_answer(None);
    let mut window = window_that_asks(&chooser);

    window.do_this(Command::ExportVideo);

    let asked = chooser.questions_asked();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].saving);
    assert_eq!(asked[0].title, "Export the video");
    assert_eq!(asked[0].suggested_file_name.as_deref(), Some("holiday_fastforward.mp4"));
    assert_eq!(extensions_of(&asked[0], "Video files"), ["mp4", "mkv", "mov"]);
    assert!(
        !window.application().is_working_in_the_background(),
        "nothing was exported"
    );
}

#[test]
fn loading_a_project_asks_for_a_project_file_when_nothing_would_be_lost() {
    let chooser = ScriptedFileChooser::default();
    chooser.will_answer(None);
    let mut window = window_that_asks(&chooser);
    assert!(!window.application().has_unsaved_changes());

    window.do_this(Command::LoadProject);

    let asked = chooser.questions_asked();
    assert_eq!(asked.len(), 1);
    assert!(!asked[0].saving);
    assert_eq!(asked[0].title, "Load a project");
    assert_eq!(names_of_the_kinds(&asked[0]), ["Project files", "All files"]);
    assert_eq!(
        window.clips(),
        [(0, 300, 1)],
        "choosing none left the project as it was"
    );
}

#[test]
fn loading_a_project_or_starting_a_new_one_asks_whether_to_discard_the_unsaved_changes_before_asking_for_any_file() {
    for (command, then) in [
        (Command::LoadProject, AfterConfirmation::LoadProject),
        (Command::NewProject, AfterConfirmation::NewProject),
    ] {
        let chooser = ScriptedFileChooser::default();
        let mut window = window_that_asks(&chooser);
        window.do_this(Command::AddTrack);
        assert!(window.application().has_unsaved_changes());

        window.do_this(command);

        assert_eq!(
            window.application().dialog,
            Some(Dialog::ConfirmDiscardChanges { then }),
            "the changes would be lost, so it asks first"
        );
        assert!(chooser.questions_asked().is_empty(), "and not for a file yet");
    }
}
