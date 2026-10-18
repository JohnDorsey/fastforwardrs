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

fn digest_of(text: &str) -> Sha256Digest {
    Sha256Digest::of_bytes(text.as_bytes())
}

#[test]
fn a_plugin_is_approved_by_its_path_and_its_contents_and_by_nothing_less() {
    let mut store = ApprovalStore::in_memory();
    let path = Path::new("/plugins/amp.so");
    assert!(!store.is_approved(path, digest_of("one")));
    store.approve(path, digest_of("one"));
    assert!(store.is_approved(path, digest_of("one")));
    assert!(
        !store.is_approved(path, digest_of("changed")),
        "the file has changed: it is another file"
    );
    assert!(
        !store.is_approved(Path::new("/elsewhere/amp.so"), digest_of("one")),
        "the same file somewhere else is asked about"
    );
    assert_eq!((store.len(), store.is_empty()), (1, false));
    assert_eq!(store.last_problem(), None);
}

#[test]
fn approvals_are_kept_in_a_file_and_read_back_by_the_next_run_and_the_file_is_the_same_text_each_time() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("settings folder").join("plugin_approvals.json");
    let mut store = ApprovalStore::at(file.clone());
    store.approve(Path::new("/b/second.so"), digest_of("two"));
    store.approve(Path::new("/a/first.so"), digest_of("one"));
    assert_eq!(store.last_problem(), None);
    let text = fs::read_to_string(&file).unwrap();
    assert!(
        text.find("/a/first.so").unwrap() < text.find("/b/second.so").unwrap(),
        "in order, so that the file is the same text for the same approvals"
    );

    let next_run = ApprovalStore::at(file.clone());
    assert!(next_run.is_approved(Path::new("/a/first.so"), digest_of("one")));
    assert!(next_run.is_approved(Path::new("/b/second.so"), digest_of("two")));
    assert_eq!(next_run.len(), 2);
    let mut again = ApprovalStore::at(file.clone());
    again.approve(Path::new("/a/first.so"), digest_of("one"));
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        text,
        "approving what is approved changes nothing"
    );
}

#[test]
fn a_file_of_approvals_that_cannot_be_read_is_no_approvals_and_says_so_and_does_not_stop_a_new_one() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("plugin_approvals.json");
    fs::write(&file, "this is not json").unwrap();
    let mut store = ApprovalStore::at(file.clone());
    assert!(store.is_empty());
    assert!(
        store
            .last_problem()
            .is_some_and(|problem| problem.contains("will be asked about again"))
    );
    store.approve(Path::new("/p.so"), digest_of("x"));
    assert_eq!(store.last_problem(), None, "writing it made it a file that can be read");
    assert!(ApprovalStore::at(file).is_approved(Path::new("/p.so"), digest_of("x")));

    // An entry that is no digest is left out; the others are read.
    let other = folder.path().join("other.json");
    fs::write(&other, r#"{"approved":[{"path":"/a","sha256":"nonsense"},{"path":"/b","sha256":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}]}"#).unwrap();
    let store = ApprovalStore::at(other);
    assert_eq!(store.len(), 1);
    assert!(store.is_approved(Path::new("/b"), digest_of("abc")));
}

#[test]
fn a_file_of_approvals_that_cannot_be_written_is_said_and_the_approval_holds_until_the_program_ends() {
    let folder = tempfile::tempdir().unwrap();
    // The place for the file is a folder.
    let in_the_way = folder.path().join("plugin_approvals.json");
    fs::create_dir(&in_the_way).unwrap();
    let mut store = ApprovalStore::at(in_the_way);
    store.approve(Path::new("/p.so"), digest_of("x"));
    assert!(
        store
            .last_problem()
            .is_some_and(|problem| problem.contains("kept only until the program ends"))
    );
    assert!(store.is_approved(Path::new("/p.so"), digest_of("x")));
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
fn a_linked_plugin_is_asked_about_until_it_is_approved_and_asked_about_again_when_the_file_changes() {
    let folder = tempfile::tempdir().unwrap();
    let path = a_file_with(folder.path(), "amp.so", "version one");
    let libraries = libraries_of(vec![linked("amp.so", &path, None)]);
    let (mut approvals, mut session) = nothing_answered();
    let needed = what_is_needed(&libraries, None, THIS, &approvals, &session);
    let [question] = &needed.questions[..] else {
        panic!("{needed:?}")
    };
    assert_eq!(
        *question,
        Question::Linked {
            library: "amp.so".to_owned(),
            path: path.clone(),
            digest: digest_of("version one"),
        }
    );

    session.answer(question, Answer::Load, &mut approvals);
    assert!(
        what_is_needed(&libraries, None, THIS, &approvals, &session)
            .questions
            .is_empty()
    );
    // A later run has the approval but not the session: still not asked, because the file is the same.
    assert!(
        what_is_needed(&libraries, None, THIS, &approvals, &TrustSession::default())
            .questions
            .is_empty()
    );

    // The file changes: it is another file.
    fs::write(&path, "version two").unwrap();
    let again = what_is_needed(&libraries, None, THIS, &approvals, &TrustSession::default());
    assert_eq!(again.questions.len(), 1, "asked again");
    assert!(matches!(&again.questions[0], Question::Linked { digest, .. } if *digest == digest_of("version two")));
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
    assert!(
        matches!(&needed.questions[..], [Question::Linked { path: found, .. }] if *found == path),
        "{needed:?}"
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
fn a_yes_to_an_embedded_plugin_is_never_written_down_and_a_yes_to_a_linked_one_is() {
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
fn a_linked_plugin_that_was_approved_is_loaded_where_it_is_and_not_once_it_has_changed() {
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
        Err(WhyNot::NotAsked),
        "a file that has changed is asked about again, and is not loaded until it is"
    );
    assert_eq!(loader.asked.len(), 1);
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
fn the_command_line_refuses_the_embedded_plugins_for_this_computer_and_the_linked_ones_that_are_not_approved() {
    let folder = tempfile::tempdir().unwrap();
    let approved_path = a_file_with(folder.path(), "approved.so", "fine");
    let unapproved_path = a_file_with(folder.path(), "unapproved.so", "not yet");
    let libraries = libraries_of(vec![
        embedded("embedded.dll", vec![build(THIS, "e")]),
        embedded("for_linux.dll", vec![build(ANOTHER, "e")]),
        linked("approved.so", &approved_path, None),
        linked("unapproved.so", &unapproved_path, None),
        linked("missing.so", &folder.path().join("missing.so"), None),
    ]);
    let mut approvals = ApprovalStore::in_memory();
    approvals.approve(&approved_path, digest_of("fine"));

    let refused = what_the_command_line_refuses(&libraries, None, THIS, &approvals);

    assert_eq!(refused.len(), 3, "{refused:#?}");
    assert!(refused[0].starts_with("embedded.dll") && refused[0].contains("inside the project file"));
    assert!(refused[1].starts_with("unapproved.so") && refused[1].contains("not been approved"));
    assert!(refused[2].starts_with("missing.so") && refused[2].contains("cannot be read"));
    assert!(what_the_command_line_refuses(&PluginLibraries::default(), None, THIS, &approvals).is_empty());
}
