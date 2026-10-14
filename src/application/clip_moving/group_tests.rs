//! Tests of how far clips can be dragged where there are clip groups: a clip that is in none goes into the rectangle of one, and joins it, as
//! far as the clips that are in the way and the rules let it (it stops where its being in the group would make the rectangle take in a clip
//! that is not in it, and a clip that is in another group stops against the rectangle); a clip that is in one stops where its group would
//! take in a clip that is not in it; and clips that are all of a group go along with its rectangle.

use super::tests::project_of;
use super::*;
use crate::project::groups::random_edits_tests::{
    project_after_random_edits, project_after_random_edits_with_more_clip_groups,
};
use crate::project::{GroupKind, Member, SpeedFactor};
use crate::test_support::{DeterministicRandomNumbers, check_every_case_at_once};

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
fn a_clip_that_is_not_in_a_clip_group_goes_into_its_rectangle_as_far_as_the_clips_in_the_way_let_it() {
    let (project, [x, ..]) = two_tracks_with_a_clip_group();

    // X ends at frame 20, and the rectangle begins at frame 40 (on both tracks) and ends at 120. The clip B, which is in the group, is at
    // 100 on the same track: X goes in as far as it, 80 frames on, where it is at 80 to 100.
    assert_eq!(along(&project, &[x], 500), 80);
    assert_eq!(along(&project, &[x], 20), 20, "just short of the rectangle");
    assert_eq!(along(&project, &[x], 21), 21, "one frame into it: it is in the group");
    assert_eq!(
        along(&project, &[x], 7),
        7,
        "or not as far, if that is as far as it was taken"
    );
    assert_eq!(along(&project, &[x], -500), 0, "and it is at the start of the timeline");
}

#[test]
fn a_clip_that_is_in_no_clip_group_is_stopped_by_a_clip_that_is_in_the_way_whether_or_not_it_is_in_a_rectangle() {
    let (project, [x, _, _, z]) = two_tracks_with_a_clip_group();

    for wanted in [60, 79, 80, 130, 300, 1000] {
        assert_eq!(
            along(&project, &[x], wanted),
            wanted.min(80),
            "{wanted} frames were wanted"
        );
    }
    // Z is on the back track, with nothing after it.
    assert_eq!(along(&project, &[z], 1000), 1000);
    assert_eq!(
        along(&project, &[z], -1000),
        -140,
        "A is the clip in the way: it ends at 60, and Z begins at 200"
    );
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
fn a_clip_that_has_room_beyond_a_clip_group_goes_through_its_rectangle_where_nothing_is_in_the_way_and_comes_out_at_the_other_side()
 {
    let (project, [x, y]) = two_clips_with_room_beyond_the_rectangle_of_a_clip_group();

    // X could go to 180 (where it touches Y), and every place on the way is allowed: it is in the group, and the rectangle is made longer to
    // have all of it, in the places where it is partly in the rectangle, and it is out of the group again after it.
    assert_eq!(along(&project, &[x], 300), 180);
    // Y could go back to 20 (where it touches X), and it is the same: 180 frames back.
    assert_eq!(along(&project, &[y], -300), -180);
    // (The way that tries each place is what the arithmetic is compared with.)
    assert_eq!(stopped_by_trying_each_place(&project, &[x], 0, 180, &[]), 180);
    assert_eq!(stopped_by_trying_each_place(&project, &[y], 0, -180, &[]), -180);
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
fn a_clip_that_goes_down_to_a_track_that_plays_faster_lasts_less_there_and_goes_further_before_a_clip_is_in_its_way() {
    let (project, x) = a_clip_above_a_clip_group_that_has_a_track_that_plays_faster_in_it();

    // On the middle track X lasts 20 frames, and A is at 100 there: X goes in to the rectangle (which begins at frame 60) and as far as A, 80
    // frames on, where it is at 80 to 100. (If it lasted 40 frames, as it does where it is, that would be 60 frames on.)
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
            frames: 80
        }
    );
    assert_eq!(
        stop_found_where_the_rules_of_clip_groups_can_change(&project, &[x], 1, 80, &[]),
        Some(80)
    );
    assert_eq!(stopped_by_trying_each_place(&project, &[x], 1, 80, &[]), 80);
}

