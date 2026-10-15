// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Frame positions in the two different "time lines" the editor deals with.
//!
//! The source video has its own frames; the edited timeline has frames of its own, and once a
//! clip is sped up the two stop lining up one-to-one. Giving each its own type means the compiler
//! stops us from ever mixing them up.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The position of a frame in a source video: 0 is the first frame of the video.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SourceFrameIndex(u64);

impl SourceFrameIndex {
    pub const fn new(frame_number: u64) -> Self {
        Self(frame_number)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

impl fmt::Display for SourceFrameIndex {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "source frame {}", self.0)
    }
}

/// The position of a frame on the edited timeline (the finished video): 0 is its first frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TimelineFrameIndex(u64);

impl TimelineFrameIndex {
    pub const fn new(frame_number: u64) -> Self {
        Self(frame_number)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

impl fmt::Display for TimelineFrameIndex {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "timeline frame {}", self.0)
    }
}
