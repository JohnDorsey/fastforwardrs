// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Everything the user can ask for. The parts of the screen only describe what was asked (by
//! pushing a [`Command`]); one place afterwards decides what to do about each, so that no drawing
//! code ever has to change the project while it is still being drawn from.

use super::history::EditKey;
use super::selection::{SelectionChange, SelectionItem};
use crate::project::{
    AnimationTarget, ChainItemKey, ClipGroupBeforeTheDrag, ClipId, ClipMove, ClipSide, EffectInstanceId, EffectKind,
    FrameRate, Gain, GroupColour, GroupId, GroupKind, InputColourOverride, KeyframeId, KeyframeMove, Member, Placement,
    ProjectSettings, PropertyId, PropertyValue, RackDestination, RackId, Settings, SourceMediaIdentifier, SpeedFactor,
    SpeedIndicatorLayout, TextObject, TimeDirection, TimelineFrameIndex, TrackHeight, TrackId, TrackSide,
};

/// Something the user has asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    // The File menu.
    LoadProject,
    SaveProject,
    /// Save the project to a file chosen now, which is the project's file from then on.
    SaveProjectAs,
    NewProject,
    /// Choose a video, a sound or a picture and put all of it on the timeline, at the current frame, and put it in the project file
    /// too, so that the project file holds it (the project file has to be a `.ffrs` file: if it is not one, the user is asked to
    /// save it as one first).
    EmbedAndImportFile,
    /// Choose a video, a sound or a picture and put all of it on the timeline, at the current frame, where it stays the file that it
    /// is: the project remembers where it is, and does not hold it.
    LinkAndImportFile,
    ExportVideo,

    // The Edit menu.
    /// Go back to the project as it was before the latest edit.
    Undo,
    /// Do again the edit that was last undone.
    Redo,
    /// Select no clip.
    Deselect,
    ShowProjectSettings,
    ShowGlobalSettings,

    /// The project's own settings were changed in the project settings window.
    SetProjectSettings(ProjectSettings),
    /// A frame rate was chosen for the project in the project settings window: everything on the
    /// timeline is counted again at it, and lasts as long as it did.
    SetProjectFrameRate(FrameRate),
    /// The global settings were changed in the global settings window.
    SetGlobalSettings(Settings),

    // The play buttons.
    Pause,
    Play,
    Stop,

    /// Move the current frame to a place (by dragging the slider along the bottom, or clicking
    /// the ruler above the tracks).
    SeekTo(TimelineFrameIndex),

    /// Move the current frame this many frames on (or, if negative, back), with the arrow keys.
    MoveCurrentFrameBy(i64),

    // How the window shows things (nothing about the project changes).
    /// Make the picture of the preview fit its panel, as the button that does so does (with the F key while the pointer is over
    /// the preview).
    FitThePreview,
    /// Scroll and zoom the timeline so that what is selected (clips, keyframes and racks of keyframes) is in the middle of it and
    /// nearly fills its width (with the F key anywhere else). When nothing is selected nothing happens.
    FrameTheSelectionInTheTimeline,
    /// Go back to the view the timeline had before the latest thing that was done to it (zooming it, scrolling it, framing what is
    /// selected), as the Back button of a mouse does. (When it has no earlier view, nothing happens.)
    GoBackInTheTimelineViewHistory,
    /// Go forward again to the view the timeline was gone back from, as the Forward button of a mouse does. (When there is none,
    /// nothing happens.)
    GoForwardInTheTimelineViewHistory,

    /// Change what is selected (by clicking something, or drawing a rectangle round some things): select `items`
    /// and nothing else, or as well as what is selected, or the other way round for each of them, as `change` says.
    /// (A track is selected as itself, which is not all the clips on it.)
    SelectItems {
        items: Vec<SelectionItem>,
        change: SelectionChange,
    },

    /// Cut the clips under this frame in two: only those in `only` if that is given (with the S key, which cuts the selected
    /// clips there), and otherwise every clip there is under the frame, on every track (with Shift and the S key).
    SplitAt {
        frame: TimelineFrameIndex,
        only: Option<Vec<ClipId>>,
    },

    /// Take clips off their tracks (with the Delete key).
    DeleteClips(Vec<ClipId>),

    /// Put a copy of each of these clips right beside it, on this side of it, with a copy of everything in its drawer (from
    /// the menu of a clip), and select the copies. A clip with no room beside it is not copied.
    DuplicateClips {
        clips: Vec<ClipId>,
        side: ClipSide,
    },

    /// Move clips, by dragging them: each to the track and the frame given for it. `before_the_drag` says which clip group each of them was in
    /// when the drag that this is a move of began (see [`crate::project::Project::move_clips_remembering`], which is what a clip that goes into the
    /// rectangle of a clip group joins it by, and leaves it by when it goes out again): empty for a move that is not of a drag.
    MoveClips {
        moves: Vec<ClipMove>,
        before_the_drag: Vec<ClipGroupBeforeTheDrag>,
    },

    // The settings of a clip, from the box that opens above it.
    SetClipSpeed {
        clip: ClipId,
        speed_factor: SpeedFactor,
    },
    SetClipGain {
        clip: ClipId,
        gain: Gain,
    },
    SetClipMuted {
        clip: ClipId,
        muted: bool,
    },
    /// A video or text clip was moved, resized or turned on the preview.
    SetClipPlacement {
        clip: ClipId,
        placement: Placement,
    },
    /// Choose again the file a clip is made from, because it cannot be found.
    RelinkSource(SourceMediaIdentifier),
    /// Put the files of these source files in the project file, from the menu of a clip. Only a project file that is a `.ffrs` file
    /// can have files in it: if it is not one, the user is asked to save it as one first. (It is not an edit that can be undone,
    /// as choosing a file again is not.)
    IncludeFilesInTheProjectFile(Vec<SourceMediaIdentifier>),
    /// Take the files of these source files out of the project file again, from the menu of a clip: each is put back where it was
    /// when it was included, if that can be done.
    TakeFilesOutOfTheProjectFile(Vec<SourceMediaIdentifier>),
    /// Put the tables of colours called so in the project file, from the menu of the box of a look-up table effect (RS-04). Like the files of videos, it needs a `.ffrs` file
    /// and is not an edit that can be undone.
    IncludeTablesInTheProjectFile(Vec<String>),
    /// Take the tables of colours called so out of the project file again, each put back where it was when it was included, if that can be done.
    TakeTablesOutOfTheProjectFile(Vec<String>),

    // The tracks.
    /// Add an empty track in front of the others.
    AddTrack,
    SetTrackTitle {
        track: TrackId,
        title: String,
    },
    SetTrackSpeed {
        track: TrackId,
        speed_factor: SpeedFactor,
    },
    SetTrackGain {
        track: TrackId,
        gain: Gain,
    },
    SetTrackMuted {
        track: TrackId,
        muted: bool,
    },
    /// Take a track, and everything on it, out of the project (from the menu of its title box).
    DeleteTrack(TrackId),
    /// Put a copy of a track, with all its clips and everything in all its drawers, right in front of it or right behind it
    /// (from the menu of its title box), and select the copy.
    DuplicateTrack {
        track: TrackId,
        side: TrackSide,
    },
    /// A track was moved to another place in the list of tracks (0 is the front), by dragging its title box
    /// and letting go of it there.
    MoveTrack {
        track: TrackId,
        to_position: usize,
    },
    /// The row of a track was made taller or shorter, by dragging the boundary under its title box.
    SetTrackHeight {
        track: TrackId,
        height: TrackHeight,
    },

    // Text objects.
    /// Put a new text object on a track, starting at a frame, and select it, so that its properties show.
    NewTextObject {
        track: TrackId,
        frame: TimelineFrameIndex,
    },
    /// A text object was changed: what it says, how it looks, or where it is on the picture.
    SetTextObject {
        clip: ClipId,
        object: TextObject,
    },
    /// A text clip or a picture was made to last this many frames.
    SetClipDuration {
        clip: ClipId,
        frames: u64,
    },

    // Effects.
    /// Put an effect on a clip or a track (with the plus button of the properties panel), with the values it begins with. (It may have
    /// effects of that kind already.)
    AddEffect {
        target: AnimationTarget,
        kind: EffectKind,
    },
    /// Delete an effect of a clip or a track (with *Delete effect*, in the menu of its box in the properties panel), with the
    /// keyframes of its values: the one that is named `instance` in its chain, and no other of its kind.
    RemoveEffect {
        target: AnimationTarget,
        instance: EffectInstanceId,
    },
    /// Switch an effect of a clip, a track or a group off (bypass it) or on (from the menu of its box): the one that is named `instance` in its chain. It stays
    /// where it is with its values and keyframes.
    SetEffectEnabled {
        owner: Member,
        instance: EffectInstanceId,
        enabled: bool,
    },
    /// Put a copy of an effect of a clip, a track or a group right after it (from the menu of its box): the one that is named `instance` in its chain, with its values (and not
    /// its keyframes).
    DuplicateEffect {
        owner: Member,
        instance: EffectInstanceId,
    },
    /// Choose a table of colours (a `.cube` file) and put an effect on `owner` that looks the colours up in it (from the menu of the plus button). What is done with the file
    /// is asked, if the settings say so (see [`Self::UseATable`]).
    AddEffectFromFile {
        owner: Member,
    },
    /// Choose a table of colours (a `.cube` file) for the effect `instance` of `owner`, which looks the colours up in a table (from the button of its table).
    ChooseTableFile {
        owner: Member,
        instance: EffectInstanceId,
    },
    /// Add the file `bytes` as a resource called `wanted_name` of the project (kept as `link` says: embedded or linked), and use it: for the effect `instance` of `owner`
    /// that looks colours up in a table, or, if that is `None`, for a new such effect put on `owner`. (What the two commands above come to when the file has been chosen
    /// and read, and the user has said how to keep it if they were asked.)
    UseATable {
        owner: Member,
        instance: Option<EffectInstanceId>,
        wanted_name: String,
        bytes: std::sync::Arc<[u8]>,
        link: crate::project::SourceMediaLink,
    },
    /// Give a value of an effect of a clip or a track (a property of it) a value: at the playhead if it has keyframes, as for any
    /// property.
    SetEffectValue {
        target: AnimationTarget,
        property: PropertyId,
        value: PropertyValue,
    },

    // Groups.
    /// Make a group of kind `kind` of `members` (from the menu of a selection of clips, or of tracks), and select it and nothing else.
    /// A group that cannot be made (see [`crate::project::Project::create_group`]) is not, and the user is told why.
    CreateGroup {
        kind: GroupKind,
        members: Vec<Member>,
    },
    /// Put these in a group that is there already (from the menu of a selection of clips), last.
    AddToGroup {
        group: GroupId,
        members: Vec<Member>,
    },
    /// Take something out of a group, which it stays in the project (from the menu of an item in the list of what is in the group).
    RemoveFromGroup {
        group: GroupId,
        member: Member,
    },
    /// Move the left or the right edge of a clip group, by dragging it on the timeline: the group has the clips that are inside these frames
    /// (from the first to the end, which is not in them) when it is done (see [`crate::project::Project::set_the_frames_of_the_clip_group`]).
    SetTheFramesOfAClipGroup {
        group: GroupId,
        first_frame: u64,
        end_frame: u64,
    },
    /// Take a group apart, with *Disband* in the menu of its title bar, and with the Delete key when it is selected: what was in it is in
    /// the project still (see [`crate::project::Project::disband_group`]).
    DisbandGroup(GroupId),
    /// Delete a group and all that is in it, with *Delete all* in the menu of its title bar.
    DeleteGroupAndEverythingInIt(GroupId),
    /// Name a group, which is typed in the title bar of its box.
    RenameGroup {
        group: GroupId,
        name: String,
    },
    /// Give a group another colour, with the colour control of the title bar of its box.
    SetGroupColour {
        group: GroupId,
        colour: GroupColour,
    },
    /// Put an effect on a group (with the plus button of its box), with the values it begins with.
    AddGroupEffect {
        group: GroupId,
        kind: EffectKind,
    },
    /// Take an effect off a group: the one that is named `instance` in its chain.
    RemoveGroupEffect {
        group: GroupId,
        instance: EffectInstanceId,
    },
    /// Give a value of an effect of a group a value. (It is not animated.)
    SetGroupEffectValue {
        group: GroupId,
        property: PropertyId,
        value: PropertyValue,
    },
    /// Move something in the chain of effects of a clip, a track or a group (by dragging it in the properties panel): to right before
    /// `before`, or last (after all that is shown of the chain) if that is `None`.
    MoveChainItem {
        owner: Member,
        item: ChainItemKey,
        before: Option<ChainItemKey>,
    },

    // Keyframes.
    /// Put a keyframe on a property of a clip or a track at a frame (with the stopwatch), with the value the property
    /// has there now.
    AddKeyframe {
        target: AnimationTarget,
        property: PropertyId,
        frame: TimelineFrameIndex,
    },
    /// Take away the keyframe of a property of a clip or a track at a frame (with the stopwatch).
    RemoveKeyframe {
        target: AnimationTarget,
        property: PropertyId,
        frame: TimelineFrameIndex,
    },
    /// Move keyframes, by dragging them: each to the frame given for it, counted from the start of the drawer its rack
    /// is in.
    MoveKeyframes(Vec<KeyframeMove>),
    /// Take keyframes away, whichever racks they are in (with the Delete key).
    DeleteKeyframes(Vec<KeyframeId>),
    /// Make a copy of each of the keyframes in the rack it is in, after them or before them, as far from them as the first of
    /// them is from the last and then `gap_in_frames` more (see [`crate::project::Project::duplicate_keyframes`]). The copies are
    /// selected, so that duplicating again goes on from them.
    DuplicateKeyframes {
        keyframes: Vec<KeyframeId>,
        towards: TimeDirection,
        gap_in_frames: u64,
    },
    /// Take racks away with the keyframes in them (with the Delete key).
    DeleteRacks(Vec<RackId>),
    /// Make one rack of racks of one property of one thing, which has the keyframes of all of them (see
    /// [`crate::project::Project::merge_racks`]).
    MergeRacks(Vec<RackId>),
    /// Move racks of keyframes to another drawer, or onto an empty stretch of a track (where they are put in one keyframe
    /// crate made for them), by dragging them, with all their keyframes this many frames later in time (earlier, if negative). The
    /// first has `slot` racks above it in the drawer they go to if that is said (not counting themselves), and the others follow it,
    /// and otherwise each keeps its place there, or goes under the others if it is not in that drawer yet.
    MoveRacks {
        racks: Vec<RackId>,
        to: RackDestination,
        shift_in_frames: i64,
        slot: Option<usize>,
    },

    /// Move or resize the fast-forward symbol.
    SetSpeedIndicatorLayout(SpeedIndicatorLayout),

    /// Open the window that says what the colours of a source file are and lets the user say them differently (from the menu of a clip made from the file).
    ShowTheColoursOfAFile(SourceMediaIdentifier),
    /// Say what the colours of a source file are, instead of what it says (every part automatic, to say nothing): the pictures that are made of it are made again.
    SetTheColoursOfAFile {
        source: SourceMediaIdentifier,
        colour: InputColourOverride,
    },
}

