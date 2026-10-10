//! The video file a project was made from, and how the project finds it again.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::FrameRate;

/// Tells source videos apart inside one project. Clips refer to their video by this number, which
/// is what lets the project file hold more than one video even though this version of the editor
/// only works with one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SourceVideoIdentifier(u32);

impl SourceVideoIdentifier {
    pub const fn new(number: u32) -> Self {
        Self(number)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

impl fmt::Display for SourceVideoIdentifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// What the project needs to remember about a source video so that it can still lay out the
/// timeline, count frames and show timecodes while the video file itself is missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceVideoProperties {
    pub frame_rate: FrameRate,
    /// How many frames the video has in total.
    pub total_frame_count: u64,
    /// The size of the picture as it is shown: after the video's rotation and non-square pixel
    /// shape (if it has either) have been applied.
    pub display_width_in_pixels: u32,
    pub display_height_in_pixels: u32,
    pub has_audio: bool,
}

/// Where a source video file lives. The project never embeds the video; it remembers the place in
/// two ways so that it can usually find the file again: the absolute path, and a path relative to
/// the folder holding the project file (which still works if the project and the video were moved
/// together).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceVideoLink {
    pub absolute_path: PathBuf,
    /// Relative to the folder containing the project file. `None` until the project has been
    /// saved, or when no relative path exists (for example the video is on another drive).
    pub relative_path: Option<PathBuf>,
}

impl SourceVideoLink {
    /// A link that only knows the absolute path (a video that was just chosen, in a project that
    /// has not been saved yet).
    pub fn from_absolute_path(absolute_path: PathBuf) -> Self {
        Self {
            absolute_path,
            relative_path: None,
        }
    }

    /// The places the video may be, best guess first: the absolute path, then the relative path
    /// resolved against `project_directory`.
    pub fn candidate_paths(&self, project_directory: Option<&Path>) -> Vec<PathBuf> {
        let mut candidates = vec![self.absolute_path.clone()];
        if let (Some(relative_path), Some(project_directory)) = (&self.relative_path, project_directory) {
            let resolved_relative_path = project_directory.join(relative_path);
            if resolved_relative_path != self.absolute_path {
                candidates.push(resolved_relative_path);
            }
        }
        candidates
    }
}

/// A source video: a link to a file on disk plus the properties it had when it was imported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceVideo {
    pub identifier: SourceVideoIdentifier,
    pub link: SourceVideoLink,
    pub properties: SourceVideoProperties,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(relative_path: Option<&str>) -> SourceVideoLink {
        SourceVideoLink {
            absolute_path: PathBuf::from("/videos/holiday.mp4"),
            relative_path: relative_path.map(PathBuf::from),
        }
    }

    #[test]
    fn the_absolute_path_is_tried_before_the_relative_one() {
        let candidates = link(Some("../videos/holiday.mp4")).candidate_paths(Some(Path::new("/projects/summer")));
        assert_eq!(
            candidates,
            vec![
                PathBuf::from("/videos/holiday.mp4"),
                PathBuf::from("/projects/summer").join("../videos/holiday.mp4"),
            ]
        );
    }

    #[test]
    fn without_a_relative_path_or_project_folder_only_the_absolute_path_is_a_candidate() {
        let expected = vec![PathBuf::from("/videos/holiday.mp4")];
        assert_eq!(link(None).candidate_paths(Some(Path::new("/projects"))), expected);
        assert_eq!(link(Some("a.mp4")).candidate_paths(None), expected);
    }

    #[test]
    fn a_relative_path_that_resolves_to_the_absolute_path_is_not_listed_twice() {
        let mut link = link(Some("holiday.mp4"));
        link.absolute_path = PathBuf::from("/projects/summer").join("holiday.mp4");
        assert_eq!(link.candidate_paths(Some(Path::new("/projects/summer"))).len(), 1);
    }

    #[test]
    fn a_link_made_from_an_absolute_path_has_no_relative_path_yet() {
        let link = SourceVideoLink::from_absolute_path(PathBuf::from("/videos/holiday.mp4"));
        assert_eq!(link.relative_path, None);
    }
}
