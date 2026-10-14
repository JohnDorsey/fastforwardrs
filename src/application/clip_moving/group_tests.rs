//! Tests of how far clips can be dragged where there are clip groups: a clip that is not in a clip group stops against its rectangle, and
//! does not go through it to the other side; a clip that is in one stops where its group would take in a clip that is not in it; and clips
//! that are all of a group go along with its rectangle.

use super::tests::project_of;
use super::*;
use crate::project::groups::random_edits_tests::{
    project_after_random_edits, project_after_random_edits_with_more_clip_groups,
};
use crate::project::{GroupKind, Member, SpeedFactor};
use crate::test_support::DeterministicRandomNumbers;

/// A project of two tracks (the front one first):
///
/// ```text
/// frame     0    20        40   60        100  120        200  220
/// front     [X ]                          [ B  ]
/// back                     [ A ]                          [ Z  ]
/// ```
///
/// where `A` and `B` are in a clip group, whose rectangle is frames 40 to 120 across both tracks, and `X` and `Z` are not in it. Gives the
/// project and `X`, `A`, `B` and `Z`.
fn two_tracks_with_a_clip_group() -> (Project, [ClipId; 4]) {
    let (mut project, clips) = project_of(&[&[(0, 20), (80, 20)], &[(40, 20), (140, 20)]]);
    let [x, b] = [clips[0][0], clips[0][1]];
    let [a, z] = [clips[1][0], clips[1][1]];
    project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(a), Member::Clip(b)])
        .unwrap();
    (project, [x, a, b, z])
}

fn along(project: &Project, moving: &[ClipId], wanted: i64) -> i64 {
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

#[test]
fn a_clip_that_is_not_in_a_clip_group_stops_against_its_rectangle_where_it_could_otherwise_go_on() {
    let (project, [x, ..]) = two_tracks_with_a_clip_group();

    // X ends at frame 20 and the rectangle begins at frame 40. (With no clip group it could go on to the clip B, which is at 100: 80.)
    assert_eq!(along(&project, &[x], 500), 20);
    assert_eq!(along(&project, &[x], 20), 20, "all the way there");
    assert_eq!(
        along(&project, &[x], 7),
        7,
        "or not as far, if that is as far as it was taken"
    );
    assert_eq!(along(&project, &[x], -500), 0, "and it is at the start of the timeline");
}

#[test]
fn a_clip_that_is_dragged_far_past_a_clip_group_does_not_go_through_it_and_come_out_at_the_other_side() {
    // The same as before, with a lot of room beyond: the place where X would be after the rectangle is far off, and between there and where
    // it is are places where it would be inside.
    let (project, [x, ..]) = two_tracks_with_a_clip_group();
    for wanted in [21, 60, 130, 300, 1000] {
        assert_eq!(along(&project, &[x], wanted), 20, "{wanted} frames were wanted");
    }
}

/// A project of three tracks (the front one first):
///
/// ```text
/// frame     0    20       40   60   80   100        180  200  220
/// front                   [ A ]
/// middle    [X  ]                                        [Y  ]
/// back                              [ B ]
/// ```
///
/// where `A` and `B` are in a clip group, whose rectangle is frames 40 to 100 across all three tracks, and `X` and `Y` are not (they are on the
/// track between those of the group): there is room for them beyond the rectangle, on both sides of it. Gives the project and `X` and `Y`.
fn two_clips_with_room_beyond_the_rectangle_of_a_clip_group() -> (Project, [ClipId; 2]) {
    let (mut project, clips) = project_of(&[&[(40, 20)], &[(0, 20), (180, 20)], &[(80, 20)]]);
    let [x, y] = [clips[1][0], clips[1][1]];
    project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(clips[0][0]), Member::Clip(clips[2][0])],
        )
        .unwrap();
    (project, [x, y])
}

