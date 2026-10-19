// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The rules about who may run a plugin, with a loader that loads nothing and says what it was asked.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::extraction::remove_old_folders_for_a_test;
use super::*;
use crate::digest::Sha256Digest;
use crate::project::{
    EmbeddedBuild, LinkedLibrary, PluginLibraries, PluginLibrary, PluginLibraryKind, PluginLibrarySource,
    is_a_platform_name,
};

const THIS: &str = "windows-x86_64";
const ANOTHER: &str = "linux-x86_64";

// --- the hold ---

/// Tells it that the button is held (or not) at every tenth of a second from `from` for `seconds`, the way a window that is drawn ten times a second would, and gives the time
/// of the last tell. (The times are whole numbers of tenths, so that they do not add up errors.)
fn hold_for(hold: &mut HoldToConfirm, from: f64, seconds: f64, held: bool) -> f64 {
    let tenths = (seconds * 10.0).round() as u32;
    for tenth in 0..=tenths {
        hold.update(from + f64::from(tenth) / 10.0, held);
    }
    from + f64::from(tenths) / 10.0
}

#[test]
fn holding_for_five_seconds_without_a_break_says_yes_and_not_a_moment_before() {
    let mut hold = HoldToConfirm::default();
    hold_for(&mut hold, 100.0, 4.9, true);
    assert!(!hold.is_confirmed(), "4.9 seconds");
    hold.update(104.99, true);
    assert!(!hold.is_confirmed(), "4.99 seconds");
    let progress = hold.update(105.0, true);
    assert!((progress - 1.0).abs() < 1e-9, "{progress}");
    assert!(hold.is_confirmed());
    assert_eq!(SECONDS_TO_HOLD, 5.0);
    assert_eq!(
        HoldToConfirm::default().update(1.0, true),
        0.0,
        "the moment it is pressed it has got nowhere"
    );
}

#[test]
fn a_hold_that_is_let_go_has_got_nowhere_and_never_gets_past_the_end() {
    let mut hold = HoldToConfirm::default();
    hold_for(&mut hold, 10.0, 2.0, true);
    assert_eq!(hold.update(12.1, false), 0.0, "letting go says nowhere");
    let mut hold = HoldToConfirm::default();
    hold.update(0.0, true);
    for step in 1..=14 {
        let progress = hold.update(f64::from(step) * 0.4, true);
        assert!(progress <= 1.0, "{progress} at step {step}");
    }
}

#[test]
fn a_silence_of_half_a_second_goes_on_and_a_longer_one_starts_over() {
    let mut hold = HoldToConfirm::default();
    hold.update(0.0, true);
    hold.update(0.5, true);
    assert!(
        (hold.update(1.0, true) - 0.2).abs() < 1e-9,
        "0.5 s of silence is not too long"
    );
    let mut hold = HoldToConfirm::default();
    hold.update(0.0, true);
    hold.update(0.55, true);
    assert_eq!(hold.update(1.1, true), 0.0, "0.55 s of silence starts it over");
}

#[test]
fn the_folders_of_a_run_that_ended_badly_are_removed_after_a_day() {
    assert_eq!(super::extraction::OLD_ENOUGH_TO_BE_REMOVED, Duration::from_secs(86_400));
}

#[test]
fn how_far_it_has_got_goes_up_while_it_is_held_and_is_drawn_from_the_time_it_is_asked_at() {
    let mut hold = HoldToConfirm::default();
    hold_for(&mut hold, 10.0, 1.0, true);
    assert!((hold.progress_at(11.0) - 0.2).abs() < 1e-9);
    assert!(
        (hold.progress_at(11.25) - 0.25).abs() < 1e-9,
        "a moment later, as it will be drawn"
    );
    assert_eq!(
        hold.progress_at(30.0),
        0.0,
        "not told of for long: it is not being held any more"
    );
    assert_eq!(HoldToConfirm::default().progress_at(1.0), 0.0);
}

#[test]
fn letting_go_starts_it_over_however_far_it_had_got() {
    let mut hold = HoldToConfirm::default();
    let now = hold_for(&mut hold, 0.0, 4.5, true);
    assert!(hold.progress_at(now) > 0.85);
    hold.update(now + 0.05, false);
    assert_eq!(hold.progress_at(now + 0.05), 0.0);
    assert_eq!(hold.update(now + 0.1, true), 0.0, "held again: from nothing");
    let now = hold_for(&mut hold, now + 0.1, 4.0, true);
    assert!(!hold.is_confirmed(), "4 seconds of the second hold are not 5");
    hold_for(&mut hold, now, 1.0, true);
    assert!(hold.is_confirmed());
}

