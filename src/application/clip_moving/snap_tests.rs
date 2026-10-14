//! Tests of holding a dragged clip to the time it started at (see [`Snap`]).

use super::tests::project_of;
use super::*;
use crate::test_support::DeterministicRandomNumbers;

const FIRST: TrackId = TrackId::new(0);
const SECOND: TrackId = TrackId::new(1);

/// A hold on `clip` to `start`, that reaches `within_frames` and is not free on any track.
fn hold(clip: ClipId, start: u64, within_frames: u64) -> Snap {
    Snap {
        clip,
        start,
        within_frames,
        free_on: None,
    }
}

fn move_of(project: &Project, clips: &[ClipId], tracks_down: i64, frames: i64, snap: Snap) -> MoveBy {
    allowed_move_holding(project, clips, MoveBy { tracks_down, frames }, Some(snap), &[])
}

#[test]
fn a_clip_going_to_another_track_is_held_to_its_time_when_the_pointer_would_put_it_near_that() {
    let (project, clips) = project_of(&[&[(100, 50)], &[]]);
    let clip = [clips[0][0]];
    let held = hold(clips[0][0], 100, 5);

    assert_eq!(
        move_of(&project, &clip, 1, 0, held),
        MoveBy {
            tracks_down: 1,
            frames: 0
        }
    );
    assert_eq!(
        move_of(&project, &clip, 1, 3, held),
        MoveBy {
            tracks_down: 1,
            frames: 0
        },
        "3 frames along is near: it stays where it started"
    );
    assert_eq!(
        move_of(&project, &clip, 1, -5, held),
        MoveBy {
            tracks_down: 1,
            frames: 0
        },
        "and so is 5 frames back, which is as far as the hold reaches"
    );
    assert_eq!(
        move_of(&project, &clip, 1, 5, held),
        MoveBy {
            tracks_down: 1,
            frames: 0
        }
    );
    assert_eq!(
        move_of(&project, &clip, 1, 6, held),
        MoveBy {
            tracks_down: 1,
            frames: 6
        },
        "further than that it is let go and follows the pointer"
    );
    assert_eq!(
        move_of(&project, &clip, 1, -6, held),
        MoveBy {
            tracks_down: 1,
            frames: -6
        }
    );
}

#[test]
fn a_hold_that_reaches_no_frames_holds_only_what_is_already_exactly_at_the_time() {
    let (project, clips) = project_of(&[&[(100, 50)], &[]]);
    let clip = [clips[0][0]];
    let held = hold(clips[0][0], 100, 0);
    assert_eq!(move_of(&project, &clip, 1, 1, held).frames, 1);
    assert_eq!(move_of(&project, &clip, 1, 0, held).frames, 0);
}

#[test]
fn the_hold_is_measured_from_where_the_pointer_would_put_the_clip_and_the_move_is_from_where_it_is_now() {
    // The clip was moved to 103 along the way, so it starts at 103 and the time it started at was 100.
    let (project, clips) = project_of(&[&[(103, 50)], &[]]);
    let clip = [clips[0][0]];
    let held = hold(clips[0][0], 100, 5);

    // The pointer would put it at 105, which is near enough: back 3 frames to 100.
    assert_eq!(
        move_of(&project, &clip, 1, 2, held),
        MoveBy {
            tracks_down: 1,
            frames: -3
        }
    );
    // At 109 it is not near enough, and the clip goes there, 6 frames on.
    assert_eq!(
        move_of(&project, &clip, 1, 6, held),
        MoveBy {
            tracks_down: 1,
            frames: 6
        }
    );
}

#[test]
fn a_clip_is_not_held_on_the_track_it_is_free_on_but_is_on_any_other() {
    let (project, clips) = project_of(&[&[(100, 50)], &[]]);
    let clip = [clips[0][0]];
    let held = Snap {
        free_on: Some(FIRST),
        ..hold(clips[0][0], 100, 5)
    };

    assert_eq!(
        move_of(&project, &clip, 0, 3, held),
        MoveBy {
            tracks_down: 0,
            frames: 3
        },
        "along the track it started on it goes where the pointer goes"
    );
    assert_eq!(
        move_of(&project, &clip, 1, 3, held),
        MoveBy {
            tracks_down: 1,
            frames: 0
        },
        "and it is held as it arrives on another"
    );
}

