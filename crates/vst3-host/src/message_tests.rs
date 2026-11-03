// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The handler and the messages that a plugin makes with the host (`IHostApplication::createInstance` of an `IMessage` or an `IAttributeList`), as a plugin does that sends what happens in its window
//! to the other half of itself: **a plugin built with DPF (Dragonfly Reverb, MVerb, Wolf Shaper and the others of its kind) sends every edit of its window that way, and without them
//! its window changes nothing** (found by moving a knob of Dragonfly Hall in a real window).

#![allow(unsafe_code)]

use std::ffi::{CStr, c_void};

use vst3::ComPtr;
use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;

use crate::objects::HostObject;
use crate::tests::make;
use vst3_test_plugins as plugins;

/// A text in the units that VST3 uses for text (16 bits), with the zero that ends it.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn a_list() -> ComPtr<IAttributeList> {
    let host = HostObject::new();
    let mut class = IAttributeList_iid;
    let mut interface = IAttributeList_iid;
    let mut object: *mut c_void = std::ptr::null_mut();
    // SAFETY: the host object is live; the ids and the place for the answer live through the call.
    let result = unsafe { host.createInstance(&mut class, &mut interface, &mut object) };
    assert_eq!(result, kResultOk);
    // SAFETY: the call gave a pointer to an object that is ours to let go of.
    unsafe { ComPtr::from_raw(object.cast::<IAttributeList>()) }.expect("an attribute list was made")
}

fn a_message() -> ComPtr<IMessage> {
    let host = HostObject::new();
    let mut class = IMessage_iid;
    let mut interface = IMessage_iid;
    let mut object: *mut c_void = std::ptr::null_mut();
    // SAFETY: the host object is live; the ids and the place for the answer live through the call.
    let result = unsafe { host.createInstance(&mut class, &mut interface, &mut object) };
    assert_eq!(result, kResultOk);
    // SAFETY: the call gave a pointer to an object that is ours to let go of.
    unsafe { ComPtr::from_raw(object.cast::<IMessage>()) }.expect("a message was made")
}

#[test]
fn a_number_a_float_a_text_and_some_bytes_come_back_as_they_were_given() {
    let list = a_list();
    let bytes = [1_u8, 2, 3, 250];

    // SAFETY: the list is live; the names end in a zero, the text ends in one, and the bytes live through the call.
    unsafe {
        assert_eq!(list.setInt(c"rindex".as_ptr(), -7), kResultOk);
        assert_eq!(list.setFloat(c"value".as_ptr(), 0.625), kResultOk);
        assert_eq!(list.setString(c"note".as_ptr(), wide("h\u{e9}llo").as_ptr()), kResultOk);
        assert_eq!(
            list.setBinary(c"blob".as_ptr(), bytes.as_ptr().cast(), bytes.len() as u32),
            kResultOk
        );
    }

    let mut number = 0_i64;
    let mut float = 0.0_f64;
    let mut text = [0_u16; 16];
    let mut data: *const c_void = std::ptr::null();
    let mut size = 0_u32;
    // SAFETY: the list is live; the places for the answers live through the calls.
    unsafe {
        assert_eq!(list.getInt(c"rindex".as_ptr(), &mut number), kResultOk);
        assert_eq!(list.getFloat(c"value".as_ptr(), &mut float), kResultOk);
        assert_eq!(list.getString(c"note".as_ptr(), text.as_mut_ptr(), 32), kResultOk);
        assert_eq!(list.getBinary(c"blob".as_ptr(), &mut data, &mut size), kResultOk);
    }
    assert_eq!(number, -7);
    assert_eq!(float, 0.625);
    assert_eq!(&text[..6], wide("h\u{e9}llo").as_slice(), "the text and its zero");
    assert_eq!(size, 4);
    // SAFETY: the list gave a pointer to as many bytes as it said.
    assert_eq!(unsafe { std::slice::from_raw_parts(data.cast::<u8>(), 4) }, &bytes);
}

#[test]
fn a_name_that_was_not_given_or_was_given_as_another_kind_is_not_found_and_a_name_given_again_is_replaced() {
    let list = a_list();
    let mut number = 0_i64;
    let mut float = 0.0_f64;

    // SAFETY: the list is live; the names end in a zero and the places for the answers live through the calls.
    unsafe {
        assert_ne!(list.getInt(c"nothing".as_ptr(), &mut number), kResultOk);
        list.setFloat(c"value".as_ptr(), 1.5);
        assert_ne!(
            list.getInt(c"value".as_ptr(), &mut number),
            kResultOk,
            "a float is not a number"
        );
        list.setInt(c"value".as_ptr(), 9);
        assert_eq!(list.getInt(c"value".as_ptr(), &mut number), kResultOk);
        assert_ne!(
            list.getFloat(c"value".as_ptr(), &mut float),
            kResultOk,
            "given again as a number, it is no longer a float"
        );
        assert_eq!(number, 9);
        assert_ne!(list.setInt(std::ptr::null(), 1), kResultOk, "no name");
    }
}