#[test]
fn a_clip_of_one_frame_goes_through_a_clip_group_that_is_one_frame_wide_and_is_in_it_for_the_one_frame() {
    // The rectangle is frame 60 only, across all three tracks. X and Y are on the track between, one frame long each: each is inside the
    // rectangle in one place only, and is in the group there.
    let (mut project, clips) = project_of(&[&[(60, 1)], &[(0, 1), (199, 1)], &[(60, 1)]]);
    let group = project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(clips[0][0]), Member::Clip(clips[2][0])],
        )
        .unwrap();
    let [x, y] = [clips[1][0], clips[1][1]];

    assert_eq!(along(&project, &[x], 100), 100, "nothing stops it");
    assert_eq!(
        along(&project, &[y], -300),
        -199,
        "Y is at 200 and goes back to where X ends, through the rectangle at frame 60"
    );
    assert_eq!(
        stop_found_where_the_rules_of_clip_groups_can_change(&project, &[x], 0, 100, &[]),
        Some(100)
    );
    // At the frame of the rectangle it is in the group; a frame before or after it is not.
    for (shift, is_in_it) in [(59, false), (60, true), (61, false)] {
        let mut moved = project.clone();
        moved
            .move_clips(&moves_by(
                &moved,
                &[x],
                MoveBy {
                    tracks_down: 0,
                    frames: shift,
                },
            ))
            .unwrap();
        assert_eq!(
            moved.group(group).unwrap().has_as_a_member(Member::Clip(x)),
            is_in_it,
            "X {shift} frames on"
        );
    }
}

#[test]
fn the_arithmetic_says_nothing_where_the_clips_are_in_a_place_that_is_not_allowed_as_they_are_going_either_way() {
    // (See the project that is made: X is not allowed where it is on the middle track, whichever way it is wanted.)
    let (project, x) =
        a_clip_that_is_not_allowed_on_the_track_below_where_it_is_because_the_rectangle_would_take_in_another();
    assert_eq!(
        stop_found_where_the_rules_of_clip_groups_can_change(&project, &[x], 1, 5, &[]),
        None
    );
    assert_eq!(
        stop_found_where_the_rules_of_clip_groups_can_change(&project, &[x], 1, -5, &[]),
        None
    );
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

    assert_eq!(
        stop_found_where_the_rules_of_clip_groups_can_change(&project, &[x], 0, 90, &[]),
        Some(90)
    );
    assert_eq!(stopped_by_the_groups(&project, &[x], 0, 90, &[]), 80);
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
/// frame     30  35         55  60  62    65  70        80  90
/// top                      [X     ]
/// middle    [A ]                                       [Y ]
/// bottom                   [B ]    [W   ]
/// ```
///
/// where `A` and `B` are in a clip group (whose rectangle is frames 30 to 60 across the middle and bottom tracks), and `X`, `Y` and `W` are
/// not. Gives the project and `X`, which can be moved down to the middle track, where it would be in the rectangle by the frames 55 to 60,
/// and so in the group, and the rectangle would be made longer to frame 65, which `W` (from 62) would be in: that is not allowed there. A
/// little to the left X is in the group and the rectangle ends where it does, short of `W`; and a little to the right it is out of it.
fn a_clip_that_is_not_allowed_on_the_track_below_where_it_is_because_the_rectangle_would_take_in_another()
-> (Project, ClipId) {
    let (mut project, clips) = project_of(&[&[(55, 10)], &[(30, 5), (45, 10)], &[(55, 5), (2, 8)]]);
    project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(clips[1][0]), Member::Clip(clips[2][0])],
        )
        .unwrap();
    (project, clips[0][0])
}

#[test]
fn a_clip_taken_down_to_a_track_where_it_would_make_the_rectangle_take_in_another_clip_cannot_stay_there_but_can_be_at_a_time_where_it_does_not()
 {
    let (project, x) =
        a_clip_that_is_not_allowed_on_the_track_below_where_it_is_because_the_rectangle_would_take_in_another();

    // (Where X is now, it would be at frames 55 to 65 on the middle track, in the group, with the rectangle made longer to take W in, which is
    // not allowed: and so the time it is wanted at, if the place there is allowed, is where it goes. This is what a clip that is held to the
    // time it started at is: the tracks are chosen for the time it is held to, which is not the time it is at.)
    assert_eq!(
        stopped_by_the_groups(&project, &[x], 1, 15, &[]),
        15,
        "70 to 80: after the rectangle and before Y, and not in the group"
    );
    assert_eq!(
        stopped_by_the_groups(&project, &[x], 1, -20, &[]),
        -20,
        "35 to 45: in the group, inside the rectangle"
    );
    assert_eq!(
        stopped_by_the_groups(&project, &[x], 1, -3, &[]),
        -3,
        "52 to 62: in the group, the rectangle made longer to 62, which is where W begins"
    );
}