#[test]
fn a_window_that_was_not_drawn_for_a_while_is_a_hand_that_let_go() {
    let mut hold = HoldToConfirm::default();
    hold_for(&mut hold, 0.0, 0.4, true);
    // Nothing for ten seconds (another window was in front), and then "held": a new hold, not the old one grown.
    assert_eq!(hold.update(10.4, true), 0.0);
    assert!(!hold.is_confirmed());
    hold.update(10.8, true);
    assert!(hold.progress_at(10.8) < 0.1);
    // Four and a half seconds in steps a half a second apart are held: a window that is drawn twice a second is still a hand that stays.
    let mut slow = HoldToConfirm::default();
    for step in 0..=10 {
        slow.update(f64::from(step) * 0.5, true);
    }
    assert!(slow.is_confirmed());
}

#[test]
fn once_it_has_said_yes_it_stays_yes() {
    let mut hold = HoldToConfirm::default();
    let now = hold_for(&mut hold, 0.0, 5.0, true);
    assert!(hold.is_confirmed());
    assert_eq!(hold.update(now + 0.1, false), 1.0);
    assert!(hold.is_confirmed());
    assert_eq!(hold.progress_at(now + 100.0), 1.0);
}

// --- the platform ---

#[test]
fn this_platform_is_named_as_a_build_names_one() {
    let name = this_platform();
    assert!(is_a_platform_name(&name), "{name}");
    assert_eq!(name, format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH));
}

// --- the approvals ---
//
// (Rewritten on purpose on 2026-10-19: a plugin used to be approved by its path and what was in it, and a file that changed was asked about again. The maintainer does not want a
// question for every locally installed plugin: **a folder is approved, once, and what is in it is approved with it.**)

fn digest_of(text: &str) -> Sha256Digest {
    Sha256Digest::of_bytes(text.as_bytes())
}

#[test]
fn a_folder_is_approved_for_what_is_in_it_and_not_for_the_folders_in_it_or_beside_it() {
    let root = tempfile::tempdir().unwrap();
    let plugins = root.path().join("plugins");
    let inside = plugins.join("more");
    let beside = root.path().join("others");
    for folder in [&plugins, &inside, &beside] {
        fs::create_dir_all(folder).unwrap();
    }
    let mut store = ApprovalStore::in_memory();
    assert!(!store.is_folder_approved(&plugins));
    store.approve_folder(&plugins);
    assert!(store.is_folder_approved(&plugins));
    assert!(
        !store.is_folder_approved(&inside),
        "a folder in it is another folder, asked about on its own"
    );
    assert!(!store.is_folder_approved(&beside));
    assert!(
        !store.is_folder_approved(root.path()),
        "and the folder it is in is not approved by it"
    );
    assert_eq!((store.len(), store.is_empty()), (1, false));
    assert_eq!(store.last_problem(), None);
}

#[test]
fn a_folder_reached_in_another_way_is_the_same_folder() {
    let root = tempfile::tempdir().unwrap();
    let plugins = root.path().join("plugins");
    fs::create_dir_all(plugins.join("more")).unwrap();
    let mut store = ApprovalStore::in_memory();
    store.approve_folder(&plugins);
    assert!(store.is_folder_approved(&plugins.join("more").join("..")));
    assert!(store.is_folder_approved(&plugins.join(".")));
    store.approve_folder(&plugins.join("more").join(".."));
    assert_eq!(store.len(), 1, "it was one folder all the time");
    // The folder of a file is the folder it is in.
    let file = a_file_with(&plugins, "amp.so", "bytes");
    assert_eq!(folder_of(&file), folder_key(&plugins));
}

#[test]
fn a_folder_that_is_not_there_is_a_folder_all_the_same_by_the_way_it_was_named() {
    let mut store = ApprovalStore::in_memory();
    store.approve_folder(Path::new("/not/there/plugins"));
    assert!(store.is_folder_approved(Path::new("/not/there/plugins")));
    assert!(!store.is_folder_approved(Path::new("/not/there/other")));
}

