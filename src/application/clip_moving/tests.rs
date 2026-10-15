// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;

use super::*;
use crate::project::{
    Clip, FrameRate, MediaClip, SourceFrameIndex, SourceMedia, SourceMediaIdentifier, SourceMediaLink,
    SourceVideoProperties, SpeedFactor, SpeedIndicatorLayout, Track, TrackId,
};
use crate::test_support::DeterministicRandomNumbers;

const VIDEO: SourceMediaIdentifier = SourceMediaIdentifier::new(1);

/// How many frames the clips `moving` can be moved along their tracks, when `wanted` is how many are wanted.
fn frames_allowed(project: &Project, moving: &[ClipId], wanted: i64) -> i64 {
    allowed_move(
        project,
        moving,
        MoveBy {
            tracks_down: 0,
            frames: wanted,
        },
    )
    .frames
}

/// The moves that put the clips `moving` `frames` frames later, on the tracks they are on.
fn moves_along(project: &Project, moving: &[ClipId], frames: i64) -> Vec<ClipMove> {
    moves_by(project, moving, MoveBy { tracks_down: 0, frames })
}

/// What each of the moves says: the clip, the position in the list of tracks of the track it goes to, and
/// the frame it starts at.
fn as_numbers(project: &Project, moves: &[ClipMove]) -> Vec<(ClipId, usize, u64)> {
    moves
        .iter()
        .map(|moved| {
            (
                moved.clip,
                project.track_position(moved.to_track).unwrap(),
                moved.start.value(),
            )
        })
        .collect()
}

/// A project with a long video and these tracks (the one in front first), each a list of clips as
/// the number of empty frames before it and how many frames it lasts. Also gives where each clip is.
pub(super) fn project_of(tracks: &[&[(u64, u64)]]) -> (Project, Vec<Vec<ClipId>>) {
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
        .map(|track| track.entries().iter().map(|entry| entry.id()).collect())
        .collect();
    (project, locations)
}

#[test]
fn a_clip_with_room_all_round_goes_as_far_as_wanted() {
    let (project, clips) = project_of(&[&[(100, 50)]]);
    assert_eq!(frames_allowed(&project, &clips[0], 30), 30);
    assert_eq!(frames_allowed(&project, &clips[0], -30), -30);
    assert_eq!(frames_allowed(&project, &clips[0], 0), 0);
    assert_eq!(frames_allowed(&project, &clips[0], 1_000_000), 1_000_000);
}

#[test]
fn a_clip_cannot_be_moved_to_before_the_start_of_the_timeline() {
    let (project, clips) = project_of(&[&[(100, 50)]]);
    assert_eq!(frames_allowed(&project, &clips[0], -300), -100);
}

#[test]
fn a_clip_stops_where_it_touches_the_clip_after_it_and_the_one_before_it() {
    // Clips at 10..30, 60..80 and 120..140.
    let (project, clips) = project_of(&[&[(10, 20), (30, 20), (40, 20)]]);
    let middle = [clips[0][1]];
    assert_eq!(frames_allowed(&project, &middle, 500), 40, "up to the clip after it");
    assert_eq!(
        frames_allowed(&project, &middle, -500),
        -30,
        "and back to the clip before it"
    );
    assert_eq!(
        frames_allowed(&project, &middle, 15),
        15,
        "or not as far, if that is as far as it was taken"
    );
}

#[test]
fn a_clip_that_already_touches_the_next_one_cannot_go_towards_it_but_can_go_away() {
    let (project, clips) = project_of(&[&[(0, 20), (0, 20)]]);
    let first = [clips[0][0]];
    assert_eq!(frames_allowed(&project, &first, 10), 0);
    assert_eq!(
        frames_allowed(&project, &first, -10),
        0,
        "and it is at the start of the timeline"
    );
    let second = [clips[0][1]];
    assert_eq!(frames_allowed(&project, &second, 10), 10);
    assert_eq!(frames_allowed(&project, &second, -10), 0);
}

#[test]
fn clips_that_move_together_do_not_stop_each_other_and_only_the_clips_that_stay_stop_them() {
    // Clips at 0..20, 20..40 (touching), 100..120 and 200..220.
    let (project, clips) = project_of(&[&[(0, 20), (0, 20), (60, 20), (80, 20)]]);
    let first_two = [clips[0][0], clips[0][1]];
    assert_eq!(
        frames_allowed(&project, &first_two, 500),
        60,
        "until the second touches the third"
    );
    let first_and_third = [clips[0][0], clips[0][2]];
    assert_eq!(
        frames_allowed(&project, &first_and_third, 500),
        0,
        "the first is up against the second, which stays"
    );
    let last_two = [clips[0][2], clips[0][3]];
    assert_eq!(
        frames_allowed(&project, &last_two, -500),
        -60,
        "back to the end of the second clip"
    );
}

