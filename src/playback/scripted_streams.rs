// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Streams of pictures that do what a test tells them, instead of running ffmpeg: for the tests of
//! the things that are supplied pictures by streams.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use super::supplier::{PictureStream, PictureStreamStarter};
use crate::media::{DecodedFrame, FrameStreamError, FrameStreamEvent, FrameStreamRequest};

/// What the scripted streams do, shared between the test and the streams.
#[derive(Default)]
struct ScriptState {
    streams: Vec<ScriptedStreamState>,
    fail_to_start: bool,
    start_attempts: usize,
}

struct ScriptedStreamState {
    request: FrameStreamRequest,
    queue: Rc<RefCell<VecDeque<FrameStreamEvent>>>,
    delivered: Cell<u64>,
}

#[derive(Clone, Default)]
pub(super) struct Script(Rc<RefCell<ScriptState>>);

pub(super) struct ScriptedStream {
    queue: Rc<RefCell<VecDeque<FrameStreamEvent>>>,
}

impl PictureStream for ScriptedStream {
    fn try_next_event(&self) -> Option<FrameStreamEvent> {
        self.queue.borrow_mut().pop_front()
    }
}

/// Starts the scripted streams of a [`Script`]. Cloned, it starts streams of the same script.
#[derive(Clone)]
pub(super) struct ScriptedStarter(pub(super) Script);

impl PictureStreamStarter for ScriptedStarter {
    type Stream = ScriptedStream;

    fn start(&self, request: &FrameStreamRequest) -> Result<ScriptedStream, FrameStreamError> {
        let mut state = self.0.0.borrow_mut();
        state.start_attempts += 1;
        if state.fail_to_start {
            return Err(FrameStreamError::EmptyPicture);
        }
        let queue = Rc::new(RefCell::new(VecDeque::new()));
        state.streams.push(ScriptedStreamState {
            request: request.clone(),
            queue: Rc::clone(&queue),
            delivered: Cell::new(0),
        });
        Ok(ScriptedStream { queue })
    }
}

impl Script {
    pub(super) fn requests(&self) -> Vec<FrameStreamRequest> {
        self.0
            .borrow()
            .streams
            .iter()
            .map(|stream| stream.request.clone())
            .collect()
    }

    pub(super) fn started_count(&self) -> usize {
        self.0.borrow().streams.len()
    }

    /// Makes the next `count` pictures of stream number `stream_number` ready. Each picture is a
    /// 1x1 picture whose red value is its source frame number, so a test can tell which it is.
    pub(super) fn deliver(&self, stream_number: usize, count: u64) {
        let state = self.0.borrow();
        let stream = &state.streams[stream_number];
        let step = u64::from(stream.request.speed_factor.value());
        let picture_count = stream.request.source_frame_count.div_ceil(step);
        for _ in 0..count {
            let picture_number = stream.delivered.get();
            if picture_number >= picture_count {
                break;
            }
            let source_frame = stream.request.first_source_frame.value() + picture_number * step;
            stream
                .queue
                .borrow_mut()
                .push_back(FrameStreamEvent::Frame(DecodedFrame {
                    width_in_pixels: 1,
                    height_in_pixels: 1,
                    rgba_pixels: vec![source_frame as u8, 0, 0, 255],
                }));
            stream.delivered.set(picture_number + 1);
        }
    }

    pub(super) fn end(&self, stream_number: usize, error_output: Option<&str>) {
        let state = self.0.borrow();
        state.streams[stream_number]
            .queue
            .borrow_mut()
            .push_back(FrameStreamEvent::Finished {
                error_output: error_output.map(str::to_owned),
            });
    }

    pub(super) fn make_starting_fail(&self) {
        self.0.borrow_mut().fail_to_start = true;
    }

    pub(super) fn start_attempts(&self) -> usize {
        self.0.borrow().start_attempts
    }
}
