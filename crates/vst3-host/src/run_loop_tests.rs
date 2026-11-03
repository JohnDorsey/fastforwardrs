// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The run loop that the host gives a plugin's window (`IRunLoop`, which a plugin asks the frame for). **On Linux a plugin's window has no loop of its own**: it asks for its timers to
//! be called and for the descriptors of its connection to the display to be looked at, and the host does that when it looks (`Instance::view_events`). The timers are the same on every
//! system and are tried everywhere; the descriptors are Linux's.

#![allow(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use vst3::Steinberg::Linux::*;
use vst3::Steinberg::*;
use vst3::{Class, ComPtr, ComWrapper};

use crate::objects::FrameObject;

/// A timer handler that counts its calls.
struct Counting {
    calls: Rc<Cell<u32>>,
}

impl Class for Counting {
    type Interfaces = (ITimerHandler,);
}

impl ITimerHandlerTrait for Counting {
    unsafe fn onTimer(&self) {
        self.calls.set(self.calls.get() + 1);
    }
}

/// A timer handler that takes itself out of the run loop when it is called (a plugin may unregister in the call).
struct TakesItselfOut {
    run_loop: RefCell<Option<ComPtr<IRunLoop>>>,
    me: Cell<*mut ITimerHandler>,
    calls: Rc<Cell<u32>>,
}

impl Class for TakesItselfOut {
    type Interfaces = (ITimerHandler,);
}

impl ITimerHandlerTrait for TakesItselfOut {
    unsafe fn onTimer(&self) {
        self.calls.set(self.calls.get() + 1);
        if let Some(run_loop) = self.run_loop.borrow_mut().take() {
            // SAFETY: the run loop is live; the handler is the one that was registered.
            unsafe { run_loop.unregisterTimer(self.me.get()) };
        }
    }
}

fn the_run_loop() -> (ComWrapper<FrameObject>, ComPtr<IRunLoop>) {
    let frame = FrameObject::new();
    let run_loop = frame.to_com_ptr::<IRunLoop>().expect("the frame is also a run loop");
    (frame, run_loop)
}

fn a_counting_timer() -> (Rc<Cell<u32>>, ComPtr<ITimerHandler>, ComWrapper<Counting>) {
    let calls = Rc::new(Cell::new(0));
    let wrapper = ComWrapper::new(Counting {
        calls: Rc::clone(&calls),
    });
    let pointer = wrapper.to_com_ptr::<ITimerHandler>().expect("a timer handler");
    (calls, pointer, wrapper)
}

#[test]
fn a_timer_is_called_when_it_is_due_once_for_each_time_and_not_before() {
    let (frame, run_loop) = the_run_loop();
    let (calls, handler, _keeps) = a_counting_timer();
    let before = Instant::now();
    // SAFETY: the run loop and the handler are live.
    assert_eq!(unsafe { run_loop.registerTimer(handler.as_ptr(), 10) }, kResultOk);

    frame.run_the_loop_at(before + Duration::from_millis(2));
    assert_eq!(calls.get(), 0, "not due yet");

    let later = before + Duration::from_secs(1);
    frame.run_the_loop_at(later);
    assert_eq!(calls.get(), 1, "due");
    frame.run_the_loop_at(later + Duration::from_millis(2));
    assert_eq!(
        calls.get(),
        1,
        "once for each time: the next is an interval after this one"
    );
    frame.run_the_loop_at(later + Duration::from_millis(11));
    assert_eq!(calls.get(), 2);
}

#[test]
fn timers_of_different_intervals_are_each_called_when_they_are_due() {
    let (frame, run_loop) = the_run_loop();
    let (quick, quick_handler, _a) = a_counting_timer();
    let (slow, slow_handler, _b) = a_counting_timer();
    let before = Instant::now();
    // SAFETY: the run loop and the handlers are live.
    unsafe {
        run_loop.registerTimer(quick_handler.as_ptr(), 10);
        run_loop.registerTimer(slow_handler.as_ptr(), 500);
    }

    frame.run_the_loop_at(before + Duration::from_millis(100));

    assert_eq!(
        (quick.get(), slow.get()),
        (1, 0),
        "the quick one is due, the slow one is not"
    );
    frame.run_the_loop_at(before + Duration::from_millis(700));
    assert_eq!((quick.get(), slow.get()), (2, 1));
}

#[test]
fn a_timer_that_was_taken_out_is_not_called_and_a_handler_that_is_not_a_handler_or_an_interval_of_none_is_refused() {
    let (frame, run_loop) = the_run_loop();
    let (calls, handler, _keeps) = a_counting_timer();
    let before = Instant::now();
    // SAFETY: the run loop and the handler are live; a null pointer is what a plugin that has nothing to give would pass.
    unsafe {
        assert_eq!(run_loop.registerTimer(handler.as_ptr(), 10), kResultOk);
        assert_eq!(run_loop.unregisterTimer(handler.as_ptr()), kResultOk);
        assert_ne!(run_loop.registerTimer(std::ptr::null_mut(), 10), kResultOk);
        assert_ne!(
            run_loop.registerTimer(handler.as_ptr(), 0),
            kResultOk,
            "a timer that is called every 0 ms is not one"
        );
    }

    frame.run_the_loop_at(before + Duration::from_secs(5));

    assert_eq!(calls.get(), 0);
}