#[test]
fn approvals_are_kept_in_a_file_and_read_back_by_the_next_run_and_the_file_is_the_same_text_each_time() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("settings folder").join("plugin_approvals.json");
    let (first, second) = (root.path().join("a_first"), root.path().join("b_second"));
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    let mut store = ApprovalStore::at(file.clone());
    store.approve_folder(&second);
    store.approve_folder(&first);
    assert_eq!(store.last_problem(), None);
    let text = fs::read_to_string(&file).unwrap();
    assert!(
        text.find("a_first").unwrap() < text.find("b_second").unwrap(),
        "in order, so that the file is the same text for the same approvals"
    );

    let next_run = ApprovalStore::at(file.clone());
    assert!(next_run.is_folder_approved(&first));
    assert!(next_run.is_folder_approved(&second));
    assert_eq!(next_run.len(), 2);
    let mut again = ApprovalStore::at(file.clone());
    again.approve_folder(&first);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        text,
        "approving what is approved changes nothing"
    );
}

#[test]
fn a_file_of_approvals_that_cannot_be_read_is_no_approvals_and_says_so_and_does_not_stop_a_new_one() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("plugin_approvals.json");
    fs::write(&file, "this is not json").unwrap();
    let mut store = ApprovalStore::at(file.clone());
    assert!(store.is_empty());
    assert!(
        store
            .last_problem()
            .is_some_and(|problem| problem.contains("will be asked about again"))
    );
    let plugins = root.path().join("plugins");
    fs::create_dir(&plugins).unwrap();
    store.approve_folder(&plugins);
    assert_eq!(store.last_problem(), None, "writing it made it a file that can be read");
    assert!(ApprovalStore::at(file).is_folder_approved(&plugins));
}

#[test]
fn the_approvals_of_the_first_version_which_were_files_by_path_and_hash_are_not_read_and_are_no_complaint() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("plugin_approvals.json");
    fs::write(
        &file,
        r#"{"approved":[{"path":"/b","sha256":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}]}"#,
    )
    .unwrap();
    let store = ApprovalStore::at(file);
    assert!(store.is_empty(), "a person is asked about the folder instead");
    assert_eq!(store.last_problem(), None);
}

#[test]
fn a_file_of_approvals_that_cannot_be_written_is_said_and_the_approval_holds_until_the_program_ends() {
    let root = tempfile::tempdir().unwrap();
    // The place for the file is a folder.
    let in_the_way = root.path().join("plugin_approvals.json");
    fs::create_dir(&in_the_way).unwrap();
    let mut store = ApprovalStore::at(in_the_way);
    store.approve_folder(root.path());
    assert!(
        store
            .last_problem()
            .is_some_and(|problem| problem.contains("kept only until the program ends"))
    );
    assert!(store.is_folder_approved(root.path()));
}

#[test]
fn a_folder_is_said_to_a_person_without_the_prefix_that_windows_gives_a_long_path() {
    assert_eq!(folder_for_a_person(Path::new(r"\\?\C:\plugins")), r"C:\plugins");
    assert_eq!(folder_for_a_person(Path::new("/usr/lib/ladspa")), "/usr/lib/ladspa");
}

// --- the libraries and the files ---

fn build(platform: &str, text: &str) -> EmbeddedBuild {
    EmbeddedBuild::new(platform, Arc::from(text.as_bytes())).unwrap()
}

fn embedded(name: &str, builds: Vec<EmbeddedBuild>) -> PluginLibrary {
    PluginLibrary::new(name, PluginLibraryKind::Ladspa, PluginLibrarySource::Embedded(builds)).unwrap()
}

fn linked(name: &str, path: &Path, relative: Option<&str>) -> PluginLibrary {
    PluginLibrary::new(
        name,
        PluginLibraryKind::Ladspa,
        PluginLibrarySource::Linked(LinkedLibrary {
            absolute_path: path.to_owned(),
            relative_path: relative.map(PathBuf::from),
        }),
    )
    .unwrap()
}

fn libraries_of(list: Vec<PluginLibrary>) -> PluginLibraries {
    let mut libraries = PluginLibraries::default();
    for library in list {
        libraries.add(library).unwrap();
    }
    libraries
}

/// Loads nothing, and says what it was asked to load.
#[derive(Default)]
struct RecordingLoader {
    asked: Vec<PathBuf>,
    contents_when_asked: Vec<Vec<u8>>,
    says_no: Option<String>,
}

impl LibraryLoader for RecordingLoader {
    type Loaded = PathBuf;

