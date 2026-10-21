// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The folders that sound plugins are in, in the editor (phase 7, piece 7.6): **one question for a folder**, asked when a project that has plugins on its clips is opened, when
//! the scan finds a plugin that a project needed, when the sound comes to a plugin that nobody has answered for, and before an export; and what the answer does to the host that
//! runs the plugins. The host and the sound are tried with the real plugins in `plugin_host::tests` and `tests/sound_plugins_in_the_sound.rs`; the libraries of the plugins of
//! these tests are not files, so nothing is loaded here.

use egui_kittest::kittest::Queryable;

use super::keyframe_tests::window_with_a_video;
use super::preview_layer_tests::select;
use super::test_window::Window;
use crate::application::dialogs::{Dialog, PLUGIN_DONT_TRUST_LABEL, PLUGIN_TRUST_LABEL};
use crate::application::file_chooser::ScriptedFileChooser;
use crate::application::properties_panel::ADD_EFFECT_LABEL;
use crate::composition::LayerEffect;
use crate::media::export::{LoudnessToExport, SoundClipToExport};
use crate::plugin_scan::{LibraryOutcome, LibraryReport};
use crate::plugin_trust::{FolderStanding, Question, folder_of};
use crate::project::{Animation, ClipId, EffectKind, FrameRate, InstalledPlugin, SpeedFactor, set_installed_plugins};
use crate::sound_engine::{Notes, PluginRunner};
use crate::test_support::{
    an_installed_sound_plugin, the_installed_plugins_to_myself, the_plugins_of_the_test_library,
};

fn a_reverb() -> InstalledPlugin {
    let mut plugin = an_installed_sound_plugin(
        "ladspa:tap_reverb:tap_reverb",
        "Tom Szilagyi",
        &[("decay time", 2500.0, None)],
    );
    plugin.descriptor.name = "tap reverb".into();
    plugin
}

fn a_chorus_in_the_same_folder() -> InstalledPlugin {
    let mut plugin = an_installed_sound_plugin("ladspa:tap_reverb:tap_chorus", "Tom Szilagyi", &[("rate", 1.0, None)]);
    plugin.descriptor.name = "tap chorus".into();
    plugin
}

fn a_plugin_of_another_folder() -> InstalledPlugin {
    let mut plugin = an_installed_sound_plugin("ladspa:amp:amp", "Steve Harris", &[("amps gain", 0.0, None)]);
    plugin.library = std::path::PathBuf::from("/other_plugins/amp.so");
    plugin.descriptor.name = "amp".into();
    plugin
}

fn the_folder_of(plugin: &InstalledPlugin) -> std::path::PathBuf {
    folder_of(&plugin.library)
}

fn add_to_the_clip(window: &mut Window, name_in_the_menu: &str) {
    let plus = window.rectangle_of(ADD_EFFECT_LABEL).center();
    window.click(plus);
    window.open_the_smaller_menu("LADSPA");
    window.harness.get_by_label(name_in_the_menu).click();
    window.settle();
}

/// A window with the video of its clip selected, the plugins `installed` installed, and the effects called `names` (as the menu says them) put on the clip.
fn window_with_plugins_on_the_clip(installed: Vec<InstalledPlugin>, names: &[&str]) -> (Window, ClipId) {
    set_installed_plugins(installed);
    let (mut window, clip) = window_with_a_video();
    select(&mut window, clip);
    for name in names {
        add_to_the_clip(&mut window, name);
    }
    (window, clip)
}

/// The same project, opened again (which is when the plugins on its clips are asked about).
fn open_it_again(window: &mut Window) {
    let project = window.application().project().clone();
    window.show_another_project(project);
}

fn the_question_shown(window: &Window) -> Option<Question> {
    match &window.application().interface.dialog {
        Some(Dialog::AskAboutAPlugin { question }) => Some(question.clone()),
        _ => None,
    }
}

fn the_standing_in_the_host(window: &Window, plugin: &InstalledPlugin) -> FolderStanding {
    window.application().plugin_host().standing_of(&the_folder_of(plugin))
}

