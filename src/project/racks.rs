//! The racks of a project: where they all are, and what the racks of one property of a clip or a track say together.
//!
//! The racks of keyframes are kept in drawers (those of the clips and those of the tracks), and what a rack animates need
//! not be what has the drawer it is in. So what a property of a clip or a track is, when it has keyframes, is not found in
//! one place: it is what *all* the racks that animate it say, wherever they are, together. Their keyframes are put at the
//! frames of the timeline they are at (counted from the start of the drawer they are in, which is the clip's first frame or
//! the start of the timeline), and are one curve: between two of them the property changes evenly from the value of the one
//! to that of the other, whichever racks they are in. Where two keyframes of one property of one thing are at the same
//! frame, the one in the rack that comes later (see [`Project::located_racks`]) is the one that counts.
//!
//! Everything that makes what is shown and heard (the picture plan, the sound plan) works from [`ResolvedAnimations`], which
//! is that, worked out once for everything.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::animation::{Animation, Keyframe, Keyframes, PropertyId};
use super::tracks::{Clip, ClipId, Drawer, KeyframeId, Rack, RackId, RackKeyframe, Track};
use super::{AnimationTarget, Project};

/// The frames of the timeline that the keyframes of the racks in a drawer can be at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameRoom {
    pub first_frame: u64,
    /// The frame after the last that they can be at. `None` for the drawer of a track, which goes on as long as the timeline does.
    pub end_frame: Option<u64>,
}

impl FrameRoom {
    /// Whether a keyframe can be at the frame `timeline_frame`.
    pub fn contains(&self, timeline_frame: u64) -> bool {
        self.first_frame <= timeline_frame && self.end_frame.is_none_or(|end| timeline_frame < end)
    }

    /// How many frames there are, if there is an end to them.
    pub fn length(&self) -> Option<u64> {
        self.end_frame.map(|end| end - self.first_frame)
    }
}

/// A rack, with where the drawer it is in is.
#[derive(Clone, Copy, Debug)]
pub struct LocatedRack<'project> {
    /// The clip or track that has the drawer.
    pub owner: AnimationTarget,
    /// The frame of the timeline that the frames of the rack are counted from: the first frame of the clip, or 0 for a track.
    pub start: u64,
    /// The frames the keyframes of the rack can be at: those of the clip, which are as many as it lasts; all of them from the
    /// start of the timeline, for the drawer of a track; and for a keyframe crate, which lasts as long as its keyframes need, the
    /// empty frames round it as well as its own, as far as the end of the video. (It is made longer if a keyframe is put there.)
    pub room: FrameRoom,
    pub rack: &'project Rack,
}

impl LocatedRack<'_> {
    /// The frame of the timeline that the keyframe at `frame_in_the_rack` is at.
    pub fn timeline_frame_of(&self, frame_in_the_rack: u64) -> u64 {
        self.start + frame_in_the_rack
    }
}

/// A keyframe, with where the rack it is in is.
#[derive(Clone, Copy, Debug)]
pub struct LocatedKeyframe<'project> {
    pub rack: LocatedRack<'project>,
    pub keyframe: &'project RackKeyframe,
}

impl LocatedKeyframe<'_> {
    /// The frame of the timeline that the keyframe is at.
    pub fn timeline_frame(&self) -> u64 {
        self.rack.timeline_frame_of(self.keyframe.frame)
    }
}

/// Where a drawer is on the timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawerExtent {
    /// The frame of the timeline that the frames of the racks in the drawer are counted from.
    pub start: u64,
    /// The frames its racks can have keyframes at (see [`LocatedRack::room`]).
    pub room: FrameRoom,
}

/// A keyframe that was just made, and where.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NewKeyframe {
    /// The clip or track whose drawer the keyframe's rack is in.
    pub owner: AnimationTarget,
    pub rack: RackId,
    pub keyframe: KeyframeId,
}

/// Where a keyframe is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyframeLocation {
    pub owner: AnimationTarget,
    pub rack: RackId,
    /// Its frame, counted from the start of the drawer it is in.
    pub frame_in_the_rack: u64,
}

/// What every clip and every track that has keyframes is animated by, in frames of the timeline, from all the racks there are.
#[derive(Clone, Debug, Default)]
pub struct ResolvedAnimations {
    by_target: HashMap<AnimationTarget, Animation>,
}

/// What is animated by nothing.
static NOTHING_IS_ANIMATED: Animation = Animation::none();