    fn load(&mut self, file: &Path) -> Result<PathBuf, String> {
        self.asked.push(file.to_owned());
        self.contents_when_asked.push(fs::read(file).unwrap_or_default());
        match &self.says_no {
            Some(reason) => Err(reason.clone()),
            None => Ok(file.to_owned()),
        }
    }
}

// --- writing a plugin out ---

#[test]
fn a_build_is_written_out_after_being_checked_and_the_folder_goes_when_the_cache_does() {
    let cache = PluginCache::in_the_temporary_files().unwrap();
    let folder = cache.folder().to_owned();
    assert!(folder.is_dir());
    assert!(
        folder
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("fastforwardrs-plugins-")
    );
    let build = build(THIS, "the library bytes");
    let path = cache.extract("amp.dll", &build).unwrap();
    assert!(path.starts_with(&folder));
    assert_eq!(fs::read(&path).unwrap(), b"the library bytes");
    assert!(path.file_name().unwrap().to_string_lossy().ends_with("amp.dll"));
    assert!(
        path.file_name()
            .unwrap()
            .to_string_lossy()
            .contains(&build.digest().short_hex()),
        "the name has what is in the file in it"
    );
    // Again: the same file, not written again.
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    assert_eq!(cache.extract("amp.dll", &build).unwrap(), path);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    drop(cache);
    assert!(!folder.exists(), "removed with the cache");
}

#[test]
fn a_build_that_is_not_the_one_the_project_says_it_is_is_not_written_out_at_all() {
    let folder = tempfile::tempdir().unwrap();
    let cache = PluginCache::in_folder(folder.path().join("cache")).unwrap();
    let lying = EmbeddedBuild::as_recorded(THIS, digest_of("what it says"), Arc::from(&b"what it is"[..])).unwrap();
    let error = cache.extract("amp.dll", &lying).unwrap_err();
    assert!(matches!(error, ExtractionError::DoesNotMatch { .. }), "{error}");
    assert!(error.to_string().contains("not loaded"), "{error}");
    assert_eq!(fs::read_dir(cache.folder()).unwrap().count(), 0, "nothing was written");
}

#[test]
fn a_file_with_another_in_its_place_that_is_not_what_it_should_be_is_written_again() {
    let folder = tempfile::tempdir().unwrap();
    let cache = PluginCache::in_folder(folder.path().join("cache")).unwrap();
    let build = build(THIS, "the right bytes");
    let path = cache.extract("amp.dll", &build).unwrap();
    fs::write(&path, "somebody changed it").unwrap();
    let again = cache.extract("amp.dll", &build).unwrap();
    assert_eq!(again, path);
    assert_eq!(
        fs::read(&path).unwrap(),
        b"the right bytes",
        "what was there is not trusted"
    );
}

#[test]
fn the_folders_left_by_a_run_that_ended_badly_are_removed_when_they_are_old_and_nothing_else_is() {
    let base = tempfile::tempdir().unwrap();
    let left = base.path().join("fastforwardrs-plugins-1234-5678");
    let other = base.path().join("somebody-elses-folder");
    fs::create_dir(&left).unwrap();
    fs::write(left.join("amp.dll"), "x").unwrap();
    fs::create_dir(&other).unwrap();
    remove_old_folders_for_a_test(base.path(), Duration::from_secs(3600));
    assert!(left.exists(), "it is not old");
    remove_old_folders_for_a_test(base.path(), Duration::ZERO);
    assert!(!left.exists());
    assert!(other.exists(), "only the folders of this program");
}

// --- what has to be asked ---

fn nothing_answered() -> (ApprovalStore, TrustSession) {
    (ApprovalStore::in_memory(), TrustSession::default())
}

fn a_file_with(folder: &Path, name: &str, text: &str) -> PathBuf {
    let path = folder.join(name);
    fs::write(&path, text).unwrap();
    path
}

#[test]
fn an_embedded_plugin_is_asked_about_with_the_build_for_this_computer_and_a_missing_build_is_said() {
    let libraries = libraries_of(vec![
        embedded(
            "amp.dll",
            vec![build(THIS, "windows bytes"), build(ANOTHER, "linux bytes")],
        ),
        embedded("only_linux.dll", vec![build(ANOTHER, "linux")]),
    ]);
    let (approvals, session) = nothing_answered();
    let needed = what_is_needed(&libraries, None, THIS, &approvals, &session);
    assert_eq!(
        needed.questions,
        [Question::Embedded {
            library: "amp.dll".to_owned(),
            platform: THIS.to_owned(),
            size: "windows bytes".len(),
            digest: digest_of("windows bytes"),
        }]
    );
    assert_eq!(
        needed.unavailable,
        [(
            "only_linux.dll".to_owned(),
            Unavailable::NoBuildForThisPlatform {
                platforms: vec![ANOTHER.to_owned()]
            }
        )]
    );
    assert_eq!(needed.questions[0].library(), "amp.dll");
}