/// What the sound does when it comes to a clip that has `kind` on it: asks the host for the effect, as the graph does.
fn the_sound_comes_to(window: &Window, kind: EffectKind, notes: &Notes) {
    let effect = LayerEffect {
        instance: crate::project::EffectInstanceId::FIRST,
        kind,
        values: kind.default_values(),
        animation: Animation::default(),
        tables: Vec::new(),
    };
    let clip = SoundClipToExport {
        source_input: 0,
        first_source_frame: 0,
        end_source_frame: 30,
        speed_factor: SpeedFactor::new(1).unwrap(),
        timeline_frame_count: 30,
        loudness: LoudnessToExport::UNCHANGED,
        first_timeline_frame: 0,
        frames_of_the_clip_before: 0,
        effects: vec![effect.clone()],
    };
    let _ =
        window
            .application()
            .plugin_host()
            .processor_for(&effect, &clip, FrameRate::new(30, 1).unwrap(), notes, true);
}

fn say_yes(window: &mut Window) {
    let at = window.rectangle_of(PLUGIN_TRUST_LABEL).center();
    window.click(at);
}

fn say_no(window: &mut Window) {
    let at = window.rectangle_of(PLUGIN_DONT_TRUST_LABEL).center();
    window.click(at);
}

// --- when a project is opened ---

#[test]
fn a_project_with_plugins_on_its_clips_asks_one_question_for_their_folder_that_names_the_libraries_when_it_is_opened() {
    let _alone = the_installed_plugins_to_myself();
    let (reverb, chorus) = (a_reverb(), a_chorus_in_the_same_folder());
    let (mut window, _) = window_with_plugins_on_the_clip(vec![reverb.clone(), chorus], &["Tap reverb", "Tap chorus"]);
    assert!(
        the_question_shown(&window).is_none(),
        "nothing is asked about when a plugin is put on a clip"
    );

    open_it_again(&mut window);

    let question = the_question_shown(&window).expect("the question is asked");
    assert_eq!(
        question,
        Question::linked(
            &the_folder_of(&reverb),
            vec![
                "ladspa_tap_reverb_tap_chorus.so".to_owned(),
                "ladspa_tap_reverb_tap_reverb.so".to_owned()
            ]
        ),
        "one question, for the folder, with both libraries"
    );
    assert_eq!(
        window.application().plugin_trust().waiting.len(),
        0,
        "and no other waits"
    );
}

#[test]
fn plugins_in_two_folders_ask_two_questions_one_after_the_other() {
    let _alone = the_installed_plugins_to_myself();
    let (reverb, amp) = (a_reverb(), a_plugin_of_another_folder());
    let (mut window, _) = window_with_plugins_on_the_clip(vec![reverb.clone(), amp.clone()], &["Tap reverb", "Amp"]);
    open_it_again(&mut window);

    let first = the_question_shown(&window).expect("the first question");
    say_yes(&mut window);
    let second = the_question_shown(&window).expect("the second question");

    let folders: Vec<_> = [first, second]
        .into_iter()
        .map(|question| match question {
            Question::Linked { folder, .. } => folder,
            other => panic!("{other:?}"),
        })
        .collect();
    assert!(
        folders.contains(&the_folder_of(&reverb)) && folders.contains(&the_folder_of(&amp)),
        "{folders:?}"
    );
    assert_ne!(folders[0], folders[1]);
}

#[test]
fn a_plugin_that_is_not_on_any_clip_is_not_asked_about_and_nor_is_one_that_is_not_installed() {
    let _alone = the_installed_plugins_to_myself();
    // The reverb is installed but on no clip; the chorus is on a clip but then is not installed.
    let (mut window, _) =
        window_with_plugins_on_the_clip(vec![a_reverb(), a_chorus_in_the_same_folder()], &["Tap chorus"]);
    set_installed_plugins(vec![a_reverb()]);
    open_it_again(&mut window);
    assert!(the_question_shown(&window).is_none());
}

#[test]
fn a_yes_approves_the_folder_for_the_host_and_writes_it_down_and_a_project_opened_again_does_not_ask() {
    let _alone = the_installed_plugins_to_myself();
    let reverb = a_reverb();
    let (mut window, _) = window_with_plugins_on_the_clip(vec![reverb.clone()], &["Tap reverb"]);
    let file = tempfile::tempdir().unwrap();
    let approvals = file.path().join("plugin_approvals.json");
    window.application_mut().keep_plugin_approvals_in(approvals.clone());
    open_it_again(&mut window);
    assert_eq!(the_standing_in_the_host(&window, &reverb), FolderStanding::NotAsked);
    let generation = window.application().plugin_host().generation();

    say_yes(&mut window);

    assert_eq!(the_standing_in_the_host(&window, &reverb), FolderStanding::Approved);
    assert_ne!(
        window.application().plugin_host().generation(),
        generation,
        "the sound is made again with the plugin in it"
    );
    assert!(approvals.exists(), "the folder is written down");
    open_it_again(&mut window);
    assert!(the_question_shown(&window).is_none(), "and not asked about again");
}