#[test]
fn a_clip_that_has_been_to_another_track_is_held_when_it_goes_back_to_the_first() {
    // The clip is on the second track now, at 103; the time it started at was 100, on the first.
    let (project, clips) = project_of(&[&[], &[(103, 50)]]);
    let clip = [clips[1][0]];
    let held = hold(clips[1][0], 100, 5); // free nowhere

    assert_eq!(
        move_of(&project, &clip, -1, 1, held),
        MoveBy {
            tracks_down: -1,
            frames: -3
        }
    );
    let free_on_the_first = Snap {
        free_on: Some(FIRST),
        ..held
    };
    assert_eq!(
        move_of(&project, &clip, -1, 1, free_on_the_first),
        MoveBy {
            tracks_down: -1,
            frames: 1
        },
        "unless it is free there"
    );
    assert_eq!(
        move_of(&project, &clip, 0, 1, free_on_the_first),
        MoveBy {
            tracks_down: 0,
            frames: -3
        },
        "and it is held where it is, on the second"
    );
}

#[test]
fn a_clip_goes_to_a_track_where_it_fits_at_the_time_it_is_held_to_but_not_where_it_is() {
    // The clip is at 103..153 and the clip below is at 152..172: one frame in the way. At 100..150 there is room.
    let (project, clips) = project_of(&[&[(103, 50)], &[(152, 20)]]);
    let clip = [clips[0][0]];
    let held = hold(clips[0][0], 100, 5);

    assert_eq!(
        allowed_move(
            &project,
            &clip,
            MoveBy {
                tracks_down: 1,
                frames: 0
            }
        ),
        MoveBy {
            tracks_down: 0,
            frames: 0
        },
        "without the hold it stays on its own track, as it did"
    );
    assert_eq!(
        move_of(&project, &clip, 1, 0, held),
        MoveBy {
            tracks_down: 1,
            frames: -3
        },
        "with it, it goes down and is put at the time it started at"
    );
}

#[test]
fn a_clip_is_not_held_to_a_time_that_it_does_not_fit_at_it_stops_as_near_as_it_can_get() {
    // The clip is at 103..153. The clip below is at 60..101, which would overlap the time it is held to by a frame.
    let (project, clips) = project_of(&[&[(103, 50)], &[(60, 41)]]);
    let clip = [clips[0][0]];
    let held = hold(clips[0][0], 100, 5);

    assert_eq!(
        move_of(&project, &clip, 1, 0, held),
        MoveBy {
            tracks_down: 1,
            frames: -2
        },
        "up against the end of the clip it cannot be on top of"
    );
}

#[test]
fn a_clip_that_fits_nowhere_on_the_track_it_is_over_stays_on_its_own_and_is_held_there_unless_it_is_free() {
    // The clip below covers 90..200, which is in the way at any time near 100.
    let (project, clips) = project_of(&[&[(103, 50)], &[(90, 110)]]);
    let clip = [clips[0][0]];
    let free_on_the_first = Snap {
        free_on: Some(FIRST),
        ..hold(clips[0][0], 100, 5)
    };

    assert_eq!(
        move_of(&project, &clip, 1, 2, free_on_the_first),
        MoveBy {
            tracks_down: 0,
            frames: 2
        },
        "it stays on the first track, where it is free, and goes along it"
    );
    assert_eq!(
        move_of(&project, &clip, 1, 2, hold(clips[0][0], 100, 5)),
        MoveBy {
            tracks_down: 0,
            frames: -3
        },
        "or if it is held there too, it goes back to the time"
    );
}

#[test]
fn clips_that_move_together_all_go_by_as_much_as_the_one_that_is_held() {
    let (project, clips) = project_of(&[&[(103, 20), (30, 20)], &[(300, 20)]]);
    let both = [clips[0][0], clips[0][1]];
    let held = hold(clips[0][0], 100, 5);

    assert_eq!(
        move_of(&project, &both, 1, 1, held),
        MoveBy {
            tracks_down: 1,
            frames: -3
        }
    );
    assert_eq!(
        as_starts(&project, &both, move_of(&project, &both, 1, 1, held)),
        vec![100, 150],
        "the first at the time it started at, and the second as far from it as it was"
    );
}

