// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The exact shape of a project file on disk (the current version, 12).
//!
//! These types are deliberately separate from the editing model in [`crate::project`]: the file
//! format has to stay readable by future versions, while the model is free to change. Converting
//! between the two is where files from the outside world get validated. (Files of earlier
//! versions have their own types, in [`super::legacy`].)

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::effects::{EffectDocument, PluginDocument};
use crate::colour_space::{StandardPrimaries, TransferFunction, VideoMatrix, VideoRange};
use crate::project::{
    ClipTailEdge, DisplayTransform, EdgeFade, FrameRate, Gain, GroupColour, GroupTailEdge, InputColourOverride,
    OutputColourSpace, ResourceStorage, SourceFrameIndex, SourceMediaIdentifier, SpedUpClipSound, SpeedFactor,
    TailDepth, TextColour, TrackHeight, TrackSharing, WorkingColourSpace,
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectDocument {
    /// Always [`super::FILE_FORMAT_NAME`]; lets us recognise (and refuse) other JSON files.
    pub file_format: String,
    pub file_format_version: u32,
    pub sources: Vec<SourceDocument>,
    /// A snapshot of the descriptor of each type of effect that the project uses. Left out when it uses none, and in the files of versions before
    /// 11, which say what an effect is by its name (see `super::effects`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<PluginDocument>,
    /// The files that the effects use (format 13 and after), each once. Left out when there are none, and in the files of versions before 13.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<ResourceDocument>,
    /// The libraries of native code that the sound effects come from (format 14 and after): linked, or with their builds inside the archive. Left out when there are none, and
    /// in the files of versions before 14.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugin_libraries: Vec<PluginLibraryDocument>,
    /// The tracks, the one in front first.
    pub tracks: Vec<TrackDocument>,
    /// The groups, in the order they were made in. A group is named in the rest of the file by its place in this list, counted from 0.
    /// Left out when there are none, and in the files of versions before 10.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<GroupDocument>,
    pub speed_indicator: SpeedIndicatorDocument,
    /// What the project has set for itself. Left out when it follows every global setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<ProjectSettingsDocument>,
    /// The frame rate the project was given. Left out while it follows its first video, and in the
    /// files of versions before 5 (in which it always did).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_rate: Option<FrameRate>,
}

/// A file that the effects of the project use, and where it is kept (see `super::resources`).
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResourceDocument {
    /// What the effects that use it call it: no two resources have one name.
    pub name: String,
    /// What kind of file it is: `"cube"`.
    pub kind: String,
    /// How many bytes it had when the project was saved, and the CRC-32 of them. Left out for a resource that was missing then (the program did not have its bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crc32: Option<u32>,
    /// The name of the entry of the project file (an archive) that has it, if it is included. The two paths are then where it was when it was included.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub included_as: Option<String>,
    pub absolute_path: String,
    /// Relative to the folder that has the project file. Left out when there is none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_path: Option<String>,
}

/// A library of plugins (see `super::plugin_libraries`): either `linked` (where its file is) or `embedded` (its builds, inside the archive), and one of the two.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PluginLibraryDocument {
    pub name: String,
    /// What kind of plugins it has in it: `"ladspa"`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked: Option<LinkedLibraryDocument>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedded: Option<EmbeddedLibraryDocument>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LinkedLibraryDocument {
    pub absolute_path: String,
    /// Relative to the folder that has the project file. Left out when there is none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EmbeddedLibraryDocument {
    pub builds: Vec<EmbeddedBuildDocument>,
}

/// One build of an embedded library, for one kind of computer.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EmbeddedBuildDocument {
    /// The operating system and the processor it is for: `windows-x86_64`.
    pub platform: String,
    /// The SHA-256 of its bytes (64 hexadecimal digits).
    pub sha256: String,
    /// How many bytes it has.
    pub size: u64,
    /// The name of the entry of the project file (an archive) that has it.
    pub entry: String,
}