#[test]
fn the_folders_that_were_approved_before_the_editor_began_are_approved_for_the_host_when_the_file_is_read() {
    let _alone = the_installed_plugins_to_myself();
    let reverb = a_reverb();
    let folder = tempfile::tempdir().unwrap();
    let approvals = folder.path().join("plugin_approvals.json");
    crate::plugin_trust::ApprovalStore::at(approvals.clone()).approve_folder(&the_folder_of(&reverb));
    let mut window = Window::showing(super::test_window::project_of(100));
    assert_eq!(the_standing_in_the_host(&window, &reverb), FolderStanding::NotAsked);

    window.application_mut().keep_plugin_approvals_in(approvals);

    assert_eq!(the_standing_in_the_host(&window, &reverb), FolderStanding::Approved);
}

#[test]
fn a_no_declines_the_folder_for_the_host_for_as_long_as_the_project_is_open_and_it_is_asked_again_when_a_project_is_opened()
 {
    let _alone = the_installed_plugins_to_myself();
    let reverb = a_reverb();
    let (mut window, _) = window_with_plugins_on_the_clip(vec![reverb.clone()], &["Tap reverb"]);
    open_it_again(&mut window);

    say_no(&mut window);

    assert_eq!(the_standing_in_the_host(&window, &reverb), FolderStanding::Declined);
    assert!(the_question_shown(&window).is_none());
    // (The sound comes to the plugin: it is not asked about again.)
    the_sound_comes_to(&window, EffectKind::installed_plugin_kinds()[0], &Notes::default());
    window.settle();
    assert!(the_question_shown(&window).is_none(), "an answer is an answer");

    open_it_again(&mut window);
    assert!(
        the_question_shown(&window).is_some(),
        "another time the project is opened, it is asked again"
    );
    assert_eq!(the_standing_in_the_host(&window, &reverb), FolderStanding::NotAsked);
}

// --- when the sound comes to a plugin ---

#[test]
fn a_plugin_that_the_sound_comes_to_before_anybody_has_answered_for_its_folder_is_asked_about_once() {
    let _alone = the_installed_plugins_to_myself();
    let reverb = a_reverb();
    let (mut window, _) = window_with_plugins_on_the_clip(vec![reverb.clone()], &["Tap reverb"]);
    let kind = EffectKind::installed_plugin_kinds()[0];
    assert!(the_question_shown(&window).is_none());

    the_sound_comes_to(&window, kind, &Notes::default());
    the_sound_comes_to(&window, kind, &Notes::default());
    window.settle();

    assert!(
        matches!(the_question_shown(&window), Some(Question::Linked { folder, .. }) if folder == the_folder_of(&reverb)),
        "the folder is asked about"
    );
    assert_eq!(
        window.application().plugin_trust().waiting.len(),
        0,
        "once, though the sound came to it twice"
    );
    // (And while the question is on the screen the sound comes to it again, as a sound that was started again does: still once.)
    the_sound_comes_to(&window, kind, &Notes::default());
    window.settle();
    say_yes(&mut window);
    window.settle();
    assert!(the_question_shown(&window).is_none(), "the one question was all");
}

// --- the scan ---

#[test]
fn a_scan_that_finds_a_plugin_that_a_project_needed_asks_about_its_folder() {
    let _alone = the_installed_plugins_to_myself();
    let test_plugins = the_plugins_of_the_test_library();
    set_installed_plugins(test_plugins.clone());
    let (mut window, _) = {
        let (mut window, clip) = window_with_a_video();
        select(&mut window, clip);
        add_to_the_clip(&mut window, "Test plugin ffrs_test_gain");
        (window, clip)
    };
    // The plugin is gone when the project is opened (the scan has not found it yet), so nothing is asked.
    set_installed_plugins(Vec::new());
    open_it_again(&mut window);
    assert!(the_question_shown(&window).is_none());

    let library = ladspa_test_plugins::library_file_for_tests();
    let scan = ladspa_host::scan_library(&library, |_| {}).unwrap();
    window.application_mut().pretend_the_scan_found(vec![LibraryReport {
        path: library.clone(),
        outcome: LibraryOutcome::Scanned(scan),
        from_cache: false,
    }]);
    window.settle();

    assert!(
        matches!(the_question_shown(&window), Some(Question::Linked { folder, .. }) if folder == folder_of(&library)),
        "the scan found the plugin: its folder is asked about"
    );
}

