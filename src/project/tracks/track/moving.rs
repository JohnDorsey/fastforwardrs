//! Moving clips along a track.

use super::editing::ClipEditError;
use super::{Track, TrackEntry};
use crate::project::tracks::ids::ClipId;

impl Track {
    /// Moves clips so that each starts at the frame given for it, and leaves every other clip where
    /// it is. No two clips may be at the same frame afterwards, nor may any start before the start
    /// of the timeline: if they would be, nothing is moved at all. Clips that are moved past others
    /// that stay (which the arrangement allows when they do not overlap in the end) are put in their
    /// new order. Nothing else is moved: a clip that touched a clip that is moved away is left where
    /// it was, with a gap now before it.
    pub fn move_clips(&mut self, new_starts: &[(ClipId, u64)]) -> Result<(), ClipEditError> {
        let mut starts = self.first_timeline_frames.clone();
        for (clip, start) in new_starts {
            let index = self.index_of(*clip).ok_or(ClipEditError::UnknownClip { clip: *clip })?;
            starts[index] = *start;
        }
        let mut order: Vec<usize> = (0..self.entries.len()).collect();
        order.sort_by_key(|&index| starts[index]);
        let mut previous_end = 0;
        for &index in &order {
            if starts[index] < previous_end {
                return Err(ClipEditError::WouldOverlap {
                    clip: self.entries[index].id,
                });
            }
            previous_end = starts[index] + self.entries[index].clip.timeline_frame_count(self.speed);
        }

        let mut entries: Vec<Option<TrackEntry>> = std::mem::take(&mut self.entries).into_iter().map(Some).collect();
        let mut previous_end = 0;
        for index in order {
            let mut entry = entries[index].take().expect("every clip is placed once");
            entry.gap_before_frames = starts[index] - previous_end;
            previous_end = starts[index] + entry.clip.timeline_frame_count(self.speed);
            self.entries.push(entry);
        }
        self.rebuild_derived_values();
        Ok(())
    }
}
