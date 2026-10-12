//! What is selected: any number of clips, on any of the tracks, any number of tracks, any number of keyframes and any number
//! of racks of keyframes, all at once if that is how they were chosen.
//!
//! Selecting is something the person doing the editing does, not something the project is made of,
//! so the project knows nothing about it. A clip is named by its [`ClipId`], which does not
//! change when other clips are cut, deleted or moved around it, or when it is moved to another track.
//! (The settings of what is selected are shown, and changed, in the properties panel, when one thing
//! is selected; Delete, S and dragging are about the selected clips.)

use std::collections::BTreeSet;

use crate::project::{ClipId, KeyframeId, Project, RackId, TrackId};

/// One thing that can be selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SelectionItem {
    Clip(ClipId),
    /// A track itself, which is not all the clips on it.
    Track(TrackId),
    /// A keyframe, in whichever rack it is.
    Keyframe(KeyframeId),
    /// A rack of keyframes itself, which is not all the keyframes in it.
    Rack(RackId),
}

impl SelectionItem {
    /// Whether `project` has what this is of: the clip, the track, the keyframe or the rack.
    pub fn is_in(&self, project: &Project) -> bool {
        match self {
            Self::Clip(clip) => project.placement_of(*clip).is_some(),
            Self::Track(track) => project.track(*track).is_some(),
            Self::Keyframe(keyframe) => project.locate_keyframe(*keyframe).is_some(),
            Self::Rack(rack) => project.located_rack(*rack).is_some(),
        }
    }
}

/// The things that are selected.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    items: BTreeSet<SelectionItem>,
}

/// How a click (or a rectangle drawn round clips) changes what is selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionChange {
    /// Only what is given is selected afterwards.
    Replace,
    /// Each thing given is selected if it was not, and is not any more if it was.
    Toggle,
}

impl Selection {
    pub fn empty() -> Self {
        Self::default()
    }

    /// A selection of just `clip`.
    pub fn only(clip: ClipId) -> Self {
        Self::from_clips([clip])
    }

    pub fn from_clips(clips: impl IntoIterator<Item = ClipId>) -> Self {
        Self::from_items(clips.into_iter().map(SelectionItem::Clip))
    }

    pub fn from_items(items: impl IntoIterator<Item = SelectionItem>) -> Self {
        Self {
            items: items.into_iter().collect(),
        }
    }

    /// Whether nothing at all is selected.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Whether `clip` is selected.
    pub fn contains(&self, clip: ClipId) -> bool {
        self.items.contains(&SelectionItem::Clip(clip))
    }

    /// Whether `track` is selected.
    pub fn contains_track(&self, track: TrackId) -> bool {
        self.items.contains(&SelectionItem::Track(track))
    }

    /// Whether `keyframe` is selected.
    pub fn contains_keyframe(&self, keyframe: KeyframeId) -> bool {
        self.items.contains(&SelectionItem::Keyframe(keyframe))
    }

    /// Whether the rack `rack` is selected.
    pub fn contains_rack(&self, rack: RackId) -> bool {
        self.items.contains(&SelectionItem::Rack(rack))
    }