/// The settings a project has set for itself. A setting that follows the global one is not in the
/// file at all, which is what "not saved" means for it.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectSettingsDocument {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autosave: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sped_up_clip_sound: Option<SpedUpClipSound>,
    /// What versions that could only mute or not mute wrote instead of `sped_up_clip_sound`. It is
    /// read (when `sped_up_clip_sound` is not there) and never written.
    #[serde(default, skip_serializing)]
    pub mute_sped_up_clips: Option<bool>,
    /// The colour settings (format 12 and after): what the screen does with the numbers, what the pictures are put together in, and what the exported video's numbers are.
    /// Not there in the files of earlier versions, and not there for a setting that follows the global one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_transform: Option<DisplayTransform>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_colour_space: Option<WorkingColourSpace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_colour_space: Option<OutputColourSpace>,
    /// How the files that effects use are kept when they are added (format 13 and after). Not there in the files of earlier versions, and not there for a setting that
    /// follows the global one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_storage: Option<ResourceStorage>,
    /// How long the fade is where a sound meets the edge of a group (format 20 and after). Not there in the files of earlier versions, and not there for a setting that
    /// follows the global one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_fade: Option<EdgeFade>,
}

/// A file the project is made from.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum SourceDocument {
    Video {
        identifier: SourceMediaIdentifier,
        /// Where the file was, or is: where it was when it was included in the project file, for a file that is.
        absolute_path: String,
        /// Relative to the folder that holds the project file. Always written with `/` separators so
        /// that it means the same thing on every operating system.
        relative_path: Option<String>,
        /// The name of the entry of the project file that has the file in it, if the file is included in the project file
        /// (which has to be a `.ffrs` file, an archive, to have files in it). Left out for a file that is not, and in the files
        /// of versions before 9.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        included_as: Option<String>,
        frame_rate: FrameRate,
        total_frame_count: u64,
        display_width_in_pixels: u32,
        display_height_in_pixels: u32,
        has_audio: bool,
        /// What the user says the colours of the video are, instead of what it says. Left out when they say nothing, and in the files of versions before 12.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        colour: Option<InputColourDocument>,
    },
    Audio {
        identifier: SourceMediaIdentifier,
        absolute_path: String,
        relative_path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        included_as: Option<String>,
        duration_in_microseconds: u64,
    },
    Image {
        identifier: SourceMediaIdentifier,
        absolute_path: String,
        relative_path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        included_as: Option<String>,
        display_width_in_pixels: u32,
        display_height_in_pixels: u32,
        /// What the user says the colours of the picture are, instead of sRGB. Left out when they say nothing, and in the files of versions before 12.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        colour: Option<InputColourDocument>,
    },
}

/// What the user says the colours of a source file are (format 12 and after). A part that is not there is automatic: what the file says, or what is assumed where it says
/// nothing.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InputColourDocument {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matrix: Option<VideoMatrix>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<VideoRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primaries: Option<StandardPrimaries>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve: Option<TransferFunction>,
}

impl From<&InputColourOverride> for InputColourDocument {
    fn from(said: &InputColourOverride) -> Self {
        Self {
            matrix: said.matrix,
            range: said.range,
            primaries: said.primaries,
            curve: said.curve,
        }
    }
}