#[test]
fn clips_on_different_tracks_are_not_in_each_others_way() {
    let (project, clips) = project_of(&[&[(10, 20)], &[(25, 20)]]);
    assert_eq!(frames_allowed(&project, &clips[0], 100), 100);
    assert_eq!(
        frames_allowed(&project, &clips[1], -100),
        -25,
        "only the start of the timeline"
    );
    let both = [clips[0][0], clips[1][0]];
    assert_eq!(
        frames_allowed(&project, &both, -100),
        -10,
        "the one that is nearest to the start stops them"
    );
}

#[test]
fn a_clip_that_is_not_there_does_not_hold_anything_up() {
    let (project, clips) = project_of(&[&[(10, 20)]]);
    let gone = ClipId::new(clips[0][0].origin(), 99);
    assert_eq!(frames_allowed(&project, &[gone], 40), 40);
    assert_eq!(frames_allowed(&project, &[], 40), 40);
    assert_eq!(frames_allowed(&project, &[gone, clips[0][0]], -40), -10);
}

#[test]
fn the_new_starts_are_the_old_ones_moved_and_never_before_the_start_of_the_timeline() {
    let (project, clips) = project_of(&[&[(10, 20), (30, 20)]]);
    let moved = moves_along(&project, &clips[0], 7);
    assert_eq!(
        as_numbers(&project, &moved),
        vec![(clips[0][0], 0, 17), (clips[0][1], 0, 67)]
    );
    let moved = moves_along(&project, &clips[0], -10_000);
    assert_eq!(as_numbers(&project, &moved)[0].2, 0);
}

#[test]
fn clips_moved_down_go_to_the_track_that_many_below_with_the_start_they_had_and_those_that_would_be_off_the_tracks_are_left_out()
 {
    let (project, clips) = project_of(&[&[(10, 20)], &[(30, 20)], &[(50, 20)]]);
    let all = [clips[0][0], clips[1][0], clips[2][0]];
    let by = |tracks_down, frames| as_numbers(&project, &moves_by(&project, &all, MoveBy { tracks_down, frames }));
    assert_eq!(
        by(1, 5),
        vec![(clips[0][0], 1, 15), (clips[1][0], 2, 35)],
        "the one on the last track would go off the tracks"
    );
    assert_eq!(by(-1, 0), vec![(clips[1][0], 0, 30), (clips[2][0], 1, 50)]);
    assert_eq!(by(-5, 0), vec![]);
}

// --- going to other tracks ---

/// How far the clips `moving` are allowed to go when `wanted` is how far, and where that puts them.
fn where_they_go(project: &Project, moving: &[ClipId], wanted: MoveBy) -> (MoveBy, Vec<(ClipId, usize, u64)>) {
    let allowed = allowed_move(project, moving, wanted);
    let moves = moves_by(project, moving, allowed);
    (allowed, as_numbers(project, &moves))
}

/// How many tracks down the clip `clip` is allowed to go when `tracks_down` are wanted (and it stays at its frame).
fn tracks_allowed(project: &Project, clip: ClipId, tracks_down: i64) -> i64 {
    allowed_move(project, &[clip], MoveBy { tracks_down, frames: 0 }).tracks_down
}

#[test]
fn clips_go_down_to_the_track_wanted_when_they_fit_there_where_they_are_along_the_timeline() {
    // Clips at 100..150 on the first track, 0..40 on the second, and nothing on the third.
    let (project, clips) = project_of(&[&[(100, 50)], &[(0, 40)], &[]]);
    assert_eq!(tracks_allowed(&project, clips[0][0], 1), 1);
    assert_eq!(
        tracks_allowed(&project, clips[0][0], 2),
        2,
        "past a track that it would also fit on"
    );
    assert_eq!(tracks_allowed(&project, clips[1][0], -1), -1);
    assert_eq!(tracks_allowed(&project, clips[0][0], 0), 0);
}

#[test]
fn clips_do_not_go_past_the_first_or_the_last_track() {
    let (project, clips) = project_of(&[&[(100, 50)], &[(0, 40)], &[]]);
    assert_eq!(
        tracks_allowed(&project, clips[0][0], 5),
        2,
        "the last track is the third"
    );
    assert_eq!(tracks_allowed(&project, clips[0][0], -3), 0, "the first is where it is");
    assert_eq!(tracks_allowed(&project, clips[1][0], -9), -1);
}

