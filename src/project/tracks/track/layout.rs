// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Working out where the clips of a track are, after an edit has changed how long some of them are.
//!
//! The rule (see the module documentation of [`super`]) is applied the same way for every kind of
//! edit: before the edit each clip is noted down with where it started and whether it was one of
//! the clips that move along with the clip before it (a clip that comes from a file and touched
//! the end of the one before it). The edit is then made, and finally every clip is put back
//! from the first to the last: one that moves along starts where the clip before it now ends,
//! and every other one starts where it did, unless the clip before it now reaches past that, in
//! which case it starts right after it.

use super::Track;

/// Where a clip was before an edit, and how it behaves when the clips before it change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Anchor {
    /// The timeline frame the clip started at.
    pub start: u64,
    /// Whether the clip moves along with the clip before it, so that it starts where that ends.
    pub moves_along: bool,
    /// How many frames of the timeline the clip lasted, which is what its keyframes were counted against.
    pub length: u64,
}

impl Track {
    /// Where every clip is now, noted down before an edit. There is one anchor for each clip, in
    /// order.
    pub(super) fn anchors(&self) -> Vec<Anchor> {
        self.entries
            .iter()
            .enumerate()
            .map(|(index, entry)| Anchor {
                start: self.first_timeline_frames[index],
                moves_along: index > 0 && entry.gap_before_frames == 0 && entry.clip.is_media(),
                length: entry.clip.timeline_frame_count(self.speed),
            })
            .collect()
    }

    /// Puts every clip where `anchors` say, as described above. `anchors` has one for each clip,
    /// in order.
    pub(super) fn reposition(&mut self, anchors: &[Anchor]) {
        debug_assert_eq!(anchors.len(), self.entries.len());
        let track_speed = self.speed;
        let mut previous_end = 0;
        for (entry, anchor) in self.entries.iter_mut().zip(anchors) {
            let start = if anchor.moves_along {
                previous_end
            } else {
                anchor.start.max(previous_end)
            };
            entry.gap_before_frames = start - previous_end;
            let length = entry.clip.timeline_frame_count(track_speed);
            if length != anchor.length {
                // The clip got longer or shorter: the keyframes in its drawer are as far through it as they were.
                entry.drawer = entry
                    .drawer
                    .with_each_rack_changed(|rack| rack.rescaled(anchor.length, length));
            }
            previous_end = start + length;
        }
        self.rebuild_derived_values();
    }

    /// Works out where each clip starts, and how long the track is, from the gaps and the lengths.
    pub(super) fn rebuild_derived_values(&mut self) {
        self.first_timeline_frames.clear();
        let mut next_frame = 0;
        for entry in &self.entries {
            next_frame += entry.gap_before_frames;
            self.first_timeline_frames.push(next_frame);
            next_frame += entry.clip.timeline_frame_count(self.speed);
        }
        self.total_frame_count = next_frame;
    }
}
