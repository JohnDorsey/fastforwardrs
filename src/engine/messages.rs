// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What is said between the window and the engine's thread, both ways.

use std::sync::Arc;
use std::time::Duration;

use super::device::AdapterChoice;
use super::{FrameDescription, FrameStatistics, PictureData, PictureId, RenderedFrame};

/// What the engine calls when it has something to tell: the window's way of being asked to draw again, so that the engine's thread does not
/// have to be waited for. (It is called from the engine's thread.)
pub type WakeUp = Arc<dyn Fn() + Send + Sync>;

/// What the window asks of the engine.
#[derive(Debug)]
pub enum EngineRequest {
    /// Here is a picture, which frames can then refer to by `id`. (If the engine has a picture under that id already, this one replaces it: the
    /// window uses the same id only for the same pixels, so it does not send a picture again.) The bytes are shared, not copied.
    Upload { id: PictureId, data: Arc<PictureData> },
    /// Let go of these pictures.
    Forget(Vec<PictureId>),
    /// Make the frame `description` describes. If more requests have come by the time the engine looks, only the newest of them is made, and
    /// the others are not answered: the window only wants the latest.
    Render { serial: u64, description: FrameDescription },
    /// Stop.
    Stop,
}

/// What the engine tells the window.
#[derive(Debug)]
pub enum EngineEvent {
    /// The frame of the request with this serial number.
    Frame {
        serial: u64,
        frame: Arc<RenderedFrame>,
        statistics: FrameStatistics,
    },
    /// The frame of the request with this serial number could not be made. The engine goes on.
    FrameFailed { serial: u64, problem: String },
    /// The frame of the request with this serial number uses pictures that the engine does not have (they were not sent, or were let go of to
    /// make room, or the graphics device was replaced). Send them, and ask again.
    PicturesMissing { serial: u64, ids: Vec<PictureId> },
    /// A picture that was sent could not be used, and is not kept.
    PictureRejected { id: PictureId, problem: String },
    /// The engine has stopped, and will make no more frames (it could not start, or could not go on, or was told to stop).
    Stopped { problem: String },
}

/// How the engine is set up.
#[derive(Clone, Debug)]
pub struct EngineSettings {
    pub adapter: AdapterChoice,
    /// How long the card gets to make a frame before the engine gives up on it.
    pub frame_timeout: Duration,
    /// How many finished frames are kept for when the same frame is asked for again.
    pub most_cached_frames: usize,
    /// How many bytes the kept frames take up at most.
    pub most_cached_frame_bytes: usize,
    /// For the tests: the thread does nothing, once it has its device, until this is opened, so that a test can have requests pile up.
    #[cfg(test)]
    pub(crate) hold_the_first_batch_until: Option<Arc<super::testing::Gate>>,
}

impl EngineSettings {
    /// The usual settings, with the device made from `adapter`.
    pub fn new(adapter: AdapterChoice) -> Self {
        Self {
            adapter,
            frame_timeout: Duration::from_secs(2),
            most_cached_frames: 32,
            most_cached_frame_bytes: 256 * 1024 * 1024,
            #[cfg(test)]
            hold_the_first_batch_until: None,
        }
    }
}