impl From<InputColourDocument> for InputColourOverride {
    fn from(document: InputColourDocument) -> Self {
        Self {
            matrix: document.matrix,
            range: document.range,
            primaries: document.primaries,
            curve: document.curve,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TrackDocument {
    pub title: String,
    pub gain_decibels: Gain,
    /// How much faster than normal everything on the track plays, on top of each clip's own speed.
    pub speed_factor: SpeedFactor,
    pub muted: bool,
    /// How tall the row of the track is on the timeline. Left out for a track of the normal
    /// height, and in the files of versions before 4, which had no heights.
    #[serde(default = "normal_height_in_points", skip_serializing_if = "is_the_normal_height")]
    pub height_in_points: f32,
    /// The keyframes of the track's own properties (how loud it is, and whether it is silent), counted in frames from the
    /// start of the timeline, as the files of versions before 7 had them: a rack of them in the drawer of the track is what is
    /// written now. It is read, and never written.
    #[serde(default, skip_serializing)]
    pub animation: AnimationDocument,
    /// The racks of keyframes that are kept with the track, whatever they are the keyframes of, their frames counted from the
    /// start of the timeline. Left out for a track that has none, and in the files of versions before 7.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drawer: Vec<RackDocument>,
    /// The effects the track has, in the order they were put on in. Left out for a track that has none, and in the files of
    /// versions before 8.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<EffectDocument>,
    /// How much of the track one instance of its effects covers, for its picture effects and for its sound effects (format 20 and after). Left out for a track whose two are
    /// the default (one copy for each run of clips), and in the files of earlier versions.
    #[serde(default, skip_serializing_if = "SharingDocument::is_the_default")]
    pub sharing: SharingDocument,
    /// What the tail of a clip does at the edge of a group it is not in, for the clips where that is not the default (format 20 and after): the clip is named by its place in
    /// `clips` (counted from 0).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clip_tails: Vec<ClipTailDocument>,
    pub clips: Vec<ClipDocument>,
}

/// The sharing of a track, as the file has it: a part that is the default is left out.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SharingDocument {
    #[serde(default, skip_serializing_if = "is_the_default_sharing")]
    pub picture: TrackSharing,
    #[serde(default, skip_serializing_if = "is_the_default_sharing")]
    pub sound: TrackSharing,
}

fn is_the_default_sharing(sharing: &TrackSharing) -> bool {
    *sharing == TrackSharing::default()
}

impl SharingDocument {
    pub fn is_the_default(&self) -> bool {
        is_the_default_sharing(&self.picture) && is_the_default_sharing(&self.sound)
    }
}

/// What the tail of one clip does at the edge of a group it is not in (see [`TrackDocument::clip_tails`]): a part that is the default is left out.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ClipTailDocument {
    pub clip: usize,
    #[serde(default, skip_serializing_if = "is_the_default_clip_edge")]
    pub edge: ClipTailEdge,
    #[serde(default, skip_serializing_if = "is_the_default_depth")]
    pub depth: TailDepth,
}

fn is_the_default_clip_edge(edge: &ClipTailEdge) -> bool {
    *edge == ClipTailEdge::default()
}

fn is_the_default_depth(depth: &TailDepth) -> bool {
    *depth == TailDepth::default()
}

fn is_the_default_group_edge(edge: &GroupTailEdge) -> bool {
    *edge == GroupTailEdge::default()
}

/// What the tails of the members of a clip group or a track group do at its edge, as the file has it: a part that is the default is left out.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GroupTailDocument {
    #[serde(default, skip_serializing_if = "is_the_default_group_edge")]
    pub edge: GroupTailEdge,
    #[serde(default, skip_serializing_if = "is_the_default_depth")]
    pub depth: TailDepth,
}

impl GroupTailDocument {
    pub fn is_the_default(&self) -> bool {
        is_the_default_group_edge(&self.edge) && is_the_default_depth(&self.depth)
    }
}

fn normal_height_in_points() -> f32 {
    TrackHeight::DEFAULT_IN_POINTS
}

fn is_the_normal_height(points: &f32) -> bool {
    TrackHeight::from_points(*points).is_default()
}