impl Command {
    /// The command that moves clips as asked (and is not a move of a drag: the clip groups they were in are not told).
    #[cfg(test)]
    pub fn move_clips(moves: Vec<ClipMove>) -> Self {
        Self::MoveClips {
            moves,
            before_the_drag: Vec::new(),
        }
    }

    /// The command that changes what is selected by these clips, in the way `change` says.
    pub fn select_clips(clips: impl IntoIterator<Item = ClipId>, change: SelectionChange) -> Self {
        Self::SelectItems {
            items: clips.into_iter().map(SelectionItem::Clip).collect(),
            change,
        }
    }

    /// The command that changes what is selected by this track (by clicking its title box), in the way `change` says.
    pub fn select_track(track: TrackId, change: SelectionChange) -> Self {
        Self::SelectItems {
            items: vec![SelectionItem::Track(track)],
            change,
        }
    }

    /// Whether doing this can change the project, so that it may be an edit that can be undone.
    /// (Everything else leaves the project alone, or changes what is not part of what is edited:
    /// the settings, the selection, which frame is current.) Written out for every command, with
    /// no catch-all, so that a command that is added has to be put on one side or the other.
    pub fn may_change_the_project(&self) -> bool {
        match self {
            Self::SplitAt { .. }
            | Self::DeleteClips(_)
            | Self::DuplicateClips { .. }
            | Self::MoveClips { .. }
            | Self::SetClipSpeed { .. }
            | Self::SetClipGain { .. }
            | Self::SetClipMuted { .. }
            | Self::SetClipPlacement { .. }
            | Self::AddTrack
            | Self::DeleteTrack(_)
            | Self::DuplicateTrack { .. }
            | Self::MoveTrack { .. }
            | Self::SetTrackTitle { .. }
            | Self::SetTrackSpeed { .. }
            | Self::SetTrackGain { .. }
            | Self::SetTrackMuted { .. }
            | Self::SetTrackHeight { .. }
            | Self::NewTextObject { .. }
            | Self::SetTextObject { .. }
            | Self::SetClipDuration { .. }
            | Self::AddEffect { .. }
            | Self::RemoveEffect { .. }
            | Self::SetEffectEnabled { .. }
            | Self::DuplicateEffect { .. }
            | Self::UseATable { .. }
            | Self::SetEffectValue { .. }
            | Self::CreateGroup { .. }
            | Self::AddToGroup { .. }
            | Self::RemoveFromGroup { .. }
            | Self::SetTheFramesOfAClipGroup { .. }
            | Self::DisbandGroup(_)
            | Self::DeleteGroupAndEverythingInIt(_)
            | Self::RenameGroup { .. }
            | Self::SetGroupColour { .. }
            | Self::AddGroupEffect { .. }
            | Self::RemoveGroupEffect { .. }
            | Self::SetGroupEffectValue { .. }
            | Self::MoveChainItem { .. }
            | Self::AddKeyframe { .. }
            | Self::RemoveKeyframe { .. }
            | Self::MoveKeyframes(_)
            | Self::DeleteKeyframes(_)
            | Self::DuplicateKeyframes { .. }
            | Self::DeleteRacks(_)
            | Self::MergeRacks(_)
            | Self::MoveRacks { .. }
            | Self::SetProjectFrameRate(_)
            | Self::SetSpeedIndicatorLayout(_)
            | Self::SetTheColoursOfAFile { .. } => true,

            Self::LoadProject
            | Self::SaveProject
            | Self::SaveProjectAs
            | Self::NewProject
            | Self::EmbedAndImportFile
            | Self::LinkAndImportFile
            | Self::ExportVideo
            | Self::Undo
            | Self::Redo
            | Self::Deselect
            | Self::ShowProjectSettings
            | Self::ShowGlobalSettings
            | Self::SetProjectSettings(_)
            | Self::SetGlobalSettings(_)
            | Self::Pause
            | Self::Play
            | Self::Stop
            | Self::SeekTo(_)
            | Self::MoveCurrentFrameBy(_)
            | Self::FitThePreview
            | Self::FrameTheSelectionInTheTimeline
            | Self::GoBackInTheTimelineViewHistory
            | Self::GoForwardInTheTimelineViewHistory
            | Self::SelectItems { .. }
            | Self::RelinkSource(_)
            | Self::IncludeFilesInTheProjectFile(_)
            | Self::TakeFilesOutOfTheProjectFile(_)
            | Self::IncludeTablesInTheProjectFile(_)
            | Self::TakeTablesOutOfTheProjectFile(_)
            | Self::AddEffectFromFile { .. }
            | Self::ChooseTableFile { .. }
            | Self::ShowTheColoursOfAFile(_) => false,
        }
    }

