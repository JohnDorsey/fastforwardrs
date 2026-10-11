//! Which clips are selected: any number of them, on any of the tracks.
//!
//! Selecting is something the person doing the editing does, not something the project is made of,
//! so the project knows nothing about it. A clip is named by its [`ClipLocation`], which does not
//! change when other clips are cut, deleted or moved along the timeline around it.

use std::collections::BTreeSet;

use crate::project::{ClipLocation, Project};

/// The clips that are selected.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    clips: BTreeSet<ClipLocation>,
}

/// How a click (or a rectangle drawn round clips) changes what is selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionChange {
    /// Only the clips given are selected afterwards.
    Replace,
    /// Each clip given is selected if it was not, and is not any more if it was.
    Toggle,
}

impl Selection {
    pub fn empty() -> Self {
        Self::default()
    }

    /// A selection of just `clip`.
    pub fn only(clip: ClipLocation) -> Self {
        Self::from_clips([clip])
    }

    pub fn from_clips(clips: impl IntoIterator<Item = ClipLocation>) -> Self {
        Self {
            clips: clips.into_iter().collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }

    pub fn contains(&self, clip: ClipLocation) -> bool {
        self.clips.contains(&clip)
    }

    /// The selected clips, in the order of their tracks in the project's ids and then of their ids.
    pub fn iter(&self) -> impl Iterator<Item = ClipLocation> + '_ {
        self.clips.iter().copied()
    }

    pub fn to_vec(&self) -> Vec<ClipLocation> {
        self.iter().collect()
    }

    /// The selected clip, if exactly one clip is selected.
    pub fn single(&self) -> Option<ClipLocation> {
        let mut clips = self.clips.iter();
        match (clips.next(), clips.next()) {
            (Some(clip), None) => Some(*clip),
            _ => None,
        }
    }

    pub fn clear(&mut self) {
        self.clips.clear();
    }

    /// Changes what is selected by `clips`, in the way `change` says.
    pub fn change(&mut self, clips: impl IntoIterator<Item = ClipLocation>, change: SelectionChange) {
        match change {
            SelectionChange::Replace => self.clips = clips.into_iter().collect(),
            SelectionChange::Toggle => {
                for clip in clips {
                    if !self.clips.remove(&clip) {
                        self.clips.insert(clip);
                    }
                }
            }
        }
    }

    /// Lets go of the clips that `project` does not have (any more).
    pub fn forget_clips_that_are_gone(&mut self, project: &Project) {
        self.clips.retain(|clip| project.placement_of(*clip).is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{ClipId, TrackId};

    fn clip(track: u32, clip: u32) -> ClipLocation {
        ClipLocation {
            track: TrackId::new(track),
            clip: ClipId::new(clip),
        }
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
        let existing = ClipLocation {
            track: outcome.track,
            clip: outcome.clip.unwrap(),
        };
        let no_such_clip = ClipLocation {
            track: outcome.track,
            clip: ClipId::new(99),
        };
        let on_no_such_track = clip(77, 0);

        let mut selection = Selection::from_clips([existing, no_such_clip, on_no_such_track]);
        selection.forget_clips_that_are_gone(&project);

        assert_eq!(selection.to_vec(), vec![existing]);
    }
}