/// One clip on a track, with how many frames of empty time come before it.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum ClipDocument {
    Video {
        #[serde(default, skip_serializing_if = "is_zero")]
        gap_before_frames: u64,
        source_identifier: SourceMediaIdentifier,
        first_source_frame: SourceFrameIndex,
        /// The frame just after the clip's last frame.
        end_source_frame: SourceFrameIndex,
        speed_factor: SpeedFactor,
        #[serde(default, skip_serializing_if = "is_no_gain")]
        gain_decibels: Gain,
        #[serde(default, skip_serializing_if = "is_false")]
        muted: bool,
        /// Where the video is on the picture, if it was moved, resized or turned. Left out for a
        /// video that is simply fitted inside the picture.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placement: Option<PlacementDocument>,
        /// The keyframes of what is animated, counted in frames from the clip's first frame, as the files of versions
        /// before 7 had them: a rack of them in the drawer of the clip is what is written now. It is read, and never written.
        #[serde(default, skip_serializing)]
        animation: AnimationDocument,
        /// The racks of keyframes that are kept with the clip, whatever they are the keyframes of, their frames counted from
        /// the clip's first frame. Left out for a clip that has none, and in the files of versions before 7.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        drawer: Vec<RackDocument>,
        /// The effects the video has, in the order they were put on in. Left out for a video that has none, and in the files
        /// of versions before 8.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        effects: Vec<EffectDocument>,
        /// The badges in the chain of the clip (see [`BadgeDocument`]). Left out for a clip that is in no tag, and has the badge of its
        /// track last, and in the files of versions before 10.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        badges: Vec<BadgeDocument>,
    },
    Audio {
        #[serde(default, skip_serializing_if = "is_zero")]
        gap_before_frames: u64,
        source_identifier: SourceMediaIdentifier,
        first_source_frame: SourceFrameIndex,
        end_source_frame: SourceFrameIndex,
        speed_factor: SpeedFactor,
        #[serde(default, skip_serializing_if = "is_no_gain")]
        gain_decibels: Gain,
        #[serde(default, skip_serializing_if = "is_false")]
        muted: bool,
        #[serde(default, skip_serializing)]
        animation: AnimationDocument,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        drawer: Vec<RackDocument>,
        /// The effects the sound clip has, in the order they were put on in: the effects of sound plugins. Left out for a clip that has none, and in the files written before
        /// they were written (they were put on in the window and not saved: found with a reverb on the first clip of a song that was gone when the project was loaded).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        effects: Vec<EffectDocument>,
        /// The badges in the chain of the clip (see [`BadgeDocument`]).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        badges: Vec<BadgeDocument>,
    },
    /// A MIDI clip (format 21, phase 13): a run of one track of a MIDI file, which is a resource of the project (`resources`, of the kind `midi`) that the clip names. A frame of its source is a stretch
    /// of the music as long as a frame of the project's video, counted from the start of the file. Its notes are played by the first synthesizer among its effects and those of what it is in.
    Midi {
        #[serde(default, skip_serializing_if = "is_zero")]
        gap_before_frames: u64,
        /// The name of the resource that has the file.
        resource: String,
        /// Which track of the file it plays, counted from 0.
        midi_track: u16,
        first_source_frame: SourceFrameIndex,
        /// The frame just after the clip's last frame.
        end_source_frame: SourceFrameIndex,
        speed_factor: SpeedFactor,
        #[serde(default, skip_serializing_if = "is_no_gain")]
        gain_decibels: Gain,
        #[serde(default, skip_serializing_if = "is_false")]
        muted: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        drawer: Vec<RackDocument>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        effects: Vec<EffectDocument>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        badges: Vec<BadgeDocument>,
    },
    Image {
        #[serde(default, skip_serializing_if = "is_zero")]
        gap_before_frames: u64,
        source_identifier: SourceMediaIdentifier,
        duration_in_frames: u64,
        /// Where the picture is on the screen, if it was moved, resized or turned. Left out for a
        /// picture that is simply fitted inside the picture.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placement: Option<PlacementDocument>,
        #[serde(default, skip_serializing)]
        animation: AnimationDocument,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        drawer: Vec<RackDocument>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        effects: Vec<EffectDocument>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        badges: Vec<BadgeDocument>,
    },
    /// A text (what it says, its font, its size and its colour are the values of its text effect since format 18: `content` names that effect, in the chain of `effects`, as a generator's
    /// `content` does). A file of an earlier version has no `content` and says all of that in the clip itself, in the four fields that are only read.
    Text {
        #[serde(default, skip_serializing_if = "is_zero")]
        gap_before_frames: u64,
        duration_in_frames: u64,
        /// The name, in the chain of `effects`, of the text effect that the clip is made of.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<u32>,
        /// What the text said, in a file of format 17 or earlier (it is the value `text` of the text effect since then). Never written.
        #[serde(default, skip_serializing)]
        text: Option<String>,
        /// Its font, in a file of format 17 or earlier: empty for the program's own default font. Never written.
        #[serde(default, skip_serializing)]
        font_family: Option<String>,
        /// Its size, in a file of format 17 or earlier. Never written.
        #[serde(default, skip_serializing)]
        font_size_fraction_of_frame_height: Option<f32>,
        /// Its colour, in a file of format 17 or earlier. Never written.
        #[serde(default, skip_serializing)]
        colour: Option<ColourDocument>,
        area: CanvasAreaDocument,
        /// How far the area is turned, clockwise. Left out for a text that is upright.
        #[serde(default, skip_serializing_if = "is_not_turned")]
        rotation_degrees: f32,
        #[serde(default, skip_serializing)]
        animation: AnimationDocument,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        drawer: Vec<RackDocument>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        effects: Vec<EffectDocument>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        badges: Vec<BadgeDocument>,
    },
    /// A keyframe crate: a stretch of the track that has racks of keyframes in its drawer and nothing else.
    Crate {
        #[serde(default, skip_serializing_if = "is_zero")]
        gap_before_frames: u64,
        duration_in_frames: u64,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        drawer: Vec<RackDocument>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        badges: Vec<BadgeDocument>,
    },
    /// A generator clip (format 16): no media; what it makes is its effect number `content`, a shader that reads no picture. It is somewhere on the picture, like a text object.
    Generator {
        #[serde(default, skip_serializing_if = "is_zero")]
        gap_before_frames: u64,
        duration_in_frames: u64,
        /// The name, in the chain of `effects`, of the effect that the clip makes its picture with.
        content: u32,
        area: CanvasAreaDocument,
        /// How far the area is turned, clockwise. Left out for one that is upright.
        #[serde(default, skip_serializing_if = "is_not_turned")]
        rotation_degrees: f32,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        drawer: Vec<RackDocument>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        effects: Vec<EffectDocument>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        badges: Vec<BadgeDocument>,
    },
    /// An adjustment clip (format 15): no picture of its own; its effects are done to everything behind it, for as long as it lasts.
    Adjustment {
        #[serde(default, skip_serializing_if = "is_zero")]
        gap_before_frames: u64,
        duration_in_frames: u64,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        drawer: Vec<RackDocument>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        effects: Vec<EffectDocument>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        badges: Vec<BadgeDocument>,
    },
}

