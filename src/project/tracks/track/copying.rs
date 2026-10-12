//! Making copies of clips and of tracks, with what is in their drawers.

use std::collections::HashMap;

use super::{Track, TrackEntry};
use crate::project::tracks::drawer::Drawer;
use crate::project::tracks::ids::{ClipId, TrackId};
use crate::project::{AnimationTarget, TimelineFrameIndex};

impl Track {
    /// Puts a copy of the clip `clip` on the track so that it starts at `timeline_frame`, with a copy of each of the racks in
    /// its drawer in the drawer of the copy. The keyframes in them are of what they were of, except that those of the clip
    /// itself are of the copy. (The racks kept in other drawers that are of the clip are not copied.)
    ///
    /// Gives the name of the copy, or `None`, and changes nothing, if the track has no such clip or has no room for a copy of
    /// it there.
    pub(in crate::project) fn insert_a_copy_of_the_clip_at(
        &mut self,
        clip: ClipId,
        timeline_frame: TimelineFrameIndex,
    ) -> Option<ClipId> {
        let index = self.index_of(clip)?;
        let copied_clip = self.entries[index].clip.clone();
        let copied_drawer = self.entries[index].drawer.clone();
        let copy = self.insert_clip_at(timeline_frame, copied_clip).ok()?;
        let racks: Vec<_> = copied_drawer
            .racks()
            .iter()
            .map(|rack| {
                let target = if rack.target() == AnimationTarget::Clip(clip) {
                    AnimationTarget::Clip(copy)
                } else {
                    rack.target()
                };
                rack.copy_as(self.take_next_rack_id(), target)
            })
            .collect();
        let drawer = self.drawer_of_the_clip_mut(copy)?;
        for rack in racks {
            drawer.push(rack);
        }
        Some(copy)
    }

    /// A copy of the track that has the name `id` and the title `title`. It has everything else the track has: its loudness,
    /// speed, mute switch and height, its clips where they are (each of them made again, as a clip of the copy, with a name
    /// made by the copy), and a copy of each rack in its drawer and in those of its clips (with names made by the copy too).
    ///
    /// The keyframes in the racks are of what they were of, except that those of the track are of the copy, and those of a
    /// clip of the track are of the copy of that clip: the copy is animated by what it holds, as the track is.
    #[must_use]
    pub(in crate::project) fn copy_as(&self, id: TrackId, title: String) -> Track {
        let mut copy = Track {
            id,
            title,
            gain: self.gain,
            speed: self.speed,
            muted: self.muted,
            height: self.height,
            drawer: Drawer::default(),
            entries: Vec::with_capacity(self.entries.len()),
            next_clip_number: 0,
            next_rack_number: 0,
            first_timeline_frames: Vec::new(),
            total_frame_count: 0,
        };
        let names_of_the_copies: HashMap<ClipId, ClipId> = self
            .entries
            .iter()
            .map(|entry| (entry.id, copy.take_next_clip_id()))
            .collect();
        let animates_in_the_copy = |target: AnimationTarget| match target {
            AnimationTarget::Track(track) if track == self.id => AnimationTarget::Track(id),
            AnimationTarget::Clip(clip) => {
                AnimationTarget::Clip(names_of_the_copies.get(&clip).copied().unwrap_or(clip))
            }
            other => other,
        };
        let copied_drawer_of = |drawer: &Drawer, copy: &mut Track| {
            Drawer::with_racks(
                drawer
                    .racks()
                    .iter()
                    .map(|rack| rack.copy_as(copy.take_next_rack_id(), animates_in_the_copy(rack.target()))),
            )
        };
        copy.drawer = copied_drawer_of(&self.drawer, &mut copy);
        for entry in &self.entries {
            let drawer = copied_drawer_of(&entry.drawer, &mut copy);
            copy.entries.push(TrackEntry {
                id: names_of_the_copies[&entry.id],
                clip: entry.clip.clone(),
                drawer,
                gap_before_frames: entry.gap_before_frames,
            });
        }
        copy.rebuild_derived_values();
        copy
    }
}