#[test]
fn a_clip_that_has_room_beyond_a_clip_group_is_still_stopped_by_its_rectangle_and_does_not_come_out_at_the_other_side()
{
    let (project, [x, y]) = two_clips_with_room_beyond_the_rectangle_of_a_clip_group();

    // X could go to 180 (where it touches Y), and all of the places from 100 on are allowed: it is stopped at the rectangle, 20 frames on.
    assert_eq!(along(&project, &[x], 300), 20);
    // Y could go back to 20 (where it touches X), and from 20 to 100 it is the same. It stops at the rectangle, 100 frames back.
    assert_eq!(along(&project, &[y], -300), -100);
    // (The way that tries each place is what the arithmetic is compared with, and it does not jump either.)
    assert_eq!(stopped_by_trying_each_place(&project, &[x], 0, 180), 20);
    assert_eq!(stopped_by_trying_each_place(&project, &[y], 0, -180), -100);
}

/// A project of three tracks (the front one first), of which the middle one plays twice as fast as the others:
///
/// ```text
/// frame     0    20   40   60   80   100  120
/// front     [X                 ]
/// middle                            [ A ]
/// back                    [ B ]
/// ```
///
/// where `A` and `B` are in a clip group, whose rectangle is frames 60 to 120 across the middle and back tracks, and `X` is not. `X` is 40
/// frames long where it is, and `A` is 40 frames of its file, which last only 20 frames on the middle track. Gives the project and `X`.
fn a_clip_above_a_clip_group_that_has_a_track_that_plays_faster_in_it() -> (Project, ClipId) {
    let (mut project, clips) = project_of(&[&[(0, 40)], &[(100, 40)], &[(60, 20)]]);
    let middle = project.tracks()[1].id();
    project.set_track_speed(middle, SpeedFactor::new(2).unwrap()).unwrap();
    project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(clips[1][0]), Member::Clip(clips[2][0])],
        )
        .unwrap();
    let a = project.rectangle_of_the_clip(clips[1][0]).unwrap();
    assert_eq!(
        (a.first_frame, a.end_frame),
        (100, 120),
        "the setup: A lasts 20 frames on the track that plays twice as fast"
    );
    (project, clips[0][0])
}

#[test]
fn a_clip_that_goes_down_to_a_track_that_plays_faster_lasts_less_there_and_goes_further_before_a_rectangle_is_in_its_way()
 {
    let (project, x) = a_clip_above_a_clip_group_that_has_a_track_that_plays_faster_in_it();

    // On the middle track X lasts 20 frames, and is inside the rectangle that begins at frame 60 when it ends after that: 41 frames on. (If it
    // lasted 40 frames, as it does where it is, that would be 21 frames on.)
    let allowed = allowed_move(
        &project,
        &[x],
        MoveBy {
            tracks_down: 1,
            frames: 100,
        },
    );
    assert_eq!(
        allowed,
        MoveBy {
            tracks_down: 1,
            frames: 40
        }
    );
    assert_eq!(stop_worked_out_by_arithmetic(&project, &[x], 1, 80), Some(40));
    assert_eq!(stopped_by_trying_each_place(&project, &[x], 1, 80), 40);
}

#[test]
fn a_clip_of_one_frame_is_stopped_by_a_clip_group_that_is_one_frame_wide_and_does_not_jump_over_it() {
    // The rectangle is frame 60 only, across all three tracks. X and Y are on the track between, one frame long each: each is inside the
    // rectangle in one place only, which it must not be taken past either.
    let (mut project, clips) = project_of(&[&[(60, 1)], &[(0, 1), (199, 1)], &[(60, 1)]]);
    project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(clips[0][0]), Member::Clip(clips[2][0])],
        )
        .unwrap();
    let [x, y] = [clips[1][0], clips[1][1]];

    assert_eq!(along(&project, &[x], 100), 59);
    assert_eq!(stop_worked_out_by_arithmetic(&project, &[x], 0, 100), Some(59));
    assert_eq!(
        along(&project, &[y], -300),
        -139,
        "Y is at 200 and would be inside the rectangle 140 frames back"
    );
}