impl ResolvedAnimations {
    /// What animates `target`: nothing at all, if no rack does.
    pub fn of(&self, target: AnimationTarget) -> &Animation {
        self.by_target.get(&target).unwrap_or(&NOTHING_IS_ANIMATED)
    }
}

impl Project {
    /// Every rack of the project with where its drawer is, in the order that decides which of two keyframes of one property of
    /// one thing at the same frame counts: the one in the rack that comes later. (The tracks from the front; of each its
    /// own drawer first, then the drawers of its clips in order.)
    pub fn located_racks(&self) -> impl Iterator<Item = LocatedRack<'_>> {
        let end_of_the_video = self.total_frame_count();
        self.tracks.iter().flat_map(move |track| {
            let in_the_drawer_of_the_track = track.drawer().racks().iter().map(move |rack| LocatedRack {
                owner: AnimationTarget::Track(track.id()),
                start: 0,
                room: FrameRoom {
                    first_frame: 0,
                    end_frame: None,
                },
                rack,
            });
            let in_the_drawers_of_its_clips = track.placements().flat_map(move |placement| {
                let room = room_of_the_drawer_of(track, &placement, end_of_the_video);
                placement.drawer.racks().iter().map(move |rack| LocatedRack {
                    owner: AnimationTarget::Clip(placement.id),
                    start: placement.first_timeline_frame.value(),
                    room,
                    rack,
                })
            });
            in_the_drawer_of_the_track.chain(in_the_drawers_of_its_clips)
        })
    }

    /// Where an empty stretch of a track that goes up to `until_frame` (for ever, if `None`) stops being room for a keyframe crate:
    /// at its end, or where the video ends if that comes sooner (a crate does not make the video longer).
    pub fn end_of_the_room_for_a_crate_in_a_stretch(&self, until_frame: Option<u64>) -> u64 {
        end_of_the_room_for_a_crate_in_a_stretch(until_frame, self.total_frame_count())
    }

    /// How many keyframes there are in the project, in all its racks. (What a change that is meant to keep them all is held to:
    /// the keyframes that are lost are the difference.)
    pub fn keyframe_count(&self) -> usize {
        self.located_racks().map(|located| located.rack.len()).sum()
    }

    /// The rack called `id`, with where it is.
    pub fn located_rack(&self, id: RackId) -> Option<LocatedRack<'_>> {
        self.located_racks().find(|located| located.rack.id() == id)
    }

    /// The keyframe called `id`, with where it is: in the rack it was made in, unless it has been moved to another since.
    pub fn locate_keyframe(&self, id: KeyframeId) -> Option<LocatedKeyframe<'_>> {
        fn found_in(rack: LocatedRack<'_>, id: KeyframeId) -> Option<LocatedKeyframe<'_>> {
            rack.rack
                .keyframe(id)
                .map(|keyframe| LocatedKeyframe { rack, keyframe })
        }
        // (Nearly always it is where it was made, which is quick to look at: all the racks are looked in only if it is not.)
        self.located_rack(id.origin())
            .and_then(|rack| found_in(rack, id))
            .or_else(|| self.located_racks().find_map(|rack| found_in(rack, id)))
    }

    /// The drawer of the clip or the track `owner`.
    pub fn drawer_of(&self, owner: AnimationTarget) -> Option<&Drawer> {
        match owner {
            AnimationTarget::Clip(clip) => self.placement_of(clip).map(|placement| placement.drawer),
            AnimationTarget::Track(track) => self.track(track).map(Track::drawer),
        }
    }

    pub(super) fn drawer_mut(&mut self, owner: AnimationTarget) -> Option<&mut Drawer> {
        match owner {
            AnimationTarget::Clip(clip) => {
                let position = self.track_position_holding(clip)?;
                self.tracks[position].drawer_of_the_clip_mut(clip)
            }
            AnimationTarget::Track(track) => self.track_mut(track).map(Track::drawer_mut),
        }
    }

    /// Whether anything animates `property` of `target`: a rack of it, wherever it is.
    pub fn is_animated(&self, target: AnimationTarget, property: PropertyId) -> bool {
        self.located_racks()
            .any(|located| located.rack.target() == target && located.rack.property() == property)
    }

    /// The keyframes of `property` of `target`, from all the racks that have some, at the frames of the timeline they are at.
    pub fn keyframes_of(&self, target: AnimationTarget, property: PropertyId) -> Keyframes {
        Keyframes::from_unordered(
            self.located_racks()
                .filter(|located| located.rack.target() == target && located.rack.property() == property)
                .flat_map(keyframes_at_timeline_frames),
        )
    }

    /// What all the racks that animate `target` say, as one animation, at the frames of the timeline.
    pub fn animation_of(&self, target: AnimationTarget) -> Animation {
        let mut by_property: BTreeMap<PropertyId, Vec<Keyframe>> = BTreeMap::new();
        for located in self.located_racks().filter(|located| located.rack.target() == target) {
            by_property
                .entry(located.rack.property())
                .or_default()
                .extend(keyframes_at_timeline_frames(located));
        }
        animation_of_the_keyframes(by_property)
    }

    /// What all the racks say of every clip and track that has keyframes, at the frames of the timeline: worked out once, for
    /// the plans of what is shown and heard.
    pub fn resolved_animations(&self) -> ResolvedAnimations {
        let mut by_target: HashMap<AnimationTarget, BTreeMap<PropertyId, Vec<Keyframe>>> = HashMap::new();
        for located in self.located_racks() {
            by_target
                .entry(located.rack.target())
                .or_default()
                .entry(located.rack.property())
                .or_default()
                .extend(keyframes_at_timeline_frames(located));
        }
        ResolvedAnimations {
            by_target: by_target
                .into_iter()
                .map(|(target, by_property)| (target, animation_of_the_keyframes(by_property)))
                .collect(),
        }
    }

    /// Where the keyframe of `property` of `target` that counts at `timeline_frame` is kept, if there is one there.
    pub fn locate_the_keyframe_at(
        &self,
        target: AnimationTarget,
        property: PropertyId,
        timeline_frame: u64,
    ) -> Option<KeyframeLocation> {
        self.located_racks()
            .filter(|located| located.rack.target() == target && located.rack.property() == property)
            .filter_map(|located| {
                let frame_in_the_rack = timeline_frame.checked_sub(located.start)?;
                located.rack.at(frame_in_the_rack)?;
                Some(KeyframeLocation {
                    owner: located.owner,
                    rack: located.rack.id(),
                    frame_in_the_rack,
                })
            })
            .last()
    }

    /// Takes away the racks, wherever they are, that animate `target` (a clip or a track that is gone, so that nothing
    /// is left that animates nothing). A keyframe crate that has racks left is made as short as their keyframes need, and one that has
    /// none left goes.
    pub(super) fn remove_the_racks_that_animate(&mut self, target: AnimationTarget) {
        let drawers_that_lost_racks: Vec<AnimationTarget> = self
            .tracks
            .iter_mut()
            .flat_map(|track| track.remove_the_racks_that_animate(target))
            .map(AnimationTarget::Clip)
            .collect();
        let fitted = self.fit_the_crates(&drawers_that_lost_racks);
        debug_assert!(fitted.is_ok(), "a crate that loses racks only gets shorter: {fitted:?}");
    }

    /// What is wrong with the racks of the project, in words, if anything is: a rack that animates what is not there or a
    /// property that what it animates does not have, keyframes that are not in the drawer they are in or not in the order of
    /// their frames, two racks of the same name, two keyframes of the same name, or a keyframe crate that has no keyframes or does not
    /// last exactly as long as they need. The edits of the project leave none of these; this is for what is loaded from a file, and
    /// for the tests, which check it after every edit they make.
    pub fn problems_with_the_racks(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut names_seen = HashSet::new();
        let mut keyframe_names_seen = HashSet::new();
        for located in self.located_racks() {
            let rack = located.rack;
            if !names_seen.insert(rack.id()) {
                problems.push(format!("there is more than one {}", rack.id()));
            }
            for keyframe in rack.keyframes() {
                if !keyframe_names_seen.insert(keyframe.id()) {
                    problems.push(format!("there is more than one {}", keyframe.id()));
                }
            }
            if rack.keyframes().windows(2).any(|pair| pair[0].frame >= pair[1].frame) {
                problems.push(format!(
                    "the keyframes of {} are not in the order of their frames, each at a frame of its own",
                    rack.id()
                ));
            }
            if rack.is_empty() {
                problems.push(format!("{} has no keyframes", rack.id()));
            }
            let does_the_target_have_the_property = match rack.target() {
                AnimationTarget::Clip(clip) => self.placement_of(clip).map(|placement| {
                    rack.property()
                        .is_had_by_a_clip(placement.clip.kind(), placement.effects)
                }),
                AnimationTarget::Track(track) => self
                    .track(track)
                    .map(|track| rack.property().is_had_by_a_track(track.effects())),
            };
            match does_the_target_have_the_property {
                None => problems.push(format!(
                    "{} animates something that is not there: {:?}",
                    rack.id(),
                    rack.target()
                )),
                Some(false) => problems.push(format!(
                    "{} animates {:?} of something that does not have it: {:?}",
                    rack.id(),
                    rack.property(),
                    rack.target()
                )),
                Some(true) => {}
            }
            if let Some(last_frame) = rack.last_frame()
                && !located.room.contains(located.timeline_frame_of(last_frame))
            {
                problems.push(format!(
                    "{} has a keyframe at frame {last_frame} of the drawer it is in, which has no room for it",
                    rack.id()
                ));
            }
            if rack
                .keyframes()
                .iter()
                .any(|keyframe| keyframe.value.kind() != rack.property().kind())
            {
                problems.push(format!(
                    "{} has a value that is not the kind of {:?}",
                    rack.id(),
                    rack.property()
                ));
            }
        }
        problems.extend(self.problems_with_the_crates());
        problems
    }

    /// What is wrong with the keyframe crates, in words: a crate lasts exactly as long as the keyframes in it need, from the first of
    /// them to the last, and has some (see the `crates` module).
    fn problems_with_the_crates(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for placement in self.tracks.iter().flat_map(Track::placements) {
            if !matches!(placement.clip, Clip::Crate(_)) {
                continue;
            }
            let frames = placement
                .drawer
                .racks()
                .iter()
                .filter_map(|rack| rack.first_frame().zip(rack.last_frame()));
            let first_and_last = frames.fold(None, |found: Option<(u64, u64)>, (first, last)| {
                Some(found.map_or((first, last), |(earliest, latest)| {
                    (earliest.min(first), latest.max(last))
                }))
            });
            match first_and_last {
                None => problems.push(format!("the keyframe crate {} has no keyframes in it", placement.id)),
                Some((first, last)) if first != 0 || last + 1 != placement.timeline_frame_count() => {
                    problems.push(format!(
                        "the keyframe crate {} lasts {} frames, and its keyframes are at frames {first} to {last} of it",
                        placement.id,
                        placement.timeline_frame_count()
                    ));
                }
                Some(_) => {}
            }
        }
        problems
    }
}