#[test]
fn the_plugins_of_a_folder_are_asked_about_once_and_not_again_when_the_files_change_or_another_is_put_in_the_folder() {
    let folder = tempfile::tempdir().unwrap();
    let path = a_file_with(folder.path(), "amp.so", "version one");
    let other = a_file_with(folder.path(), "reverb.so", "reverb");
    let libraries = libraries_of(vec![linked("amp.so", &path, None), linked("reverb.so", &other, None)]);
    let (mut approvals, mut session) = nothing_answered();
    let needed = what_is_needed(&libraries, None, THIS, &approvals, &session);
    let [question] = &needed.questions[..] else {
        panic!("one question for the folder, whatever is in it: {needed:?}")
    };
    assert_eq!(
        *question,
        Question::linked(folder.path(), vec!["amp.so".to_owned(), "reverb.so".to_owned()])
    );
    assert_eq!(question.library(), "amp.so");

    session.answer(question, Answer::Load, &mut approvals);
    assert!(
        what_is_needed(&libraries, None, THIS, &approvals, &session)
            .questions
            .is_empty()
    );
    // A later run has the approval but not the session: still not asked.
    assert!(
        what_is_needed(&libraries, None, THIS, &approvals, &TrustSession::default())
            .questions
            .is_empty()
    );

    // The file changes, and another is put in the folder: the folder was said yes to, so it is not asked about again.
    fs::write(&path, "version two").unwrap();
    let third = a_file_with(folder.path(), "new.so", "something new");
    let with_the_new_one = libraries_of(vec![
        linked("amp.so", &path, None),
        linked("reverb.so", &other, None),
        linked("new.so", &third, None),
    ]);
    assert!(
        what_is_needed(&with_the_new_one, None, THIS, &approvals, &TrustSession::default())
            .questions
            .is_empty(),
        "(this is what the maintainer asked for)"
    );
}

#[test]
fn plugins_in_two_folders_are_asked_about_once_for_each_folder_in_the_order_of_the_libraries() {
    let root = tempfile::tempdir().unwrap();
    let (one, two) = (root.path().join("one"), root.path().join("two"));
    fs::create_dir_all(&one).unwrap();
    fs::create_dir_all(&two).unwrap();
    let libraries = libraries_of(vec![
        linked("a.so", &a_file_with(&one, "a.so", "a"), None),
        linked("b.so", &a_file_with(&two, "b.so", "b"), None),
        linked("c.so", &a_file_with(&one, "c.so", "c"), None),
    ]);
    let (approvals, session) = nothing_answered();
    let questions = what_is_needed(&libraries, None, THIS, &approvals, &session).questions;
    assert_eq!(
        questions,
        [
            Question::linked(&one, vec!["a.so".to_owned(), "c.so".to_owned()]),
            Question::linked(&two, vec!["b.so".to_owned()]),
        ]
    );
}

#[test]
fn a_linked_plugin_that_is_not_there_is_unavailable_and_one_that_moved_with_the_project_is_found_by_its_relative_path()
{
    let folder = tempfile::tempdir().unwrap();
    let path = a_file_with(folder.path(), "amp.so", "bytes");
    let gone = folder.path().join("old place").join("amp.so");
    let libraries = libraries_of(vec![linked("amp.so", &gone, None)]);
    let (approvals, session) = nothing_answered();
    let needed = what_is_needed(&libraries, Some(folder.path()), THIS, &approvals, &session);
    assert!(needed.questions.is_empty());
    assert_eq!(
        needed.unavailable,
        [(
            "amp.so".to_owned(),
            Unavailable::FileNotFound {
                tried: vec![gone.clone()]
            }
        )]
    );

    let with_a_relative_path = libraries_of(vec![linked("amp.so", &gone, Some("amp.so"))]);
    let needed = what_is_needed(&with_a_relative_path, Some(folder.path()), THIS, &approvals, &session);
    assert_eq!(
        needed.questions,
        [Question::linked(path.parent().unwrap(), vec!["amp.so".to_owned()])],
        "asked about the folder it is in now"
    );
    assert!(needed.unavailable.is_empty());
}

