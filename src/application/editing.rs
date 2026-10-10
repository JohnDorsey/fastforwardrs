//! Edits to the project that also have to keep the current frame where the user left it.

use crate::project::{Project, SpeedFactor, TimelineFrameIndex, UnknownClip};

/// Plays the clip at `clip_index` at `speed_factor`, and returns where the current frame has to
/// move to so that the picture on screen stays the same.
///
/// Changing a clip's speed changes its length, which moves every clip after it along the timeline.
/// Without this the current frame would stay at the same position on the timeline and so show
/// something else. Instead it follows the source frame it was showing.
pub fn set_clip_speed_keeping_the_picture(
    project: &mut Project,
    current_frame: TimelineFrameIndex,
    clip_index: usize,
    speed_factor: SpeedFactor,
) -> Result<TimelineFrameIndex, UnknownClip> {
    let shown_before = project.video_track().locate(current_frame).and_then(|location| {
        project
            .video_track()
            .clips()
            .get(location.clip_index)
            .map(|clip| (clip.source_video_identifier(), location.source_frame))
    });

    project.set_clip_speed(clip_index, speed_factor)?;

    let last_frame = project.video_track().total_frame_count().saturating_sub(1);
    let following = shown_before.and_then(|(video, source_frame)| {
        project
            .video_track()
            .timeline_frame_showing_source_frame(video, source_frame)
    });
    Ok(following.unwrap_or(TimelineFrameIndex::new(current_frame.value().min(last_frame))))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::project::{
        FrameRate, SourceFrameIndex, SourceVideo, SourceVideoIdentifier, SourceVideoLink, SourceVideoProperties,
    };

    fn project_with_clips_at(split_frames: &[u64]) -> Project {
        let mut project = Project::empty();
        project
            .import_source_video(SourceVideo {
                identifier: SourceVideoIdentifier::new(1),
                link: SourceVideoLink::from_absolute_path(PathBuf::from("holiday.mp4")),
                properties: SourceVideoProperties {
                    frame_rate: FrameRate::new(30, 1).unwrap(),
                    total_frame_count: 600,
                    display_width_in_pixels: 320,
                    display_height_in_pixels: 180,
                    has_audio: false,
                },
            })
            .unwrap();
        for &frame in split_frames {
            project.split_at(TimelineFrameIndex::new(frame)).unwrap();
        }
        project
    }

    fn source_frame_shown(project: &Project, frame: TimelineFrameIndex) -> SourceFrameIndex {
        project.video_track().locate(frame).unwrap().source_frame
    }

    #[test]
    fn speeding_up_an_earlier_clip_moves_the_current_frame_back_to_keep_the_same_picture() {
        let mut project = project_with_clips_at(&[200, 400]);
        let current = TimelineFrameIndex::new(450); // in the last clip, showing source frame 450
        let shown_before = source_frame_shown(&project, current);

        // Clip 0 (200 frames) at 4x becomes 50 frames: everything after it moves 150 frames earlier.
        let moved = set_clip_speed_keeping_the_picture(&mut project, current, 0, SpeedFactor::new(4).unwrap()).unwrap();

        assert_eq!(moved, TimelineFrameIndex::new(300));
        assert_eq!(source_frame_shown(&project, moved), shown_before);
    }

    #[test]
    fn speeding_up_the_clip_the_current_frame_is_in_keeps_the_picture_it_shows() {
        let mut project = project_with_clips_at(&[200, 400]);
        let current = TimelineFrameIndex::new(250); // in clip 1 (200..400), source frame 250

        let moved = set_clip_speed_keeping_the_picture(&mut project, current, 1, SpeedFactor::new(5).unwrap()).unwrap();

        // Clip 1 now has 40 frames starting at 200; source frame 250 is 50 in: picture number 10.
        assert_eq!(moved, TimelineFrameIndex::new(210));
        assert_eq!(source_frame_shown(&project, moved), SourceFrameIndex::new(250));
    }

    #[test]
    fn speeding_up_a_clip_after_the_current_frame_leaves_it_where_it_is() {
        let mut project = project_with_clips_at(&[200, 400]);
        let current = TimelineFrameIndex::new(100);
        let moved = set_clip_speed_keeping_the_picture(&mut project, current, 2, SpeedFactor::new(8).unwrap()).unwrap();
        assert_eq!(moved, current);
    }

    #[test]
    fn slowing_a_clip_back_down_returns_the_current_frame_to_where_it_was() {
        let mut project = project_with_clips_at(&[200, 400]);
        let current = TimelineFrameIndex::new(500);
        let sped_up =
            set_clip_speed_keeping_the_picture(&mut project, current, 0, SpeedFactor::new(10).unwrap()).unwrap();
        assert_ne!(sped_up, current);
        let restored = set_clip_speed_keeping_the_picture(&mut project, sped_up, 0, SpeedFactor::NORMAL).unwrap();
        assert_eq!(restored, current);
    }

    #[test]
    fn the_current_frame_never_ends_up_beyond_the_end_of_the_timeline() {
        let mut project = project_with_clips_at(&[200]);
        let last_frame = TimelineFrameIndex::new(project.video_track().total_frame_count() - 1);
        let moved =
            set_clip_speed_keeping_the_picture(&mut project, last_frame, 1, SpeedFactor::new(50).unwrap()).unwrap();
        assert!(moved.value() < project.video_track().total_frame_count());
    }

    #[test]
    fn an_unknown_clip_is_an_error_and_changes_nothing() {
        let mut project = project_with_clips_at(&[200]);
        let before = project.clone();
        let result = set_clip_speed_keeping_the_picture(
            &mut project,
            TimelineFrameIndex::new(10),
            9,
            SpeedFactor::new(2).unwrap(),
        );
        assert_eq!(
            result,
            Err(UnknownClip {
                clip_index: 9,
                clip_count: 2
            })
        );
        assert_eq!(project, before);
    }

    #[test]
    fn an_empty_project_has_no_clip_to_change() {
        let mut project = Project::empty();
        let result = set_clip_speed_keeping_the_picture(
            &mut project,
            TimelineFrameIndex::new(0),
            0,
            SpeedFactor::new(2).unwrap(),
        );
        assert!(result.is_err());
    }
}
