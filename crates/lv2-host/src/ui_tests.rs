// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the windows of plugins (`ui.rs`) with the user interfaces of the test plugins (`crates/lv2-test-plugins`, `ui.rs`), loaded in this process, **with no window to put them in**
//! (there is no screen where these run): what is tried is what the host gives an interface, how it is told values and given time, and what it does comes out as events.

use std::path::PathBuf;

use lv2_test_plugins::{FAILS, FAILS_UI, GAIN, GAIN_UI, LATENCY, QUIET_UI, UI_CLOSE_VALUE, UI_EDIT_VALUE, UI_SIZE};
use tempfile::TempDir;

use crate::description::{PluginDescription, UiDescription, describe_bundle, ui_class_of_a_window, window_of};
use crate::{DEFAULT_WINDOW_SIZE, UiError, UiEvent, UiWindow};

/// What this program calls a window of the system that these tests run on.
fn this_system() -> &'static str {
    if cfg!(windows) {
        "win32"
    } else if cfg!(target_os = "macos") {
        "cocoa"
    } else {
        "x11"
    }
}

struct Bundle {
    _folder: TempDir,
    plugins: Vec<PluginDescription>,
}

fn the_bundle() -> Bundle {
    let folder = tempfile::tempdir().unwrap();
    let bundle: PathBuf = lv2_test_plugins::write_bundle(folder.path());
    let described = describe_bundle(&bundle).expect("the bundle is described");
    assert_eq!(described.problems, Vec::<String>::new());
    Bundle {
        _folder: folder,
        plugins: described.plugins,
    }
}

impl Bundle {
    fn plugin(&self, uri: &str) -> &PluginDescription {
        self.plugins.iter().find(|plugin| plugin.uri == uri).unwrap()
    }

    fn window_of(&self, plugin: &str) -> &UiDescription {
        window_of(self.plugin(plugin), this_system()).expect("the plugin has a window here")
    }

    fn gain_port(&self) -> u32 {
        self.plugin(GAIN)
            .ports
            .iter()
            .find(|port| port.symbol == "gain")
            .unwrap()
            .index
    }

    /// The window of the gain plugin, made with no parent, and told these values.
    fn open_the_gain_window(&self, values: &[(u32, f64)]) -> UiWindow {
        UiWindow::open_without_a_window(self.plugin(GAIN), self.window_of(GAIN), values).expect("the window is made")
    }
}

#[test]
fn the_test_bundle_says_which_plugins_have_a_window_of_this_system_and_the_others_have_none() {
    let bundle = the_bundle();

    for plugin in &bundle.plugins {
        let has_one = plugin.uri == GAIN || plugin.uri == FAILS || plugin.uri == LATENCY;
        assert_eq!(window_of(plugin, this_system()).is_some(), has_one, "{}", plugin.uri);
    }
    assert_eq!(bundle.window_of(GAIN).uri, GAIN_UI);
    assert_eq!(bundle.window_of(FAILS).uri, FAILS_UI);
    assert_eq!(bundle.window_of(LATENCY).uri, QUIET_UI);
    for api in ["win32", "x11", "cocoa"] {
        assert_eq!(
            window_of(bundle.plugin(GAIN), api).is_some(),
            api == this_system(),
            "the window is of the class of {api}: {:?}",
            ui_class_of_a_window(api)
        );
    }
    let window = bundle.window_of(GAIN);
    assert!(
        window
            .extension_data
            .iter()
            .any(|uri| uri == "http://lv2plug.in/ns/extensions/ui#idleInterface")
    );
}

#[test]
fn a_window_asks_for_its_size_and_that_is_how_big_it_is_when_there_is_no_system_window_to_ask() {
    let bundle = the_bundle();

    let mut window = bundle.open_the_gain_window(&[]);

    assert_eq!(window.size(), UI_SIZE);
    assert_eq!(
        window.poll(),
        vec![UiEvent::ResizeRequested {
            width: UI_SIZE.0,
            height: UI_SIZE.1
        }],
        "the interface said it with ui:resize as it was made"
    );
    assert!(window.poll().is_empty(), "what is polled is taken");
    assert!(window.can_be_resized(), "it answers to ui:resize");
    assert!(window.set_size(500, 400), "it said that it was resized");
    assert!(
        !window.set_size(u32::MAX, 400),
        "a size that is not a size is not asked for"
    );
}