#[test]
fn a_clip_goes_as_far_down_as_it_fits_when_the_track_wanted_has_something_in_the_way() {
    // The clip at 100..150 on the first track; the second has something at 0..40, the third something at 90..110.
    let (project, clips) = project_of(&[&[(100, 50)], &[(0, 40)], &[(90, 20)]]);
    assert_eq!(
        tracks_allowed(&project, clips[0][0], 2),
        1,
        "it would be on the clip on the third track, so it goes to the second"
    );
    // And if the second is no better, it stays where it is.
    let (project, clips) = project_of(&[&[(100, 50)], &[(110, 20)], &[(90, 20)]]);
    assert_eq!(tracks_allowed(&project, clips[0][0], 2), 0);
}

#[test]
fn clips_that_move_down_together_keep_their_tracks_in_relation_to_one_another() {
    let (project, clips) = project_of(&[&[(0, 20)], &[(40, 20)], &[], &[]]);
    let both = [clips[0][0], clips[1][0]];
    let (allowed, moves) = where_they_go(
        &project,
        &both,
        MoveBy {
            tracks_down: 2,
            frames: 0,
        },
    );
    assert_eq!(allowed.tracks_down, 2);
    assert_eq!(moves, vec![(clips[0][0], 2, 0), (clips[1][0], 3, 40)]);
    // The lower of them is on the second, so they can only go two down: that is the end of the tracks.
    let (allowed, _) = where_they_go(
        &project,
        &both,
        MoveBy {
            tracks_down: 5,
            frames: 0,
        },
    );
    assert_eq!(allowed.tracks_down, 2);
    // And one that does not fit stops them all.
    let (project, clips) = project_of(&[&[(0, 20)], &[(40, 20)], &[], &[(30, 20)]]);
    let both = [clips[0][0], clips[1][0]];
    let (allowed, _) = where_they_go(
        &project,
        &both,
        MoveBy {
            tracks_down: 2,
            frames: 0,
        },
    );
    assert_eq!(
        allowed.tracks_down, 1,
        "the lower would be on the fourth track, over the clip there"
    );
}

#[test]
fn how_far_along_the_timeline_a_clip_can_go_is_counted_on_the_track_it_goes_to() {
    // 0..50 on the first track, 80..120 on the second.
    let (mut project, clips) = project_of(&[&[(0, 50)], &[(80, 40)]]);
    let moving = [clips[0][0]];
    let (allowed, _) = where_they_go(
        &project,
        &moving,
        MoveBy {
            tracks_down: 1,
            frames: 100,
        },
    );
    assert_eq!(
        allowed,
        MoveBy {
            tracks_down: 1,
            frames: 30
        },
        "it stops against the clip on the second track"
    );
    let (allowed, _) = where_they_go(
        &project,
        &moving,
        MoveBy {
            tracks_down: 0,
            frames: 100,
        },
    );
    assert_eq!(allowed.frames, 100, "and not at all on its own");

    // On a track that plays twice as fast it lasts 25 frames, and goes 25 further.
    let second = project.tracks()[1].id();
    project.set_track_speed(second, SpeedFactor::new(2).unwrap()).unwrap();
    let (allowed, _) = where_they_go(
        &project,
        &moving,
        MoveBy {
            tracks_down: 1,
            frames: 100,
        },
    );
    assert_eq!(
        allowed,
        MoveBy {
            tracks_down: 1,
            frames: 55
        }
    );
}

#[test]
fn clips_that_would_be_longer_on_the_track_they_go_to_and_so_would_not_fit_there_do_not_go() {
    // Two clips of 100 frames that touch, on a track that plays four times as fast: 25 frames each.
    let (mut project, clips) = project_of(&[&[(0, 100), (0, 100)], &[]]);
    let first = project.tracks()[0].id();
    project.set_track_speed(first, SpeedFactor::new(4).unwrap()).unwrap();
    // On the normal one below they would be 100 frames long each, and the second starts at 25.
    let both = [clips[0][0], clips[0][1]];
    assert_eq!(
        allowed_move(
            &project,
            &both,
            MoveBy {
                tracks_down: 1,
                frames: 0
            }
        )
        .tracks_down,
        0
    );
    // One of them alone fits.
    assert_eq!(tracks_allowed(&project, clips[0][1], 1), 1);
}