#[test]
fn a_clip_wanted_where_it_cannot_be_goes_back_towards_where_it_is_to_the_nearest_place_where_it_can_and_nowhere_if_there_is_none()
 {
    let (project, x) =
        a_clip_that_is_not_allowed_on_the_track_below_where_it_is_because_the_rectangle_would_take_in_another();

    // 25 on is 80 to 90 on the middle track, where Y is; the nearest place back from there that is allowed is 15 on (70 to 80, against Y).
    assert_eq!(stopped_by_the_groups(&project, &[x], 1, 25, &[]), 15);
    // 30 back is 25 to 35, on A; the nearest place towards where X is that is allowed is 20 back (35 to 45, against A).
    assert_eq!(stopped_by_the_groups(&project, &[x], 1, -30, &[]), -20);
    // 3 on is 58 to 68, and so is every place from there back to where X is, as far as 2 back (it is in the group and the rectangle takes
    // in W): there is none.
    assert_eq!(stopped_by_the_groups(&project, &[x], 1, 3, &[]), 0);
}

#[test]
fn a_clip_that_is_not_moved_is_not_stopped() {
    let (project, x) =
        a_clip_that_is_not_allowed_on_the_track_below_where_it_is_because_the_rectangle_would_take_in_another();

    assert_eq!(stopped_by_the_groups(&project, &[x], 1, 0, &[]), 0);
    assert_eq!(stopped_by_the_groups(&project, &[x], 0, 0, &[]), 0);
}

#[test]
fn a_clip_that_goes_to_a_track_that_plays_faster_lasts_less_there_and_is_stopped_by_the_rules_as_a_clip_of_that_length()
{
    // The first track has Z (from 0 to 60), a clip W (80 to 90) and B (180 to 200). The second track plays twice as fast, so A there, which
    // is 40 frames long, lasts 20: from 100 to 120. A and B are a clip group, whose rectangle is 100 to 200 of both tracks; W is just before it.
    let (mut project, clips) = project_of(&[&[(0, 60), (20, 10), (90, 20)], &[(100, 40)]]);
    let second = project.tracks()[1].id();
    project.set_track_speed(second, SpeedFactor::new(2).unwrap()).unwrap();
    let (z, a, b) = (clips[0][0], clips[1][0], clips[0][2]);
    project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(a), Member::Clip(b)])
        .unwrap();

    // Z goes down to the second track, where it lasts 30 frames. It comes to A at 70 (it is then from 70 to 100). A frame later it would be in
    // the rectangle, and join the group, and its rectangle, which would begin at 71, would have W in it (W is on the first track, which the
    // rectangle reaches across, and begins before 90).
    assert_eq!(
        allowed_move(
            &project,
            &[z],
            MoveBy {
                tracks_down: 1,
                frames: 200
            }
        ),
        MoveBy {
            tracks_down: 1,
            frames: 70
        },
        "as a clip of 30 frames, and not of the 60 that it lasts on the track it comes from"
    );
}

