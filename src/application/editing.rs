//! Edits to the project that also have to keep the current frame where the user left it.
//!
//! Changing the speed of a clip or a track changes how long it is, and taking a clip away closes
//! the gap it leaves, so the clips after it move along the timeline. Without this the current
//! frame would stay at the same position on the timeline and so show something else. Instead it
//! follows the source frame it was showing.

use crate::project::{
    Clip, ClipLocation, EditError, Project, SourceFrameIndex, SpeedFactor, TimelineFrameIndex, TrackId,
};

/// What the picture on screen at some frame is made from: the video clip that is in front there,
/// and which of its source frames shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ShownPicture {
    clip: ClipLocation,
    source_frame: SourceFrameIndex,
}

/// The picture shown at `frame`, or `None` if it is black there (there is no video at the frame).
fn picture_shown_at(project: &Project, frame: TimelineFrameIndex) -> Option<ShownPicture> {
    project.tracks().iter().find_map(|track| {
        let placement = track.placement_covering(frame)?;
        if !matches!(placement.clip, Clip::Video(_)) {
            return None;
        }
        Some(ShownPicture {
            clip: ClipLocation {
                track: track.id(),
                clip: placement.id,
            },
            source_frame: placement.source_frame_at(frame)?,
        })
    })
}

/// The frame at which the picture `shown` is on screen now, if the clip it comes from is still
/// there and still has it.
fn frame_showing(project: &Project, shown: ShownPicture) -> Option<TimelineFrameIndex> {
    let placement = project.placement_of(shown.clip)?;
    let offset = placement
        .clip
        .as_media()?
        .timeline_offset_of_source_frame(placement.effective_speed, shown.source_frame)?;
    Some(TimelineFrameIndex::new(placement.first_timeline_frame.value() + offset))
}

/// Makes `edit` to the project and returns where the current frame has to move to so that the
/// picture on screen stays the same. If the picture is gone (its clip was taken away, say) the
/// current frame stays where it is, but never goes beyond the end of the timeline.
fn keeping_the_picture<Failure>(
    project: &mut Project,
    current_frame: TimelineFrameIndex,
    edit: impl FnOnce(&mut Project) -> Result<(), Failure>,
) -> Result<TimelineFrameIndex, Failure> {
    let shown_before = picture_shown_at(project, current_frame);
    edit(project)?;
    let last_frame = project.total_frame_count().saturating_sub(1);
    let following = shown_before.and_then(|shown| frame_showing(project, shown));
    Ok(following.unwrap_or(TimelineFrameIndex::new(current_frame.value().min(last_frame))))
}

/// Plays the clip `clip` at `speed_factor` (on top of the speed of its track), and returns where
/// the current frame has to move to so that the picture on screen stays the same.
pub fn set_clip_speed_keeping_the_picture(
    project: &mut Project,
    current_frame: TimelineFrameIndex,
    clip: ClipLocation,
    speed_factor: SpeedFactor,
) -> Result<TimelineFrameIndex, EditError> {
    keeping_the_picture(project, current_frame, |project| {
        project.set_clip_speed(clip, speed_factor)
    })
}

/// Makes everything on the track `track` play at `speed_factor` times the speed of the clips' own,
/// and returns where the current frame has to move to so that the picture on screen stays the same.
pub fn set_track_speed_keeping_the_picture(
    project: &mut Project,
    current_frame: TimelineFrameIndex,
    track: TrackId,
    speed_factor: SpeedFactor,
) -> Result<TimelineFrameIndex, EditError> {
    keeping_the_picture(project, current_frame, |project| {
        project.set_track_speed(track, speed_factor)
    })
}

/// Takes the clip `clip` off its track, and returns where the current frame has to move to so
/// that the picture on screen stays the same (if the picture was not the deleted clip's).
pub fn delete_clip_keeping_the_picture(
    project: &mut Project,
    current_frame: TimelineFrameIndex,
    clip: ClipLocation,
) -> Result<TimelineFrameIndex, EditError> {
    keeping_the_picture(project, current_frame, |project| project.delete_clip(clip))
}