#[test]
fn what_a_person_answered_is_not_asked_again_in_the_same_session_and_is_forgotten_with_the_project() {
    let libraries = libraries_of(vec![embedded("amp.dll", vec![build(THIS, "bytes")])]);
    let (mut approvals, mut session) = nothing_answered();
    let question = what_is_needed(&libraries, None, THIS, &approvals, &session)
        .questions
        .remove(0);

    session.answer(&question, Answer::DontLoad, &mut approvals);
    assert!(
        what_is_needed(&libraries, None, THIS, &approvals, &session)
            .questions
            .is_empty(),
        "no is an answer too"
    );
    session.answer(&question, Answer::Load, &mut approvals);
    assert!(
        what_is_needed(&libraries, None, THIS, &approvals, &session)
            .questions
            .is_empty()
    );

    session.forget_everything();
    assert_eq!(
        what_is_needed(&libraries, None, THIS, &approvals, &session)
            .questions
            .len(),
        1
    );
}

#[test]
fn a_yes_to_an_embedded_plugin_is_never_written_down_and_a_yes_to_the_folder_of_a_linked_one_is() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("plugin_approvals.json");
    let path = a_file_with(folder.path(), "amp.so", "bytes");
    let libraries = libraries_of(vec![
        embedded("e.dll", vec![build(THIS, "e")]),
        linked("amp.so", &path, None),
    ]);
    let mut approvals = ApprovalStore::at(file.clone());
    let mut session = TrustSession::default();
    let questions = what_is_needed(&libraries, None, THIS, &approvals, &session).questions;
    assert_eq!(questions.len(), 2);

    session.answer(&questions[0], Answer::Load, &mut approvals);
    assert!(
        approvals.is_empty(),
        "an embedded plugin is asked about every time the project is opened"
    );
    assert!(!file.exists(), "and nothing is written");

    session.answer(&questions[1], Answer::Load, &mut approvals);
    assert_eq!(approvals.len(), 1);
    assert!(
        approvals.is_folder_approved(folder.path()),
        "it is the folder that was written down"
    );
    let next_run = ApprovalStore::at(file);
    let next = what_is_needed(&libraries, None, THIS, &next_run, &TrustSession::default());
    assert_eq!(
        next.questions.len(),
        1,
        "only the embedded one is asked about in the next run"
    );
    assert!(matches!(next.questions[0], Question::Embedded { .. }));
}

// --- the only way to load ---

fn cache_in(folder: &Path) -> PluginCache {
    PluginCache::in_folder(folder.join("cache")).unwrap()
}

#[test]
fn nothing_is_loaded_before_the_person_has_said_yes_whatever_the_library_is() {
    let folder = tempfile::tempdir().unwrap();
    let path = a_file_with(folder.path(), "amp.so", "bytes");
    let libraries = libraries_of(vec![
        linked("amp.so", &path, None),
        embedded("e.dll", vec![build(THIS, "e")]),
    ]);
    let (approvals, session) = nothing_answered();
    let cache = cache_in(folder.path());
    let mut loader = RecordingLoader::default();
    for library in &libraries {
        let result = load_library(library, None, THIS, &approvals, &session, &cache, &mut loader);
        assert_eq!(result, Err(WhyNot::NotAsked), "{}", library.name());
    }
    assert!(loader.asked.is_empty(), "the loader was not asked");
    assert_eq!(
        fs::read_dir(cache.folder()).unwrap().count(),
        0,
        "and nothing was written out"
    );
}

#[test]
fn a_plugin_that_the_person_declined_is_not_loaded_and_is_said_to_have_been_declined() {
    let folder = tempfile::tempdir().unwrap();
    let path = a_file_with(folder.path(), "amp.so", "bytes");
    let libraries = libraries_of(vec![
        linked("amp.so", &path, None),
        embedded("e.dll", vec![build(THIS, "e")]),
    ]);
    let (mut approvals, mut session) = nothing_answered();
    for question in what_is_needed(&libraries, None, THIS, &approvals, &session).questions {
        session.answer(&question, Answer::DontLoad, &mut approvals);
    }
    let cache = cache_in(folder.path());
    let mut loader = RecordingLoader::default();
    for library in &libraries {
        assert_eq!(
            load_library(library, None, THIS, &approvals, &session, &cache, &mut loader),
            Err(WhyNot::Declined)
        );
    }
    assert!(loader.asked.is_empty());
    assert!(approvals.is_empty());
}