#[test]
fn the_arithmetic_says_nothing_where_the_clips_are_inside_a_rectangle_by_one_frame_at_its_edge_as_they_are_going_either_way()
 {
    // The rectangle (frames 49 to 80, across the middle and back tracks) begins one frame before X ends: put on the middle track as it is, X
    // is inside it by one frame; one frame earlier it would not be. So the first place that is not allowed is the one where X is.
    let (mut project, clips) = project_of(&[&[(40, 10)], &[(49, 11)], &[(70, 10)]]);
    project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(clips[1][0]), Member::Clip(clips[2][0])],
        )
        .unwrap();
    let x = clips[0][0];
    assert_eq!(stop_worked_out_by_arithmetic(&project, &[x], 1, 5), None);
    assert_eq!(stop_worked_out_by_arithmetic(&project, &[x], 1, -5), None);

    // And the rectangle (frames 20 to 41) ends one frame after X begins: the last place that is not allowed is the one where X is.
    let (mut project, clips) = project_of(&[&[(40, 10)], &[(20, 10)], &[(31, 10)]]);
    project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(clips[1][0]), Member::Clip(clips[2][0])],
        )
        .unwrap();
    let x = clips[0][0];
    assert_eq!(stop_worked_out_by_arithmetic(&project, &[x], 1, 5), None);
    assert_eq!(stop_worked_out_by_arithmetic(&project, &[x], 1, -5), None);
}

#[test]
fn what_the_arithmetic_says_is_checked_against_the_move_itself_so_that_a_place_that_is_not_allowed_for_another_reason_is_not_given()
 {
    // X is at 0 to 20 and the clip Y at 100 to 120 on its track, and a clip group is elsewhere, on other tracks. The arithmetic only knows
    // the rectangles, and says that X can go 90 frames on, where it would be on Y: the move does not allow that, and X stops before.
    let (mut project, clips) = project_of(&[&[(0, 20), (80, 20)], &[(300, 10)], &[(300, 10)]]);
    project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(clips[1][0]), Member::Clip(clips[2][0])],
        )
        .unwrap();
    let x = clips[0][0];

    assert_eq!(stop_worked_out_by_arithmetic(&project, &[x], 0, 90), Some(90));
    assert_eq!(stopped_by_the_groups(&project, &[x], 0, 90), 80);
}

#[test]
fn a_clip_in_a_clip_group_stops_where_the_group_would_take_in_a_clip_that_is_not_in_it() {
    let (project, [_, _, b, _]) = two_tracks_with_a_clip_group();

    // B is at 100 to 120 and the clip Z is at 200 to 220, across the back track. When the end of B comes past the start of Z the rectangle
    // takes Z in: at 80 frames on, B is at 180 to 200 and the rectangle ends where Z begins.
    assert_eq!(along(&project, &[b], 500), 80);
    assert_eq!(along(&project, &[b], 79), 79);
    // Going the other way is as free as the clip X allows: it is on the same track, and ends at 20.
    assert_eq!(along(&project, &[b], -500), -80);
}

#[test]
fn the_clips_that_are_all_of_a_clip_group_go_along_with_its_rectangle_and_stop_where_it_comes_to_a_clip_that_is_not_in_it()
 {
    let (project, [_, a, b, _]) = two_tracks_with_a_clip_group();
    let both = [a, b];

    // Towards Z the rectangle (ending at 120) has 80 frames until the clip Z; the other way, 20 frames until X ends at 20, though A alone
    // could have gone all the way back to 0.
    assert_eq!(along(&project, &both, 500), 80);
    assert_eq!(along(&project, &both, -500), -20);
    assert_eq!(
        along(&project, &[a], -500),
        -20,
        "(and A alone would take the rectangle against X as well)"
    );
}

