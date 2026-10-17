// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the editor asks when it needs a file, checked with a chooser that gives the answers a test
//! tells it to (a real file dialog would wait for someone to answer it).

use std::path::{Path, PathBuf};

use egui_kittest::kittest::{NodeT, Queryable};

use super::test_window::{Window, project_of};
use crate::application::commands::Command;
use crate::application::dialogs::{AfterConfirmation, Dialog};
use crate::application::file_chooser::{AskedQuestion, ScriptedFileChooser};
use crate::project::Project;

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

    window.do_this(Command::LinkAndImportFile);

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

    window.do_this(Command::LinkAndImportFile);

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
    // (A .ffrs file is the one that is offered first, and suggested: it is the project file that can do all that the other can.)
    assert_eq!(asked[0].suggested_file_name.as_deref(), Some("holiday.ffrs"));
    assert_eq!(
        names_of_the_kinds(&asked[0]),
        ["Project files (.ffrs)", "Plain project files (.json)"]
    );
    assert_eq!(extensions_of(&asked[0], "Project files (.ffrs)"), ["ffrs"]);
    assert_eq!(extensions_of(&asked[0], "Plain project files (.json)"), ["json"]);
    assert_eq!(
        window.application().project_file_path(),
        None,
        "it was not saved anywhere"
    );
}

/// A window showing a project that has been saved to `file_name` in `folder`, with a chooser that will be asked for files.
fn window_with_a_project_file(chooser: &ScriptedFileChooser, folder: &Path, file_name: &str) -> (Window, PathBuf) {
    let mut window = window_that_asks(chooser);
    let path = folder.join(file_name);
    window.application_mut().save_project_to(path.clone());
    (window, path)
}

#[test]
fn saving_as_asks_where_even_when_the_project_has_a_file_and_offers_its_name_and_its_folder() {
    let folder = tempfile::tempdir().unwrap();
    let chooser = ScriptedFileChooser::default();
    chooser.will_answer(None);
    let (mut window, path) = window_with_a_project_file(&chooser, folder.path(), "trip.fastforwardrs.json");

    window.do_this(Command::SaveProjectAs);

    let asked = chooser.questions_asked();
    assert_eq!(asked.len(), 1, "one question, though the project has a file");
    assert!(asked[0].saving, "where to save");
    assert_eq!(asked[0].title, "Save the project as");
    assert_eq!(asked[0].suggested_file_name.as_deref(), Some("trip.fastforwardrs.json"));
    assert_eq!(asked[0].starting_directory.as_deref(), Some(folder.path()));
    assert_eq!(
        names_of_the_kinds(&asked[0]),
        ["Project files (.ffrs)", "Plain project files (.json)"]
    );
    assert_eq!(
        window.application().project_file_path(),
        Some(path.as_path()),
        "nothing changed"
    );
}

#[test]
fn saving_as_a_project_that_has_no_file_yet_offers_the_name_of_its_first_file_and_no_folder() {
    let chooser = ScriptedFileChooser::default();
    chooser.will_answer(None);
    let mut window = window_that_asks(&chooser);

    window.do_this(Command::SaveProjectAs);

    let asked = chooser.questions_asked();
    assert_eq!(asked[0].title, "Save the project as");
    assert_eq!(asked[0].suggested_file_name.as_deref(), Some("holiday.ffrs"));
    assert_eq!(asked[0].starting_directory, None);
}

#[test]
fn saving_for_the_first_time_does_not_pick_a_folder_but_leaves_it_to_the_system() {
    let chooser = ScriptedFileChooser::default();
    chooser.will_answer(None);
    let mut window = window_that_asks(&chooser);

    window.do_this(Command::SaveProject);

    assert_eq!(chooser.questions_asked()[0].starting_directory, None);
}

