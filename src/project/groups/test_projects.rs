// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Projects with clips at places that are known, to try groups on. (Text clips are used because they last exactly as long as they are
//! asked to, and need no file. **They are text clips that have no text effect**, which a text clip that the program made always has, as the first of its chain: what the tests of
//! groups are about is the groups and the chains, and the chain of each of these clips is empty to begin with, as they were before a text was made of an effect. What a text clip is
//! made of is tried by the tests of texts.)

use crate::project::{
    ClipId, FrameRate, Group, GroupColour, GroupId, GroupKind, Member, Project, SourceMedia, SourceMediaIdentifier,
    SourceMediaLink, SourceVideoProperties, TextObject, TimelineFrameIndex, TrackId,
};

/// A project that has been laid out, and what is in it.
pub(crate) struct Laid {
    pub project: Project,
    /// The tracks, the one in front first.
    pub tracks: Vec<TrackId>,
    /// The clips of each track, in the order they were given, which is the order they are in on the track.
    pub clips: Vec<Vec<ClipId>>,
}

impl Laid {
    /// The clip at place `index` of the track at place `track` in the list of tracks.
    pub fn clip(&self, track: usize, index: usize) -> ClipId {
        self.clips[track][index]
    }

    /// The same, as what a group has in it.
    pub fn clip_member(&self, track: usize, index: usize) -> Member {
        Member::Clip(self.clip(track, index))
    }

    pub fn track_member(&self, track: usize) -> Member {
        Member::Track(self.tracks[track])
    }

    /// Puts a group of kind `kind` with `members` in the project **as it is, whatever the rules about groups say**, and, if it is a tag,
    /// with the badge of it in the chain of each of its members (which is what the editing does, and is a rule of its own, which the tests
    /// of that rule leave out). For making what the editing would refuse, to see that the rules find it. Returns the group's name.
    pub fn put_in(&mut self, kind: GroupKind, members: &[Member]) -> GroupId {
        let id = GroupId::new(u32::try_from(self.project.groups.len()).unwrap());
        let colour = GroupColour::PALETTE[self.project.groups.len() % GroupColour::PALETTE.len()];
        self.project
            .groups
            .push(Group::new(id, kind, format!("group {}", id.value()), colour).with_members(members.iter().copied()));
        self.project.next_group_number = id.value() + 1;
        if kind == GroupKind::Tag {
            for member in members {
                self.project.put_the_badge_of_a_tag_in_the_chain_of(*member, id);
            }
        }
        id
    }
}

/// A video of `frames` frames at 30 frames a second, which is the rate that a project that has no video follows.
pub(crate) fn video_source(identifier: u32, frames: u64) -> SourceMedia {
    SourceMedia::video(
        SourceMediaIdentifier::new(identifier),
        SourceMediaLink::from_absolute_path(format!("video {identifier}.mp4").into()),
        SourceVideoProperties {
            frame_rate: FrameRate::new(30, 1).expect("30 frames a second is a frame rate"),
            total_frame_count: frames,
            display_width_in_pixels: 320,
            display_height_in_pixels: 180,
            has_audio: false,
        },
    )
}

/// A project with a track for each of `layout`, the first in front, that has a text clip for each pair on it: where it starts, and how
/// many frames it lasts. (The text clips have no text effect: see the head of this file. [`lay_out_whole_texts`] makes them whole.)
pub(crate) fn lay_out(layout: &[&[(u64, u64)]]) -> Laid {
    lay_out_texts(layout, false)
}

/// [`lay_out`], with texts that are whole: each has the text effect that a text that the program makes has, as the first of its chain, saying what a new text says, so that it has what
/// it is called by (its words), makes a picture, and is a project that can be saved.
pub(crate) fn lay_out_whole_texts(layout: &[&[(u64, u64)]]) -> Laid {
    lay_out_texts(layout, true)
}

fn lay_out_texts(layout: &[&[(u64, u64)]], whole: bool) -> Laid {
    let mut project = Project::empty();
    while project.tracks().len() < layout.len() {
        project.add_track_in_front();
    }
    let tracks: Vec<TrackId> = project.tracks().iter().map(|track| track.id()).collect();
    let mut clips = Vec::new();
    for (track, placed) in tracks.iter().zip(layout) {
        let mut on_the_track = Vec::new();
        for &(first_frame, frames) in *placed {
            let first_frame = TimelineFrameIndex::new(first_frame);
            let (clip, lasts) = if whole {
                project.insert_text(*track, first_frame, TextObject::default(), frames)
            } else {
                project.insert_a_text_clip_that_has_no_text_effect(*track, first_frame, frames)
            }
            .expect("there is room for the clip");
            assert_eq!(lasts, frames, "the clip lasts as long as it was asked to");
            on_the_track.push(clip);
        }
        clips.push(on_the_track);
    }
    Laid { project, tracks, clips }
}