/// A group, as the file has it: its kind, its name and colour, what is in it, its effects, and where the badges of the tags that it is in
/// are in its chain of effects.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GroupDocument {
    pub kind: GroupKindDocument,
    pub name: String,
    pub colour: GroupColourDocument,
    /// What is in the group, in the order it was put in.
    pub members: Vec<MemberDocument>,
    /// The effects the group has, in the order they are applied in. Left out for a group that has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<EffectDocument>,
    /// The badges in the chain of the group: those of the tags it is in. Left out for a group that is in no tag.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub badges: Vec<BadgeDocument>,
    /// What the tails of its members do at its edge (format 20 and after). Left out for a tag, and for a group where that is the default.
    #[serde(default, skip_serializing_if = "GroupTailDocument::is_the_default")]
    pub tail: GroupTailDocument,
}

/// The kinds of group, as the file names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum GroupKindDocument {
    Tag,
    ClipGroup,
    TrackGroup,
}

/// What is in a group: a clip or a track, by where it is in the file (the number of its track in the list of tracks and, for a clip, the number
/// of the clip in the list of its track's clips, both counted from 0, as the racks name what they are of), or a group, by its place in the list
/// of groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum MemberDocument {
    Clip { track: usize, clip: usize },
    Track(usize),
    Group(usize),
}