    /// What the user is told this command is, in the question that is asked before it is carried out if it would lose keyframes,
    /// if it is one that changes how long something lasts (a speed, how long a clip lasts, the frame rate of the project, the
    /// track a clip is on). Such a command counts the keyframes of what lasts less time again, and there may not be room for all
    /// of them (see [`crate::project::Rack::rescaled`]). `None` for every other command. Written out for every command, with no
    /// catch-all, so that a command that is added has to be put on one side or the other.
    pub fn name_in_a_question_about_losing_keyframes(&self) -> Option<&'static str> {
        match self {
            Self::SetClipSpeed { .. } => Some("Playing this clip at that speed"),
            Self::SetTrackSpeed { .. } => Some("Playing this track at that speed"),
            Self::SetClipDuration { .. } => Some("Making this clip last that long"),
            Self::SetProjectFrameRate(_) => Some("Counting everything again at that frame rate"),
            Self::MoveClips { .. } => Some("Putting the clips there"),

            Self::LoadProject
            | Self::SaveProject
            | Self::SaveProjectAs
            | Self::NewProject
            | Self::EmbedAndImportFile
            | Self::LinkAndImportFile
            | Self::ExportVideo
            | Self::Undo
            | Self::Redo
            | Self::Deselect
            | Self::ShowProjectSettings
            | Self::ShowGlobalSettings
            | Self::SetProjectSettings(_)
            | Self::SetGlobalSettings(_)
            | Self::Pause
            | Self::Play
            | Self::Stop
            | Self::SeekTo(_)
            | Self::MoveCurrentFrameBy(_)
            | Self::FitThePreview
            | Self::FrameTheSelectionInTheTimeline
            | Self::GoBackInTheTimelineViewHistory
            | Self::GoForwardInTheTimelineViewHistory
            | Self::SelectItems { .. }
            | Self::SplitAt { .. }
            | Self::DeleteClips(_)
            | Self::DuplicateClips { .. }
            | Self::SetClipGain { .. }
            | Self::SetClipMuted { .. }
            | Self::SetClipPlacement { .. }
            | Self::RelinkSource(_)
            | Self::IncludeFilesInTheProjectFile(_)
            | Self::TakeFilesOutOfTheProjectFile(_)
            | Self::IncludeTablesInTheProjectFile(_)
            | Self::TakeTablesOutOfTheProjectFile(_)
            | Self::AddTrack
            | Self::SetTrackTitle { .. }
            | Self::SetTrackGain { .. }
            | Self::SetTrackMuted { .. }
            | Self::DeleteTrack(_)
            | Self::DuplicateTrack { .. }
            | Self::MoveTrack { .. }
            | Self::SetTrackHeight { .. }
            | Self::NewTextObject { .. }
            | Self::SetTextObject { .. }
            | Self::AddEffect { .. }
            | Self::RemoveEffect { .. }
            | Self::SetEffectEnabled { .. }
            | Self::DuplicateEffect { .. }
            | Self::UseATable { .. }
            | Self::SetEffectValue { .. }
            | Self::CreateGroup { .. }
            | Self::AddToGroup { .. }
            | Self::RemoveFromGroup { .. }
            | Self::SetTheFramesOfAClipGroup { .. }
            | Self::DisbandGroup(_)
            | Self::DeleteGroupAndEverythingInIt(_)
            | Self::RenameGroup { .. }
            | Self::SetGroupColour { .. }
            | Self::AddGroupEffect { .. }
            | Self::RemoveGroupEffect { .. }
            | Self::SetGroupEffectValue { .. }
            | Self::MoveChainItem { .. }
            | Self::AddKeyframe { .. }
            | Self::RemoveKeyframe { .. }
            | Self::MoveKeyframes(_)
            | Self::DeleteKeyframes(_)
            | Self::DuplicateKeyframes { .. }
            | Self::DeleteRacks(_)
            | Self::MergeRacks(_)
            | Self::MoveRacks { .. }
            | Self::SetSpeedIndicatorLayout(_)
            | Self::AddEffectFromFile { .. }
            | Self::ChooseTableFile { .. }
            | Self::ShowTheColoursOfAFile(_)
            | Self::SetTheColoursOfAFile { .. } => None,
        }
    }

    /// Whether carrying this command out may lose keyframes, because it changes how long something lasts: the user is asked first
    /// if it would (see [`Self::name_in_a_question_about_losing_keyframes`]).
    pub fn may_lose_keyframes(&self) -> bool {
        self.name_in_a_question_about_losing_keyframes().is_some()
    }

    /// What this command is about, if it is the kind of edit that a gesture makes over and over (a
    /// slider dragged, a title typed): edits about the same thing, one after another, are one step
    /// of the history. `None` for an edit that is to be a step of its own, and for what is no edit.
    pub fn gesture_key(&self) -> Option<EditKey> {
        match self {
            Self::MoveClips { moves, .. } => {
                let mut clips: Vec<ClipId> = moves.iter().map(|moved| moved.clip).collect();
                clips.sort();
                Some(EditKey::about(&("the place of", clips)))
            }
            Self::SetClipSpeed { clip, .. } => Some(EditKey::about(&("the speed of", clip))),
            Self::SetClipGain { clip, .. } => Some(EditKey::about(&("the gain of", clip))),
            Self::SetClipPlacement { clip, .. } => Some(EditKey::about(&("the place on the picture of", clip))),
            Self::SetTrackTitle { track, .. } => Some(EditKey::about(&("the title of", track))),
            Self::SetTrackSpeed { track, .. } => Some(EditKey::about(&("the speed of", track))),
            Self::SetTrackGain { track, .. } => Some(EditKey::about(&("the gain of", track))),
            Self::SetTrackHeight { track, .. } => Some(EditKey::about(&("the height of", track))),
            Self::SetTextObject { clip, .. } => Some(EditKey::about(&("the text object of", clip))),
            Self::SetClipDuration { clip, .. } => Some(EditKey::about(&("how long it lasts of", clip))),
            Self::SetEffectValue { target, property, .. } => {
                Some(EditKey::about(&("the value of the effect", target, property)))
            }
            Self::RenameGroup { group, .. } => Some(EditKey::about(&("the name of", group))),
            Self::SetGroupColour { group, .. } => Some(EditKey::about(&("the colour of", group))),
            Self::SetGroupEffectValue { group, property, .. } => Some(EditKey::about(&(
                "the value of the effect of the group",
                group,
                property,
            ))),
            Self::MoveKeyframes(moves) => {
                let mut keyframes: Vec<KeyframeId> = moves.iter().map(|moved| moved.keyframe).collect();
                keyframes.sort();
                Some(EditKey::about(&("the place of the keyframes", keyframes)))
            }
            Self::MoveRacks { racks, .. } => {
                let mut racks: Vec<RackId> = racks.clone();
                racks.sort();
                Some(EditKey::about(&("the place of the racks", racks)))
            }
            Self::SetSpeedIndicatorLayout(_) => Some(EditKey::about(&"where the fast-forward symbol is")),
            Self::SetTheColoursOfAFile { source, .. } => Some(EditKey::about(&("the colours of the file", source))),
            _ => None,
        }
    }
}