#[test]
fn a_clip_is_stopped_before_the_place_where_it_would_be_in_the_rectangles_of_two_groups_that_are_not_one_in_the_other()
{
    // Two groups whose rectangles are over one another, with no clip of the one in the rectangle of the other. The first is P0 (first track,
    // 40 to 50) and P2 (third track, 0 to 10): its rectangle is 0 to 50 across the first three tracks. The second is Q1 (second track, 60 to 70)
    // and Q3 (fourth track, 20 to 30): its rectangle is 20 to 70 across the last three. Both have the frames 20 to 50 of the second and third
    // tracks. X is not in a group, and is on the third track from 100: it joins the second group at 69, when it is in its rectangle, and cannot go
    // on into the first as well (it is not one in the other), which it would at 49.
    let (mut project, clips) = project_of(&[&[(40, 10)], &[(60, 10)], &[(0, 10), (90, 10)], &[(20, 10)]]);
    let (p0, q1, p2, x, q3) = (clips[0][0], clips[1][0], clips[2][0], clips[2][1], clips[3][0]);
    project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(p0), Member::Clip(p2)])
        .unwrap();
    project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(q1), Member::Clip(q3)])
        .unwrap();
    assert_eq!(project.problems_with_the_groups(), vec![], "the setup");

    // It goes as far as 50, where it begins: the frame where the first rectangle ends.
    assert_eq!(along(&project, &[x], -100), -50);
    assert_eq!(
        stop_found_where_the_rules_of_clip_groups_can_change(&project, &[x], 0, -100, &[]),
        Some(-50),
        "found by arithmetic as well"
    );
}

#[test]
fn a_clip_that_goes_into_a_group_in_a_group_joins_the_inner_one_even_where_the_outer_one_was_made_before_it_was_put_round_it()
 {
    // The outer group O is made first (of B0 and B2, from 100 to 110 on the first and third tracks), and the inner one I after (of A0 and A2,
    // from 0 to 10 on the first and third), and I is then put in O: so O comes before I in the list. O's rectangle is 0 to 110 across all
    // three tracks, and I's is 0 to 10. X is on the second track from 200, and goes left: into O's rectangle at 109, and into I's at 9.
    let (mut project, clips) = project_of(&[&[(0, 10), (90, 10)], &[(200, 10)], &[(0, 10), (90, 10)]]);
    let (a0, b0, x, a2, b2) = (clips[0][0], clips[0][1], clips[1][0], clips[2][0], clips[2][1]);
    let outer = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(b0), Member::Clip(b2)])
        .unwrap();
    let inner = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(a0), Member::Clip(a2)])
        .unwrap();
    project.add_to_the_group(outer, Member::Group(inner)).unwrap();
    assert_eq!(project.problems_with_the_groups(), vec![], "the setup");
    assert_eq!(
        project.groups().iter().map(|group| group.id()).collect::<Vec<_>>(),
        [outer, inner],
        "the setup: the outer one is first"
    );

    // X can go all the way to 0 (200 frames): it joins the inner group, and so the outer one, when it is in the rectangle of both.
    assert_eq!(along(&project, &[x], -300), -200);
    assert_eq!(
        stop_found_where_the_rules_of_clip_groups_can_change(&project, &[x], 0, -300, &[]),
        Some(-300),
        "found by arithmetic as well (nothing stops it before the start of the timeline, which is not what this is about)"
    );
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

/// One drag of the test below, found at random and ready to be checked: the project as it is where the drag is, which clips are dragged
/// down how many tracks and how far along they are wanted to go (as far as they can without being in one another's way), and what each
/// of them was in when the drag began.
struct DragToCheck {
    seed: u64,
    trial: u64,
    project: Project,
    moving: Vec<ClipId>,
    tracks_down: i64,
    frames: i64,
    before_the_drag: Vec<ClipGroupBeforeTheDrag>,
    of_a_group: bool,
}

/// What came of checking one [`DragToCheck`].
#[derive(Default)]
struct WhatCameOfADrag {
    /// Whether the clips are allowed where they are and go somewhere, so that the arithmetic that looks for the first place that is not
    /// allowed is what was compared with trying each place.
    was_compared: bool,
    of_the_clips_of_a_group: bool,
    stopped_going_later: bool,
    stopped_going_earlier: bool,
}

