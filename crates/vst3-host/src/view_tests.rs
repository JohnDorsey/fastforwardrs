// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The window of a plugin (piece 15.6 of `docs/engine-phase-15-vst3-design.md`), against the window of the test plugins (`vst3-test-plugins`): which classes have one, putting it in a
//! parent, its size both ways, and what it does as a hand would (keys make it do those things, as the test plugin says): an edit as a gesture, a request for another size, "the
//! state changed", "the values changed".

use vst3_test_plugins as plugins;

use crate::tests::make;
use crate::{Instance, ViewError, ViewEvent};

/// A parent that is not a window (the test window only wants one that is not null).
const A_PARENT: usize = 1;

fn gain_with_a_window() -> Instance {
    let mut instance = make(plugins::GAIN);
    instance
        .open_view(A_PARENT, 1.0)
        .expect("the window is put in a parent");
    instance
}

fn width_and_height(size: (u32, u32)) -> (i32, i32) {
    (i32::try_from(size.0).unwrap(), i32::try_from(size.1).unwrap())
}

#[test]
fn a_plugin_with_a_controller_that_makes_a_window_says_so_and_one_without_does_not() {
    assert!(
        make(plugins::GAIN).has_a_view(),
        "the gain effect: a controller of its own"
    );
    assert!(
        make(plugins::SINGLE).has_a_view(),
        "a plugin that is its own controller has the window too"
    );
    assert!(!make(plugins::LATENCY).has_a_view(), "no controller at all");
    assert!(
        !make(plugins::INSTRUMENT).has_a_view(),
        "a controller that makes no window (it gives a null pointer)"
    );
}

#[test]
fn a_window_is_put_in_a_parent_and_says_how_big_it_is_and_that_it_can_be_resized() {
    let mut instance = make(plugins::GAIN);

    let size = instance.open_view(A_PARENT, 1.0).expect("put in a parent");

    assert_eq!((size.width, size.height), (300, 200));
    assert!(size.can_resize);
    assert_eq!(instance.view_size(), Some((300, 200)));
}

#[test]
fn a_window_is_not_put_in_nothing_and_not_twice_and_a_plugin_without_one_says_why() {
    let mut instance = make(plugins::GAIN);

    let no_parent = instance.open_view(0, 1.0).unwrap_err();
    assert!(matches!(no_parent, ViewError::WouldNotBeAttached(_)), "{no_parent:?}");
    assert_eq!(
        instance.view_size(),
        None,
        "a window that was not attached is not kept, and does not count as open"
    );

    instance.open_view(A_PARENT, 1.0).expect("put in a parent");
    assert!(matches!(
        instance.open_view(A_PARENT, 1.0).unwrap_err(),
        ViewError::AlreadyOpen
    ));

    assert!(matches!(
        make(plugins::LATENCY).open_view(A_PARENT, 1.0).unwrap_err(),
        ViewError::NoWindow
    ));
}

#[test]
fn a_size_is_brought_inside_what_the_window_allows_and_the_window_is_told_the_size_it_is_given() {
    let mut instance = gain_with_a_window();

    assert_eq!(
        instance.adjust_view_size(5000, 5000),
        Some(plugins::VIEW_MOST).map(|(w, h)| (w as u32, h as u32))
    );
    assert_eq!(
        instance.adjust_view_size(10, 10),
        Some((plugins::VIEW_LEAST.0 as u32, plugins::VIEW_LEAST.1 as u32))
    );
    assert_eq!(
        instance.adjust_view_size(400, 300),
        Some((400, 300)),
        "inside: as it is"
    );

    assert!(instance.set_view_size(400, 300));
    assert_eq!(instance.view_size(), Some((400, 300)), "the window took it");
}

#[test]
fn a_window_that_is_not_open_has_no_size_to_adjust_or_take_and_nothing_to_say() {
    let mut instance = make(plugins::GAIN);

    assert_eq!(instance.view_size(), None);
    assert_eq!(instance.adjust_view_size(400, 300), None);
    assert!(!instance.set_view_size(400, 300));
    assert!(!instance.press_key('e'), "no window to press a key in");
    assert!(instance.view_events().is_empty());
    instance.close_view();
}

#[test]
fn the_scale_is_told_before_the_window_is_put_in_its_parent_and_the_window_is_as_big_as_it_makes_it() {
    let mut instance = make(plugins::GAIN);

    let size = instance.open_view(A_PARENT, 2.0).expect("put in a parent");

    // (Not on a Mac: a window there is in points, and the plugin is not told the scale.)
    let expected = if cfg!(target_os = "macos") {
        (300, 200)
    } else {
        (600, 400)
    };
    assert_eq!(
        (size.width, size.height),
        expected,
        "twice as big: the test window is as big as the scale makes it until a host says another"
    );
}

