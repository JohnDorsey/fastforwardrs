//! Moving clips along a track, and laying a track's clips out again.

use super::editing::ClipEditError;
use super::{Track, TrackEntry};
use crate::project::Effects;
use crate::project::tracks::clip::Clip;
use crate::project::tracks::drawer::Drawer;
use crate::project::tracks::ids::ClipId;

/// A clip that is to be put on a track: with its id, its drawer of racks of keyframes, its effects, where it starts, and how long
/// it lasted where it was (which is not how long it will last if it comes from a track that played at another speed).
#[derive(Clone, Debug, PartialEq)]
pub struct ClipToArrange {
    pub id: ClipId,
    pub clip: Clip,
    pub drawer: Drawer,
    pub effects: Effects,
    pub start: u64,
    pub frames_it_lasted_before: u64,
}

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
        let clips = self
            .entries
            .iter()
            .zip(starts)
            .map(|(entry, start)| ClipToArrange {
                id: entry.id,
                clip: entry.clip.clone(),
                drawer: entry.drawer.clone(),
                effects: entry.effects.clone(),
                start,
                frames_it_lasted_before: entry.clip.timeline_frame_count(self.speed),
            })
            .collect();
        self.arrange_clips(clips)
    }

    /// Makes the clips of the track exactly `clips`, each with its id and the frame it starts at, in any
    /// order: they are put in the order of their starts, with the gaps between them that follow. A clip
    /// of another track may be among them, and keeps its id. How long each lasts is counted by this track's
    /// speed. If one starts before the one before it ends, nothing is changed and that clip, the one that
    /// starts inside the other, is named.
    pub fn arrange_clips(&mut self, mut clips: Vec<ClipToArrange>) -> Result<(), ClipEditError> {
        // (A stable sort: two clips that start at the same frame stay in the order they were given in.)
        clips.sort_by_key(|arranged| arranged.start);
        let mut previous_end = 0;
        let mut entries = Vec::with_capacity(clips.len());
        for arranged in clips {
            if arranged.start < previous_end {
                return Err(ClipEditError::WouldOverlap { clip: arranged.id });
            }
            let gap_before_frames = arranged.start - previous_end;
            let length = arranged.clip.timeline_frame_count(self.speed);
            previous_end = arranged.start + length;
            entries.push(TrackEntry {
                id: arranged.id,
                clip: arranged.clip,
                // On a track that plays at another speed the clip lasts another time, and the keyframes in its drawer
                // are as far through it as they were.
                drawer: arranged
                    .drawer
                    .with_each_rack_changed(|rack| rack.rescaled(arranged.frames_it_lasted_before, length)),
                effects: arranged.effects,
                gap_before_frames,
            });
        }
        self.entries = entries;
        self.rebuild_derived_values();
        Ok(())
    }
}