#[test]
fn the_first_time_a_window_has_its_time_it_does_what_a_hand_would_and_what_is_not_a_number_is_thrown_away() {
    let bundle = the_bundle();
    let gain = bundle.gain_port();
    let mut window = bundle.open_the_gain_window(&[]);
    window.poll();

    assert!(!window.idle(), "it does not ask to close");

    assert_eq!(
        window.poll(),
        vec![
            UiEvent::Touch {
                port: gain,
                grabbed: true
            },
            UiEvent::Write {
                port: gain,
                value: UI_EDIT_VALUE
            },
            UiEvent::Touch {
                port: gain,
                grabbed: false
            },
        ],
        "the port is the one that ui:portMap says is `gain`; a write that is not protocol 0 is not an event"
    );
    assert!(!window.idle(), "and it does it once");
    assert!(window.poll().is_empty());
}

#[test]
fn a_window_that_asks_for_nothing_is_the_default_size_cannot_be_resized_and_is_given_no_time_and_one_that_is_told_a_value_does_not_mind()
 {
    let bundle = the_bundle();
    let mut window = UiWindow::open_without_a_window(bundle.plugin(LATENCY), bundle.window_of(LATENCY), &[(2, 1.0)])
        .expect("the window is made");

    assert_eq!(window.size(), DEFAULT_WINDOW_SIZE);
    assert_eq!(DEFAULT_WINDOW_SIZE, (400, 300));
    assert!(window.poll().is_empty());
    assert!(!window.can_be_resized(), "it has no ui:resize to answer to");
    assert!(!window.set_size(500, 400));
    assert!(
        !window.idle(),
        "it has no idle interface: it is not given time, and does not ask to close"
    );
    window.tell_the_values(&[(2, 3.0), (7, 1.0)]);
    assert!(!window.idle());
}

#[test]
fn a_window_is_told_what_the_ports_are_and_the_one_that_is_told_the_value_that_closes_it_asks_to_close() {
    let bundle = the_bundle();
    let gain = bundle.gain_port();
    // (Told as it is made: the program that shows windows is told the values before there is a window.)
    let mut told_when_made = bundle.open_the_gain_window(&[(gain, f64::from(UI_CLOSE_VALUE))]);
    assert!(
        told_when_made.idle(),
        "it was told the value that closes it as it was made"
    );

    let mut told_after = bundle.open_the_gain_window(&[(gain, 1.0)]);
    assert!(!told_after.idle());
    told_after.tell_the_values(&[(gain, 2.0)]);
    assert!(!told_after.idle(), "another value");
    told_after.tell_the_values(&[(gain + 1, f64::from(UI_CLOSE_VALUE))]);
    assert!(!told_after.idle(), "the value that closes it, for another port");
    told_after.tell_the_values(&[(gain, f64::from(UI_CLOSE_VALUE))]);
    assert!(told_after.idle(), "the value that closes it, for the gain");
}

#[test]
fn an_interface_that_gives_nothing_a_library_that_is_not_one_and_a_name_that_it_does_not_have_are_errors_that_say_so() {
    let bundle = the_bundle();
    let failing = UiWindow::open_without_a_window(bundle.plugin(FAILS), bundle.window_of(FAILS), &[]);
    assert!(
        matches!(failing, Err(UiError::InstantiateFailed)),
        "{:?}",
        failing.err()
    );

    let mut other_name = bundle.window_of(GAIN).clone();
    other_name.uri = "urn:not:there".to_owned();
    let error = UiWindow::open_without_a_window(bundle.plugin(GAIN), &other_name, &[]).err();
    assert!(matches!(error, Some(UiError::NoSuchInterface(uri)) if uri == "urn:not:there"));

    let mut no_binary = bundle.window_of(GAIN).clone();
    no_binary.binary = None;
    let error = UiWindow::open_without_a_window(bundle.plugin(GAIN), &no_binary, &[]).err();
    assert!(matches!(error, Some(UiError::NoBinary)));

    let mut not_a_library = bundle.window_of(GAIN).clone();
    not_a_library.binary = Some(bundle.plugin(GAIN).bundle.join("manifest.ttl"));
    let error = UiWindow::open_without_a_window(bundle.plugin(GAIN), &not_a_library, &[]).err();
    assert!(matches!(error, Some(UiError::CouldNotBeLoaded(_))), "{error:?}");
}

#[test]
fn a_window_that_is_taken_down_takes_its_interface_down_and_one_taken_down_does_nothing_more() {
    let bundle = the_bundle();
    let gain = bundle.gain_port();
    let mut window = bundle.open_the_gain_window(&[]);
    window.destroy();
    window.destroy();

    window.tell_the_values(&[(gain, 1.0)]);

    assert!(!window.idle(), "it has no interface to give time to");
    assert!(!window.set_size(100, 100));
    assert!(!window.can_be_resized(), "it has no interface to ask");
}
