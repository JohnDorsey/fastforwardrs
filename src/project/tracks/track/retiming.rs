//! Counting a track again at another frame rate.

use super::Track;
use crate::project::FrameRate;

impl Track {
    /// The track as it is when its project runs at the frame rate `to` and not `from`. Everything lasts as
    /// long as it did and starts when it did, to the nearest frame (see [`crate::project::Clip::retimed`]
    /// for how long a clip lasts).
    ///
    /// A clip that touched the one before it still does, and one that had a gap before it still has one.
    /// A run of clips that touch is counted again clip by clip, so it can end a frame early or late for
    /// each clip of it that is sped up (a clip that is sped up lasts a whole number of frames), but not
    /// for the others: pieces of one video cut at normal speed keep fitting together exactly.
    #[must_use]
    pub fn retimed(&self, from: FrameRate, to: FrameRate) -> Self {
        let mut retimed = self.clone();
        let mut previous_end = 0;
        for (index, entry) in retimed.entries.iter_mut().enumerate() {
            let length_before = entry.clip.timeline_frame_count(self.speed);
            entry.clip = entry.clip.retimed(from, to);
            entry.animation = entry
                .animation
                .rescaled(length_before, entry.clip.timeline_frame_count(self.speed));
            let touches_the_one_before = index > 0 && entry.gap_before_frames == 0;
            let start = if touches_the_one_before {
                previous_end
            } else {
                from.convert_frame_count_to(self.first_timeline_frames[index], to)
                    .max(previous_end)
            };
            entry.gap_before_frames = start - previous_end;
            previous_end = start + entry.clip.timeline_frame_count(self.speed);
        }
        // What the track itself has animated is at the same moments of time as it was.
        retimed.animation = self
            .animation
            .mapped_frames(|frame| from.convert_frame_count_to(frame, to));
        retimed.rebuild_derived_values();
        retimed
    }
}