#[test]
fn a_linked_plugin_in_an_approved_folder_is_loaded_where_it_is_even_after_it_has_changed_and_one_in_another_folder_is_not()
 {
    let folder = tempfile::tempdir().unwrap();
    let path = a_file_with(folder.path(), "amp.so", "bytes");
    let library = linked("amp.so", &path, None);
    let libraries = libraries_of(vec![library.clone()]);
    let (mut approvals, mut session) = nothing_answered();
    let question = what_is_needed(&libraries, None, THIS, &approvals, &session)
        .questions
        .remove(0);
    session.answer(&question, Answer::Load, &mut approvals);
    let cache = cache_in(folder.path());
    let mut loader = RecordingLoader::default();

    assert_eq!(
        load_library(&library, None, THIS, &approvals, &session, &cache, &mut loader),
        Ok(path.clone())
    );
    assert_eq!(loader.asked, std::slice::from_ref(&path));

    // The file is changed: the folder was said yes to.
    fs::write(&path, "bytes that somebody changed").unwrap();
    assert_eq!(
        load_library(
            &library,
            None,
            THIS,
            &approvals,
            &TrustSession::default(),
            &cache,
            &mut loader
        ),
        Ok(path.clone())
    );
    // A plugin in a folder that was not said yes to is not loaded.
    let elsewhere = tempfile::tempdir().unwrap();
    let other_path = a_file_with(elsewhere.path(), "other.so", "bytes");
    assert_eq!(
        load_library(
            &linked("other.so", &other_path, None),
            None,
            THIS,
            &approvals,
            &TrustSession::default(),
            &cache,
            &mut loader
        ),
        Err(WhyNot::NotAsked)
    );
    assert_eq!(loader.asked.len(), 2);
}

#[test]
fn what_is_known_of_a_folder_is_that_it_is_approved_declined_or_not_asked_and_a_no_is_for_the_session() {
    let folder = tempfile::tempdir().unwrap();
    let (mut approvals, mut session) = nothing_answered();
    assert_eq!(
        session.standing_of_folder(&approvals, folder.path()),
        FolderStanding::NotAsked
    );
    let question = Question::linked(folder.path(), vec!["amp.so".to_owned()]);
    session.answer(&question, Answer::DontLoad, &mut approvals);
    assert_eq!(
        session.standing_of_folder(&approvals, folder.path()),
        FolderStanding::Declined
    );
    assert!(approvals.is_empty(), "a no is not written down");
    assert_eq!(
        TrustSession::default().standing_of_folder(&approvals, folder.path()),
        FolderStanding::NotAsked,
        "and is forgotten with the project"
    );
    session.answer(&question, Answer::Load, &mut approvals);
    assert_eq!(
        session.standing_of_folder(&approvals, folder.path()),
        FolderStanding::Approved
    );
    assert_eq!(
        TrustSession::default().standing_of_folder(&approvals, folder.path()),
        FolderStanding::Approved,
        "a yes is written down"
    );
}

#[test]
fn an_embedded_plugin_that_the_session_allowed_is_written_out_checked_and_loaded_from_there() {
    let folder = tempfile::tempdir().unwrap();
    let library = embedded(
        "e.dll",
        vec![build(THIS, "the windows build"), build(ANOTHER, "the linux build")],
    );
    let libraries = libraries_of(vec![library.clone()]);
    let (mut approvals, mut session) = nothing_answered();
    let question = what_is_needed(&libraries, None, THIS, &approvals, &session)
        .questions
        .remove(0);
    session.answer(&question, Answer::Load, &mut approvals);
    let cache = cache_in(folder.path());
    let mut loader = RecordingLoader::default();

    let loaded = load_library(&library, None, THIS, &approvals, &session, &cache, &mut loader).unwrap();

    assert!(
        loaded.starts_with(cache.folder()),
        "it is loaded from the cache, not from the project file"
    );
    assert_eq!(
        loader.contents_when_asked,
        [b"the windows build".to_vec()],
        "the build for this computer and no other"
    );
}

