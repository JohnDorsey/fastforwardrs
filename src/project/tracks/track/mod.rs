//! A track: a row of clips on the timeline, with a title and a volume, a speed and a mute switch
//! of its own.
//!
//! The clips on a track are in order, one after another, and may have empty time between them (or
//! before the first). Which clip is at which frame of the timeline is worked out here, in whole
//! frames, and nowhere else, so that the pictures and the sound, the live preview and the export,
//! all agree.
//!
//! **What moves when a clip changes its length.** A clip that comes from a file (a video or a
//! sound) gets longer or shorter when it is sped up or slowed down, and when the speed of the
//! track changes; and it can be deleted. The clips *touching* its end move along so that no gap
//! opens up or closes; a gap, though, stops the movement, and the clips after it stay where they
//! are (and are pushed on only if the clip before them grows into them). Text objects never move
//! because of another clip's change: they stay exactly where they were put (and are pushed on only
//! the same way). So a video made of clips that touch moves up as one, and captions stay in time.

#[cfg(test)]
mod animation_tests;
mod copying;
mod editing;
mod effects;
#[cfg(test)]
mod effects_tests;
mod keyframing;
mod layout;
mod moving;
mod retiming;
#[cfg(test)]
mod tests;

pub use editing::{ClipEditError, CutEdges, InsertError, RackCut, SplitError, SplitOutcome};
pub use moving::ClipToArrange;

use super::clip::Clip;
use super::drawer::Drawer;
use super::gain::Gain;
use super::ids::{ClipId, RackId, TrackId};
use super::track_height::TrackHeight;
use crate::project::{Animation, AnimationTarget, Effects, SourceFrameIndex, SpeedFactor, TimelineFrameIndex};

/// A clip on a track, and where it is in relation to the clip before it.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackEntry {
    id: ClipId,
    clip: Clip,
    /// The racks of keyframes that are kept with the clip, counted in frames from the clip's first frame. They belong to
    /// the clip: they go where it goes, and are cut, stretched and shortened as it is, whatever they are the keyframes of.
    drawer: Drawer,
    /// The effects the clip has: what is done to its picture on its way to the finished picture.
    effects: Effects,
    /// How many frames of empty timeline come before the clip: after the end of the clip before
    /// it, or after the start of the timeline for the first clip.
    gap_before_frames: u64,
}

impl TrackEntry {
    pub fn id(&self) -> ClipId {
        self.id
    }

    pub fn clip(&self) -> &Clip {
        &self.clip
    }

    /// The racks of keyframes that are kept with the clip.
    pub fn drawer(&self) -> &Drawer {
        &self.drawer
    }

    /// The effects the clip has.
    pub fn effects(&self) -> &Effects {
        &self.effects
    }

    /// What the clip's own racks say of the clip's own properties, counted in frames from its first frame. (The racks in
    /// its drawer that are of other things' properties are not in it, nor are the racks of its properties that are kept
    /// somewhere else: what they all say together is the project's to work out.)
    pub fn own_animation(&self) -> Animation {
        animation_of_the_racks_of(&self.drawer, AnimationTarget::Clip(self.id))
    }

    pub fn gap_before_frames(&self) -> u64 {
        self.gap_before_frames
    }
}

