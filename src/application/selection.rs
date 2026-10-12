//! What is selected: any number of clips, on any of the tracks, and any number of tracks.
//!
//! Selecting is something the person doing the editing does, not something the project is made of,
//! so the project knows nothing about it. A clip is named by its [`ClipId`], which does not
//! change when other clips are cut, deleted or moved around it, or when it is moved to another track.
//! (The settings of what is selected are shown, and changed, in the properties panel, when one thing
//! is selected; Delete, S and dragging are about the selected clips.)

use std::collections::BTreeSet;

use crate::project::{ClipId, Project, TrackId};

/// One thing that can be selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SelectionItem {
    Clip(ClipId),
    /// A track itself, which is not all the clips on it.
    Track(TrackId),
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

    /// The selected clips, in the order of their ids (which is by the track they were made on, and then
    /// the order they were made in).
    pub fn iter(&self) -> impl Iterator<Item = ClipId> + '_ {
        self.items.iter().filter_map(|item| match item {
            SelectionItem::Clip(clip) => Some(*clip),
            SelectionItem::Track(_) => None,
        })
    }

    /// The selected clips.
    pub fn to_vec(&self) -> Vec<ClipId> {
        self.iter().collect()
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

    /// Changes what is selected by `clips`, in the way `change` says.
    pub fn change(&mut self, clips: impl IntoIterator<Item = ClipId>, change: SelectionChange) {
        self.change_items(clips.into_iter().map(SelectionItem::Clip), change);
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

    /// Lets go of the clips and the tracks that `project` does not have (any more).
    pub fn forget_what_is_gone(&mut self, project: &Project) {
        self.items.retain(|item| match item {
            SelectionItem::Clip(clip) => project.placement_of(*clip).is_some(),
            SelectionItem::Track(track) => project.track(*track).is_some(),
        });
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
        selection.change([clip(1, 5)], SelectionChange::Replace);
        assert_eq!(selection.to_vec(), vec![clip(1, 5)]);
        selection.change([], SelectionChange::Replace);
        assert!(selection.is_empty());
    }

    #[test]
    fn toggling_selects_what_was_not_and_lets_go_of_what_was() {
        let mut selection = Selection::from_clips([clip(0, 1), clip(0, 2)]);
        selection.change([clip(0, 2), clip(0, 3)], SelectionChange::Toggle);
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
        selection.change([clip(0, 2)], SelectionChange::Replace);
        assert_eq!(selection.to_items(), vec![SelectionItem::Clip(clip(0, 2))]);
        selection.change_items([SelectionItem::Track(TrackId::new(0))], SelectionChange::Replace);
        assert_eq!(selection.to_items(), vec![SelectionItem::Track(TrackId::new(0))]);
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
}