/// A small random project: up to three tracks of up to six clips each.
fn random_project(random: &mut DeterministicRandomNumbers) -> (Project, Vec<ClipId>) {
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
        let moving: Vec<ClipId> = all_clips
            .iter()
            .copied()
            .filter(|_| random.next_below(3) == 0)
            .collect();
        let wanted = random.next_below(400) as i64 - 200;

        let allowed = frames_allowed(&project, &moving, wanted);

        // It is between nothing and what was wanted.
        assert!(
            (allowed >= 0 && allowed <= wanted) || (allowed <= 0 && allowed >= wanted),
            "round {round}: {allowed} is not between 0 and {wanted}"
        );
        // It can be done...
        let mut moved = project.clone();
        let starts = moves_along(&project, &moving, allowed);
        assert_eq!(moved.move_clips(&starts), Ok(()), "round {round}: moving by {allowed}");
        // ...and one frame more in the same direction, if that was wanted, could not.
        if allowed != wanted && !moving.is_empty() {
            let one_more = allowed + wanted.signum();
            let starts = moves_along(&project, &moving, one_more);
            let mut moved = project.clone();
            let cannot_be_done = moved.move_clips(&starts).is_err()
                || starts.iter().any(|moved| {
                    // A start before the start of the timeline is made 0 instead, which hides that it was refused.
                    let placement = project.placement_of(moved.clip).unwrap();
                    (placement.first_timeline_frame.value() as i64 + one_more) < 0 && moved.start.value() == 0
                });
            assert!(
                cannot_be_done,
                "round {round}: {one_more} was allowed after all, but {allowed} was given"
            );
        }
    }
}

/// A random project of two to four tracks, some of them playing faster than the others, with up to six
/// clips on each, and some of its clips chosen to be moved.
fn random_project_with_tracks_of_different_speeds(random: &mut DeterministicRandomNumbers) -> (Project, Vec<ClipId>) {
    let track_count = 2 + random.next_below(3) as usize;
    let tracks: Vec<Vec<(u64, u64)>> = (0..track_count)
        .map(|_| {
            (0..random.next_below(7))
                .map(|_| (random.next_below(30), 1 + random.next_below(60)))
                .collect()
        })
        .collect();
    let slices: Vec<&[(u64, u64)]> = tracks.iter().map(Vec::as_slice).collect();
    let (mut project, clips) = project_of(&slices);
    let track_ids: Vec<TrackId> = project.tracks().iter().map(|track| track.id()).collect();
    for track in track_ids {
        let speed = SpeedFactor::new(1 + random.next_below(3) as u32).unwrap();
        project.set_track_speed(track, speed).unwrap();
    }
    let all: Vec<ClipId> = clips.into_iter().flatten().collect();
    let moving = all.iter().copied().filter(|_| random.next_below(3) == 0).collect();
    (project, moving)
}

#[test]
fn whatever_is_allowed_to_move_down_or_along_can_be_done_and_a_little_more_could_not_be_for_random_projects_and_moves()
{
    let mut random = DeterministicRandomNumbers::with_seed(31);
    for round in 0..400 {
        let (project, moving) = random_project_with_tracks_of_different_speeds(&mut random);
        let wanted = MoveBy {
            tracks_down: random.next_below(9) as i64 - 4,
            frames: random.next_below(400) as i64 - 200,
        };
        let allowed = allowed_move(&project, &moving, wanted);
        let context = format!("round {round}: {wanted:?} was wanted, {allowed:?} was allowed for {moving:?}");
        let between_nothing_and =
            |allowed: i64, wanted: i64| (allowed >= 0 && allowed <= wanted) || (allowed <= 0 && allowed >= wanted);
        assert!(
            between_nothing_and(allowed.tracks_down, wanted.tracks_down),
            "{context}"
        );
        assert!(between_nothing_and(allowed.frames, wanted.frames), "{context}");

        // What was allowed can be done.
        let can_be_done = |by: MoveBy| {
            let moves = moves_by(&project, &moving, by);
            moves.len() == moving.len() && project.clone().move_clips(&moves).is_ok()
        };
        assert!(can_be_done(allowed) || moving.is_empty(), "{context}");

        // The tracks are as many of the wanted as can be had: all the ones between those and the ones that were
        // given, which are closer to the wanted, are refused (or are off the tracks).
        let step = wanted.tracks_down.signum();
        let mut closer = allowed.tracks_down;
        while closer != wanted.tracks_down && !moving.is_empty() {
            closer += step;
            assert!(
                !can_be_done(MoveBy {
                    tracks_down: closer,
                    frames: 0
                }),
                "{context}: {closer} tracks would have done"
            );
        }

        // And along the timeline, one more frame in the direction wanted could not be done.
        if allowed.frames != wanted.frames && !moving.is_empty() {
            let one_more = MoveBy {
                tracks_down: allowed.tracks_down,
                frames: allowed.frames + wanted.frames.signum(),
            };
            let moves = moves_by(&project, &moving, one_more);
            let before_the_start = moving.iter().any(|clip| {
                let placement = project.placement_of(*clip).unwrap();
                placement.first_timeline_frame.value() as i64 + one_more.frames < 0
            });
            assert!(
                before_the_start || !can_be_done(one_more),
                "{context}: {one_more:?} could be done ({} moves)",
                moves.len()
            );
        }
    }
}
