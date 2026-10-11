use std::path::PathBuf;

use super::*;
use crate::project::{
    Clip, FrameRate, MediaClip, SourceFrameIndex, SourceMedia, SourceMediaIdentifier, SourceMediaLink,
    SourceVideoProperties, SpeedFactor, SpeedIndicatorLayout, Track, TrackId,
};
use crate::test_support::DeterministicRandomNumbers;

const VIDEO: SourceMediaIdentifier = SourceMediaIdentifier::new(1);

/// A project with a long video and these tracks (the one in front first), each a list of clips as
/// the number of empty frames before it and how many frames it lasts. Also gives where each clip is.
fn project_of(tracks: &[&[(u64, u64)]]) -> (Project, Vec<Vec<ClipLocation>>) {
    let video = SourceMedia::video(
        VIDEO,
        SourceMediaLink::from_absolute_path(PathBuf::from("holiday.mp4")),
        SourceVideoProperties {
            frame_rate: FrameRate::new(30, 1).unwrap(),
            total_frame_count: 1_000_000,
            display_width_in_pixels: 320,
            display_height_in_pixels: 180,
            has_audio: false,
        },
    );
    let tracks: Vec<Track> = tracks
        .iter()
        .enumerate()
        .map(|(number, clips)| {
            Track::new(TrackId::new(number as u32), format!("track {number}")).with_clips(clips.iter().map(
                |&(gap, frames)| {
                    let clip = MediaClip::new(
                        VIDEO,
                        SourceFrameIndex::new(0),
                        SourceFrameIndex::new(frames),
                        SpeedFactor::NORMAL,
                    )
                    .unwrap();
                    (gap, Clip::Video(clip))
                },
            ))
        })
        .collect();
    let project = Project::from_parts(vec![video], tracks, SpeedIndicatorLayout::DEFAULT).unwrap();
    let locations = project
        .tracks()
        .iter()
        .map(|track| {
            track
                .entries()
                .iter()
                .map(|entry| ClipLocation {
                    track: track.id(),
                    clip: entry.id(),
                })
                .collect()
        })
        .collect();
    (project, locations)
}

#[test]
fn a_clip_with_room_all_round_goes_as_far_as_wanted() {
    let (project, clips) = project_of(&[&[(100, 50)]]);
    assert_eq!(allowed_move_in_frames(&project, &clips[0], 30), 30);
    assert_eq!(allowed_move_in_frames(&project, &clips[0], -30), -30);
    assert_eq!(allowed_move_in_frames(&project, &clips[0], 0), 0);
    assert_eq!(allowed_move_in_frames(&project, &clips[0], 1_000_000), 1_000_000);
}

#[test]
fn a_clip_cannot_be_moved_to_before_the_start_of_the_timeline() {
    let (project, clips) = project_of(&[&[(100, 50)]]);
    assert_eq!(allowed_move_in_frames(&project, &clips[0], -300), -100);
}

#[test]
fn a_clip_stops_where_it_touches_the_clip_after_it_and_the_one_before_it() {
    // Clips at 10..30, 60..80 and 120..140.
    let (project, clips) = project_of(&[&[(10, 20), (30, 20), (40, 20)]]);
    let middle = [clips[0][1]];
    assert_eq!(
        allowed_move_in_frames(&project, &middle, 500),
        40,
        "up to the clip after it"
    );
    assert_eq!(
        allowed_move_in_frames(&project, &middle, -500),
        -30,
        "and back to the clip before it"
    );
    assert_eq!(
        allowed_move_in_frames(&project, &middle, 15),
        15,
        "or not as far, if that is as far as it was taken"
    );
}

#[test]
fn a_clip_that_already_touches_the_next_one_cannot_go_towards_it_but_can_go_away() {
    let (project, clips) = project_of(&[&[(0, 20), (0, 20)]]);
    let first = [clips[0][0]];
    assert_eq!(allowed_move_in_frames(&project, &first, 10), 0);
    assert_eq!(
        allowed_move_in_frames(&project, &first, -10),
        0,
        "and it is at the start of the timeline"
    );
    let second = [clips[0][1]];
    assert_eq!(allowed_move_in_frames(&project, &second, 10), 10);
    assert_eq!(allowed_move_in_frames(&project, &second, -10), 0);
}

#[test]
fn clips_that_move_together_do_not_stop_each_other_and_only_the_clips_that_stay_stop_them() {
    // Clips at 0..20, 20..40 (touching), 100..120 and 200..220.
    let (project, clips) = project_of(&[&[(0, 20), (0, 20), (60, 20), (80, 20)]]);
    let first_two = [clips[0][0], clips[0][1]];
    assert_eq!(
        allowed_move_in_frames(&project, &first_two, 500),
        60,
        "until the second touches the third"
    );
    let first_and_third = [clips[0][0], clips[0][2]];
    assert_eq!(
        allowed_move_in_frames(&project, &first_and_third, 500),
        0,
        "the first is up against the second, which stays"
    );
    let last_two = [clips[0][2], clips[0][3]];
    assert_eq!(
        allowed_move_in_frames(&project, &last_two, -500),
        -60,
        "back to the end of the second clip"
    );
}