#[test]
fn saving_as_writes_the_project_to_the_file_chosen_which_is_the_projects_file_from_then_on() {
    let folder = tempfile::tempdir().unwrap();
    let chooser = ScriptedFileChooser::default();
    let (mut window, old_path) = window_with_a_project_file(&chooser, folder.path(), "first.fastforwardrs.json");
    let old_text = std::fs::read_to_string(&old_path).unwrap();
    window.do_this(Command::AddTrack);
    assert!(window.application().has_unsaved_changes());
    // (No ending is typed: the one of project archives, which is the one that is offered first, is put on.)
    chooser.will_answer(Some(folder.path().join("second")));

    window.do_this(Command::SaveProjectAs);

    let new_path = folder.path().join("second.ffrs");
    assert!(new_path.exists(), "written where it was said");
    assert_eq!(window.application().project_file_path(), Some(new_path.as_path()));
    assert!(!window.application().has_unsaved_changes(), "everything is saved");
    assert_eq!(
        std::fs::read_to_string(&old_path).unwrap(),
        old_text,
        "the file the project came from is as it was"
    );
    assert!(
        crate::project_file::archive::is_an_archive(&new_path),
        "it is an archive, as its name says"
    );
    assert_ne!(
        crate::project_file::archive::read_project_text(&new_path).unwrap(),
        old_text,
        "the new file has the track that was added since"
    );

    // From then on saving is to the new file, without a question.
    window.do_this(Command::AddTrack);
    window.do_this(Command::SaveProject);
    assert_eq!(
        chooser.questions_asked().len(),
        1,
        "only the question about where to save as"
    );
    assert_eq!(
        crate::project_file::read_project_file(&new_path)
            .unwrap()
            .tracks()
            .len(),
        3,
        "the new file has the second track added: it is the one that is saved to"
    );
    assert_eq!(
        crate::project_file::read_project_file(&old_path)
            .unwrap()
            .tracks()
            .len(),
        1,
        "and the old one is still as it was"
    );
}

#[test]
fn choosing_no_place_to_save_as_changes_nothing_and_leaves_the_changes_unsaved() {
    let folder = tempfile::tempdir().unwrap();
    let chooser = ScriptedFileChooser::default();
    let (mut window, path) = window_with_a_project_file(&chooser, folder.path(), "kept.fastforwardrs.json");
    window.do_this(Command::AddTrack);
    chooser.will_answer(None);

    window.do_this(Command::SaveProjectAs);

    assert_eq!(window.application().project_file_path(), Some(path.as_path()));
    assert!(window.application().has_unsaved_changes(), "nothing was saved");
    assert_eq!(window.application().message_showing(), None);
}

#[test]
fn saving_as_is_the_item_after_save_project_and_is_there_to_use_when_there_is_something_to_save() {
    let chooser = ScriptedFileChooser::default();
    chooser.will_answer(None);
    let mut window = window_that_asks(&chooser);
    window.harness.get_by_label("File").click();
    window.settle();
    assert!(
        !window
            .harness
            .get_by_label("Save project as")
            .accesskit_node()
            .is_disabled()
    );

    window.harness.get_by_label("Save project as").click();
    window.settle();

    assert_eq!(
        chooser.questions_asked()[0].title,
        "Save the project as",
        "the item asks"
    );
}