#[test]
fn a_text_that_does_not_fit_is_cut_short_with_its_zero_and_there_is_no_room_for_none() {
    let list = a_list();
    // SAFETY: the list is live; the name and the text end in a zero.
    unsafe { list.setString(c"note".as_ptr(), wide("abcdef").as_ptr()) };
    let mut text = [0x7777_u16; 8];

    // SAFETY: the list is live; the place for the answer is as big as the size says.
    let result = unsafe { list.getString(c"note".as_ptr(), text.as_mut_ptr(), 8) };

    assert_eq!(result, kResultOk);
    assert_eq!(
        &text[..4],
        wide("abc").as_slice(),
        "four units of room: three letters and the zero"
    );
    assert_eq!(text[4], 0x7777, "nothing is written past the room that was given");
    // SAFETY: as above, with room for none.
    let none = unsafe { list.getString(c"note".as_ptr(), text.as_mut_ptr(), 0) };
    assert_ne!(none, kResultOk);
}

#[test]
fn a_message_has_an_id_that_is_kept_and_one_list_of_attributes_that_is_the_same_each_time() {
    let message = a_message();

    // SAFETY: the message is live; the id ends in a zero; the pointers the message gives are good while it is.
    unsafe {
        message.setMessageID(c"parameter-set".as_ptr());
        assert_eq!(CStr::from_ptr(message.getMessageID()).to_bytes(), b"parameter-set");
        message.setMessageID(c"ready".as_ptr());
        assert_eq!(
            CStr::from_ptr(message.getMessageID()).to_bytes(),
            b"ready",
            "it can be changed"
        );

        let first = message.getAttributes();
        let second = message.getAttributes();
        assert!(!first.is_null());
        assert_eq!(first, second);
    }
    // (What is put in the attributes is what is read from the attributes of the same message.)
    // SAFETY: as above.
    unsafe {
        let attributes = message.getAttributes();
        let list = vst3::ComRef::from_raw(attributes).expect("a list");
        list.setInt(c"rindex".as_ptr(), 5);
        let mut number = 0_i64;
        let again = vst3::ComRef::from_raw(message.getAttributes()).expect("a list");
        assert_eq!(again.getInt(c"rindex".as_ptr(), &mut number), kResultOk);
        assert_eq!(number, 5);
    }
}

#[test]
fn a_message_that_has_no_id_yet_says_an_empty_one_and_the_host_makes_nothing_else() {
    let message = a_message();
    // SAFETY: the message is live; the pointer it gives is good while it is.
    assert_eq!(unsafe { CStr::from_ptr(message.getMessageID()) }.to_bytes(), b"");

    let host = HostObject::new();
    let mut other = IPlugView_iid;
    let mut interface = IPlugView_iid;
    let mut object: *mut c_void = std::ptr::null_mut();
    // SAFETY: the host object is live; the ids and the place for the answer live through the call.
    let result = unsafe { host.createInstance(&mut other, &mut interface, &mut object) };
    assert_ne!(result, kResultOk);
    assert!(object.is_null(), "nothing was made");
}

#[test]
fn a_plugin_that_makes_a_message_fills_it_and_reads_it_back_through_the_host_of_its_own_instance() {
    // (The test window does what a plugin built with DPF does: makes a message of the host it was given, puts a number, a float, a text and some bytes in it, and reads them back: only
    // if they all come back as they were put does it make the edit that the message is about. The key is `m`.)
    let mut instance = make(plugins::GAIN);
    instance.open_view(1, 1.0).expect("put in a parent");

    assert!(instance.press_key('m'), "the key is one of the window's");

    assert_eq!(
        instance.view_events(),
        vec![
            crate::ViewEvent::Begin {
                id: plugins::PARAMETER_GAIN
            },
            crate::ViewEvent::Value {
                id: plugins::PARAMETER_GAIN,
                plain: 3.0
            },
            crate::ViewEvent::End {
                id: plugins::PARAMETER_GAIN
            },
        ],
        "the message went through the host whole"
    );
}

#[test]
fn a_plugin_that_says_it_is_clean_says_nothing_new_and_one_that_says_it_is_dirty_is_heard_once() {
    // (The handler directly: the plugin of the tests only says "dirty".)
    let host = HostObject::new();
    host.record_the_gestures(true);

    // SAFETY: the host object is live; the calls have numbers only.
    unsafe {
        host.setDirty(0);
        assert!(!host.take_the_gestures().1, "clean is not a change");
        host.setDirty(1);
        assert!(host.take_the_gestures().1);
        assert!(!host.take_the_gestures().1, "said once");
    }
    host.record_the_gestures(false);
    // SAFETY: as above.
    unsafe { host.setDirty(1) };
    assert!(!host.take_the_gestures().1, "nothing is kept while no window is open");
}