#[test]
fn clips_on_different_tracks_are_not_in_each_others_way() {
    let (project, clips) = project_of(&[&[(10, 20)], &[(25, 20)]]);
    assert_eq!(allowed_move_in_frames(&project, &clips[0], 100), 100);
    assert_eq!(
        allowed_move_in_frames(&project, &clips[1], -100),
        -25,
        "only the start of the timeline"
    );
    let both = [clips[0][0], clips[1][0]];
    assert_eq!(
        allowed_move_in_frames(&project, &both, -100),
        -10,
        "the one that is nearest to the start stops them"
    );
}

#[test]
fn a_clip_that_is_not_there_does_not_hold_anything_up() {
    let (project, clips) = project_of(&[&[(10, 20)]]);
    let gone = ClipLocation {
        track: clips[0][0].track,
        clip: crate::project::ClipId::new(99),
    };
    assert_eq!(allowed_move_in_frames(&project, &[gone], 40), 40);
    assert_eq!(allowed_move_in_frames(&project, &[], 40), 40);
    assert_eq!(allowed_move_in_frames(&project, &[gone, clips[0][0]], -40), -10);
}

#[test]
fn the_new_starts_are_the_old_ones_moved_and_never_before_the_start_of_the_timeline() {
    let (project, clips) = project_of(&[&[(10, 20), (30, 20)]]);
    let moved = starts_after_moving_by(&project, &clips[0], 7);
    assert_eq!(
        moved,
        vec![
            (clips[0][0], TimelineFrameIndex::new(17)),
            (clips[0][1], TimelineFrameIndex::new(67))
        ]
    );
    let moved = starts_after_moving_by(&project, &clips[0], -10_000);
    assert_eq!(moved[0].1, TimelineFrameIndex::new(0));
}

/// A small random project: up to three tracks of up to six clips each.
fn random_project(random: &mut DeterministicRandomNumbers) -> (Project, Vec<ClipLocation>) {
    let track_count = 1 + random.next_below(3) as usize;
    let tracks: Vec<Vec<(u64, u64)>> = (0..track_count)
        .map(|_| {
            (0..random.next_below(7))
                .map(|_| (random.next_below(30), 1 + random.next_below(40)))
                .collect()
        })
        .collect();
    let slices: Vec<&[(u64, u64)]> = tracks.iter().map(Vec::as_slice).collect();
    let (project, locations) = project_of(&slices);
    (project, locations.into_iter().flatten().collect())
}

#[test]
fn whatever_is_allowed_can_be_done_and_a_little_more_could_not_be_for_random_projects_and_moves() {
    let mut random = DeterministicRandomNumbers::with_seed(2024);
    for round in 0..400 {
        let (project, all_clips) = random_project(&mut random);
        if all_clips.is_empty() {
            continue;
        }
        let moving: Vec<ClipLocation> = all_clips
            .iter()
            .copied()
            .filter(|_| random.next_below(3) == 0)
            .collect();
        let wanted = random.next_below(400) as i64 - 200;

        let allowed = allowed_move_in_frames(&project, &moving, wanted);

        // It is between nothing and what was wanted.
        assert!(
            (allowed >= 0 && allowed <= wanted) || (allowed <= 0 && allowed >= wanted),
            "round {round}: {allowed} is not between 0 and {wanted}"
        );
        // It can be done...
        let mut moved = project.clone();
        let starts = starts_after_moving_by(&project, &moving, allowed);
        assert_eq!(moved.move_clips(&starts), Ok(()), "round {round}: moving by {allowed}");
        // ...and one frame more in the same direction, if that was wanted, could not.
        if allowed != wanted && !moving.is_empty() {
            let one_more = allowed + wanted.signum();
            let starts = starts_after_moving_by(&project, &moving, one_more);
            let mut moved = project.clone();
            let cannot_be_done = moved.move_clips(&starts).is_err()
                || starts.iter().any(|(clip, start)| {
                    // A start before the start of the timeline is made 0 instead, which hides that it was refused.
                    let placement = project.placement_of(*clip).unwrap();
                    (placement.first_timeline_frame.value() as i64 + one_more) < 0 && start.value() == 0
                });
            assert!(
                cannot_be_done,
                "round {round}: {one_more} was allowed after all, but {allowed} was given"
            );
        }
    }
}