/// Where an empty stretch of a track that goes up to `until_frame` (for ever, if `None`) stops being room for a keyframe crate: at its
/// end, or where the video ends, which is `end_of_the_video`, if that comes sooner (a crate does not make the video longer).
fn end_of_the_room_for_a_crate_in_a_stretch(until_frame: Option<u64>, end_of_the_video: u64) -> u64 {
    until_frame.map_or(end_of_the_video, |until| until.min(end_of_the_video))
}

/// The frames that the racks in the drawer of the clip at `placement` on `track` can have keyframes at: the frames of the clip, and
/// for a keyframe crate, which lasts as long as its keyframes need, the empty frames round it too (it is made longer if a keyframe is
/// put there), as far as the end of the video, which is `end_of_the_video`.
pub(super) fn room_of_the_drawer_of(
    track: &Track,
    placement: &crate::project::ClipPlacement<'_>,
    end_of_the_video: u64,
) -> FrameRoom {
    if matches!(placement.clip, Clip::Crate(_))
        && let Some((from_frame, until_frame)) = track.stretch_around(placement.id)
    {
        return FrameRoom {
            first_frame: from_frame,
            end_frame: Some(end_of_the_room_for_a_crate_in_a_stretch(until_frame, end_of_the_video)),
        };
    }
    FrameRoom {
        first_frame: placement.first_timeline_frame.value(),
        end_frame: Some(placement.end_timeline_frame.value()),
    }
}

/// The keyframes of `located`, each at the frame of the timeline it is at.
fn keyframes_at_timeline_frames(located: LocatedRack<'_>) -> impl Iterator<Item = Keyframe> + '_ {
    located.rack.keyframes().iter().map(move |keyframe| Keyframe {
        frame: located.timeline_frame_of(keyframe.frame),
        value: keyframe.value.clone(),
    })
}

fn animation_of_the_keyframes(by_property: BTreeMap<PropertyId, Vec<Keyframe>>) -> Animation {
    Animation::from_keyframes(
        by_property
            .into_iter()
            .map(|(property, keyframes)| (property, Keyframes::from_unordered(keyframes))),
    )
}

/// The clips (as ids) that are on `track`, which is what a track that is deleted takes with it.
pub(super) fn ids_of_the_clips_on(track: &Track) -> Vec<ClipId> {
    track.entries().iter().map(|entry| entry.id()).collect()
}
