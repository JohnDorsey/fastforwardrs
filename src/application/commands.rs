//! Everything the user can ask for. The parts of the screen only describe what was asked (by
//! pushing a [`Command`]); one place afterwards decides what to do about each, so that no drawing
//! code ever has to change the project while it is still being drawn from.

use super::history::EditKey;
use super::selection::{SelectionChange, SelectionItem};
use crate::project::{
    AnimationTarget, ClipId, ClipMove, ClipSide, EffectKind, FrameRate, Gain, KeyframeId, KeyframeMove, Placement,
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
    /// Choose a video or a sound file and put all of it on the timeline, at the current frame.
    ImportFile,
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

    /// Change what is selected (by clicking something, or drawing a rectangle round some things): select `items`
    /// and nothing else, or as well as what is selected, or the other way round for each of them, as `change` says.
    /// (A track is selected as itself, which is not all the clips on it.)
    SelectItems {
        items: Vec<SelectionItem>,
        change: SelectionChange,
    },

    /// Cut the clips under this frame in two (with the S key): only those in `only` if that is
    /// given, and otherwise every clip there is under the frame, on every track.
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

    /// Move clips, by dragging them: each to the track and the frame given for it.
    MoveClips(Vec<ClipMove>),

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
    /// Put an effect on a clip or a track (with the plus button of the properties panel), with the values it begins with.
    AddEffect {
        target: AnimationTarget,
        kind: EffectKind,
    },
    /// Take an effect off a clip or a track (from the menu of its box in the properties panel), with the keyframes of its values.
    RemoveEffect {
        target: AnimationTarget,
        kind: EffectKind,
    },
    /// Give a value of an effect of a clip or a track (a property of it) a value: at the playhead if it has keyframes, as for any
    /// property.
    SetEffectValue {
        target: AnimationTarget,
        property: PropertyId,
        value: PropertyValue,
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
    /// Move a rack of keyframes to another drawer, or onto an empty stretch of a track (where it is put in a keyframe
    /// crate made for it), by dragging it, with all its keyframes this many frames later in time (earlier, if negative). It
    /// has `slot` racks above it in the drawer it goes to if that is said (not counting itself), and otherwise it keeps its
    /// place there, or goes under the others if it is not in that drawer yet.
    MoveRack {
        rack: RackId,
        to: RackDestination,
        shift_in_frames: i64,
        slot: Option<usize>,
    },

    /// Move or resize the fast-forward symbol.
    SetSpeedIndicatorLayout(SpeedIndicatorLayout),
}

impl Command {
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
            | Self::MoveClips(_)
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
            | Self::SetEffectValue { .. }
            | Self::AddKeyframe { .. }
            | Self::RemoveKeyframe { .. }
            | Self::MoveKeyframes(_)
            | Self::DeleteKeyframes(_)
            | Self::DuplicateKeyframes { .. }
            | Self::DeleteRacks(_)
            | Self::MergeRacks(_)
            | Self::MoveRack { .. }
            | Self::SetProjectFrameRate(_)
            | Self::SetSpeedIndicatorLayout(_) => true,

            Self::LoadProject
            | Self::SaveProject
            | Self::SaveProjectAs
            | Self::NewProject
            | Self::ImportFile
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
            | Self::SelectItems { .. }
            | Self::RelinkSource(_) => false,
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
            Self::MoveClips(_) => Some("Putting the clips there"),

            Self::LoadProject
            | Self::SaveProject
            | Self::SaveProjectAs
            | Self::NewProject
            | Self::ImportFile
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
            | Self::SelectItems { .. }
            | Self::SplitAt { .. }
            | Self::DeleteClips(_)
            | Self::DuplicateClips { .. }
            | Self::SetClipGain { .. }
            | Self::SetClipMuted { .. }
            | Self::SetClipPlacement { .. }
            | Self::RelinkSource(_)
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
            | Self::SetEffectValue { .. }
            | Self::AddKeyframe { .. }
            | Self::RemoveKeyframe { .. }
            | Self::MoveKeyframes(_)
            | Self::DeleteKeyframes(_)
            | Self::DuplicateKeyframes { .. }
            | Self::DeleteRacks(_)
            | Self::MergeRacks(_)
            | Self::MoveRack { .. }
            | Self::SetSpeedIndicatorLayout(_) => None,
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
            Self::MoveClips(moves) => {
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
            Self::MoveKeyframes(moves) => {
                let mut keyframes: Vec<KeyframeId> = moves.iter().map(|moved| moved.keyframe).collect();
                keyframes.sort();
                Some(EditKey::about(&("the place of the keyframes", keyframes)))
            }
            Self::MoveRack { rack, .. } => Some(EditKey::about(&("the place of the rack", rack))),
            Self::SetSpeedIndicatorLayout(_) => Some(EditKey::about(&"where the fast-forward symbol is")),
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
            Command::MoveRack {
                rack: RackId::new(TrackId::new(0), 0),
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
                        to_frame: *to_frame,
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
        let dragging = |rack: u32, owner: u32, shift: i64| Command::MoveRack {
            rack: RackId::new(TrackId::new(0), rack),
            to: RackDestination::Drawer(AnimationTarget::Clip(clip(owner))),
            shift_in_frames: shift,
            slot: None,
        };
        assert_eq!(dragging(1, 0, 5).gesture_key(), dragging(1, 3, -8).gesture_key());
        assert!(dragging(1, 0, 5).gesture_key().is_some());
        assert_ne!(dragging(1, 0, 5).gesture_key(), dragging(2, 0, 5).gesture_key());
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
            Command::MoveClips(
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
        let to_another_track = Command::MoveClips(vec![ClipMove {
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