// --- the export ---

#[test]
fn an_export_waits_for_the_question_about_the_folder_of_its_plugins_and_goes_on_when_it_is_answered() {
    let _alone = the_installed_plugins_to_myself();
    let (mut window, _) = window_with_plugins_on_the_clip(vec![a_reverb()], &["Tap reverb"]);
    let chooser = ScriptedFileChooser::default();
    window.use_file_chooser(&chooser);
    chooser.will_answer(None);

    window.application_mut().ask_where_to_export_and_export();
    window.settle();

    assert!(the_question_shown(&window).is_some(), "the folder is asked about first");
    assert!(
        chooser.questions_asked().is_empty(),
        "and the person is not asked where to save yet"
    );

    say_yes(&mut window);
    window.settle();

    let asked = chooser.questions_asked();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(asked[0].title, "Export the video");
}

#[test]
fn an_export_does_not_wait_when_the_folders_of_its_plugins_have_been_answered_for_even_with_a_no() {
    let _alone = the_installed_plugins_to_myself();
    let (mut window, _) = window_with_plugins_on_the_clip(vec![a_reverb()], &["Tap reverb"]);
    open_it_again(&mut window);
    say_no(&mut window);
    let chooser = ScriptedFileChooser::default();
    window.use_file_chooser(&chooser);
    chooser.will_answer(None);

    window.application_mut().ask_where_to_export_and_export();
    window.settle();

    assert!(the_question_shown(&window).is_none());
    assert_eq!(
        chooser.questions_asked().len(),
        1,
        "the person is asked where to save at once"
    );
}

#[test]
fn the_plugins_an_export_would_not_run_are_listed_with_the_reason_until_the_folder_is_approved() {
    let _alone = the_installed_plugins_to_myself();
    let reverb = a_reverb();
    let (mut window, _) = window_with_plugins_on_the_clip(
        vec![reverb.clone(), a_chorus_in_the_same_folder()],
        &["Tap reverb", "Tap chorus"],
    );
    open_it_again(&mut window);

    let lines = window.application().plugins_that_an_export_would_not_run();
    assert_eq!(lines.len(), 1, "one line for the folder: {lines:?}");
    assert!(
        lines[0].contains("\u{ab}tap chorus\u{bb}, \u{ab}tap reverb\u{bb}"),
        "{lines:?}"
    );
    assert!(lines[0].contains("nobody has said yes"), "{lines:?}");

    say_no(&mut window);
    let lines = window.application().plugins_that_an_export_would_not_run();
    assert!(lines[0].contains("you said no"), "{lines:?}");

    open_it_again(&mut window);
    say_yes(&mut window);
    assert!(window.application().plugins_that_an_export_would_not_run().is_empty());
}

// --- what the plugins say ---

#[test]
fn a_note_of_a_plugin_is_told_once_while_the_sound_plays_and_again_when_it_plays_again() {
    let mut window = Window::showing(super::test_window::project_of(100));
    let note = "The sound plugin \u{ab}tap reverb\u{bb} was too slow.".to_owned();

    window.application_mut().tell_these_notes(vec![note.clone()], true);
    window.application_mut().tell_these_notes(vec![note.clone()], true);
    let (title, text) = window.application().message_showing().expect("a message");
    assert_eq!(title, "A sound plugin");
    assert_eq!(text.matches("too slow").count(), 1, "told once: {text}");

    window.application_mut().interface.dialog = None;
    window.application_mut().tell_these_notes(vec![note.clone()], true);
    assert!(
        window.application().message_showing().is_none(),
        "not told again while it plays"
    );

    window.application_mut().tell_these_notes(Vec::new(), false);
    window.application_mut().tell_these_notes(vec![note], true);
    assert!(
        window.application().message_showing().is_some(),
        "told again when it plays again"
    );
}

// --- where libraries are looked for ---