#[test]
fn a_clip_that_goes_to_another_track_is_not_put_in_the_rectangle_of_a_clip_group_there() {
    let (project, [x, ..]) = two_tracks_with_a_clip_group();
    // X is at 0 to 20 on the front track: the back track has room there, and the rectangle of the group is not there: it goes.
    let allowed = allowed_move(
        &project,
        &[x],
        MoveBy {
            tracks_down: 1,
            frames: 0,
        },
    );
    assert_eq!(allowed.tracks_down, 1);
    // But if it were moved along first to where the rectangle is, the other track is no place for it.
    let allowed = allowed_move(
        &project,
        &[x],
        MoveBy {
            tracks_down: 1,
            frames: 20,
        },
    );
    assert_eq!(
        allowed,
        MoveBy {
            tracks_down: 1,
            frames: 20
        },
        "just short of the rectangle"
    );
    let allowed = allowed_move(
        &project,
        &[x],
        MoveBy {
            tracks_down: 1,
            frames: 30,
        },
    );
    assert_eq!(
        allowed,
        MoveBy {
            tracks_down: 1,
            frames: 20
        },
        "and it stops at the rectangle on the back track as on the front one"
    );
}

#[test]
fn where_there_are_no_clip_groups_the_groups_have_nothing_to_say_about_how_far_clips_go() {
    // A tag has no rectangle that anything is kept out of.
    let (mut project, clips) = project_of(&[&[(0, 20), (80, 20)], &[(40, 20), (140, 20)]]);
    project
        .create_group(GroupKind::Tag, &[Member::Clip(clips[1][0]), Member::Clip(clips[0][1])])
        .unwrap();
    assert_eq!(along(&project, &[clips[0][0]], 500), 80, "as far as the clip after it");
    assert_eq!(along(&project, &[clips[0][1]], 500), 500);
}

/// The moves that put the clips `moving` `by` as far, and whether the project lets them be made.
fn can_be_done(project: &Project, moving: &[ClipId], by: MoveBy) -> bool {
    let moves = moves_by(project, moving, by);
    moves.len() == moving.len() && project.clone().move_clips(&moves).is_ok()
}

/// Every clip of the project, in the order of the tracks.
fn every_clip(project: &Project) -> Vec<ClipId> {
    project
        .tracks()
        .iter()
        .flat_map(|track| track.entries().iter().map(|entry| entry.id()))
        .collect()
}

/// One clip, or two, or three, of `all` (each only once), chosen at random.
fn some_clips_at_random(random: &mut DeterministicRandomNumbers, all: &[ClipId]) -> Vec<ClipId> {
    (0..1 + random.next_below(3))
        .map(|_| all[random.next_below(all.len() as u64) as usize])
        .fold(Vec::new(), |mut found, clip| {
            if !found.contains(&clip) {
                found.push(clip);
            }
            found
        })
}

#[test]
fn whatever_is_dragged_stops_at_the_first_place_that_is_not_allowed_in_random_projects_that_have_groups() {
    let mut random = DeterministicRandomNumbers::with_seed(77);
    let mut stopped_by_a_rectangle = 0;
    for seed in 1..=40u64 {
        let project = project_after_random_edits(seed, 120);
        let all = every_clip(&project);
        if all.is_empty() {
            continue;
        }
        for trial in 0..25 {
            let moving = some_clips_at_random(&mut random, &all);
            let wanted = MoveBy {
                tracks_down: random.next_below(5) as i64 - 2,
                frames: random.next_below(300) as i64 - 150,
            };
            let allowed = allowed_move(&project, &moving, wanted);
            let context =
                format!("seed {seed}, trial {trial}: {wanted:?} was wanted, {allowed:?} was allowed for {moving:?}");

            // What is allowed can be done...
            assert!(can_be_done(&project, &moving, allowed), "{context}");
            // ...and so can every place on the way to it, as far as the clips go along the tracks they are then on: nothing is jumped over.
            let direction = allowed.frames.signum();
            for step in 0..allowed.frames.unsigned_abs() {
                let on_the_way = MoveBy {
                    tracks_down: allowed.tracks_down,
                    frames: direction * step as i64,
                };
                assert!(
                    can_be_done(&project, &moving, on_the_way),
                    "{context}: {on_the_way:?} on the way"
                );
            }
            // ...and a frame more, if that was wanted, could not be done (a start before the start of the timeline is made 0, which hides
            // that it was refused).
            if allowed.frames != wanted.frames {
                let one_more = MoveBy {
                    tracks_down: allowed.tracks_down,
                    frames: allowed.frames + wanted.frames.signum(),
                };
                let moves = moves_by(&project, &moving, one_more);
                let before_the_start = moves.iter().any(|moved| {
                    let placement = project.placement_of(moved.clip).unwrap();
                    (placement.first_timeline_frame.value() as i64 + one_more.frames) < 0 && moved.start.value() == 0
                });
                assert!(
                    before_the_start || !can_be_done(&project, &moving, one_more),
                    "{context}: {one_more:?} was allowed after all"
                );
                if !before_the_start && !project.groups().is_empty() {
                    stopped_by_a_rectangle += 1;
                }
            }
        }
    }
    assert!(
        stopped_by_a_rectangle >= 50,
        "the test is worth little if drags are seldom stopped in projects that have groups: {stopped_by_a_rectangle}"
    );
}