impl DragToCheck {
    /// Checks that where the clips stop is the same worked out by arithmetic and found by trying each place.
    fn check(&self) -> WhatCameOfADrag {
        let context = format!(
            "seed {}, trial {}: {:?} down {} tracks and {} frames",
            self.seed, self.trial, self.moving, self.tracks_down, self.frames
        );
        let by_trying = stopped_by_trying_each_place(
            &self.project,
            &self.moving,
            self.tracks_down,
            self.frames,
            &self.before_the_drag,
        );
        assert_eq!(
            stopped_by_the_groups(
                &self.project,
                &self.moving,
                self.tracks_down,
                self.frames,
                &self.before_the_drag
            ),
            by_trying,
            "{context}"
        );
        let is_allowed_where_they_are = the_clips_fit_on_the_tracks_they_would_be_on(
            &self.project,
            &self.moving,
            self.tracks_down,
            0,
            &self.before_the_drag,
        );
        if self.frames == 0 || !is_allowed_where_they_are {
            return WhatCameOfADrag::default();
        }
        assert_eq!(
            stop_found_where_the_rules_of_clip_groups_can_change(
                &self.project,
                &self.moving,
                self.tracks_down,
                self.frames,
                &self.before_the_drag
            ),
            Some(by_trying),
            "{context}"
        );
        WhatCameOfADrag {
            was_compared: true,
            of_the_clips_of_a_group: self.of_a_group,
            stopped_going_later: by_trying != self.frames && self.frames > 0,
            stopped_going_earlier: by_trying != self.frames && self.frames < 0,
        }
    }
}

#[test]
fn the_place_that_is_worked_out_by_arithmetic_is_the_one_that_is_found_by_trying_each_place_in_random_projects_that_have_groups()
 {
    let mut random = DeterministicRandomNumbers::with_seed(4242);
    let mut that_had_joined_a_group_as_they_were_dragged = 0;
    // (The drags are found one after another, as the random numbers are one sequence; then each is checked, several at once, as trying
    // each of its places is what takes the time.)
    let mut drags = Vec::new();
    for seed in 1..=60u64 {
        let project_before = project_after_random_edits_with_more_clip_groups(seed, 120);
        let all = every_clip(&project_before);
        let has_clip_groups = project_before
            .groups()
            .iter()
            .any(|group| group.kind() == GroupKind::ClipGroup);
        if all.is_empty() || !has_clip_groups {
            continue;
        }
        for trial in 0..80u64 {
            let of_a_group = trial % 3 == 0;
            let moving = if of_a_group {
                the_clips_of_a_clip_group_at_random(&mut random, &project_before, &all).unwrap()
            } else {
                some_clips_at_random(&mut random, &all)
            };
            // What each was in when the drag began; and, in half of the trials, a drag that has gone on for a while: the clips have been
            // moved to a place that was allowed, as the drag does, which can have put some of them in a clip group.
            let before_the_drag = project_before.clip_groups_before_a_drag_of(&moving);
            let mut project = project_before.clone();
            if trial % 2 == 1 {
                let tracks_down = tracks_down_at_random(&mut random, &project, &moving);
                let first = allowed_move_holding(
                    &project,
                    &moving,
                    MoveBy {
                        tracks_down,
                        frames: random.next_below(240) as i64 - 120,
                    },
                    None,
                    &before_the_drag,
                );
                project
                    .move_clips_remembering(&moves_by(&project, &moving, first), &before_the_drag)
                    .expect("what is allowed can be done");
                that_had_joined_a_group_as_they_were_dragged += usize::from(moving.iter().any(|clip| {
                    before_the_drag
                        .iter()
                        .any(|before| before.clip == *clip && before.group.is_none())
                        && project.shape_group_of(Member::Clip(*clip)).is_some()
                }));
            }
            let tracks_down = tracks_down_at_random(&mut random, &project, &moving);
            let wanted = random.next_below(240) as i64 - 120;
            let frames = allowed_frames(&project, &moving, tracks_down, wanted);
            drags.push(DragToCheck {
                seed,
                trial,
                project,
                moving,
                tracks_down,
                frames,
                before_the_drag,
                of_a_group,
            });
        }
    }

    let results = check_every_case_at_once(&drags, DragToCheck::check);

    let compared = results.iter().filter(|result| result.was_compared).count();
    let moved_the_clips_of_a_group = results
        .iter()
        .filter(|result| result.was_compared && result.of_the_clips_of_a_group)
        .count();
    let stopped_going_later = results.iter().filter(|result| result.stopped_going_later).count();
    let stopped_going_earlier = results.iter().filter(|result| result.stopped_going_earlier).count();
    if std::env::var_os("FASTFORWARDRS_SHOW_RANDOM_EDIT_STATISTICS").is_some() {
        println!(
            "{compared} compared, {moved_the_clips_of_a_group} of the clips of a group, {stopped_going_later} stopped going later, {stopped_going_earlier} going earlier, {that_had_joined_a_group_as_they_were_dragged} drags that had put a clip in a group"
        );
    }
    assert!(
        compared >= 800 && moved_the_clips_of_a_group >= 300,
        "{compared} places were compared, {moved_the_clips_of_a_group} of them of the clips of a group"
    );
    assert!(
        stopped_going_later >= 50 && stopped_going_earlier >= 50,
        "the test is worth little if clips are seldom stopped: {stopped_going_later} going later, {stopped_going_earlier} going earlier"
    );
    assert!(
        that_had_joined_a_group_as_they_were_dragged >= 30,
        "the test is worth little if drags seldom have clips that have joined a group: {that_had_joined_a_group_as_they_were_dragged}"
    );
}