#[test]
fn the_places_to_look_for_the_libraries_that_plugins_need_are_told_to_the_host() {
    let mut window = Window::showing(super::test_window::project_of(100));
    assert_eq!(
        window.application().plugin_host().where_dependencies_are_looked_for(),
        (Vec::new(), true)
    );

    let folder = tempfile::tempdir().unwrap();
    window.application_mut().add_dependency_folder(folder.path().to_owned());
    assert_eq!(
        window.application().plugin_host().where_dependencies_are_looked_for(),
        (vec![folder.path().to_owned()], true)
    );

    window.do_this(crate::application::commands::Command::SoundPlugins(
        crate::application::sound_plugins_window::SoundPluginsCommand::SetSearchParentFolders(false),
    ));
    assert_eq!(
        window.application().plugin_host().where_dependencies_are_looked_for(),
        (vec![folder.path().to_owned()], false)
    );
}

// --- the breadcrumb ---

#[test]
fn the_breadcrumb_of_the_libraries_in_use_is_taken_away_when_the_window_is_closed_and_not_before() {
    let _alone = the_installed_plugins_to_myself();
    let plugins = the_plugins_of_the_test_library();
    set_installed_plugins(plugins.clone());
    let mut window = Window::showing(super::test_window::project_of(100));
    let folder = tempfile::tempdir().unwrap();
    let breadcrumb = folder.path().join("in_use.json");
    window
        .application_mut()
        .keep_a_breadcrumb_of_the_plugins_in_use_in(breadcrumb.clone());
    window
        .application()
        .plugin_host()
        .approve_folder(&folder_of(&plugins[0].library));

    the_sound_comes_to(&window, EffectKind::installed_plugin_kinds()[0], &Notes::default());
    assert!(breadcrumb.exists(), "the library is written down before it is loaded");
    window.settle();
    assert!(breadcrumb.exists(), "and stays while the editor runs");

    window.ask_to_close();

    assert!(!breadcrumb.exists(), "the editor ended well");
}

#[test]
fn a_breadcrumb_left_by_a_run_that_ended_badly_fails_the_library_and_says_so() {
    let folder = tempfile::tempdir().unwrap();
    let library = folder.path().join("bad_plugin.dll");
    std::fs::write(&library, "bytes").unwrap();
    let breadcrumb = folder.path().join("in_use.json");
    let first = crate::plugin_scan::Breadcrumb::at(breadcrumb.clone());
    let mut first = first;
    first.note_use_of(&folder.path().join("good_plugin.dll")).unwrap();
    first.note_use_of(&library).unwrap();
    let cache = folder.path().join("scans.json");
    let mut window = Window::showing(super::test_window::project_of(100));
    window
        .application_mut()
        .use_this_plugin_scanner(folder.path().join("no_scan_program"), Some(cache.clone()), false);

    window
        .application_mut()
        .keep_a_breadcrumb_of_the_plugins_in_use_in(breadcrumb.clone());

    let (title, text) = window.application().message_showing().expect("a message");
    assert_eq!(title, "The editor ended badly");
    assert!(
        text.contains("bad_plugin.dll") && text.contains("1 other library"),
        "{text}"
    );
    assert!(cache.exists(), "the library is failed in the file of scans");
    assert!(!breadcrumb.exists(), "what the last run left is taken away");
}

// --- what the mutation campaign found missing ---

#[test]
fn a_note_is_told_again_in_another_project_and_not_at_all_when_the_sound_is_not_playing() {
    let mut window = Window::showing(super::test_window::project_of(100));
    let note = "The sound plugin \u{ab}tap reverb\u{bb} was too slow.".to_owned();

    window.application_mut().tell_these_notes(vec![note.clone()], false);
    assert!(
        window.application().message_showing().is_none(),
        "nothing is told while the sound is not playing"
    );

    window.application_mut().tell_these_notes(vec![note.clone()], true);
    assert!(window.application().message_showing().is_some());
    window.application_mut().interface.dialog = None;
    window.application_mut().tell_these_notes(vec![note.clone()], true);
    assert!(window.application().message_showing().is_none(), "told once");

    window.show_another_project(super::test_window::project_of(100));
    window.application_mut().tell_these_notes(vec![note], true);
    assert!(
        window.application().message_showing().is_some(),
        "what was told of another project is not kept"
    );
}

