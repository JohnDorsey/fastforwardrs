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

use std::collections::{BTreeMap, HashMap};

use super::animation::{Animation, Keyframe, Keyframes, PropertyId};
use super::tracks::{ClipId, Drawer, KeyframeId, Rack, RackId, Track};
use super::{AnimationTarget, Project};

/// A rack, with where the drawer it is in is.
#[derive(Clone, Copy, Debug)]
pub struct LocatedRack<'project> {
    /// The clip or track that has the drawer.
    pub owner: AnimationTarget,
    /// The frame of the timeline that the frames of the rack are counted from: the first frame of the clip, or 0 for a track.
    pub start: u64,
    /// How many frames the drawer lasts, which is how many frames the rack has room for: as long as the clip. `None` for the
    /// drawer of a track, which goes on as long as the timeline does.
    pub length: Option<u64>,
    pub rack: &'project Rack,
}

impl LocatedRack<'_> {
    /// The frame of the timeline that the keyframe at `frame_in_the_rack` is at.
    pub fn timeline_frame_of(&self, frame_in_the_rack: u64) -> u64 {
        self.start + frame_in_the_rack
    }
}

/// Where a drawer is on the timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawerExtent {
    /// The frame of the timeline that the frames of the racks in the drawer are counted from.
    pub start: u64,
    /// How many frames the drawer lasts, which is how many frames its racks have room for. `None` for the drawer of a track,
    /// which goes on as long as the timeline does.
    pub length: Option<u64>,
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
        self.tracks.iter().flat_map(|track| {
            let in_the_drawer_of_the_track = track.drawer().racks().iter().map(move |rack| LocatedRack {
                owner: AnimationTarget::Track(track.id()),
                start: 0,
                length: None,
                rack,
            });
            let in_the_drawers_of_its_clips = track.placements().flat_map(|placement| {
                placement.drawer.racks().iter().map(move |rack| LocatedRack {
                    owner: AnimationTarget::Clip(placement.id),
                    start: placement.first_timeline_frame.value(),
                    length: Some(placement.timeline_frame_count()),
                    rack,
                })
            });
            in_the_drawer_of_the_track.chain(in_the_drawers_of_its_clips)
        })
    }

    /// The rack called `id`, with where it is.
    pub fn located_rack(&self, id: RackId) -> Option<LocatedRack<'_>> {
        self.located_racks().find(|located| located.rack.id() == id)
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
    /// is left that animates nothing).
    pub(super) fn remove_the_racks_that_animate(&mut self, target: AnimationTarget) {
        for track in &mut self.tracks {
            track.remove_the_racks_that_animate(target);
        }
    }

    /// What is wrong with the racks of the project, in words, if anything is: a rack that animates what is not there or a
    /// property that what it animates does not have, keyframes that are not in the drawer they are in, two racks of one
    /// property of one thing in a drawer, or two racks of the same name. The edits of the project leave none of these; this is
    /// for what is loaded from a file, and for the tests, which check it after every edit they make.
    pub fn problems_with_the_racks(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut names_seen = std::collections::HashSet::new();
        for located in self.located_racks() {
            let rack = located.rack;
            if !names_seen.insert(rack.id()) {
                problems.push(format!("there is more than one {}", rack.id()));
            }
            if rack.is_empty() {
                problems.push(format!("{} has no keyframes", rack.id()));
            }
            let does_the_target_have_the_property = match rack.target() {
                AnimationTarget::Clip(clip) => self
                    .placement_of(clip)
                    .map(|placement| rack.property().applies_to_a_clip_of_kind(placement.clip.kind())),
                AnimationTarget::Track(track) => self.track(track).map(|_| rack.property().applies_to_a_track()),
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
            if let Some(length) = located.length
                && let Some(last_frame) = rack.last_frame()
                && last_frame >= length
            {
                problems.push(format!(
                    "{} has a keyframe at frame {last_frame}, and the drawer it is in lasts {length} frames",
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
        for track in &self.tracks {
            let drawers = std::iter::once((AnimationTarget::Track(track.id()), track.drawer())).chain(
                track
                    .entries()
                    .iter()
                    .map(|entry| (AnimationTarget::Clip(entry.id()), entry.drawer())),
            );
            for (owner, drawer) in drawers {
                let mut animated = std::collections::HashSet::new();
                for rack in drawer.racks() {
                    if !animated.insert((rack.target(), rack.property())) {
                        problems.push(format!(
                            "the drawer of {owner:?} has more than one rack of {:?} of {:?}",
                            rack.property(),
                            rack.target()
                        ));
                    }
                }
            }
        }
        problems
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