/// A project of three tracks:
///
/// ```text
/// frame     30  35         40  50    55  60        80  90
/// top                      [X  ]
/// middle    [A ]                                   [Y  ]
/// bottom                         [B ]
/// ```
///
/// where `A` and `B` are in a clip group (whose rectangle is frames 30 to 60 across the middle and bottom tracks), and `X` and `Y` are not.
/// Gives the project and `X`, which can be moved down to the middle track, where it would be inside the rectangle at frame 40.
fn a_clip_above_the_rectangle_of_a_clip_group_that_is_taken_down_into_it() -> (Project, ClipId) {
    let (mut project, clips) = project_of(&[&[(40, 10)], &[(30, 5), (45, 10)], &[(55, 5)]]);
    project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(clips[1][0]), Member::Clip(clips[2][0])],
        )
        .unwrap();
    (project, clips[0][0])
}

#[test]
fn a_clip_taken_down_to_a_track_where_it_would_be_inside_a_rectangle_where_it_is_cannot_stay_there_but_can_be_at_a_time_where_it_is_not()
 {
    let (project, x) = a_clip_above_the_rectangle_of_a_clip_group_that_is_taken_down_into_it();

    // (Where X is now, it would be at frames 40 to 50 on the middle track, inside the rectangle: that is not allowed, and so the time it is
    // wanted at, if the place there is allowed, is where it goes. This is what a clip that is held to the time it started at is: the tracks are
    // chosen for the time it is held to, which is not the time it is at.)
    assert_eq!(
        stopped_by_the_groups(&project, &[x], 1, 25),
        25,
        "50 to 75: after the rectangle and before Y"
    );
    assert_eq!(stopped_by_the_groups(&project, &[x], 1, -30), -30, "10 to 20: before A");
}

#[test]
fn a_clip_wanted_where_it_cannot_be_goes_back_towards_where_it_is_to_the_nearest_place_where_it_can_and_nowhere_if_there_is_none()
 {
    let (project, x) = a_clip_above_the_rectangle_of_a_clip_group_that_is_taken_down_into_it();

    // 40 on is 80 to 90 on the middle track, where Y is; the nearest place back from there that is allowed is 30 on (70 to 80, against Y).
    assert_eq!(stopped_by_the_groups(&project, &[x], 1, 40), 30);
    // 10 on is 50 to 60, inside the rectangle, and so is every place between there and where X is: there is none.
    assert_eq!(stopped_by_the_groups(&project, &[x], 1, 10), 0);
}

#[test]
fn a_clip_that_is_not_moved_is_not_stopped() {
    let (project, x) = a_clip_above_the_rectangle_of_a_clip_group_that_is_taken_down_into_it();

    assert_eq!(stopped_by_the_groups(&project, &[x], 1, 0), 0);
    assert_eq!(stopped_by_the_groups(&project, &[x], 0, 0), 0);
}

/// How many tracks down the clips `moving` can be put, chosen at random from as many up as they can go to as many down (none among them).
fn tracks_down_at_random(random: &mut DeterministicRandomNumbers, project: &Project, moving: &[ClipId]) -> i64 {
    let positions: Vec<usize> = moving
        .iter()
        .filter_map(|clip| project.track_position_holding(*clip))
        .collect();
    let furthest_up = -(*positions.iter().min().unwrap() as i64);
    let furthest_down = project.tracks().len() as i64 - 1 - *positions.iter().max().unwrap() as i64;
    furthest_up + random.next_below((furthest_down - furthest_up + 1) as u64) as i64
}

