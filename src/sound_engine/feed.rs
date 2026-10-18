// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sound of the clips as it comes to the graph: **feeds**.

use crate::media::export::SoundClipToExport;

/// The sound of one clip, from its first sample on, as it is after it has been cut out of its file and sped up (what [`clip_feed_filters`](crate::media::export::clip_feed_filters)
/// makes), before the clip's volume. It may be shorter than the clip's place on the timeline (the file has less sound than that), or longer (the graph cuts it off).
pub trait ClipFeed: Send {
    /// Puts the next samples in `left` and `right` (which are as long as each other) and says how many: fewer than their length only when the sound has ended.
    fn read(&mut self, left: &mut [f32], right: &mut [f32]) -> usize;
}

/// Which clip of the specification a feed is asked for.
#[derive(Clone, Copy, Debug)]
pub struct ClipRequest<'specification> {
    /// Which strip it is in (the strips are numbered from 0, in the order of the specification).
    pub strip: usize,
    /// Which item of the strip it is.
    pub item: usize,
    pub clip: &'specification SoundClipToExport,
}

/// What gives the graph the feed of a clip, when the clip begins to be heard. (Who is asked decides how far ahead the sound is got ready: the graph asks at the last moment.)
pub trait ClipFeeds {
    fn open(&mut self, request: &ClipRequest<'_>) -> Box<dyn ClipFeed>;
}

/// Sound that is all in memory.
pub struct SoundInMemory {
    left: Vec<f32>,
    right: Vec<f32>,
    next: usize,
}

impl SoundInMemory {
    pub fn new(left: Vec<f32>, right: Vec<f32>) -> Self {
        assert_eq!(left.len(), right.len(), "the two channels are as long as each other");
        Self { left, right, next: 0 }
    }

    /// Sound that is given as it is read from a pipe: left, right, left, right...
    pub fn from_interleaved(samples: &[f32]) -> Self {
        let (left, right) = samples
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[left, right]| (*left, *right))
            .unzip();
        Self::new(left, right)
    }

    /// How many samples there are in all.
    pub fn len(&self) -> usize {
        self.left.len()
    }

    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
    }
}

impl ClipFeed for SoundInMemory {
    fn read(&mut self, left: &mut [f32], right: &mut [f32]) -> usize {
        let count = left.len().min(right.len()).min(self.left.len() - self.next);
        left[..count].copy_from_slice(&self.left[self.next..self.next + count]);
        right[..count].copy_from_slice(&self.right[self.next..self.next + count]);
        self.next += count;
        count
    }
}
