//! Finding out whether a source video can be used: trying the places it might be until one of
//! them opens as a video.

use std::path::{Path, PathBuf};

use crate::media::ProbeError;
use crate::project::SourceVideoProperties;

/// What happened when a source video was looked for.
#[derive(Debug, PartialEq, Eq)]
pub enum OpeningOutcome {
    /// The video was found at `path` and opens.
    Opened {
        path: PathBuf,
        properties: SourceVideoProperties,
    },
    /// None of the places worked. `attempts` has each place tried and why it did not.
    Failed { attempts: Vec<(PathBuf, String)> },
}

/// Tries each of `candidates` in order, with `probe`, and uses the first that opens as a video.
pub fn open_first_that_works(
    candidates: &[PathBuf],
    probe: impl Fn(&Path) -> Result<SourceVideoProperties, ProbeError>,
) -> OpeningOutcome {
    let mut attempts = Vec::new();
    for candidate in candidates {
        match probe(candidate) {
            Ok(properties) => {
                return OpeningOutcome::Opened {
                    path: candidate.clone(),
                    properties,
                };
            }
            Err(error) => attempts.push((candidate.clone(), reason_in_words(candidate, &error))),
        }
    }
    OpeningOutcome::Failed { attempts }
}

/// Why `path` could not be used, for showing to the user.
fn reason_in_words(path: &Path, error: &ProbeError) -> String {
    if !path.exists() {
        return "there is no file there".to_owned();
    }
    match error {
        ProbeError::CouldNotOpen { message, .. } if !message.is_empty() => {
            format!("it could not be opened ({message})")
        }
        ProbeError::NoVideoStream => "it has no video in it".to_owned(),
        other => other.to_string(),
    }
}

impl OpeningOutcome {
    /// For a failure, all the attempts as one text. Empty for a success.
    pub fn describe_failure(&self) -> String {
        match self {
            Self::Opened { .. } => String::new(),
            Self::Failed { attempts } => attempts
                .iter()
                .map(|(path, reason)| format!("{}: {reason}", path.display()))
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    /// Like [`Self::describe_failure`], for when the place the video was last known to be is shown
    /// separately: a single failed attempt is that very place, so only why it failed is said. With
    /// several places the paths are what tells the lines apart, so they are kept.
    pub fn describe_failure_for_a_known_place(&self) -> String {
        match self {
            Self::Failed { attempts } if attempts.len() == 1 => attempts[0].1.clone(),
            _ => self.describe_failure(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::project::FrameRate;

    fn properties() -> SourceVideoProperties {
        SourceVideoProperties {
            frame_rate: FrameRate::new(30, 1).unwrap(),
            total_frame_count: 90,
            display_width_in_pixels: 320,
            display_height_in_pixels: 180,
            has_audio: false,
        }
    }

    fn failure() -> ProbeError {
        ProbeError::CouldNotOpen {
            path: PathBuf::from("x"),
            message: "Invalid data found".to_owned(),
        }
    }

    #[test]
    fn the_first_place_that_works_is_used_and_later_ones_are_not_tried() {
        let tried = RefCell::new(Vec::new());
        let candidates = [PathBuf::from("a.mp4"), PathBuf::from("b.mp4"), PathBuf::from("c.mp4")];
        let outcome = open_first_that_works(&candidates, |path| {
            tried.borrow_mut().push(path.to_owned());
            if path == Path::new("b.mp4") {
                Ok(properties())
            } else {
                Err(failure())
            }
        });
        assert_eq!(
            outcome,
            OpeningOutcome::Opened {
                path: PathBuf::from("b.mp4"),
                properties: properties()
            }
        );
        assert_eq!(*tried.borrow(), vec![PathBuf::from("a.mp4"), PathBuf::from("b.mp4")]);
    }

    #[test]
    fn when_nothing_works_every_attempt_is_reported_with_its_reason() {
        let candidates = [PathBuf::from("/no/such/folder/a.mp4"), PathBuf::from("/no/such/b.mp4")];
        let outcome = open_first_that_works(&candidates, |_| Err(failure()));
        let OpeningOutcome::Failed { attempts } = &outcome else {
            panic!("expected a failure, got {outcome:?}")
        };
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].0, PathBuf::from("/no/such/folder/a.mp4"));
        assert_eq!(attempts[0].1, "there is no file there");
        let description = outcome.describe_failure();
        assert!(description.contains("a.mp4: there is no file there"), "{description}");
        assert!(description.contains("b.mp4: there is no file there"), "{description}");
        assert_eq!(description.lines().count(), 2);
    }

    #[test]
    fn a_file_that_exists_but_is_not_a_video_says_so_instead_of_claiming_it_is_missing() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("notes.mp4");
        std::fs::write(&file, "text").unwrap();
        let outcome = open_first_that_works(std::slice::from_ref(&file), |_| Err(failure()));
        let OpeningOutcome::Failed { attempts } = outcome else {
            panic!("expected a failure")
        };
        assert_eq!(attempts[0].1, "it could not be opened (Invalid data found)");

        let outcome = open_first_that_works(std::slice::from_ref(&file), |_| Err(ProbeError::NoVideoStream));
        let OpeningOutcome::Failed { attempts } = outcome else {
            panic!("expected a failure")
        };
        assert_eq!(attempts[0].1, "it has no video in it");
    }

    #[test]
    fn with_no_places_to_try_there_is_nothing_to_report() {
        let outcome = open_first_that_works(&[], |_| Ok(properties()));
        assert_eq!(outcome, OpeningOutcome::Failed { attempts: vec![] });
        assert_eq!(outcome.describe_failure(), "");
    }

    #[test]
    fn when_the_place_is_shown_separately_a_single_attempt_only_says_why_it_failed() {
        let one_place = open_first_that_works(&[PathBuf::from("/no/such/a.mp4")], |_| Err(failure()));
        assert_eq!(one_place.describe_failure_for_a_known_place(), "there is no file there");

        // With several places the paths are what tells the lines apart, so they stay.
        let two_places = open_first_that_works(
            &[PathBuf::from("/no/such/a.mp4"), PathBuf::from("/no/such/b.mp4")],
            |_| Err(failure()),
        );
        assert_eq!(
            two_places.describe_failure_for_a_known_place(),
            two_places.describe_failure()
        );
        assert_eq!(
            OpeningOutcome::Failed { attempts: vec![] }.describe_failure_for_a_known_place(),
            ""
        );
    }

    #[test]
    fn a_success_has_no_failure_to_describe() {
        let outcome = OpeningOutcome::Opened {
            path: PathBuf::from("a.mp4"),
            properties: properties(),
        };
        assert_eq!(outcome.describe_failure(), "");
    }
}