/// The clips that are all of one of the clip groups of the project, chosen at random, with one more clip of `all` half of the time. `None` if
/// there is no clip group.
fn the_clips_of_a_clip_group_at_random(
    random: &mut DeterministicRandomNumbers,
    project: &Project,
    all: &[ClipId],
) -> Option<Vec<ClipId>> {
    let clip_groups: Vec<_> = project
        .groups()
        .iter()
        .filter(|group| group.kind() == GroupKind::ClipGroup)
        .collect();
    if clip_groups.is_empty() {
        return None;
    }
    let mut clips = project.clips_in(clip_groups[random.next_below(clip_groups.len() as u64) as usize].id());
    if random.next_below(2) == 0 {
        let another = all[random.next_below(all.len() as u64) as usize];
        if !clips.contains(&another) {
            clips.push(another);
        }
    }
    Some(clips)
}

#[test]
fn the_place_that_is_worked_out_by_arithmetic_is_the_one_that_is_found_by_trying_each_place_in_random_projects_that_have_groups()
 {
    let mut random = DeterministicRandomNumbers::with_seed(4242);
    let mut compared = 0;
    let mut stopped_going_later = 0;
    let mut stopped_going_earlier = 0;
    let mut moved_the_clips_of_a_group = 0;
    for seed in 1..=60u64 {
        let project = project_after_random_edits_with_more_clip_groups(seed, 120);
        let all = every_clip(&project);
        let has_clip_groups = project
            .groups()
            .iter()
            .any(|group| group.kind() == GroupKind::ClipGroup);
        if all.is_empty() || !has_clip_groups {
            continue;
        }
        for trial in 0..80 {
            let of_a_group = trial % 3 == 0;
            let moving = if of_a_group {
                the_clips_of_a_clip_group_at_random(&mut random, &project, &all).unwrap()
            } else {
                some_clips_at_random(&mut random, &all)
            };
            let tracks_down = tracks_down_at_random(&mut random, &project, &moving);
            let wanted = random.next_below(240) as i64 - 120;
            let frames = allowed_frames(&project, &moving, tracks_down, wanted);
            let context =
                format!("seed {seed}, trial {trial}: {moving:?} down {tracks_down} tracks and {frames} frames");

            let by_trying = stopped_by_trying_each_place(&project, &moving, tracks_down, frames);
            assert_eq!(
                stopped_by_the_groups(&project, &moving, tracks_down, frames),
                by_trying,
                "{context}"
            );
            let is_allowed_where_they_are =
                the_clips_fit_on_the_tracks_they_would_be_on(&project, &moving, tracks_down, 0);
            if frames == 0 || !is_allowed_where_they_are {
                continue;
            }
            assert_eq!(
                stop_worked_out_by_arithmetic(&project, &moving, tracks_down, frames),
                Some(by_trying),
                "{context}"
            );
            compared += 1;
            moved_the_clips_of_a_group += usize::from(of_a_group);
            if by_trying != frames {
                if frames > 0 {
                    stopped_going_later += 1;
                } else {
                    stopped_going_earlier += 1;
                }
            }
        }
    }
    // (Measured: 1771 compared, 723 of them of the clips of a group, 102 stopped going later and 95 going earlier.)
    assert!(
        compared >= 800 && moved_the_clips_of_a_group >= 300,
        "{compared} places were compared, {moved_the_clips_of_a_group} of them of the clips of a group"
    );
    assert!(
        stopped_going_later >= 50 && stopped_going_earlier >= 50,
        "the test is worth little if clips are seldom stopped: {stopped_going_later} going later, {stopped_going_earlier} going earlier"
    );
}

