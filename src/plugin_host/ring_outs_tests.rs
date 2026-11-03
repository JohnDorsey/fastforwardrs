// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! When the tails of the sound effects of a project are measured (docs/engine-phase-15-vst3-design.md, section 16): what is wanted is tried once, a plugin that could not be measured is not
//! tried at every frame, and whatever happens to the thread that measures lets go of what it claimed. (What a measurement finds is tested with real plugins, in
//! `tests/plugin_host_processes.rs`; here the plugins cannot be run, which is how a failure is made to happen, and a try is seen as the question the host asks of a folder.)

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{InAProcessOfItsOwn, PluginHost};
use crate::composition::{note_the_measured_tail, the_tails_to_measure_in};
use crate::project::tracks::Clip;
use crate::project::{AnimationTarget, EffectKind, Project, PropertyValue, set_installed_plugins};
use crate::test_support::{an_installed_sound_plugin, put_an_effect_on, the_installed_plugins_to_myself};

/// A project with a sound clip that has the effect of a plugin that is installed but is in a folder that nobody has said yes to (so that every try to measure it is a question).
fn a_project_with_an_effect_that_cannot_be_run() -> Project {
    let plugin = an_installed_sound_plugin("ladspa:a:reverb", "Maker", &[("decay", 1.0, Some((0.0, 10.0)))]);
    set_installed_plugins(vec![plugin]);
    let mut project = crate::composition::test_projects::project_of(vec![vec![(
        0,
        Clip::Audio(crate::composition::test_projects::media_clip(
            crate::composition::test_projects::SOUND_FILE,
            0,
            60,
            1,
        )),
    )]]);
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    put_an_effect_on(
        &mut project,
        AnimationTarget::Clip(clip),
        EffectKind::installed_plugin_kinds()[0],
        &[PropertyValue::Number(10.0)],
    );
    project
}

/// A host that would run the plugin in a program, if it were allowed to.
fn a_host() -> Arc<PluginHost> {
    Arc::new(PluginHost::new().running_in_a_process_of_its_own(InAProcessOfItsOwn::Everything))
}

/// Whether the host was asked to run the plugin since the last time this was asked (it asks the person about the folder, once, each time it is wanted).
fn was_tried(host: &PluginHost) -> bool {
    !host.take_wanted().is_empty()
}

fn wait_until(what: &str, mut is_so: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !is_so() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_tail_that_cannot_be_measured_is_tried_once_and_tried_again_only_when_what_may_be_run_has_changed() {
    let _alone = the_installed_plugins_to_myself();
    let project = a_project_with_an_effect_that_cannot_be_run();
    let host = a_host();

    host.measure_the_tails_now(&project);
    assert!(was_tried(&host), "wanted, so tried");

    host.measure_the_tails_now(&project);
    host.measure_the_tails_now(&project);
    assert!(!was_tried(&host), "not tried again at the next frame, nor the next");

    host.forget_what_was_declined();
    assert!(!was_tried(&host), "nothing changed in what may be run");
    host.decline_folder(std::path::Path::new("/somewhere/else"));
    host.measure_the_tails_now(&project);
    assert!(was_tried(&host), "what may be run changed: tried again");
}

#[test]
fn a_tail_that_is_measured_already_is_not_tried_at_all() {
    let _alone = the_installed_plugins_to_myself();
    let project = a_project_with_an_effect_that_cannot_be_run();
    let host = a_host();
    let wanted = the_tails_to_measure_in(&project);
    assert_eq!(wanted.len(), 1);
    note_the_measured_tail(wanted[0].key, 480_000);

    host.measure_the_tails_now(&project);
    host.measure_the_tails_in_the_background(&project);

    assert!(!was_tried(&host));
}

#[test]
fn the_thread_of_the_background_tries_what_is_wanted_and_lets_go_of_it_so_that_it_is_not_tried_again() {
    let _alone = the_installed_plugins_to_myself();
    let project = a_project_with_an_effect_that_cannot_be_run();
    let host = a_host();

    host.measure_the_tails_in_the_background(&project);
    let mut tried = false;
    wait_until("the thread did not try", || {
        tried |= was_tried(&host);
        tried
    });
    // (Its claim is let go of when it ends; the calls below wait for nothing, as nothing is being measured, and do not try again.)
    wait_until("the thread did not let go", || {
        host.how_many_tails_are_being_measured() == 0
    });
    host.measure_the_tails_now(&project);
    host.measure_the_tails_in_the_background(&project);
    std::thread::sleep(Duration::from_millis(50));
    assert!(
        !was_tried(&host),
        "a failure is not tried again by the background either"
    );
}

#[test]
fn a_project_with_nothing_wanted_starts_no_thread_and_asks_nothing() {
    let _alone = the_installed_plugins_to_myself();
    let project = crate::composition::test_projects::project_of(vec![vec![(
        0,
        Clip::Audio(crate::composition::test_projects::media_clip(
            crate::composition::test_projects::SOUND_FILE,
            0,
            60,
            1,
        )),
    )]]);
    let host = a_host();
    host.measure_the_tails_in_the_background(&project);
    host.measure_the_tails_now(&project);
    assert!(!was_tried(&host));
}