#[test]
fn an_export_that_was_waiting_for_the_answers_is_forgotten_when_another_project_is_opened() {
    let _alone = the_installed_plugins_to_myself();
    let (mut window, _) = window_with_plugins_on_the_clip(vec![a_reverb()], &["Tap reverb"]);
    let chooser = ScriptedFileChooser::default();
    window.use_file_chooser(&chooser);
    window.application_mut().ask_where_to_export_and_export();
    assert!(window.application().plugin_trust().export_is_waiting);

    open_it_again(&mut window);

    assert!(
        !window.application().plugin_trust().export_is_waiting,
        "the questions it waited for are gone, and it must not start when others are answered"
    );
}

// Added on 2026-10-21: the folder of the plugins that come with the program was trusted by the window of an effect and not by the questions, which asked about it all the same.
#[test]
fn the_folder_of_the_plugins_that_came_with_the_program_is_not_asked_about_when_the_project_is_opened_or_exported() {
    let _alone = the_installed_plugins_to_myself();
    let reverb = a_reverb();
    let (mut window, _) = window_with_plugins_on_the_clip(vec![reverb.clone()], &["Tap reverb"]);
    window
        .application_mut()
        .trust_the_plugins_bundled_in(&crate::plugin_trust::folder_of(&reverb.library));

    open_it_again(&mut window);

    assert!(
        the_question_shown(&window).is_none(),
        "nobody is asked about the program's own plugins"
    );
    assert!(window.application().plugin_trust().waiting.is_empty());
    assert!(
        !window.application_mut().ask_about_the_folders_the_sound_needs(),
        "nothing is left to be answered for"
    );
    assert!(
        window.application().plugins_that_an_export_would_not_run().is_empty(),
        "and the export says nothing of them: they are run"
    );
}

#[test]
fn a_question_that_is_waiting_is_not_put_in_the_queue_again() {
    let _alone = the_installed_plugins_to_myself();
    let (reverb, amp) = (a_reverb(), a_plugin_of_another_folder());
    let (mut window, _) = window_with_plugins_on_the_clip(vec![reverb, amp], &["Tap reverb", "Amp"]);
    open_it_again(&mut window);
    assert!(the_question_shown(&window).is_some());
    assert_eq!(
        window.application().plugin_trust().waiting.len(),
        1,
        "one is on the screen, the other waits"
    );

    let unanswered = window.application_mut().ask_about_the_folders_the_sound_needs();

    assert!(unanswered, "both folders are still to be answered for");
    assert_eq!(
        window.application().plugin_trust().waiting.len(),
        1,
        "the one that waits is not put in a second time"
    );
}

#[test]
fn an_export_does_not_go_on_while_a_question_is_still_on_the_screen() {
    let _alone = the_installed_plugins_to_myself();
    let (reverb, amp) = (a_reverb(), a_plugin_of_another_folder());
    let (mut window, _) = window_with_plugins_on_the_clip(vec![reverb, amp], &["Tap reverb", "Amp"]);
    let chooser = ScriptedFileChooser::default();
    window.use_file_chooser(&chooser);
    chooser.will_answer(None);

    window.application_mut().ask_where_to_export_and_export();
    window.settle();
    assert!(the_question_shown(&window).is_some());
    say_yes(&mut window);
    window.settle();

    assert!(
        the_question_shown(&window).is_some(),
        "the second folder is asked about"
    );
    assert_eq!(window.application().plugin_trust().waiting.len(), 0);
    assert!(
        chooser.questions_asked().is_empty(),
        "with the second question on the screen the person is not asked where to save"
    );
    say_yes(&mut window);
    window.settle();
    assert_eq!(
        chooser.questions_asked().len(),
        1,
        "after the last answer the export goes on"
    );
}

#[test]
fn the_places_for_libraries_that_the_settings_file_says_reach_the_host_when_it_is_read() {
    let mut window = Window::showing(super::test_window::project_of(100));
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("plugin_search.json");
    crate::plugin_scan::PluginSearchSettings {
        folders: Vec::new(),
        dependency_folders: vec![folder.path().to_owned()],
        search_parent_folders: false,
    }
    .write(&file)
    .unwrap();

    window.application_mut().keep_plugin_search_settings_in(file);

    assert_eq!(
        window.application().plugin_host().where_dependencies_are_looked_for(),
        (vec![folder.path().to_owned()], false)
    );
}