#[test]
fn a_timer_may_take_itself_out_of_the_loop_in_its_call() {
    let (frame, run_loop) = the_run_loop();
    let calls = Rc::new(Cell::new(0));
    let wrapper = ComWrapper::new(TakesItselfOut {
        run_loop: RefCell::new(Some(run_loop.clone())),
        me: Cell::new(std::ptr::null_mut()),
        calls: Rc::clone(&calls),
    });
    let handler = wrapper.to_com_ptr::<ITimerHandler>().expect("a timer handler");
    wrapper.me.set(handler.as_ptr());
    let before = Instant::now();
    // SAFETY: the run loop and the handler are live.
    unsafe { run_loop.registerTimer(handler.as_ptr(), 10) };

    frame.run_the_loop_at(before + Duration::from_secs(1));
    frame.run_the_loop_at(before + Duration::from_secs(2));
    frame.run_the_loop_at(before + Duration::from_secs(3));

    assert_eq!(calls.get(), 1, "called once, and gone from the loop after that");
}

#[cfg(target_os = "linux")]
mod descriptors {
    use super::*;

    /// An event handler that counts its calls, and reads what it was called for (a byte of the pipe).
    struct Reading {
        calls: Rc<Cell<u32>>,
        descriptor: i32,
    }

    impl Class for Reading {
        type Interfaces = (IEventHandler,);
    }

    impl IEventHandlerTrait for Reading {
        unsafe fn onFDIsSet(&self, descriptor: FileDescriptor) {
            assert_eq!(
                descriptor, self.descriptor,
                "called for the descriptor it was registered for"
            );
            let mut byte = 0_u8;
            // SAFETY: the descriptor is the read end of a pipe that is open; the byte is a place for one byte.
            if unsafe { libc::read(descriptor, (&mut byte as *mut u8).cast(), 1) } == 1 {
                self.calls.set(self.calls.get() + 1);
            }
        }
    }

    fn a_pipe() -> [i32; 2] {
        let mut ends = [0_i32; 2];
        // SAFETY: `ends` is a place for two descriptors.
        assert_eq!(unsafe { libc::pipe(ends.as_mut_ptr()) }, 0);
        ends
    }

    fn put_a_byte_in(pipe: [i32; 2]) {
        // SAFETY: the write end is open; the byte lives through the call.
        assert_eq!(unsafe { libc::write(pipe[1], [7_u8].as_ptr().cast(), 1) }, 1);
    }

    #[test]
    fn a_descriptor_is_looked_at_and_its_handler_called_when_there_is_something_to_read() {
        let (frame, run_loop) = the_run_loop();
        let pipe = a_pipe();
        let calls = Rc::new(Cell::new(0));
        let wrapper = ComWrapper::new(Reading {
            calls: Rc::clone(&calls),
            descriptor: pipe[0],
        });
        let handler = wrapper.to_com_ptr::<IEventHandler>().expect("an event handler");
        // SAFETY: the run loop and the handler are live; the descriptor is open.
        let registered = unsafe { run_loop.registerEventHandler(handler.as_ptr(), pipe[0]) };
        assert_eq!(registered, kResultOk);

        frame.run_the_loop_at(Instant::now());
        assert_eq!(calls.get(), 0, "nothing to read: not called");

        put_a_byte_in(pipe);
        frame.run_the_loop_at(Instant::now());
        assert_eq!(calls.get(), 1);
        frame.run_the_loop_at(Instant::now());
        assert_eq!(calls.get(), 1, "what was read is gone: not called again");

        // SAFETY: the run loop and the handler are live; the pipe's ends are open.
        unsafe {
            run_loop.unregisterEventHandler(handler.as_ptr());
            put_a_byte_in(pipe);
            frame.run_the_loop_at(Instant::now());
            assert_eq!(calls.get(), 1, "taken out: not called for what there is to read");
            libc::close(pipe[0]);
            libc::close(pipe[1]);
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn a_window_that_uses_the_run_loop_is_called_for_its_timer_and_its_descriptor_when_the_host_looks() {
    use crate::ViewEvent;
    use crate::tests::make;
    use vst3_test_plugins as plugins;

    // (The test window, on Linux, registers when it is told to (the key `l`) a timer that makes the edit of the key `e` at its third call, and a pipe with a byte in it that makes the request for
    // another size of the key `r`.)
    let mut instance = make(plugins::GAIN);
    instance.open_view(1, 1.0).expect("put in a parent");
    assert!(
        instance.press_key('l'),
        "the window asked the run loop for a timer and a descriptor"
    );
    let mut events = Vec::new();
    let began = Instant::now();

    while began.elapsed() < Duration::from_secs(5)
        && !(events.iter().any(|event| matches!(event, ViewEvent::End { .. }))
            && events
                .iter()
                .any(|event| matches!(event, ViewEvent::ResizeRequested { .. })))
    {
        events.extend(instance.view_events());
        std::thread::sleep(Duration::from_millis(5));
    }

    assert!(
        events.contains(&ViewEvent::Value {
            id: plugins::PARAMETER_GAIN,
            plain: 3.0
        }),
        "the timer was called: {events:?}"
    );
    let (width, height) = plugins::VIEW_ASKED;
    assert!(
        events.contains(&ViewEvent::ResizeRequested {
            width: width as u32,
            height: height as u32
        }),
        "the descriptor was looked at: {events:?}"
    );
    instance.close_view();
    assert!(
        instance.view_events().is_empty(),
        "nothing is called for after the window was taken out"
    );
}