#[test]
fn an_edit_in_the_window_is_a_gesture_of_plain_values_and_what_the_component_saves_has_it() {
    let mut instance = gain_with_a_window();

    assert!(instance.press_key('e'), "the window did what the key says");
    let events = instance.view_events();

    // (The normalised 0.75 of the gain is the plain 3, the way the controller says.)
    assert_eq!(
        events,
        vec![
            ViewEvent::Begin {
                id: plugins::PARAMETER_GAIN
            },
            ViewEvent::Value {
                id: plugins::PARAMETER_GAIN,
                plain: 3.0
            },
            ViewEvent::End {
                id: plugins::PARAMETER_GAIN
            },
        ]
    );
    // The component has been told, so that the state that the project keeps is the state with the edit in it.
    let saved = instance.save_state().expect("the state is saved");
    let mut elsewhere = make(plugins::GAIN);
    elsewhere.restore_state(&saved).expect("the state is taken");
    assert_eq!(elsewhere.plain_value(plugins::PARAMETER_GAIN), Some(3.0));
    assert!(instance.view_events().is_empty(), "said once");
}

#[test]
fn a_plugin_that_is_its_own_controller_makes_its_edits_the_same_way() {
    let mut instance = make(plugins::SINGLE);
    instance.open_view(A_PARENT, 1.0).expect("put in a parent");

    instance.press_key('e');

    assert_eq!(
        instance.view_events(),
        vec![
            ViewEvent::Begin {
                id: plugins::PARAMETER_GAIN
            },
            ViewEvent::Value {
                id: plugins::PARAMETER_GAIN,
                plain: 3.0
            },
            ViewEvent::End {
                id: plugins::PARAMETER_GAIN
            },
        ]
    );
}

#[test]
fn a_request_for_another_size_is_an_event_and_the_window_is_told_the_size_that_the_host_gave() {
    let mut instance = gain_with_a_window();

    instance.press_key('r');
    let events = instance.view_events();

    let (width, height) = plugins::VIEW_ASKED;
    assert_eq!(
        events,
        vec![ViewEvent::ResizeRequested {
            width: width as u32,
            height: height as u32
        }]
    );
    assert_eq!(
        instance.view_size().map(width_and_height),
        Some(plugins::VIEW_ASKED),
        "the window was told its new size (VST3 wants the host to say it)"
    );
}

#[test]
fn a_request_for_a_size_the_window_does_not_allow_is_answered_with_the_size_it_does() {
    let mut instance = gain_with_a_window();

    instance.press_key('b');
    let events = instance.view_events();

    let (width, height) = plugins::VIEW_MOST;
    assert_eq!(
        events,
        vec![ViewEvent::ResizeRequested {
            width: width as u32,
            height: height as u32
        }],
        "the most that the window allows, and not the 2000 by 1500 it asked for"
    );
    assert_eq!(instance.view_size().map(width_and_height), Some(plugins::VIEW_MOST));
}

#[test]
fn a_change_of_the_state_that_the_plugin_makes_in_itself_and_says_to_no_one_is_in_the_state_it_saves() {
    // (The key `s` is what a window built with DPF does when a hand draws in it: it tells its other half in a message, and the host is told nothing.)
    let mut instance = gain_with_a_window();
    let before = instance.save_state().expect("the state is saved");

    assert!(instance.press_key('s'), "the message went to the other half");

    assert!(instance.view_events().is_empty(), "nothing was said to the host");
    let after = instance.save_state().expect("the state is saved");
    assert_ne!(before, after, "the state has what the window did");
    let mut elsewhere = make(plugins::GAIN);
    elsewhere.restore_state(&after).expect("the state is taken");
    assert_eq!(
        elsewhere.plain_value(plugins::PARAMETER_SECRET),
        Some(0.25),
        "the hidden value that the message changed"
    );
}

#[test]
fn a_plugin_that_says_it_is_dirty_has_changed_its_state() {
    let mut instance = gain_with_a_window();

    instance.press_key('d');

    assert_eq!(instance.view_events(), vec![ViewEvent::StateChanged]);
    assert!(instance.view_events().is_empty());
}

#[test]
fn values_that_the_plugin_changed_by_itself_are_told_as_the_ones_that_differ_and_the_state_changed() {
    let mut instance = gain_with_a_window();

    instance.press_key('v');
    let events = instance.view_events();

    // (The normalised 0.5 is the plain 2; the gain was the plain 1 when the window was put in its parent.)
    assert_eq!(
        events,
        vec![
            ViewEvent::ValuesChanged(vec![(plugins::PARAMETER_GAIN, 2.0)]),
            ViewEvent::StateChanged,
        ]
    );

    // The same again changes no value: the state is what was said to have changed, and nothing else.
    instance.press_key('v');
    assert_eq!(instance.view_events(), vec![ViewEvent::StateChanged]);
}