/// What the edits among `commands` are about, together: `Some` if there are edits and every one is
/// the kind that a gesture makes over and over, so that the same gesture, in the next frame, has the
/// same key. (The commands of one frame are one step of the history, and are joined to the step
/// before if that was about the same.)
pub fn gesture_key_of_the_edits_among(commands: &[Command]) -> Option<EditKey> {
    let mut keys = Vec::new();
    for command in commands.iter().filter(|command| command.may_change_the_project()) {
        keys.push(command.gesture_key()?);
    }
    if keys.is_empty() {
        return None;
    }
    keys.sort();
    keys.dedup();
    Some(EditKey::about(&keys))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Angle, CanvasArea, ClipId, TextObject};

    fn clip(number: u32) -> ClipId {
        ClipId::new(TrackId::new(0), number)
    }

    #[test]
    fn choices_about_the_colours_of_one_file_are_one_gesture_and_choices_about_two_files_are_two() {
        let choice = |file: u32, said: InputColourOverride| Command::SetTheColoursOfAFile {
            source: SourceMediaIdentifier::new(file),
            colour: said,
        };
        let another_choice = InputColourOverride {
            curve: Some(crate::colour_space::TransferFunction::Gamma24),
            ..InputColourOverride::AUTOMATIC
        };
        assert_eq!(
            choice(1, InputColourOverride::AUTOMATIC).gesture_key(),
            choice(1, another_choice).gesture_key(),
            "the same file, whatever is chosen"
        );
        assert!(choice(1, another_choice).gesture_key().is_some());
        assert_ne!(
            choice(1, another_choice).gesture_key(),
            choice(2, another_choice).gesture_key(),
            "another file is another step of the history"
        );
        assert!(choice(1, another_choice).may_change_the_project());
        assert!(!Command::ShowTheColoursOfAFile(SourceMediaIdentifier::new(1)).may_change_the_project());
    }

    fn speed_of(clip_number: u32, factor: u32) -> Command {
        Command::SetClipSpeed {
            clip: clip(clip_number),
            speed_factor: SpeedFactor::new(factor).unwrap(),
        }
    }

    fn keyframe_added_at(frame: u64) -> Command {
        Command::AddKeyframe {
            target: AnimationTarget::Clip(clip(0)),
            property: PropertyId::Gain,
            frame: TimelineFrameIndex::new(frame),
        }
    }

    fn keyframe(number: u32) -> KeyframeId {
        KeyframeId::new(RackId::new(TrackId::new(0), 0), number)
    }

    #[test]
    fn selecting_clips_and_selecting_a_track_are_selecting_items() {
        assert_eq!(
            Command::select_clips([clip(1), clip(2)], SelectionChange::Toggle),
            Command::SelectItems {
                items: vec![SelectionItem::Clip(clip(1)), SelectionItem::Clip(clip(2))],
                change: SelectionChange::Toggle,
            }
        );
        assert_eq!(
            Command::select_track(TrackId::new(3), SelectionChange::Replace),
            Command::SelectItems {
                items: vec![SelectionItem::Track(TrackId::new(3))],
                change: SelectionChange::Replace,
            }
        );
        assert!(!Command::select_clips([clip(1)], SelectionChange::Replace).may_change_the_project());
    }

    #[test]
    fn moving_deleting_and_moving_racks_of_keyframes_are_edits_of_the_project() {
        assert!(Command::MoveKeyframes(vec![]).may_change_the_project());
        assert!(Command::DeleteKeyframes(vec![keyframe(0)]).may_change_the_project());
        assert!(
            Command::MoveRacks {
                racks: vec![RackId::new(TrackId::new(0), 0)],
                to: RackDestination::Drawer(AnimationTarget::Clip(clip(0))),
                shift_in_frames: 4,
                slot: None,
            }
            .may_change_the_project()
        );
    }

    #[test]
    fn copying_keyframes_taking_racks_away_and_merging_racks_are_edits_of_the_project_and_none_of_them_is_a_gesture() {
        let rack = |number| RackId::new(TrackId::new(0), number);
        for command in [
            Command::DuplicateKeyframes {
                keyframes: vec![keyframe(0)],
                towards: TimeDirection::Later,
                gap_in_frames: 3,
            },
            Command::DeleteRacks(vec![rack(1)]),
            Command::MergeRacks(vec![rack(1), rack(2)]),
        ] {
            assert!(command.may_change_the_project(), "{command:?}");
            assert_eq!(
                command.gesture_key(),
                None,
                "{command:?}: each is a step of the history of its own"
            );
        }
    }

    #[test]
    fn dragging_the_same_keyframes_is_one_gesture_whatever_frames_they_go_to_and_whatever_order_they_are_in() {
        let moving = |moves: &[(u32, u64)]| {
            Command::MoveKeyframes(
                moves
                    .iter()
                    .map(|(number, to_frame)| KeyframeMove {
                        keyframe: keyframe(*number),
                        to_rack: RackId::new(TrackId::new(0), 0),
                        to_frame: i64::try_from(*to_frame).unwrap(),
                    })
                    .collect(),
            )
        };
        let key = |command: Command| command.gesture_key().expect("a drag of keyframes is a gesture");
        assert_eq!(key(moving(&[(1, 10), (2, 20)])), key(moving(&[(2, 25), (1, 15)])));
        assert_ne!(
            key(moving(&[(1, 10), (2, 20)])),
            key(moving(&[(1, 10)])),
            "other keyframes are another drag"
        );
        assert_ne!(key(moving(&[(1, 10)])), key(moving(&[(3, 10)])));
    }

    #[test]
    fn dragging_a_rack_is_one_gesture_for_the_rack_wherever_it_is_taken() {
        let dragging = |rack: u32, owner: u32, shift: i64| Command::MoveRacks {
            racks: vec![RackId::new(TrackId::new(0), rack)],
            to: RackDestination::Drawer(AnimationTarget::Clip(clip(owner))),
            shift_in_frames: shift,
            slot: None,
        };
        assert_eq!(dragging(1, 0, 5).gesture_key(), dragging(1, 3, -8).gesture_key());
        assert!(dragging(1, 0, 5).gesture_key().is_some());
        assert_ne!(dragging(1, 0, 5).gesture_key(), dragging(2, 0, 5).gesture_key());
    }

    #[test]
    fn dragging_several_racks_is_one_gesture_for_those_racks_in_whatever_order_they_are_named() {
        let dragging = |racks: &[u32]| Command::MoveRacks {
            racks: racks.iter().map(|rack| RackId::new(TrackId::new(0), *rack)).collect(),
            to: RackDestination::Drawer(AnimationTarget::Clip(clip(0))),
            shift_in_frames: 3,
            slot: None,
        };
        assert_eq!(dragging(&[1, 2]).gesture_key(), dragging(&[2, 1]).gesture_key());
        assert!(dragging(&[1, 2]).gesture_key().is_some());
        assert_ne!(
            dragging(&[1, 2]).gesture_key(),
            dragging(&[1]).gesture_key(),
            "not the same racks"
        );
        assert_ne!(dragging(&[1, 2]).gesture_key(), dragging(&[1, 3]).gesture_key());
    }

    fn keyframe_removed_at(frame: u64) -> Command {
        Command::RemoveKeyframe {
            target: AnimationTarget::Clip(clip(0)),
            property: PropertyId::Gain,
            frame: TimelineFrameIndex::new(frame),
        }
    }

    #[test]
    fn what_changes_the_project_and_what_does_not_is_told_apart() {
        for edit in [
            Command::AddTrack,
            Command::DeleteTrack(TrackId::new(0)),
            Command::DuplicateTrack {
                track: TrackId::new(0),
                side: TrackSide::Below,
            },
            Command::MoveTrack {
                track: TrackId::new(0),
                to_position: 1,
            },
            Command::DeleteClips(vec![clip(0)]),
            Command::DuplicateClips {
                clips: vec![clip(0)],
                side: ClipSide::Left,
            },
            Command::SplitAt {
                frame: TimelineFrameIndex::new(3),
                only: None,
            },
            speed_of(0, 2),
            Command::SetTrackHeight {
                track: TrackId::new(0),
                height: TrackHeight::DEFAULT,
            },
            Command::SetTextObject {
                clip: clip(0),
                object: TextObject::default(),
            },
            keyframe_added_at(3),
            keyframe_removed_at(3),
            Command::SetProjectFrameRate(FrameRate::new(24, 1).unwrap()),
        ] {
            assert!(edit.may_change_the_project(), "{edit:?}");
        }
        for not_an_edit in [
            Command::Undo,
            Command::Redo,
            Command::Deselect,
            Command::SaveProjectAs,
            Command::Play,
            Command::SeekTo(TimelineFrameIndex::new(3)),
            Command::select_clips([clip(0)], SelectionChange::Replace),
            Command::SetProjectSettings(ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS),
            // (Where a file is kept is not what is edited: like choosing a file again, it is not a step that can be undone.)
            Command::IncludeFilesInTheProjectFile(vec![SourceMediaIdentifier::new(1)]),
            Command::TakeFilesOutOfTheProjectFile(vec![SourceMediaIdentifier::new(1)]),
        ] {
            assert!(!not_an_edit.may_change_the_project(), "{not_an_edit:?}");
        }
    }

    #[test]
    fn the_same_gesture_has_the_same_key_in_every_frame_and_different_things_have_different_keys() {
        assert_eq!(speed_of(0, 2).gesture_key(), speed_of(0, 50).gesture_key());
        assert_ne!(
            speed_of(0, 2).gesture_key(),
            speed_of(1, 2).gesture_key(),
            "another clip"
        );
        assert_ne!(
            speed_of(0, 2).gesture_key(),
            Command::SetClipGain {
                clip: clip(0),
                gain: Gain::UNCHANGED
            }
            .gesture_key(),
            "the speed and the gain of the same clip"
        );
        assert!(speed_of(0, 2).gesture_key().is_some());
    }

    #[test]
    fn each_click_of_a_stopwatch_is_a_step_of_the_history_of_its_own() {
        assert_eq!(keyframe_added_at(3).gesture_key(), None);
        assert_eq!(keyframe_removed_at(3).gesture_key(), None);
        assert_eq!(
            gesture_key_of_the_edits_among(&[keyframe_added_at(3), keyframe_added_at(4)]),
            None,
            "two of them are not one gesture either"
        );
    }

    #[test]
    fn dragging_clips_is_one_gesture_for_those_clips_wherever_they_have_got_to() {
        let moved = |clips: &[(u32, u64)]| {
            Command::move_clips(
                clips
                    .iter()
                    .map(|&(number, start)| ClipMove {
                        clip: clip(number),
                        to_track: TrackId::new(0),
                        start: TimelineFrameIndex::new(start),
                    })
                    .collect(),
            )
        };
        assert!(moved(&[(0, 10)]).may_change_the_project());
        assert!(moved(&[(0, 10)]).gesture_key().is_some());
        assert_eq!(moved(&[(0, 10)]).gesture_key(), moved(&[(0, 99)]).gesture_key());
        assert_eq!(
            moved(&[(0, 10), (1, 50)]).gesture_key(),
            moved(&[(1, 70), (0, 30)]).gesture_key(),
            "the same clips in any order"
        );
        let to_another_track = Command::move_clips(vec![ClipMove {
            clip: clip(0),
            to_track: TrackId::new(5),
            start: TimelineFrameIndex::new(10),
        }]);
        assert_eq!(
            moved(&[(0, 10)]).gesture_key(),
            to_another_track.gesture_key(),
            "the same clip is the same gesture on whichever track it is dragged to"
        );
        assert_ne!(
            moved(&[(0, 10)]).gesture_key(),
            moved(&[(1, 10)]).gesture_key(),
            "other clips"
        );
        assert_ne!(
            moved(&[(0, 10)]).gesture_key(),
            moved(&[(0, 10), (1, 50)]).gesture_key(),
            "more clips"
        );
    }

    #[test]
    fn moving_resizing_or_turning_a_clip_on_the_preview_is_one_gesture_per_clip() {
        let placed = |clip_number: u32, degrees: f32| Command::SetClipPlacement {
            clip: clip(clip_number),
            placement: Placement::new(CanvasArea::DEFAULT, Angle::from_degrees(degrees)),
        };
        assert!(placed(0, 10.0).may_change_the_project());
        assert!(placed(0, 10.0).gesture_key().is_some());
        assert_eq!(placed(0, 10.0).gesture_key(), placed(0, 80.0).gesture_key());
        assert_ne!(
            placed(0, 10.0).gesture_key(),
            placed(1, 10.0).gesture_key(),
            "another clip"
        );
        assert_ne!(
            placed(0, 10.0).gesture_key(),
            Command::SetTextObject {
                clip: clip(0),
                object: TextObject::default()
            }
            .gesture_key(),
            "what the text says is another gesture than where it is"
        );
    }

    #[test]
    fn dragging_the_height_of_a_track_is_one_gesture_per_track() {
        let height_of = |track: u32, points: f32| Command::SetTrackHeight {
            track: TrackId::new(track),
            height: TrackHeight::from_points(points),
        };
        assert!(height_of(0, 50.0).gesture_key().is_some());
        assert_eq!(height_of(0, 50.0).gesture_key(), height_of(0, 90.0).gesture_key());
        assert_ne!(height_of(0, 50.0).gesture_key(), height_of(1, 50.0).gesture_key());
        assert_ne!(
            height_of(0, 50.0).gesture_key(),
            Command::SetTrackGain {
                track: TrackId::new(0),
                gain: Gain::UNCHANGED
            }
            .gesture_key()
        );
    }

    #[test]
    fn a_cut_or_a_delete_or_a_copy_or_a_new_track_is_never_joined_to_another_edit() {
        for edit in [
            Command::AddTrack,
            Command::DeleteTrack(TrackId::new(0)),
            Command::DuplicateTrack {
                track: TrackId::new(0),
                side: TrackSide::Above,
            },
            Command::MoveTrack {
                track: TrackId::new(0),
                to_position: 1,
            },
            Command::DeleteClips(vec![clip(0)]),
            Command::DuplicateClips {
                clips: vec![clip(0), clip(1)],
                side: ClipSide::Right,
            },
            Command::SplitAt {
                frame: TimelineFrameIndex::new(3),
                only: None,
            },
            Command::SetClipMuted {
                clip: clip(0),
                muted: true,
            },
            Command::SetProjectFrameRate(FrameRate::new(24, 1).unwrap()),
        ] {
            assert_eq!(edit.gesture_key(), None, "{edit:?}");
        }
    }

    #[test]
    fn the_key_of_a_frame_is_that_of_all_its_edits_together_and_is_none_if_one_is_never_joined() {
        let one_clip = [speed_of(0, 2)];
        let two_clips = [speed_of(0, 2), speed_of(1, 2)];
        assert!(gesture_key_of_the_edits_among(&one_clip).is_some());
        assert_eq!(
            gesture_key_of_the_edits_among(&two_clips),
            gesture_key_of_the_edits_among(&[speed_of(1, 9), speed_of(0, 9)]),
            "the same clips in any order and at any speed"
        );
        assert_ne!(
            gesture_key_of_the_edits_among(&one_clip),
            gesture_key_of_the_edits_among(&two_clips)
        );
        assert_eq!(
            gesture_key_of_the_edits_among(&[speed_of(0, 2), Command::AddTrack]),
            None,
            "one edit that is never joined is enough"
        );
        assert_eq!(
            gesture_key_of_the_edits_among(&[Command::Play, Command::SeekTo(TimelineFrameIndex::new(1))]),
            None,
            "no edits at all"
        );
        assert_eq!(
            gesture_key_of_the_edits_among(&[Command::Play, speed_of(0, 2)]),
            gesture_key_of_the_edits_among(&one_clip),
            "what is not an edit does not count"
        );
    }
}