/// Where the effects of something else are applied in a chain of effects: the badge of a tag, or the badge of the track that a clip is on,
/// and how many of the effects of the chain come before it. Of the badges that are in the same place (with the same number of effects before
/// them) the first in the list is the first in the chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BadgeDocument {
    pub of: BadgeOfDocument,
    /// How many of the effects of the chain come before the badge. Left out when none do.
    #[serde(default, skip_serializing_if = "is_zero_count")]
    pub effects_before: usize,
}

/// What a badge is the badge of: a tag, by its place in the list of groups, or the track of the clip that has the chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum BadgeOfDocument {
    Tag(usize),
    Track,
}

fn is_zero_count(number: &usize) -> bool {
    *number == 0
}

/// The colour of a group, written as `#RRGGBB`: two hexadecimal digits each for red, green and blue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct GroupColourDocument(pub GroupColour);

impl GroupColourDocument {
    pub(super) fn to_text(self) -> String {
        let GroupColour { red, green, blue } = self.0;
        format!("#{red:02X}{green:02X}{blue:02X}")
    }

    pub(super) fn from_text(text: &str) -> Result<Self, String> {
        let digits = text
            .strip_prefix('#')
            .filter(|digits| digits.len() == 6 && digits.chars().all(|digit| digit.is_ascii_hexdigit()))
            .ok_or_else(|| format!("\"{text}\" is not a colour; expected \"#RRGGBB\", such as \"#FF0000\" for red"))?;
        let part = |position: usize| {
            u8::from_str_radix(&digits[position * 2..position * 2 + 2], 16).expect("the digits were checked")
        };
        Ok(Self(GroupColour::new(part(0), part(1), part(2))))
    }
}

impl Serialize for GroupColourDocument {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_text())
    }
}

impl<'de> Deserialize<'de> for GroupColourDocument {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::from_text(&text).map_err(serde::de::Error::custom)
    }
}

/// The keyframes of one property that are kept together, in a drawer.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RackDocument {
    /// The name of the property (see [`crate::project::PropertyId::name_in_a_file`]): what a clip or a track has of its own is called by its name,
    /// and the value of an effect by the id that the effect gives it (see [`crate::project::ParameterDescriptor::id`]).
    pub property: String,
    /// For a rack of the keyframes of a value of an effect: the name of that effect in the chain of the clip or the track that the keyframes
    /// are of (see [`super::effects::EffectDocument::instance`]), which says which of its effects the value is of when it has several of one kind. Left out
    /// for the properties that a clip or a track has itself, and in the files of versions before 11, in which it is the one effect that has
    /// a value of that name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<u32>,
    /// The clip or the track that the keyframes are of. Left out when it is the one whose drawer the rack is in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<TargetDocument>,
    /// In the order of their frames, which are counted from the start of what the drawer belongs to (a clip's first frame, or the
    /// start of the timeline for a track).
    pub keyframes: Vec<KeyframeDocument>,
}

/// A clip or a track, by where it is in the file: the number of its track in the list of tracks, and for a clip the number of
/// the clip in the list of that track's clips, both counted from 0.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TargetDocument {
    pub track: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<usize>,
}

/// The keyframes of the properties that a clip or a track has animated, by the name of the property (see
/// [`crate::project::PropertyId::name`]), each property's in the order of their frames. A property that is not animated is
/// not in it.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub(super) struct AnimationDocument(pub BTreeMap<String, Vec<KeyframeDocument>>);

/// What a property is at a frame.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct KeyframeDocument {
    /// How many frames after the first frame of the clip (or after the start of the timeline, for a track).
    pub frame: u64,
    pub value: KeyframeValueDocument,
}

/// The value of a keyframe, which is the kind of value that the property has: a number (how loud, in decibels; how far turned,
/// in degrees; how big the letters are), two numbers (a place, a size), yes or no, or text (words, the name of a font, a
/// colour as `#RRGGBBAA`). Which of them it has to be is told by the property, and checked when the file is loaded.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum KeyframeValueDocument {
    Flag(bool),
    Number(f32),
    Pair([f32; 2]),
    Text(String),
}