#[test]
fn how_far_clips_can_go_is_worked_out_without_trying_each_place_so_that_taking_them_very_far_takes_no_longer() {
    let (project, [x, _, _, z]) = two_tracks_with_a_clip_group();
    // (Trying each of four million places, which is what it would take to find that nothing is in the way, copies and checks the project
    // that many times: measured, 12 microseconds a place for this little project in a build for testing, so close to a minute, and more for a
    // project with more in it. Worked out, it is the same few sums as for one frame.)
    let far = 4_000_000;
    let started = std::time::Instant::now();

    assert_eq!(stopped_by_the_groups(&project, &[z], 0, far), far, "nothing is after Z");
    assert_eq!(
        stopped_by_the_groups(&project, &[x], 0, far),
        20,
        "X stops at the rectangle"
    );
    assert_eq!(
        stop_worked_out_by_arithmetic(&project, &[z], 0, i64::MAX / 4),
        Some(i64::MAX / 4)
    );
    assert_eq!(stop_worked_out_by_arithmetic(&project, &[x], 0, i64::MAX / 4), Some(20));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "it took {:?}",
        started.elapsed()
    );
}

/// A project of three tracks:
///
/// ```text
/// frame     20  30    40  50         100  110  120
/// top                               [X  ]
/// middle                [A ]        [   Y     ]
/// bottom                [B ]
/// ```
///
/// where `A` and `B` are in a clip group (whose rectangle is frames 40 to 50 across the middle and bottom tracks), and `X` and `Y` are not.
/// Gives the project and `X`, which cannot be moved down to the middle track where it is, because `Y` is there.
fn a_clip_above_a_clip_that_is_in_the_way_on_the_track_below_and_a_rectangle_before_it() -> (Project, ClipId) {
    let (mut project, clips) = project_of(&[&[(100, 10)], &[(40, 10), (50, 20)], &[(40, 10)]]);
    project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(clips[1][0]), Member::Clip(clips[2][0])],
        )
        .unwrap();
    (project, clips[0][0])
}

#[test]
fn a_clip_that_cannot_be_where_it_is_on_the_track_it_goes_to_goes_to_the_time_it_is_wanted_at_if_it_can_be_there_though_a_rectangle_is_on_the_way()
 {
    let (project, x) = a_clip_above_a_clip_that_is_in_the_way_on_the_track_below_and_a_rectangle_before_it();

    // (X is at 100 to 110, and on the middle track it would be on Y: the track was chosen for the time that it is held to, which is 80 frames
    // before, at 20 to 30, in the clear. It is not a drag along the track, which the rectangle at 40 to 50 would stop: it is going to that time.)
    assert_eq!(stopped_by_the_groups(&project, &[x], 1, -80), -80);
    // The arithmetic is about the rectangles and nothing else: it would stop X at the rectangle, 50 frames back, where it is not on Y any more.
    assert_eq!(stop_worked_out_by_arithmetic(&project, &[x], 1, -80), Some(-50));
    // Nearer than that, where there is no rectangle, is the time it is wanted at as well.
    assert_eq!(stopped_by_the_groups(&project, &[x], 1, -40), -40);
}

#[test]
fn the_arithmetic_says_nothing_where_the_clips_would_be_inside_a_rectangle_as_they_are_and_where_a_track_is_not_there()
{
    let (project, x) = a_clip_above_the_rectangle_of_a_clip_group_that_is_taken_down_into_it();

    // (X is at 40 to 50: on the middle track it would be inside the rectangle that is 30 to 60, whichever way it were wanted.)
    assert_eq!(stop_worked_out_by_arithmetic(&project, &[x], 1, 25), None);
    assert_eq!(stop_worked_out_by_arithmetic(&project, &[x], 1, -30), None);
    // There is no track below the bottom one, or above the top one.
    assert_eq!(stop_worked_out_by_arithmetic(&project, &[x], 3, 25), None);
    assert_eq!(stop_worked_out_by_arithmetic(&project, &[x], -1, 25), None);
    // Where it is, it can be taken as far as it is wanted, in the clear (the top track has no clip group on it).
    assert_eq!(stop_worked_out_by_arithmetic(&project, &[x], 0, 25), Some(25));
}