/// Makes the project what `earlier` is (see [`Project::restore`]), as undoing and redoing do, and
/// returns where the current frame has to move to so that the picture on screen stays the same: so
/// that undoing an edit that moved clips along the timeline puts the current frame back where it
/// was. If the picture is gone the current frame stays where it is, but never goes beyond the end
/// of the timeline.
pub fn restore_keeping_the_picture(
    project: &mut Project,
    current_frame: TimelineFrameIndex,
    earlier: Project,
) -> TimelineFrameIndex {
    match keeping_the_picture::<std::convert::Infallible>(project, current_frame, |project| {
        project.restore(earlier);
        Ok(())
    }) {
        Ok(frame) => frame,
        Err(never) => match never {},
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::project::{
        ClipEditError, ClipId, FrameRate, SourceMedia, SourceMediaIdentifier, SourceMediaLink, SourceVideoProperties,
        TextObject,
    };

    fn video_source() -> SourceMedia {
        SourceMedia::video(
            SourceMediaIdentifier::new(1),
            SourceMediaLink::from_absolute_path(PathBuf::from("holiday.mp4")),
            SourceVideoProperties {
                frame_rate: FrameRate::new(30, 1).unwrap(),
                total_frame_count: 600,
                display_width_in_pixels: 320,
                display_height_in_pixels: 180,
                has_audio: false,
            },
        )
    }

    /// A project whose one track has the 600 frame video cut at the given frames.
    fn project_with_clips_at(split_frames: &[u64]) -> Project {
        let mut project = Project::empty();
        project
            .import_source(video_source(), TimelineFrameIndex::new(0))
            .unwrap();
        for &frame in split_frames {
            project.split_at(TimelineFrameIndex::new(frame), None);
        }
        project
    }

    /// The `index`th clip of the front-most track.
    fn clip(project: &Project, index: usize) -> ClipLocation {
        let track = &project.tracks()[0];
        ClipLocation {
            track: track.id(),
            clip: track.placement_at_index(index).unwrap().id,
        }
    }

    fn source_frame_shown(project: &Project, frame: TimelineFrameIndex) -> SourceFrameIndex {
        picture_shown_at(project, frame).unwrap().source_frame
    }

    fn frame(number: u64) -> TimelineFrameIndex {
        TimelineFrameIndex::new(number)
    }

    #[test]
    fn speeding_up_an_earlier_clip_moves_the_current_frame_back_to_keep_the_same_picture() {
        let mut project = project_with_clips_at(&[200, 400]);
        let current = frame(450); // in the last clip, showing source frame 450
        let shown_before = source_frame_shown(&project, current);
        let first_clip = clip(&project, 0);

        // Clip 0 (200 frames) at 4x becomes 50 frames: everything after it moves 150 frames earlier.
        let moved = set_clip_speed_keeping_the_picture(&mut project, current, first_clip, SpeedFactor::new(4).unwrap())
            .unwrap();

        assert_eq!(moved, frame(300));
        assert_eq!(source_frame_shown(&project, moved), shown_before);
    }

    #[test]
    fn speeding_up_the_clip_the_current_frame_is_in_keeps_the_picture_it_shows() {
        let mut project = project_with_clips_at(&[200, 400]);
        let current = frame(250); // in clip 1 (200..400), source frame 250
        let middle_clip = clip(&project, 1);

        let moved =
            set_clip_speed_keeping_the_picture(&mut project, current, middle_clip, SpeedFactor::new(5).unwrap())
                .unwrap();

        // Clip 1 now has 40 frames starting at 200; source frame 250 is 50 in: picture number 10.
        assert_eq!(moved, frame(210));
        assert_eq!(source_frame_shown(&project, moved), SourceFrameIndex::new(250));
    }

    #[test]
    fn speeding_up_a_clip_after_the_current_frame_leaves_it_where_it_is() {
        let mut project = project_with_clips_at(&[200, 400]);
        let last_clip = clip(&project, 2);
        let moved =
            set_clip_speed_keeping_the_picture(&mut project, frame(100), last_clip, SpeedFactor::new(8).unwrap())
                .unwrap();
        assert_eq!(moved, frame(100));
    }

    #[test]
    fn slowing_a_clip_back_down_returns_the_current_frame_to_where_it_was() {
        let mut project = project_with_clips_at(&[200, 400]);
        let current = frame(500);
        let first_clip = clip(&project, 0);
        let sped_up =
            set_clip_speed_keeping_the_picture(&mut project, current, first_clip, SpeedFactor::new(10).unwrap())
                .unwrap();
        assert_ne!(sped_up, current);
        let restored =
            set_clip_speed_keeping_the_picture(&mut project, sped_up, first_clip, SpeedFactor::NORMAL).unwrap();
        assert_eq!(restored, current);
    }

    #[test]
    fn the_current_frame_never_ends_up_beyond_the_end_of_the_timeline() {
        let mut project = project_with_clips_at(&[200]);
        let last_frame = frame(project.total_frame_count() - 1);
        let second_clip = clip(&project, 1);
        let moved =
            set_clip_speed_keeping_the_picture(&mut project, last_frame, second_clip, SpeedFactor::new(50).unwrap())
                .unwrap();
        assert!(moved.value() < project.total_frame_count());
    }

    #[test]
    fn an_unknown_clip_is_an_error_and_changes_nothing() {
        let mut project = project_with_clips_at(&[200]);
        let before = project.clone();
        let nonexistent = ClipLocation {
            track: project.tracks()[0].id(),
            clip: ClipId::new(99),
        };
        let result =
            set_clip_speed_keeping_the_picture(&mut project, frame(10), nonexistent, SpeedFactor::new(2).unwrap());
        assert_eq!(
            result,
            Err(EditError::Clip(ClipEditError::UnknownClip { clip: ClipId::new(99) }))
        );
        assert_eq!(project, before);
    }

    #[test]
    fn an_empty_project_has_no_clip_to_change() {
        let mut project = Project::empty();
        let nowhere = ClipLocation {
            track: project.tracks()[0].id(),
            clip: ClipId::new(0),
        };
        assert!(
            set_clip_speed_keeping_the_picture(&mut project, frame(0), nowhere, SpeedFactor::new(2).unwrap()).is_err()
        );
    }

    #[test]
    fn speeding_up_a_track_moves_the_current_frame_to_keep_the_same_picture() {
        let mut project = project_with_clips_at(&[200, 400]);
        let track = project.tracks()[0].id();
        let current = frame(450); // source frame 450, in the third clip

        // At double speed every clip is 100 frames: the third starts at 200, and 50 source frames in is 25 frames in.
        let moved =
            set_track_speed_keeping_the_picture(&mut project, current, track, SpeedFactor::new(2).unwrap()).unwrap();

        assert_eq!(moved, frame(225));
        assert_eq!(source_frame_shown(&project, moved), SourceFrameIndex::new(450));
        let back = set_track_speed_keeping_the_picture(&mut project, moved, track, SpeedFactor::NORMAL).unwrap();
        assert_eq!(back, current);
    }

    #[test]
    fn a_track_that_does_not_exist_cannot_have_its_speed_changed() {
        let mut project = project_with_clips_at(&[]);
        let result =
            set_track_speed_keeping_the_picture(&mut project, frame(0), TrackId::new(77), SpeedFactor::new(2).unwrap());
        assert_eq!(
            result,
            Err(EditError::UnknownTrack {
                track: TrackId::new(77)
            })
        );
    }

    #[test]
    fn deleting_an_earlier_clip_moves_the_current_frame_back_with_the_picture() {
        let mut project = project_with_clips_at(&[200, 400]);
        let first_clip = clip(&project, 0);
        let current = frame(450);

        // The clips after the deleted one move up 200 frames, and the current frame goes with them.
        let moved = delete_clip_keeping_the_picture(&mut project, current, first_clip).unwrap();

        assert_eq!(moved, frame(250));
        assert_eq!(source_frame_shown(&project, moved), SourceFrameIndex::new(450));
    }

    #[test]
    fn deleting_the_clip_that_is_shown_leaves_the_current_frame_where_it_is_inside_the_timeline() {
        let mut project = project_with_clips_at(&[200, 400]);
        let last_clip = clip(&project, 2);
        // Deleting the last clip: nothing follows it, and the timeline is now 400 frames long.
        let moved = delete_clip_keeping_the_picture(&mut project, frame(550), last_clip).unwrap();
        assert_eq!(moved, frame(399));

        let middle_clip = clip(&project, 1);
        let moved = delete_clip_keeping_the_picture(&mut project, frame(250), middle_clip).unwrap();
        assert_eq!(moved, frame(199), "the timeline is 200 frames long now");
    }

    #[test]
    fn the_picture_that_is_followed_is_the_video_and_not_the_text_in_front_of_it() {
        // The video, cut in three, on the back track; a text over the whole of it on a track in front.
        let mut project = project_with_clips_at(&[200, 400]);
        let first_clip = clip(&project, 0);
        let text_track = project.add_track_in_front();
        project
            .insert_text(text_track, frame(0), TextObject::default(), 600)
            .expect("there is room for the text");

        let moved =
            set_clip_speed_keeping_the_picture(&mut project, frame(450), first_clip, SpeedFactor::new(4).unwrap())
                .unwrap();

        assert_eq!(
            moved,
            frame(300),
            "what is followed is the video, which the text is over"
        );
    }

    #[test]
    fn a_clip_on_a_track_behind_the_one_shown_does_not_move_the_current_frame() {
        // Two videos at the same place: the second one is on a new track in front of the first, so it is the one seen.
        let mut project = project_with_clips_at(&[]);
        let behind_clip = clip(&project, 0);
        let mut second_video = video_source();
        second_video.identifier = SourceMediaIdentifier::new(2);
        let imported = project.import_source(second_video, frame(0)).unwrap();
        assert!(imported.created_track);
        let front_clip = ClipLocation {
            track: imported.track,
            clip: imported.clip.unwrap(),
        };
        assert_eq!(picture_shown_at(&project, frame(100)).unwrap().clip, front_clip);

        let moved =
            set_clip_speed_keeping_the_picture(&mut project, frame(100), behind_clip, SpeedFactor::new(10).unwrap())
                .unwrap();
        assert_eq!(moved, frame(100), "the picture seen did not move");
    }

    #[test]
    fn undoing_a_speed_change_puts_the_current_frame_back_where_it_was() {
        let mut project = project_with_clips_at(&[200, 400]);
        let before = project.clone();
        let current = frame(450);
        let first_clip = clip(&project, 0);
        let moved = set_clip_speed_keeping_the_picture(&mut project, current, first_clip, SpeedFactor::new(4).unwrap())
            .unwrap();
        assert_eq!(moved, frame(300));

        let restored = restore_keeping_the_picture(&mut project, moved, before.clone());

        assert_eq!(project, before);
        assert_eq!(
            restored, current,
            "the same picture is on screen, which is where it was"
        );
    }

    #[test]
    fn restoring_a_project_in_which_the_picture_is_gone_keeps_the_current_frame_inside_the_timeline() {
        let mut project = project_with_clips_at(&[200, 400]);
        let restored = restore_keeping_the_picture(&mut project, frame(450), Project::empty());
        assert_eq!(restored, frame(0), "there is no timeline at all, so the first frame");

        let mut project = project_with_clips_at(&[200, 400]);
        let shorter = {
            let mut shorter = project.clone();
            let last = clip(&shorter, 2);
            shorter.delete_clip(last).unwrap();
            shorter
        };
        let restored = restore_keeping_the_picture(&mut project, frame(500), shorter);
        assert_eq!(restored, frame(399), "the last frame of what is left");
    }
}