/// What the racks in `drawer` that animate `target` say, as an animation.
fn animation_of_the_racks_of(drawer: &Drawer, target: AnimationTarget) -> Animation {
    Animation::from_keyframes(
        drawer
            .racks()
            .iter()
            .filter(|rack| rack.target() == target)
            .map(|rack| (rack.property(), rack.curve())),
    )
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    id: TrackId,
    title: String,
    gain: Gain,
    speed: SpeedFactor,
    muted: bool,
    /// How tall the track's row is on the timeline. (A way of looking at the track, not part of
    /// what it plays: it is saved with the project because it is something that was arranged.)
    height: TrackHeight,
    /// The racks of keyframes that are kept with the track, counted in frames from the start of the timeline: those of
    /// its own properties (its loudness, and whether it is muted) and any other.
    drawer: Drawer,
    /// The effects the track has: what is done to the pictures of all its clips, after what their own effects do.
    effects: Effects,
    entries: Vec<TrackEntry>,
    /// The number the next new clip of this track is given (see [`ClipId`]).
    next_clip_number: u32,
    /// The number the next new rack made in this track (in its own drawer or in that of a clip on it) is given (see
    /// [`RackId`]).
    next_rack_number: u32,
    /// `first_timeline_frames[i]` is the timeline frame at which `entries[i]` starts. It is
    /// strictly increasing, because every clip lasts at least one frame. Worked out from the gaps
    /// and the lengths after every edit, so that finding what is at a frame is fast however many
    /// clips there are.
    first_timeline_frames: Vec<u64>,
    /// The frame just after the last clip's last frame: how long the track is.
    total_frame_count: u64,
}

/// Where a clip sits on the timeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipPlacement<'track> {
    /// The clip's place in the order of the track's clips.
    pub index: usize,
    pub id: ClipId,
    pub clip: &'track Clip,
    /// The racks of keyframes that are kept with the clip, counted in frames from `first_timeline_frame`.
    pub drawer: &'track Drawer,
    /// The effects the clip has.
    pub effects: &'track Effects,
    pub first_timeline_frame: TimelineFrameIndex,
    /// The frame just after the clip's last frame (the end is exclusive).
    pub end_timeline_frame: TimelineFrameIndex,
    /// How much faster than normal the clip plays: its own speed times its track's. Normal speed
    /// for a text clip, which has none.
    pub effective_speed: SpeedFactor,
}

impl ClipPlacement<'_> {
    /// How many frames of the timeline the clip occupies.
    pub fn timeline_frame_count(&self) -> u64 {
        self.end_timeline_frame.value() - self.first_timeline_frame.value()
    }

    /// What the clip's own racks say of the clip's own properties: see [`TrackEntry::own_animation`].
    pub fn own_animation(&self) -> Animation {
        animation_of_the_racks_of(self.drawer, AnimationTarget::Clip(self.id))
    }

    pub fn contains(&self, timeline_frame: TimelineFrameIndex) -> bool {
        self.first_timeline_frame <= timeline_frame && timeline_frame < self.end_timeline_frame
    }

    /// How many frames into the clip `timeline_frame` is (0 for its first frame), or `None` if the
    /// frame is not in the clip.
    pub fn offset_of(&self, timeline_frame: TimelineFrameIndex) -> Option<u64> {
        self.contains(timeline_frame)
            .then(|| timeline_frame.value() - self.first_timeline_frame.value())
    }

    /// The frame of the source file that is shown at `timeline_frame`: for a video or a sound clip
    /// that is in the clip. (`None` for a text clip, or a frame the clip is not at.)
    pub fn source_frame_at(&self, timeline_frame: TimelineFrameIndex) -> Option<SourceFrameIndex> {
        let offset = self.offset_of(timeline_frame)?;
        self.clip
            .as_media()?
            .source_frame_at_timeline_offset(self.effective_speed, offset)
    }
}

/// What is at one frame of a track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Room {
    /// A clip is there.
    Occupied(ClipId),
    /// Nothing is there: empty timeline from `from_frame` up to (not including) `until_frame`, or
    /// for ever if the frame is past the last clip.
    Free { from_frame: u64, until_frame: Option<u64> },
}