#[test]
fn several_values_that_the_plugin_changed_at_once_are_told_in_the_order_of_its_list_of_parameters_every_time() {
    for _ in 0..8 {
        let mut instance = gain_with_a_window();

        instance.press_key('w');

        assert_eq!(
            instance.view_events(),
            vec![
                ViewEvent::ValuesChanged(vec![
                    (plugins::PARAMETER_GAIN, 2.0),
                    (plugins::PARAMETER_MODE, 2.0),
                    (plugins::PARAMETER_BYPASS, 1.0),
                ]),
                ViewEvent::StateChanged,
            ]
        );
    }
}

#[test]
fn a_value_that_the_host_gave_is_known_so_that_it_is_not_told_back_as_a_change() {
    let mut instance = gain_with_a_window();
    instance.set_value(plugins::PARAMETER_GAIN, 2.0, 0);

    instance.press_key('v');

    // (The plugin sets 2 and says it changed; the host has just made it 2: no value differs.)
    assert_eq!(instance.view_events(), vec![ViewEvent::StateChanged]);
}

#[test]
fn a_key_the_window_does_not_know_is_not_taken() {
    let mut instance = gain_with_a_window();

    assert!(!instance.press_key('x'));
    assert!(instance.view_events().is_empty());
}

#[test]
fn a_closed_window_is_gone_and_another_can_be_put_in_a_parent() {
    let mut instance = gain_with_a_window();
    instance.press_key('r');
    instance.view_events();

    instance.close_view();

    assert_eq!(instance.view_size(), None);
    assert!(!instance.press_key('e'));
    let size = instance.open_view(A_PARENT, 1.0).expect("put in a parent again");
    assert_eq!(
        (size.width, size.height),
        (300, 200),
        "a window made again is a new one, as big as a window is made"
    );
}

#[test]
fn an_instance_that_is_let_go_of_with_its_window_open_ends_the_window_first() {
    // (The test window ends the program if it is let go of while it is in its parent, as real ones crash: that is what this holds. A host that drops its pointers in the order they are
    // declared and forgets the window would be heard here.)
    for _ in 0..5 {
        let instance = gain_with_a_window();
        drop(instance);
    }
}

// --- the id of a window of the program ---

mod handles {
    #![allow(unsafe_code)]

    use std::num::{NonZeroIsize, NonZeroU32};
    use std::ptr::NonNull;

    use raw_window_handle::{
        AppKitWindowHandle, HandleError, HasWindowHandle, RawWindowHandle, WaylandWindowHandle, Win32WindowHandle,
        WindowHandle, XcbWindowHandle, XlibWindowHandle,
    };

    use crate::view::handle_of_a_window;

    struct Window(RawWindowHandle);

    impl HasWindowHandle for Window {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            // SAFETY: the handles of these tests are numbers that no one uses as windows: they are only read.
            Ok(unsafe { WindowHandle::borrow_raw(self.0) })
        }
    }

    #[test]
    fn a_window_of_each_system_gives_the_id_that_vst3_wants_and_one_it_does_not_know_gives_none() {
        let hwnd = Window(RawWindowHandle::Win32(Win32WindowHandle::new(
            NonZeroIsize::new(0x1234).unwrap(),
        )));
        assert_eq!(handle_of_a_window(&hwnd), Some(0x1234));

        let mut x11 = XlibWindowHandle::new(0x00AB_CDEF);
        x11.window = 0x00AB_CDEF;
        assert_eq!(
            handle_of_a_window(&Window(RawWindowHandle::Xlib(x11))),
            Some(0x00AB_CDEF)
        );

        let xcb = XcbWindowHandle::new(NonZeroU32::new(77).unwrap());
        assert_eq!(handle_of_a_window(&Window(RawWindowHandle::Xcb(xcb))), Some(77));

        let view = NonNull::new(0x4000 as *mut std::ffi::c_void).unwrap();
        assert_eq!(
            handle_of_a_window(&Window(RawWindowHandle::AppKit(AppKitWindowHandle::new(view)))),
            Some(0x4000)
        );

        // (A Wayland surface is not a window that VST3 can put a window in.)
        let wayland = WaylandWindowHandle::new(NonNull::new(0x10 as *mut std::ffi::c_void).unwrap());
        assert_eq!(handle_of_a_window(&Window(RawWindowHandle::Wayland(wayland))), None);
    }
}