    /// The selected clips, in the order of their ids (which is by the track they were made on, and then
    /// the order they were made in).
    pub fn iter(&self) -> impl Iterator<Item = ClipId> + '_ {
        self.items.iter().filter_map(|item| match item {
            SelectionItem::Clip(clip) => Some(*clip),
            SelectionItem::Track(_) | SelectionItem::Keyframe(_) | SelectionItem::Rack(_) => None,
        })
    }

    /// The selected clips.
    pub fn to_vec(&self) -> Vec<ClipId> {
        self.iter().collect()
    }

    /// The selected keyframes, in the order of their ids (by the rack they were made in, and then the order they were made in).
    pub fn keyframes(&self) -> impl Iterator<Item = KeyframeId> + '_ {
        self.items.iter().filter_map(|item| match item {
            SelectionItem::Keyframe(keyframe) => Some(*keyframe),
            SelectionItem::Clip(_) | SelectionItem::Track(_) | SelectionItem::Rack(_) => None,
        })
    }

    /// The selected keyframes.
    pub fn keyframes_to_vec(&self) -> Vec<KeyframeId> {
        self.keyframes().collect()
    }

    /// The selected racks, in the order of their names.
    pub fn racks(&self) -> impl Iterator<Item = RackId> + '_ {
        self.items.iter().filter_map(|item| match item {
            SelectionItem::Rack(rack) => Some(*rack),
            SelectionItem::Clip(_) | SelectionItem::Track(_) | SelectionItem::Keyframe(_) => None,
        })
    }

    /// The selected racks.
    pub fn racks_to_vec(&self) -> Vec<RackId> {
        self.racks().collect()
    }

    /// Everything that is selected, the clips first.
    pub fn items(&self) -> impl Iterator<Item = SelectionItem> + '_ {
        self.items.iter().copied()
    }

    pub fn to_items(&self) -> Vec<SelectionItem> {
        self.items().collect()
    }

    /// The selected thing, if exactly one thing is selected.
    pub fn single_item(&self) -> Option<SelectionItem> {
        let mut items = self.items.iter();
        match (items.next(), items.next()) {
            (Some(item), None) => Some(*item),
            _ => None,
        }
    }

    /// The selected clip, if exactly one thing is selected and it is a clip.
    pub fn single(&self) -> Option<ClipId> {
        match self.single_item() {
            Some(SelectionItem::Clip(clip)) => Some(clip),
            _ => None,
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Changes what is selected by `items`, in the way `change` says.
    pub fn change_items(&mut self, items: impl IntoIterator<Item = SelectionItem>, change: SelectionChange) {
        match change {
            SelectionChange::Replace => self.items = items.into_iter().collect(),
            SelectionChange::Toggle => {
                for item in items {
                    if !self.items.remove(&item) {
                        self.items.insert(item);
                    }
                }
            }
        }
    }

    /// Lets go of the clips, the tracks, the keyframes and the racks that `project` does not have (any more).
    pub fn forget_what_is_gone(&mut self, project: &Project) {
        self.items.retain(|item| item.is_in(project));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(track: u32, clip: u32) -> ClipId {
        ClipId::new(TrackId::new(track), clip)
    }

    #[test]
    fn nothing_is_selected_to_begin_with_and_a_single_clip_is_the_single_one() {
        let selection = Selection::empty();
        assert!(selection.is_empty() && selection.single().is_none());
        let selection = Selection::only(clip(0, 3));
        assert_eq!(selection.single(), Some(clip(0, 3)));
        assert_eq!(selection.to_vec(), vec![clip(0, 3)]);
        assert!(selection.contains(clip(0, 3)) && !selection.contains(clip(0, 4)));
    }

    #[test]
    fn there_is_no_single_selected_clip_when_there_are_several() {
        let selection = Selection::from_clips([clip(0, 1), clip(1, 2)]);
        assert_eq!(selection.single(), None);
        assert_eq!(selection.to_vec(), vec![clip(0, 1), clip(1, 2)]);
    }

    #[test]
    fn replacing_selects_only_what_is_given() {
        let mut selection = Selection::from_clips([clip(0, 1), clip(0, 2)]);
        selection.change_items([SelectionItem::Clip(clip(1, 5))], SelectionChange::Replace);
        assert_eq!(selection.to_vec(), vec![clip(1, 5)]);
        selection.change_items([], SelectionChange::Replace);
        assert!(selection.is_empty());
    }

    #[test]
    fn toggling_selects_what_was_not_and_lets_go_of_what_was() {
        let mut selection = Selection::from_clips([clip(0, 1), clip(0, 2)]);
        selection.change_items(
            [SelectionItem::Clip(clip(0, 2)), SelectionItem::Clip(clip(0, 3))],
            SelectionChange::Toggle,
        );
        assert_eq!(selection.to_vec(), vec![clip(0, 1), clip(0, 3)]);
    }

    #[test]
    fn the_selected_clips_come_in_a_steady_order_whatever_order_they_were_selected_in() {
        let selection = Selection::from_clips([clip(2, 0), clip(0, 9), clip(0, 1), clip(1, 4)]);
        assert_eq!(selection.to_vec(), vec![clip(0, 1), clip(0, 9), clip(1, 4), clip(2, 0)]);
    }

    #[test]
    fn clips_that_the_project_does_not_have_are_forgotten_and_the_others_stay() {
        use std::path::PathBuf;

        use crate::project::{
            FrameRate, SourceMedia, SourceMediaIdentifier, SourceMediaLink, SourceVideoProperties, TimelineFrameIndex,
        };

        let mut project = Project::empty();
        let outcome = project
            .import_source(
                SourceMedia::video(
                    SourceMediaIdentifier::new(1),
                    SourceMediaLink::from_absolute_path(PathBuf::from("a.mp4")),
                    SourceVideoProperties {
                        frame_rate: FrameRate::new(30, 1).unwrap(),
                        total_frame_count: 100,
                        display_width_in_pixels: 320,
                        display_height_in_pixels: 180,
                        has_audio: false,
                    },
                ),
                TimelineFrameIndex::new(0),
            )
            .unwrap();
        let existing = outcome.clip.unwrap();
        let no_such_clip = ClipId::new(outcome.track, 99);
        let on_no_such_track = clip(77, 0);

        let mut selection = Selection::from_clips([existing, no_such_clip, on_no_such_track]);
        selection.forget_what_is_gone(&project);

        assert_eq!(selection.to_vec(), vec![existing]);
    }

    #[test]
    fn a_track_can_be_selected_beside_clips_and_is_not_one_of_the_selected_clips() {
        let mut selection = Selection::from_clips([clip(0, 1)]);
        selection.change_items([SelectionItem::Track(TrackId::new(2))], SelectionChange::Toggle);

        assert!(selection.contains_track(TrackId::new(2)) && !selection.contains_track(TrackId::new(0)));
        assert_eq!(selection.to_vec(), vec![clip(0, 1)], "the clips are the clips");
        assert_eq!(
            selection.to_items(),
            vec![SelectionItem::Clip(clip(0, 1)), SelectionItem::Track(TrackId::new(2))],
            "the clips come first"
        );
        assert_eq!(selection.single(), None, "there are two things selected");
        assert!(!selection.is_empty());
    }

    #[test]
    fn a_track_that_is_the_only_thing_selected_is_the_single_item_and_there_is_no_single_clip() {
        let selection = Selection::from_items([SelectionItem::Track(TrackId::new(1))]);
        assert_eq!(selection.single_item(), Some(SelectionItem::Track(TrackId::new(1))));
        assert_eq!(selection.single(), None);
        assert!(selection.to_vec().is_empty());
        assert_eq!(
            Selection::only(clip(0, 0)).single_item(),
            Some(SelectionItem::Clip(clip(0, 0)))
        );
        assert_eq!(Selection::empty().single_item(), None);
    }

    #[test]
    fn replacing_lets_go_of_a_selected_track_and_a_track_replaces_the_clips() {
        let mut selection =
            Selection::from_items([SelectionItem::Track(TrackId::new(1)), SelectionItem::Clip(clip(0, 1))]);
        selection.change_items([SelectionItem::Clip(clip(0, 2))], SelectionChange::Replace);
        assert_eq!(selection.to_items(), vec![SelectionItem::Clip(clip(0, 2))]);
        selection.change_items([SelectionItem::Track(TrackId::new(0))], SelectionChange::Replace);
        assert_eq!(selection.to_items(), vec![SelectionItem::Track(TrackId::new(0))]);
    }

    fn keyframe(rack: u32, number: u32) -> KeyframeId {
        KeyframeId::new(crate::project::RackId::new(TrackId::new(0), rack), number)
    }

    #[test]
    fn a_keyframe_can_be_selected_beside_clips_and_tracks_and_is_neither() {
        let mut selection = Selection::from_clips([clip(0, 1)]);
        selection.change_items(
            [
                SelectionItem::Keyframe(keyframe(0, 3)),
                SelectionItem::Keyframe(keyframe(0, 1)),
                SelectionItem::Track(TrackId::new(2)),
            ],
            SelectionChange::Toggle,
        );

        assert!(selection.contains_keyframe(keyframe(0, 3)) && !selection.contains_keyframe(keyframe(0, 2)));
        assert_eq!(selection.to_vec(), vec![clip(0, 1)], "the clips are the clips");
        assert_eq!(
            selection.keyframes_to_vec(),
            vec![keyframe(0, 1), keyframe(0, 3)],
            "the keyframes are the keyframes, in the order of their names"
        );
        assert!(selection.contains_track(TrackId::new(2)));
        assert_eq!(selection.items().count(), 4);
        assert_eq!(selection.single(), None);
        assert_eq!(selection.single_item(), None);
    }

    #[test]
    fn a_keyframe_that_is_the_only_thing_selected_is_the_single_item_and_there_is_no_single_clip() {
        let selection = Selection::from_items([SelectionItem::Keyframe(keyframe(1, 0))]);
        assert_eq!(selection.single_item(), Some(SelectionItem::Keyframe(keyframe(1, 0))));
        assert_eq!(selection.single(), None);
        assert!(selection.to_vec().is_empty());
    }

    #[test]
    fn toggling_a_keyframe_lets_go_of_it_when_it_is_selected_and_replacing_lets_go_of_the_rest() {
        let mut selection =
            Selection::from_items([SelectionItem::Keyframe(keyframe(0, 1)), SelectionItem::Clip(clip(0, 0))]);
        selection.change_items([SelectionItem::Keyframe(keyframe(0, 1))], SelectionChange::Toggle);
        assert_eq!(selection.to_items(), vec![SelectionItem::Clip(clip(0, 0))]);
        selection.change_items([SelectionItem::Keyframe(keyframe(0, 2))], SelectionChange::Replace);
        assert_eq!(selection.to_items(), vec![SelectionItem::Keyframe(keyframe(0, 2))]);
    }

    #[test]
    fn a_keyframe_is_in_a_project_that_has_it_and_not_in_one_that_does_not() {
        use crate::project::{AnimationTarget, PropertyId, TimelineFrameIndex};

        let mut project = Project::empty();
        let track = project.tracks()[0].id();
        let (text, _) = project
            .insert_text(
                track,
                TimelineFrameIndex::new(0),
                crate::project::TextObject::default(),
                30,
            )
            .unwrap();
        let made = project
            .add_keyframe(
                AnimationTarget::Clip(text),
                PropertyId::Rotation,
                TimelineFrameIndex::new(3),
            )
            .unwrap()
            .expect("a keyframe is made");

        assert!(SelectionItem::Keyframe(made.keyframe).is_in(&project));
        assert!(!SelectionItem::Keyframe(keyframe(7, 7)).is_in(&project), "no such rack");
        assert!(
            !SelectionItem::Keyframe(KeyframeId::new(made.rack, made.keyframe.number() + 1)).is_in(&project),
            "no such keyframe in a rack that is there"
        );

        let mut selection = Selection::from_items([
            SelectionItem::Keyframe(made.keyframe),
            SelectionItem::Keyframe(keyframe(7, 7)),
            SelectionItem::Clip(text),
        ]);
        selection.forget_what_is_gone(&project);
        assert_eq!(
            selection.to_items(),
            vec![SelectionItem::Clip(text), SelectionItem::Keyframe(made.keyframe)]
        );
        project
            .remove_keyframe(
                AnimationTarget::Clip(text),
                PropertyId::Rotation,
                TimelineFrameIndex::new(3),
            )
            .unwrap();
        selection.forget_what_is_gone(&project);
        assert_eq!(
            selection.to_items(),
            vec![SelectionItem::Clip(text)],
            "the keyframe was taken away"
        );
    }

    #[test]
    fn a_track_that_the_project_does_not_have_is_forgotten_with_the_clips_that_are_gone() {
        let mut project = Project::empty();
        let first = project.tracks()[0].id();
        let second = project.add_track_in_front();
        let mut selection = Selection::from_items([
            SelectionItem::Track(first),
            SelectionItem::Track(second),
            SelectionItem::Track(TrackId::new(99)),
        ]);
        project.delete_track(second).unwrap();

        selection.forget_what_is_gone(&project);

        assert_eq!(selection.to_items(), vec![SelectionItem::Track(first)]);
    }

    fn rack(number: u32) -> RackId {
        RackId::new(TrackId::new(0), number)
    }

    #[test]
    fn a_rack_can_be_selected_beside_clips_tracks_and_keyframes_and_is_none_of_them() {
        let mut selection = Selection::from_clips([clip(0, 1)]);
        selection.change_items(
            [
                SelectionItem::Rack(rack(4)),
                SelectionItem::Rack(rack(2)),
                SelectionItem::Keyframe(keyframe(0, 3)),
                SelectionItem::Track(TrackId::new(2)),
            ],
            SelectionChange::Toggle,
        );

        assert!(
            selection.contains_rack(rack(4)) && selection.contains_rack(rack(2)) && !selection.contains_rack(rack(3))
        );
        assert_eq!(
            selection.racks_to_vec(),
            vec![rack(2), rack(4)],
            "in the order of their names"
        );
        assert_eq!(selection.to_vec(), vec![clip(0, 1)], "the clips are the clips");
        assert_eq!(
            selection.keyframes_to_vec(),
            vec![keyframe(0, 3)],
            "the keyframes are the keyframes"
        );
        assert!(selection.contains_track(TrackId::new(2)));
        assert_eq!(selection.items().count(), 5);
        assert_eq!(selection.single_item(), None);
        assert_eq!(selection.single(), None);
    }

    #[test]
    fn a_rack_that_is_the_only_thing_selected_is_the_single_item_and_there_is_no_single_clip() {
        let selection = Selection::from_items([SelectionItem::Rack(rack(1))]);
        assert_eq!(selection.single_item(), Some(SelectionItem::Rack(rack(1))));
        assert_eq!(selection.single(), None);
        assert!(selection.to_vec().is_empty() && selection.keyframes_to_vec().is_empty());
    }

    #[test]
    fn toggling_a_rack_lets_go_of_it_when_it_is_selected_and_replacing_lets_go_of_the_rest() {
        let mut selection = Selection::from_items([SelectionItem::Rack(rack(1)), SelectionItem::Clip(clip(0, 0))]);
        selection.change_items([SelectionItem::Rack(rack(1))], SelectionChange::Toggle);
        assert_eq!(selection.to_items(), vec![SelectionItem::Clip(clip(0, 0))]);
        selection.change_items(
            [SelectionItem::Rack(rack(2)), SelectionItem::Rack(rack(3))],
            SelectionChange::Replace,
        );
        assert_eq!(
            selection.to_items(),
            vec![SelectionItem::Rack(rack(2)), SelectionItem::Rack(rack(3))]
        );
    }

    #[test]
    fn a_rack_is_in_a_project_that_has_it_and_not_in_one_that_does_not_and_is_forgotten_once_it_is_gone() {
        use crate::project::{AnimationTarget, PropertyId, TimelineFrameIndex};

        let mut project = Project::empty();
        let track = project.tracks()[0].id();
        let (text, _) = project
            .insert_text(
                track,
                TimelineFrameIndex::new(0),
                crate::project::TextObject::default(),
                30,
            )
            .unwrap();
        let made = project
            .add_keyframe(
                AnimationTarget::Clip(text),
                PropertyId::Rotation,
                TimelineFrameIndex::new(3),
            )
            .unwrap()
            .expect("a keyframe is made");

        assert!(SelectionItem::Rack(made.rack).is_in(&project));
        assert!(!SelectionItem::Rack(rack(77)).is_in(&project), "no such rack");

        let mut selection = Selection::from_items([
            SelectionItem::Rack(made.rack),
            SelectionItem::Rack(rack(77)),
            SelectionItem::Clip(text),
        ]);
        selection.forget_what_is_gone(&project);
        assert_eq!(
            selection.to_items(),
            vec![SelectionItem::Clip(text), SelectionItem::Rack(made.rack)]
        );
        project.remove_racks(&[made.rack]).unwrap();
        selection.forget_what_is_gone(&project);
        assert_eq!(
            selection.to_items(),
            vec![SelectionItem::Clip(text)],
            "the rack was taken away"
        );
    }
}