#[test]
fn an_embedded_plugin_that_is_not_what_the_project_says_is_not_loaded_even_when_the_person_said_yes() {
    let folder = tempfile::tempdir().unwrap();
    let lying = EmbeddedBuild::as_recorded(
        THIS,
        digest_of("what the project says"),
        Arc::from(&b"what is in it"[..]),
    )
    .unwrap();
    let library = embedded("e.dll", vec![lying]);
    let libraries = libraries_of(vec![library.clone()]);
    let (mut approvals, mut session) = nothing_answered();
    let question = what_is_needed(&libraries, None, THIS, &approvals, &session)
        .questions
        .remove(0);
    session.answer(&question, Answer::Load, &mut approvals);
    let cache = cache_in(folder.path());
    let mut loader = RecordingLoader::default();

    let result = load_library(&library, None, THIS, &approvals, &session, &cache, &mut loader);

    assert!(
        matches!(&result, Err(WhyNot::CouldNotBeWrittenOut(why)) if why.contains("not loaded")),
        "{result:?}"
    );
    assert!(loader.asked.is_empty());
}

#[test]
fn a_library_with_no_build_for_this_computer_or_no_file_is_unavailable_and_the_loader_is_not_asked() {
    let folder = tempfile::tempdir().unwrap();
    let only_linux = embedded("e.dll", vec![build(ANOTHER, "linux")]);
    let missing = linked("amp.so", &folder.path().join("not there.so"), None);
    let (approvals, session) = nothing_answered();
    let cache = cache_in(folder.path());
    let mut loader = RecordingLoader::default();
    assert_eq!(
        load_library(&only_linux, None, THIS, &approvals, &session, &cache, &mut loader),
        Err(WhyNot::Unavailable(Unavailable::NoBuildForThisPlatform {
            platforms: vec![ANOTHER.to_owned()]
        }))
    );
    assert!(matches!(
        load_library(&missing, None, THIS, &approvals, &session, &cache, &mut loader),
        Err(WhyNot::Unavailable(Unavailable::FileNotFound { .. }))
    ));
    assert!(loader.asked.is_empty());
}

#[test]
fn a_loader_that_cannot_load_a_plugin_that_was_allowed_says_why() {
    let folder = tempfile::tempdir().unwrap();
    let library = embedded("e.dll", vec![build(THIS, "bytes")]);
    let libraries = libraries_of(vec![library.clone()]);
    let (mut approvals, mut session) = nothing_answered();
    let question = what_is_needed(&libraries, None, THIS, &approvals, &session)
        .questions
        .remove(0);
    session.answer(&question, Answer::Load, &mut approvals);
    let cache = cache_in(folder.path());
    let mut loader = RecordingLoader {
        says_no: Some("the library is for another processor".to_owned()),
        ..RecordingLoader::default()
    };
    assert_eq!(
        load_library(&library, None, THIS, &approvals, &session, &cache, &mut loader),
        Err(WhyNot::LoaderFailed("the library is for another processor".to_owned()))
    );
}

// --- the command line ---

#[test]
fn the_command_line_refuses_the_embedded_plugins_for_this_computer_and_the_linked_ones_whose_folder_is_not_approved() {
    let root = tempfile::tempdir().unwrap();
    let (approved, unapproved) = (root.path().join("approved"), root.path().join("unapproved"));
    fs::create_dir_all(&approved).unwrap();
    fs::create_dir_all(&unapproved).unwrap();
    let approved_path = a_file_with(&approved, "approved.so", "fine");
    let also_approved = a_file_with(&approved, "new_in_the_same_folder.so", "never seen");
    let unapproved_path = a_file_with(&unapproved, "unapproved.so", "not yet");
    let libraries = libraries_of(vec![
        embedded("embedded.dll", vec![build(THIS, "e")]),
        embedded("for_linux.dll", vec![build(ANOTHER, "e")]),
        linked("approved.so", &approved_path, None),
        linked("new_in_the_same_folder.so", &also_approved, None),
        linked("unapproved.so", &unapproved_path, None),
        linked("missing.so", &root.path().join("missing.so"), None),
    ]);
    let mut approvals = ApprovalStore::in_memory();
    approvals.approve_folder(&approved);

    let refused = what_the_command_line_refuses(&libraries, None, THIS, &approvals);

    assert_eq!(refused.len(), 3, "{refused:#?}");
    assert!(refused[0].starts_with("embedded.dll") && refused[0].contains("inside the project file"));
    assert!(
        refused[1].starts_with("unapproved.so")
            && refused[1].contains("folder")
            && refused[1].contains("not been approved")
    );
    assert!(refused[2].starts_with("missing.so") && refused[2].contains("cannot be read"));
    assert!(what_the_command_line_refuses(&PluginLibraries::default(), None, THIS, &approvals).is_empty());
}