impl Track {
    /// An empty track that plays at normal speed and normal loudness.
    pub fn new(id: TrackId, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            gain: Gain::UNCHANGED,
            speed: SpeedFactor::NORMAL,
            muted: false,
            height: TrackHeight::DEFAULT,
            drawer: Drawer::default(),
            effects: Effects::none(),
            entries: Vec::new(),
            next_clip_number: 0,
            next_rack_number: 0,
            first_timeline_frames: Vec::new(),
            total_frame_count: 0,
        }
    }

    /// A track holding `clips`, each with the number of empty frames before it.
    #[must_use]
    pub fn with_clips(self, clips: impl IntoIterator<Item = (u64, Clip)>) -> Self {
        self.with_clips_and_drawers(
            clips
                .into_iter()
                .map(|(gap_before_frames, clip)| (gap_before_frames, clip, Drawer::default())),
        )
    }

    /// A track holding `clips`, each with the number of empty frames before it and its drawer of racks, which is used as
    /// it is: the racks must be made for what is on the track, and have names (see [`RackId`]) that are not each other's.
    #[must_use]
    pub fn with_clips_and_drawers(mut self, clips: impl IntoIterator<Item = (u64, Clip, Drawer)>) -> Self {
        for (gap_before_frames, clip, drawer) in clips {
            let id = self.take_next_clip_id();
            for rack in drawer.racks() {
                self.note_the_name_of_a_rack(rack.id());
            }
            self.entries.push(TrackEntry {
                id,
                clip,
                drawer,
                effects: Effects::none(),
                gap_before_frames,
            });
        }
        self.rebuild_derived_values();
        self
    }

    /// The same track with `drawer` for the drawer of its own, which is used as it is: see [`Self::with_clips_and_drawers`].
    #[must_use]
    pub fn with_drawer(mut self, drawer: Drawer) -> Self {
        for rack in drawer.racks() {
            self.note_the_name_of_a_rack(rack.id());
        }
        self.drawer = drawer;
        self
    }

    /// Makes sure that the names this track makes for racks from now on are not that of a rack that was made for it by someone
    /// else (when a track is made from its parts).
    fn note_the_name_of_a_rack(&mut self, id: RackId) {
        if id.origin() == self.id {
            self.next_rack_number = self.next_rack_number.max(id.number().saturating_add(1));
        }
    }

    #[must_use]
    pub fn with_gain(mut self, gain: Gain) -> Self {
        self.gain = gain;
        self
    }

    /// The same track playing at `speed`. (Use [`Self::set_speed`] to change a track that is in a project.)
    #[must_use]
    pub fn with_speed(mut self, speed: SpeedFactor) -> Self {
        self.speed = speed;
        self.rebuild_derived_values();
        self
    }

    #[must_use]
    pub fn with_muted(mut self, muted: bool) -> Self {
        self.muted = muted;
        self
    }

    #[must_use]
    pub fn with_height(mut self, height: TrackHeight) -> Self {
        self.height = height;
        self
    }

    pub fn id(&self) -> TrackId {
        self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn gain(&self) -> Gain {
        self.gain
    }

    /// How much faster than normal everything on the track plays, on top of each clip's own speed.
    pub fn speed(&self) -> SpeedFactor {
        self.speed
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }

    pub fn height(&self) -> TrackHeight {
        self.height
    }

    /// The racks of keyframes that are kept with the track, counted in frames from the start of the timeline.
    pub fn drawer(&self) -> &Drawer {
        &self.drawer
    }

    /// The effects the track has.
    pub fn effects(&self) -> &Effects {
        &self.effects
    }

    /// What the track's own racks say of the track's own properties, counted in frames from the start of the timeline.
    /// (What the racks kept somewhere else say of them is not in it: what they all say together is the project's to work out.)
    pub fn own_animation(&self) -> Animation {
        animation_of_the_racks_of(&self.drawer, AnimationTarget::Track(self.id))
    }

    /// The drawer of the track, to change what is in it. (What the project does to racks: moving one from a drawer to another,
    /// taking away those of what is gone.)
    pub(in crate::project) fn drawer_mut(&mut self) -> &mut Drawer {
        &mut self.drawer
    }

    /// The drawer of the clip `clip`, to change what is in it.
    pub(in crate::project) fn drawer_of_the_clip_mut(&mut self, clip: ClipId) -> Option<&mut Drawer> {
        let index = self.index_of(clip)?;
        Some(&mut self.entries[index].drawer)
    }

    /// A name for a new rack made in this track, which is not that of any rack made in it before.
    pub(in crate::project) fn make_a_rack_id(&mut self) -> RackId {
        self.take_next_rack_id()
    }

    /// Takes away the racks in the drawers of the track and of its clips that animate `target`.
    pub(in crate::project) fn remove_the_racks_that_animate(&mut self, target: AnimationTarget) {
        self.drawer.remove_the_racks_of(target);
        for entry in &mut self.entries {
            entry.drawer.remove_the_racks_of(target);
        }
    }

    pub fn entries(&self) -> &[TrackEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many frames the track lasts: up to the end of its last clip.
    pub fn total_frame_count(&self) -> u64 {
        self.total_frame_count
    }

    pub fn placement_at_index(&self, index: usize) -> Option<ClipPlacement<'_>> {
        let entry = self.entries.get(index)?;
        let first = self.first_timeline_frames[index];
        Some(ClipPlacement {
            index,
            id: entry.id,
            clip: &entry.clip,
            drawer: &entry.drawer,
            effects: &entry.effects,
            first_timeline_frame: TimelineFrameIndex::new(first),
            end_timeline_frame: TimelineFrameIndex::new(first + entry.clip.timeline_frame_count(self.speed)),
            effective_speed: entry.clip.effective_speed(self.speed),
        })
    }

    /// Every clip with its place on the timeline, in order.
    pub fn placements(&self) -> impl Iterator<Item = ClipPlacement<'_>> {
        (0..self.entries.len()).filter_map(|index| self.placement_at_index(index))
    }

    pub fn placement_of(&self, clip: ClipId) -> Option<ClipPlacement<'_>> {
        let index = self.index_of(clip)?;
        self.placement_at_index(index)
    }

    /// The clip that is at `timeline_frame`, or `None` if nothing is (the frame is in a gap, or
    /// past the end of the track).
    pub fn placement_covering(&self, timeline_frame: TimelineFrameIndex) -> Option<ClipPlacement<'_>> {
        let index = self.index_of_clip_starting_at_or_before(timeline_frame)?;
        self.placement_at_index(index)
            .filter(|placement| placement.contains(timeline_frame))
    }

    /// What is at `timeline_frame`: a clip, or how much room there is for one.
    pub fn room_at(&self, timeline_frame: TimelineFrameIndex) -> Room {
        let frame = timeline_frame.value();
        if let Some(placement) = self.placement_covering(timeline_frame) {
            return Room::Occupied(placement.id);
        }
        let clips_starting_at_or_before = self.first_timeline_frames.partition_point(|&first| first <= frame);
        let from_frame = clips_starting_at_or_before
            .checked_sub(1)
            .and_then(|index| self.placement_at_index(index))
            .map_or(0, |previous| previous.end_timeline_frame.value());
        let until_frame = self.first_timeline_frames.get(clips_starting_at_or_before).copied();
        Room::Free {
            from_frame,
            until_frame,
        }
    }

    /// The place in the order of the clip called `clip`.
    pub fn index_of(&self, clip: ClipId) -> Option<usize> {
        self.entries.iter().position(|entry| entry.id == clip)
    }

    fn index_of_clip_starting_at_or_before(&self, timeline_frame: TimelineFrameIndex) -> Option<usize> {
        self.first_timeline_frames
            .partition_point(|&first| first <= timeline_frame.value())
            .checked_sub(1)
    }

    fn take_next_clip_id(&mut self) -> ClipId {
        let id = ClipId::new(self.id, self.next_clip_number);
        self.next_clip_number += 1;
        id
    }

    /// A name for a new rack made in this track, which is not that of any rack made in it before.
    fn take_next_rack_id(&mut self) -> RackId {
        let id = RackId::new(self.id, self.next_rack_number);
        self.next_rack_number += 1;
        id
    }
}