#[test]
fn saving_as_cannot_be_chosen_when_there_is_nothing_in_the_project_to_save() {
    let mut window = Window::showing(Project::empty());
    window.harness.get_by_label("File").click();
    window.settle();

    assert!(
        window
            .harness
            .get_by_label("Save project")
            .accesskit_node()
            .is_disabled()
    );
    assert!(
        window
            .harness
            .get_by_label("Save project as")
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn nothing_in_the_file_menu_can_be_chosen_while_a_message_is_up() {
    let mut window = Window::showing(project_of(300));
    window.harness.get_by_label("File").click();
    window.settle();
    window.application_mut().interface.dialog = Some(Dialog::Message {
        title: "Note".into(),
        text: "Read me".into(),
    });
    window.settle();

    for item in [
        "New project",
        "Save project",
        "Save project as",
        "Load project",
        "Embed and import",
        "Link and import",
        "Export",
    ] {
        assert!(
            window.harness.get_by_label(item).accesskit_node().is_disabled(),
            "{item} cannot be chosen while a message is up"
        );
    }
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

/// A window like [`window_that_asks`], whose video has an effect that the program does not have, and whose track has one of the same kind (so that
/// it is the same effect twice) and one that is of another key.
fn window_that_asks_with_effects_the_program_does_not_have(chooser: &ScriptedFileChooser) -> Window {
    let mut window = window_that_asks(chooser);
    let made_up = crate::test_support::a_kind_of_effect_the_program_does_not_have(
        "isf:export_question_made_up",
        &[("amount", crate::project::PropertyValue::Number(0.5))],
    );
    let another = crate::test_support::a_kind_of_effect_the_program_does_not_have(
        "isf:export_question_another",
        &[("amount", crate::project::PropertyValue::Number(0.5))],
    );
    let project = &mut window.application_mut().document.project;
    let clip = crate::project::AnimationTarget::Clip(project.tracks()[0].entries()[0].id());
    let track = crate::project::AnimationTarget::Track(project.tracks()[0].id());
    project.add_effect(clip, made_up).unwrap();
    project.add_effect(track, made_up).unwrap();
    project.add_effect(track, another).unwrap();
    project.add_effect(clip, crate::project::EffectKind::HUE_SHIFT).unwrap();
    window.settle();
    window
}

#[test]
fn exporting_a_project_that_has_effects_the_program_does_not_have_says_which_before_asking_anything_else() {
    let chooser = ScriptedFileChooser::default();
    chooser.will_answer(None);
    let mut window = window_that_asks_with_effects_the_program_does_not_have(&chooser);

    window.do_this(Command::ExportVideo);

    assert_eq!(chooser.questions_asked().len(), 0, "where to put it is not asked yet");
    let Some(Dialog::ConfirmExportWithMissingEffects { effects }) = window.application().interface.dialog.clone()
    else {
        panic!("a question about the effects was expected");
    };
    assert_eq!(
        effects,
        [
            "export question another (isf:export_question_another, version 1.0)",
            "export question made up (isf:export_question_made_up, version 1.0)"
        ],
        "each once, though one is on the clip and the track, and not the hue shift, which the program has"
    );
    assert!(window.harness.query_by_label("Export without some effects?").is_some());
    assert!(window.harness.query_by_label("Export anyway").is_some());
    assert!(window.harness.query_by_label("Cancel").is_some());
    assert!(
        window
            .harness
            .query_by_label("  - export question made up (isf:export_question_made_up, version 1.0)")
            .is_some(),
        "the dialog lists them"
    );
}

#[test]
fn cancelling_the_export_that_would_leave_effects_out_asks_nothing_and_exports_nothing() {
    let chooser = ScriptedFileChooser::default();
    chooser.will_answer(None);
    let mut window = window_that_asks_with_effects_the_program_does_not_have(&chooser);
    window.do_this(Command::ExportVideo);

    window.harness.get_by_label("Cancel").click();
    window.settle();

    assert!(window.application().interface.dialog.is_none());
    assert_eq!(chooser.questions_asked().len(), 0);
    assert!(!window.application().is_working_in_the_background());
}

#[test]
fn exporting_anyway_asks_where_to_put_the_video_as_exporting_does_when_nothing_is_left_out() {
    let chooser = ScriptedFileChooser::default();
    chooser.will_answer(None);
    let mut window = window_that_asks_with_effects_the_program_does_not_have(&chooser);
    window.do_this(Command::ExportVideo);

    window.harness.get_by_label("Export anyway").click();
    window.settle();

    assert!(window.application().interface.dialog.is_none());
    let asked = chooser.questions_asked();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].title, "Export the video");
}

#[test]
fn a_project_whose_effects_are_all_the_programs_is_exported_without_a_question_about_effects() {
    let chooser = ScriptedFileChooser::default();
    chooser.will_answer(None);
    let mut window = window_that_asks(&chooser);
    let clip = crate::project::AnimationTarget::Clip(window.application().project().tracks()[0].entries()[0].id());
    window
        .application_mut()
        .document
        .project
        .add_effect(clip, crate::project::EffectKind::HUE_SHIFT)
        .unwrap();

    window.do_this(Command::ExportVideo);

    assert!(window.application().interface.dialog.is_none());
    assert_eq!(chooser.questions_asked().len(), 1);
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
            window.application().interface.dialog,
            Some(Dialog::ConfirmDiscardChanges { then }),
            "the changes would be lost, so it asks first"
        );
        assert!(chooser.questions_asked().is_empty(), "and not for a file yet");
    }
}

#[test]
fn exporting_a_project_whose_look_up_table_has_no_table_to_use_lists_the_table_among_what_the_video_would_not_have() {
    let chooser = ScriptedFileChooser::default();
    let mut window = Window::showing(project_of(300));
    window.use_file_chooser(&chooser);
    let clip = crate::project::AnimationTarget::Clip(window.application().project().tracks()[0].entries()[0].id());
    let project = &mut window.application_mut().document.project;
    let instance = project.add_effect(clip, crate::project::EffectKind::LUT).unwrap();
    project
        .set_property_at(
            clip,
            crate::project::PropertyId::effect_value(crate::project::EffectKind::LUT, instance, 0),
            crate::project::PropertyValue::Words("grade.cube".to_owned()),
            crate::project::TimelineFrameIndex::new(0),
        )
        .unwrap();
    window.settle();

    window.do_this(Command::ExportVideo);

    let Some(Dialog::ConfirmExportWithMissingEffects { effects }) = window.application().interface.dialog.clone()
    else {
        panic!("a question about what the video would not have was expected");
    };
    assert_eq!(
        effects,
        ["the table grade.cube (the project does not have it, or it cannot be read)"]
    );
    assert_eq!(chooser.questions_asked().len(), 0);
}