#[test]
fn how_far_clips_can_go_is_worked_out_without_trying_each_place_so_that_taking_them_very_far_takes_no_longer() {
    let (project, [_, _, b, z]) = two_tracks_with_a_clip_group();
    // (Trying each of four million places, which is what it would take to find that nothing is in the way, copies and checks the project
    // that many times: measured, 12 microseconds a place for this little project in a build for testing, so close to a minute, and more for a
    // project with more in it. Worked out, it is the same few sums as for one frame.)
    let far = 4_000_000;
    let started = std::time::Instant::now();

    assert_eq!(
        stopped_by_the_groups(&project, &[z], 0, far, &[]),
        far,
        "nothing is after Z"
    );
    assert_eq!(
        stopped_by_the_groups(&project, &[b], 0, far, &[]),
        80,
        "B stops where the rectangle would take in Z"
    );
    assert_eq!(
        stop_found_where_the_rules_of_clip_groups_can_change(&project, &[z], 0, i64::MAX / 4, &[]),
        Some(i64::MAX / 4)
    );
    assert_eq!(
        stop_found_where_the_rules_of_clip_groups_can_change(&project, &[b], 0, i64::MAX / 4, &[]),
        Some(80)
    );
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
    // before, at 20 to 30, in the clear. It is not a drag along the track: it is going to that time, and the rectangle at 40 to 50 is on the
    // way, where X is in the group for the places it is partly or all in it.)
    assert_eq!(stopped_by_the_groups(&project, &[x], 1, -80, &[]), -80);
    // The arithmetic is about the rectangles and nothing else: it sees nothing in the way, and says X can go the 80 frames.
    assert_eq!(
        stop_found_where_the_rules_of_clip_groups_can_change(&project, &[x], 1, -80, &[]),
        Some(-80)
    );
    // Nearer than that, where there is no rectangle, is the time it is wanted at as well.
    assert_eq!(stopped_by_the_groups(&project, &[x], 1, -40, &[]), -40);
}

#[test]
fn the_arithmetic_says_nothing_where_the_clips_would_not_be_allowed_as_they_are_and_where_a_track_is_not_there() {
    let (project, x) =
        a_clip_that_is_not_allowed_on_the_track_below_where_it_is_because_the_rectangle_would_take_in_another();

    // (X is at 55 to 65: on the middle track it would be in the group, and the rectangle would take in W, whichever way it were wanted.)
    assert_eq!(
        stop_found_where_the_rules_of_clip_groups_can_change(&project, &[x], 1, 25, &[]),
        None
    );
    assert_eq!(
        stop_found_where_the_rules_of_clip_groups_can_change(&project, &[x], 1, -30, &[]),
        None
    );
    // There is no track below the bottom one, or above the top one.
    assert_eq!(
        stop_found_where_the_rules_of_clip_groups_can_change(&project, &[x], 3, 25, &[]),
        None
    );
    assert_eq!(
        stop_found_where_the_rules_of_clip_groups_can_change(&project, &[x], -1, 25, &[]),
        None
    );
    // Where it is, it can be taken as far as it is wanted, in the clear (the top track has no clip group on it).
    assert_eq!(
        stop_found_where_the_rules_of_clip_groups_can_change(&project, &[x], 0, 25, &[]),
        Some(25)
    );
}

// --- joining and leaving as a drag goes on ---

