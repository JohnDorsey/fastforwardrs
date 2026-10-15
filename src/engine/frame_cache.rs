// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The finished frames that were made lately (EN-09), by the fingerprint of their description.
//!
//! The fingerprint is made of everything that decides what a frame looks like (see [`super::FrameDescription::fingerprint`]), so a frame that an edit
//! changes is looked for under another key and the one that was kept is simply never asked for again, and is let go in its turn: there is
//! nothing to invalidate. Going back and forth over the same few frames, which is what scrubbing is, costs nothing after the first time.

use std::collections::HashMap;
use std::sync::Arc;

use super::RenderedFrame;

/// A cache that keeps at most so many frames and so many bytes, letting go of the one that was used least recently first.
#[derive(Debug)]
pub struct FrameCache {
    frames: HashMap<u64, Entry>,
    most_frames: usize,
    most_bytes: usize,
    bytes: usize,
    use_counter: u64,
}

#[derive(Debug)]
struct Entry {
    frame: Arc<RenderedFrame>,
    last_used: u64,
}

impl FrameCache {
    pub fn new(most_frames: usize, most_bytes: usize) -> Self {
        Self {
            frames: HashMap::new(),
            most_frames,
            most_bytes,
            bytes: 0,
            use_counter: 0,
        }
    }

    /// The frame kept under `fingerprint`, which then counts as used just now.
    pub fn get(&mut self, fingerprint: u64) -> Option<Arc<RenderedFrame>> {
        self.use_counter += 1;
        let entry = self.frames.get_mut(&fingerprint)?;
        entry.last_used = self.use_counter;
        Some(Arc::clone(&entry.frame))
    }

    /// Keeps `frame` under `fingerprint` (instead of what was kept under it), and lets go of the frames used least recently while there are more
    /// than are wanted. A frame that is bigger than all that is wanted is not kept.
    pub fn insert(&mut self, fingerprint: u64, frame: Arc<RenderedFrame>) {
        if let Some(replaced) = self.frames.remove(&fingerprint) {
            self.bytes -= replaced.frame.byte_count();
        }
        if frame.byte_count() > self.most_bytes {
            return;
        }
        self.use_counter += 1;
        self.bytes += frame.byte_count();
        self.frames.insert(
            fingerprint,
            Entry {
                frame,
                last_used: self.use_counter,
            },
        );
        while self.frames.len() > self.most_frames || self.bytes > self.most_bytes {
            let Some((&least_recently_used, _)) = self.frames.iter().min_by_key(|(_, entry)| entry.last_used) else {
                break;
            };
            let removed = self.frames.remove(&least_recently_used).expect("it was just found");
            self.bytes -= removed.frame.byte_count();
        }
    }

    /// How many frames are kept.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Whether no frame is kept.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// How many bytes the kept frames take up.
    pub fn byte_count(&self) -> usize {
        self.bytes
    }

    /// Lets go of every frame.
    pub fn clear(&mut self) {
        self.frames.clear();
        self.bytes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(bytes: usize, first_byte: u8) -> Arc<RenderedFrame> {
        Arc::new(RenderedFrame {
            width_in_pixels: u32::try_from(bytes / 4).unwrap(),
            height_in_pixels: 1,
            rgba_pixels: vec![first_byte; bytes],
        })
    }

    #[test]
    fn what_was_kept_is_found_by_its_fingerprint_and_nothing_else_is() {
        let mut cache = FrameCache::new(4, 1000);
        cache.insert(7, frame(40, 7));
        assert_eq!(cache.get(7).unwrap().rgba_pixels[0], 7);
        assert!(cache.get(8).is_none());
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.byte_count(), 40);
    }

    #[test]
    fn the_frame_used_least_recently_is_the_one_that_goes_when_there_are_too_many() {
        let mut cache = FrameCache::new(2, 1000);
        cache.insert(1, frame(40, 1));
        cache.insert(2, frame(40, 2));
        cache.get(1); // 1 is used again: 2 is now the one used least recently.
        cache.insert(3, frame(40, 3));
        assert!(cache.get(2).is_none());
        assert!(cache.get(1).is_some());
        assert!(cache.get(3).is_some());
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn frames_go_while_there_are_too_many_bytes_and_a_single_big_frame_may_push_out_several() {
        let mut cache = FrameCache::new(10, 100);
        cache.insert(1, frame(40, 1));
        cache.insert(2, frame(40, 2));
        cache.insert(3, frame(80, 3));
        assert!(cache.get(1).is_none());
        assert!(cache.get(2).is_none());
        assert!(cache.get(3).is_some());
        assert_eq!(cache.byte_count(), 80);
    }

    #[test]
    fn a_frame_bigger_than_all_that_is_wanted_is_not_kept_and_does_not_push_the_others_out() {
        let mut cache = FrameCache::new(10, 100);
        cache.insert(1, frame(40, 1));
        cache.insert(2, frame(400, 2));
        assert!(cache.get(2).is_none());
        assert!(cache.get(1).is_some());
        assert_eq!(cache.byte_count(), 40);
    }

    #[test]
    fn keeping_another_frame_under_the_same_fingerprint_replaces_it_and_counts_its_bytes_once() {
        let mut cache = FrameCache::new(10, 1000);
        cache.insert(1, frame(40, 1));
        cache.insert(1, frame(60, 9));
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.byte_count(), 60);
        assert_eq!(cache.get(1).unwrap().rgba_pixels[0], 9);
        // Replacing with one that is too big drops the old one and keeps nothing.
        cache.insert(1, frame(4000, 5));
        assert!(cache.is_empty());
        assert_eq!(cache.byte_count(), 0);
    }

    #[test]
    fn clearing_lets_go_of_everything() {
        let mut cache = FrameCache::new(10, 1000);
        cache.insert(1, frame(40, 1));
        cache.insert(2, frame(40, 2));
        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.byte_count(), 0);
        assert!(cache.get(1).is_none());
    }
}