fn is_zero(number: &u64) -> bool {
    *number == 0
}

fn is_no_gain(gain: &Gain) -> bool {
    gain.is_unchanged()
}

fn is_false(flag: &bool) -> bool {
    !*flag
}

fn is_not_turned(degrees: &f32) -> bool {
    *degrees == 0.0
}

/// Where something is on the picture: a rectangle, and how far it is turned about its middle.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlacementDocument {
    pub area: CanvasAreaDocument,
    /// How far the area is turned, clockwise. Left out when it is not.
    #[serde(default, skip_serializing_if = "is_not_turned")]
    pub rotation_degrees: f32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CanvasAreaDocument {
    pub left_fraction_of_frame_width: f32,
    pub top_fraction_of_frame_height: f32,
    pub width_fraction_of_frame_width: f32,
    pub height_fraction_of_frame_height: f32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SpeedIndicatorDocument {
    pub left_fraction_of_frame_width: f32,
    pub top_fraction_of_frame_height: f32,
    pub height_fraction_of_frame_height: f32,
}

/// A colour, written as `#RRGGBBAA`: two hexadecimal digits each for red, green, blue and opacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ColourDocument(pub TextColour);

impl ColourDocument {
    pub(super) fn to_text(self) -> String {
        let TextColour {
            red,
            green,
            blue,
            alpha,
        } = self.0;
        format!("#{red:02X}{green:02X}{blue:02X}{alpha:02X}")
    }

    pub(super) fn from_text(text: &str) -> Result<Self, String> {
        let digits = text
            .strip_prefix('#')
            .filter(|digits| digits.len() == 8 && digits.chars().all(|digit| digit.is_ascii_hexdigit()))
            .ok_or_else(|| {
                format!("\"{text}\" is not a colour; expected \"#RRGGBBAA\", such as \"#FFFFFFFF\" for white")
            })?;
        let part = |position: usize| {
            u8::from_str_radix(&digits[position * 2..position * 2 + 2], 16).expect("the digits were checked")
        };
        Ok(Self(TextColour {
            red: part(0),
            green: part(1),
            blue: part(2),
            alpha: part(3),
        }))
    }
}

impl Serialize for ColourDocument {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_text())
    }
}

impl<'de> Deserialize<'de> for ColourDocument {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::from_text(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colour_is_written_as_hexadecimal_red_green_blue_and_opacity() {
        assert_eq!(ColourDocument(TextColour::WHITE).to_text(), "#FFFFFFFF");
        assert_eq!(ColourDocument(TextColour::new(1, 2, 255, 0)).to_text(), "#0102FF00");
        assert_eq!(ColourDocument(TextColour::new(0, 0, 0, 128)).to_text(), "#00000080");
    }

    #[test]
    fn a_colour_comes_back_exactly_in_either_case() {
        for colour in [
            TextColour::WHITE,
            TextColour::new(0, 0, 0, 0),
            TextColour::new(18, 52, 86, 120),
            TextColour::new(255, 128, 1, 254),
        ] {
            let text = ColourDocument(colour).to_text();
            assert_eq!(
                ColourDocument::from_text(&text).unwrap(),
                ColourDocument(colour),
                "{text}"
            );
            assert_eq!(
                ColourDocument::from_text(&text.to_lowercase()).unwrap(),
                ColourDocument(colour)
            );
        }
    }

    #[test]
    fn text_that_is_not_a_colour_is_refused_with_what_was_expected() {
        for bad in [
            "",
            "FFFFFFFF",
            "#FFFFFF",
            "#FFFFFFFFF",
            "#GGGGGGGG",
            "#FFFFFF 0",
            "white",
            "#+FFFFFFF",
        ] {
            let reason = ColourDocument::from_text(bad).unwrap_err();
            assert!(reason.contains("#RRGGBBAA"), "{bad:?}: {reason}");
        }
    }
}