#[test]
fn a_clip_that_is_in_another_clip_group_stops_against_the_rectangle_of_this_one_and_does_not_go_into_it() {
    // The clips of the first group are on the first two tracks (frames 0 to 10 and 20 to 30), and those of the other are on the third track
    // (frames 0 to 10 and 20 to 30): one of them cannot be moved up to the second track, where it would be in the rectangle of the first group
    // and be in the other.
    let (mut project, clips) = project_of(&[&[(0, 10)], &[(20, 10)], &[(0, 10), (10, 10)]]);
    project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(clips[0][0]), Member::Clip(clips[1][0])],
        )
        .unwrap();
    project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(clips[2][0]), Member::Clip(clips[2][1])],
        )
        .unwrap();
    let in_the_other = clips[2][0];

    let allowed = allowed_move(
        &project,
        &[in_the_other],
        MoveBy {
            tracks_down: -1,
            frames: 5,
        },
    );

    assert_eq!(allowed.tracks_down, 0, "the second track is not one that it can be on");
    assert_eq!(
        stop_found_where_the_rules_of_clip_groups_can_change(&project, &[in_the_other], -1, 5, &[]),
        None,
        "it is not allowed where it is"
    );
}

/// A project of three tracks:
///
/// ```text
/// frame     0    10   40       60       100  120
/// top                [A ]
/// middle    [X ]                        [   ]
/// bottom             [B ]               [Z ]
/// ```
///
/// where `A` and `B` are in a clip group (whose rectangle is frames 40 to 60 across all three tracks) and `X` and `Z` are not (`X` is on the
/// middle track, which the rectangle reaches across, and `Z` is on the bottom track, after it). Gives the project and `X`.
fn a_clip_that_is_dragged_along_a_track_through_a_rectangle_which_would_take_in_a_clip_if_the_clip_stayed_in_the_group()
-> (Project, ClipId) {
    let (mut project, clips) = project_of(&[&[(40, 20)], &[(0, 10)], &[(40, 20), (40, 20)]]);
    project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(clips[0][0]), Member::Clip(clips[2][0])],
        )
        .unwrap();
    (project, clips[1][0])
}

#[test]
fn a_clip_that_joined_a_clip_group_as_it_was_dragged_is_not_stopped_by_what_its_rectangle_would_take_in_if_it_stayed_in_it()
 {
    let (mut project, x) = a_clip_that_is_dragged_along_a_track_through_a_rectangle_which_would_take_in_a_clip_if_the_clip_stayed_in_the_group();
    let before_the_drag = project.clip_groups_before_a_drag_of(&[x]);
    // Dragged in: X is at 45 to 55, inside the rectangle (40 to 60), and so in the group.
    project
        .move_clips_remembering(
            &moves_by(
                &project,
                &[x],
                MoveBy {
                    tracks_down: 0,
                    frames: 45,
                },
            ),
            &before_the_drag,
        )
        .unwrap();
    assert!(project.shape_group_of(Member::Clip(x)).is_some(), "it has joined");

    // As a member of the group it could be taken only as far as the rectangle can be made longer without taking in Z (at 100 on the bottom
    // track): so that it ends at 100, 45 frames on from where it is. As a clip that was in no group when the drag began it is out of the
    // group when it is past the rectangle, and goes on.
    let wanted = MoveBy {
        tracks_down: 0,
        frames: 120,
    };
    assert_eq!(allowed_move_holding(&project, &[x], wanted, None, &[]).frames, 45);
    assert_eq!(
        allowed_move_holding(&project, &[x], wanted, None, &before_the_drag).frames,
        120
    );
    assert_eq!(
        stopped_by_trying_each_place(&project, &[x], 0, 120, &before_the_drag),
        120,
        "the way that tries each place says the same"
    );
}

#[test]
fn a_clip_that_was_in_a_clip_group_when_the_drag_began_is_stopped_by_what_its_rectangle_takes_in_as_it_goes() {
    let (mut project, clips) = project_of(&[&[(40, 20)], &[(0, 10)], &[(40, 20), (40, 20)]]);
    let (a, b) = (clips[0][0], clips[2][0]);
    project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(a), Member::Clip(b)])
        .unwrap();
    let before_the_drag = project.clip_groups_before_a_drag_of(&[a]);

    // A is in the group, and the rectangle is made longer as A goes, to Z (on the bottom track at 100): 40 frames, to where A is at 80 to 100.
    let wanted = MoveBy {
        tracks_down: 0,
        frames: 120,
    };
    assert_eq!(
        allowed_move_holding(&project, &[a], wanted, None, &before_the_drag).frames,
        40
    );
}