fn as_starts(project: &Project, clips: &[ClipId], by: MoveBy) -> Vec<u64> {
    moves_by(project, clips, by)
        .iter()
        .map(|moved| moved.start.value())
        .collect()
}

#[test]
fn a_hold_on_a_clip_that_is_not_there_holds_nothing() {
    let (project, clips) = project_of(&[&[(100, 50)], &[]]);
    let clip = [clips[0][0]];
    let missing = hold(ClipId::new(SECOND, 99), 100, 5);
    assert_eq!(
        allowed_move_holding(
            &project,
            &clip,
            MoveBy {
                tracks_down: 1,
                frames: 3
            },
            Some(missing),
            &[]
        ),
        allowed_move(
            &project,
            &clip,
            MoveBy {
                tracks_down: 1,
                frames: 3
            }
        ),
    );
}

/// The same random projects as the tests of moving clips use.
fn random_project(random: &mut DeterministicRandomNumbers) -> (Project, Vec<ClipId>) {
    let track_count = 2 + random.next_below(3) as usize;
    let tracks: Vec<Vec<(u64, u64)>> = (0..track_count)
        .map(|_| {
            (0..random.next_below(7))
                .map(|_| (random.next_below(30), 1 + random.next_below(60)))
                .collect()
        })
        .collect();
    let slices: Vec<&[(u64, u64)]> = tracks.iter().map(Vec::as_slice).collect();
    let (project, clips) = project_of(&slices);
    let all: Vec<ClipId> = clips.into_iter().flatten().collect();
    (project, all)
}

#[test]
fn whatever_is_allowed_with_a_hold_can_be_done_and_a_hold_that_cannot_reach_changes_nothing() {
    let mut random = DeterministicRandomNumbers::with_seed(77);
    let mut held_at_least_once = 0;
    for round in 0..600 {
        let (project, all) = random_project(&mut random);
        if all.is_empty() {
            continue;
        }
        let moving: Vec<ClipId> = all.iter().copied().filter(|_| random.next_below(3) == 0).collect();
        let Some(&held_clip) = moving.first() else {
            continue;
        };
        let wanted = MoveBy {
            tracks_down: random.next_below(5) as i64 - 2,
            frames: random.next_below(60) as i64 - 30,
        };
        // A time that is often not far from where the clip is, so that the hold often reaches.
        let now = project.placement_of(held_clip).unwrap().first_timeline_frame.value();
        let held_to_the_time = Snap {
            clip: held_clip,
            start: (now + random.next_below(41)).saturating_sub(20),
            within_frames: random.next_below(30),
            free_on: [None, Some(FIRST), Some(SECOND)][random.next_below(3) as usize],
        };

        let allowed = allowed_move_holding(&project, &moving, wanted, Some(held_to_the_time), &[]);
        let context = format!(
            "round {round}: {wanted:?} was wanted of {moving:?}, {allowed:?} was allowed, {held_to_the_time:?}"
        );

        // What was allowed can be done, and it never goes further in the tracks than was wanted.
        let moves = moves_by(&project, &moving, allowed);
        assert_eq!(moves.len(), moving.len(), "{context}");
        assert_eq!(project.clone().move_clips(&moves), Ok(()), "{context}");
        assert!(
            (allowed.tracks_down >= 0 && allowed.tracks_down <= wanted.tracks_down)
                || (allowed.tracks_down <= 0 && allowed.tracks_down >= wanted.tracks_down),
            "{context}"
        );
        if allowed != allowed_move(&project, &moving, wanted) {
            held_at_least_once += 1;
        }

        // A hold on a time that is nowhere near never holds anything.
        let nowhere_near = Snap {
            start: 1_000_000,
            ..held_to_the_time
        };
        assert_eq!(
            allowed_move_holding(&project, &moving, wanted, Some(nowhere_near), &[]),
            allowed_move(&project, &moving, wanted),
            "{context}: a hold on a time a million frames away"
        );
    }
    assert!(
        held_at_least_once > 80,
        "the holds mattered in only {held_at_least_once} of the rounds"
    );
}
