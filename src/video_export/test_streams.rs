// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Streams of the pictures of the runs that are made up, for the tests of what is done with the pictures: a [`Script`] says what each stream that is started says,
//! and remembers what it was asked for. (No ffmpeg: the tests with the real one are in `tests/video_export_*.rs`.)

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use super::pictures::{RunStream, RunStreamStarter};
use crate::media::{
    DecodedFrame, DecodedYuvFrame, FrameStreamError, FrameStreamEvent, FrameStreamRequest, MediaLocation, WaitedFor,
};
use crate::playback::{SuppliedFrame, SuppliedPicture};
use crate::project::SourceMediaIdentifier;

/// What the made-up streams were asked and what they are to say, shared between a test and the starter it gave.
#[derive(Default)]
pub(crate) struct ScriptState {
    /// What each stream that was started was asked for, in the order they were started.
    pub(crate) started: Vec<(FrameStreamRequest, (u32, u32))>,
    /// What the next streams that are started say, one list for each, and then that their end is reached.
    pub(crate) to_say: VecDeque<Vec<FrameStreamEvent>>,
    /// Whether a stream that has said all it has to say waits for ever (and does not end).
    pub(crate) never_ends: bool,
    /// How many streams that were started have not been let go of.
    pub(crate) alive: usize,
    pub(crate) fail_to_start: bool,
}

/// A starter of made-up streams. (Cloning gives another handle to the same script.)
#[derive(Clone, Default)]
pub(crate) struct Script(Arc<Mutex<ScriptState>>);

impl Script {
    /// What the script says and has been asked, for a test to read or change.
    pub(crate) fn state(&self) -> MutexGuard<'_, ScriptState> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub(crate) struct MadeUpStream {
    events: VecDeque<FrameStreamEvent>,
    script: Script,
    never_ends: bool,
}

impl RunStream for MadeUpStream {
    fn wait_for_an_event(&mut self, how_long: Duration) -> WaitedFor<FrameStreamEvent> {
        match self.events.pop_front() {
            Some(event) => WaitedFor::Event(event),
            None if self.never_ends => {
                // A little of the wait, so that a test that waits for the limit does not use a processor for it.
                std::thread::sleep(how_long.min(Duration::from_millis(2)));
                WaitedFor::NothingYet
            }
            None => WaitedFor::Ended,
        }
    }
}

impl Drop for MadeUpStream {
    fn drop(&mut self) {
        self.script.state().alive -= 1;
    }
}

impl RunStreamStarter for Script {
    type Stream = MadeUpStream;

    fn start(
        &self,
        request: &FrameStreamRequest,
        delivered_size: (u32, u32),
    ) -> Result<MadeUpStream, FrameStreamError> {
        let mut state = self.state();
        if state.fail_to_start {
            return Err(FrameStreamError::EmptyPicture);
        }
        state.started.push((request.clone(), delivered_size));
        state.alive += 1;
        let events = state.to_say.pop_front().unwrap_or_default();
        let never_ends = state.never_ends;
        Ok(MadeUpStream {
            events: events.into(),
            script: self.clone(),
            never_ends,
        })
    }
}

/// A picture of 2 by 2 pixels that is all `value`.
pub(crate) fn rgba(value: u8) -> FrameStreamEvent {
    FrameStreamEvent::Frame(DecodedFrame {
        width_in_pixels: 2,
        height_in_pixels: 2,
        rgba_pixels: vec![value; 16],
    })
}

/// A picture of 2 by 2 pixels in planes, the brightness of which is `value` and which has no colour.
pub(crate) fn planes(value: u8) -> FrameStreamEvent {
    FrameStreamEvent::YuvFrame(DecodedYuvFrame {
        width_in_pixels: 2,
        height_in_pixels: 2,
        y: vec![value; 4],
        u: vec![128],
        v: vec![128],
    })
}

/// The first byte of the picture: the `value` of [`rgba`] or [`planes`] that it was made from.
pub(crate) fn the_first_byte_of(picture: &SuppliedPicture) -> u8 {
    match &picture.frame {
        SuppliedFrame::Rgba(frame) => frame.rgba_pixels[0],
        SuppliedFrame::Yuv420p { frame, .. } => frame.y[0],
    }
}

/// Where the video files are, for the test projects (the file `holiday.mp4` is source 1, `birthday.mp4` is source 2).
pub(crate) fn locations() -> HashMap<SourceMediaIdentifier, MediaLocation> {
    HashMap::from([
        (SourceMediaIdentifier::new(1), MediaLocation::from("holiday.mp4")),
        (SourceMediaIdentifier::new(2), MediaLocation::from("birthday.mp4")),
    ])
}
