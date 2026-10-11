use super::editing::{ClipEditError, InsertError, SplitError};
use super::*;
use crate::project::tracks::media_clip::MediaClip;
use crate::project::tracks::text_object::{TextClip, TextObject};
use crate::project::{SourceFrameIndex, SourceMediaIdentifier};
use crate::test_support::DeterministicRandomNumbers;

const SOURCE: SourceMediaIdentifier = SourceMediaIdentifier::new(1);

fn speed(factor: u32) -> SpeedFactor {
    SpeedFactor::new(factor).unwrap()
}

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

fn media(first: u64, end: u64, factor: u32) -> MediaClip {
    MediaClip::new(
        SOURCE,
        SourceFrameIndex::new(first),
        SourceFrameIndex::new(end),
        speed(factor),
    )
    .unwrap()
}

/// A video clip of source frames `first` up to `end` at `factor`x.
fn video(first: u64, end: u64, factor: u32) -> Clip {
    Clip::Video(media(first, end, factor))
}

fn sound(first: u64, end: u64, factor: u32) -> Clip {
    Clip::Audio(media(first, end, factor))
}

fn text(duration: u64) -> Clip {
    Clip::Text(TextClip::new(TextObject::default(), duration).unwrap())
}

/// A track with these clips, each with the number of empty frames before it.
fn track_of(clips: Vec<(u64, Clip)>) -> Track {
    Track::new(TrackId::new(1), "track 0").with_clips(clips)
}

/// Where each clip starts and ends, in order.
fn spans(track: &Track) -> Vec<(u64, u64)> {
    track
        .placements()
        .map(|placement| {
            (
                placement.first_timeline_frame.value(),
                placement.end_timeline_frame.value(),
            )
        })
        .collect()
}

fn ids(track: &Track) -> Vec<ClipId> {
    track.entries().iter().map(TrackEntry::id).collect()
}

/// Everything that must be true of a track, however it was made.
fn assert_track_is_consistent(track: &Track) {
    let mut previous_end = 0;
    let mut seen_ids = std::collections::HashSet::new();
    for (index, placement) in track.placements().enumerate() {
        let entry = &track.entries()[index];
        assert_eq!(placement.index, index);
        assert!(
            placement.end_timeline_frame > placement.first_timeline_frame,
            "a clip lasts a frame at least"
        );
        assert!(
            placement.first_timeline_frame.value() >= previous_end,
            "clip {index} overlaps the one before it: {:?}",
            spans(track)
        );
        assert_eq!(
            placement.first_timeline_frame.value() - previous_end,
            entry.gap_before_frames(),
            "the gap before clip {index} is where its clip starts"
        );
        assert!(seen_ids.insert(placement.id), "{} is used twice", placement.id);
        previous_end = placement.end_timeline_frame.value();
    }
    assert_eq!(
        track.total_frame_count(),
        previous_end,
        "the track lasts to the end of its last clip"
    );
}

// --- laying clips out ---

#[test]
fn clips_are_laid_out_one_after_another_with_the_empty_time_between_them() {
    // 100 frames, then 50 empty, then 300 source frames at 3x (100 frames), then a 30 frame text.
    let track = track_of(vec![(0, video(0, 100, 1)), (50, video(100, 400, 3)), (0, text(30))]);
    assert_eq!(spans(&track), vec![(0, 100), (150, 250), (250, 280)]);
    assert_eq!(track.total_frame_count(), 280);
    assert_track_is_consistent(&track);
}

#[test]
fn a_track_can_start_with_empty_time() {
    let track = track_of(vec![(40, video(0, 60, 1))]);
    assert_eq!(spans(&track), vec![(40, 100)]);
    assert_eq!(track.total_frame_count(), 100);
}

#[test]
fn an_empty_track_lasts_no_time_and_all_of_it_is_free() {
    let track = Track::new(TrackId::new(7), "track 6");
    assert!(track.is_empty());
    assert_eq!(track.total_frame_count(), 0);
    assert_eq!(track.id(), TrackId::new(7));
    assert_eq!(track.title(), "track 6");
    assert_eq!(
        track.room_at(frame(500)),
        Room::Free {
            from_frame: 0,
            until_frame: None
        }
    );
    assert!(track.placement_covering(frame(0)).is_none());
}

#[test]
fn a_new_track_plays_at_normal_speed_and_loudness_and_is_not_muted() {
    let track = Track::new(TrackId::new(1), "track 0");
    assert_eq!(track.speed(), SpeedFactor::NORMAL);
    assert_eq!(track.gain(), Gain::UNCHANGED);
    assert!(!track.is_muted());
}

#[test]
fn the_clip_at_a_frame_is_found_and_a_frame_in_a_gap_has_none() {
    let track = track_of(vec![(0, video(0, 100, 1)), (50, video(100, 400, 3))]);
    let covering = |number: u64| track.placement_covering(frame(number)).map(|placement| placement.index);
    assert_eq!(covering(0), Some(0));
    assert_eq!(covering(99), Some(0));
    assert_eq!(covering(100), None, "the first frame of the gap");
    assert_eq!(covering(149), None, "the last frame of the gap");
    assert_eq!(covering(150), Some(1));
    assert_eq!(covering(249), Some(1));
    assert_eq!(covering(250), None, "past the end");
    assert_eq!(covering(10_000), None);
}

#[test]
fn what_is_at_a_frame_says_whether_a_clip_is_there_or_how_much_room_there_is() {
    let track = track_of(vec![(20, video(0, 100, 1)), (50, text(10))]);
    // Clips: 20..120 and 170..180.
    let free = |from_frame, until_frame| Room::Free {
        from_frame,
        until_frame,
    };
    assert_eq!(track.room_at(frame(0)), free(0, Some(20)));
    assert_eq!(track.room_at(frame(19)), free(0, Some(20)));
    assert_eq!(track.room_at(frame(20)), Room::Occupied(ClipId::new(0)));
    assert_eq!(track.room_at(frame(119)), Room::Occupied(ClipId::new(0)));
    assert_eq!(track.room_at(frame(120)), free(120, Some(170)));
    assert_eq!(track.room_at(frame(169)), free(120, Some(170)));
    assert_eq!(track.room_at(frame(170)), Room::Occupied(ClipId::new(1)));
    assert_eq!(track.room_at(frame(180)), free(180, None));
    assert_eq!(track.room_at(frame(9999)), free(180, None));
}

#[test]
fn the_source_frame_at_a_timeline_frame_uses_the_clips_speed_times_the_tracks() {
    let track = track_of(vec![(10, video(100, 700, 3))]).with_speed(speed(2));
    // 600 source frames at 6x: 100 timeline frames, from 10 to 110.
    assert_eq!(spans(&track), vec![(10, 110)]);
    let placement = track.placement_covering(frame(12)).unwrap();
    assert_eq!(placement.effective_speed, speed(6));
    assert_eq!(placement.source_frame_at(frame(10)), Some(SourceFrameIndex::new(100)));
    assert_eq!(placement.source_frame_at(frame(12)), Some(SourceFrameIndex::new(112)));
    assert_eq!(placement.source_frame_at(frame(110)), None, "not in the clip");
    assert_eq!(placement.offset_of(frame(12)), Some(2));
}

#[test]
fn a_text_clip_has_no_source_frame_and_ignores_the_speed_of_its_track() {
    let track = track_of(vec![(0, text(30))]).with_speed(speed(5));
    assert_eq!(spans(&track), vec![(0, 30)]);
    let placement = track.placement_covering(frame(3)).unwrap();
    assert_eq!(placement.source_frame_at(frame(3)), None);
    assert_eq!(placement.effective_speed, SpeedFactor::NORMAL);
}

// --- cutting ---

#[test]
fn cutting_a_clip_makes_two_that_last_as_long_together_and_moves_nothing_else() {
    let mut track = track_of(vec![(0, video(0, 300, 1)), (40, video(300, 400, 1)), (0, text(20))]);
    let spans_before = spans(&track);
    let first_id = ids(&track)[0];

    let outcome = track.split_at(frame(120)).unwrap();

    assert_eq!(outcome.first_half, first_id, "the clip that was cut keeps its name");
    assert_ne!(outcome.second_half, first_id);
    assert_eq!(spans(&track), vec![(0, 120), (120, 300), (340, 440), (440, 460)]);
    assert_eq!(track.total_frame_count(), spans_before.last().unwrap().1);
    assert_eq!(track.entries()[1].gap_before_frames(), 0, "the halves touch");
    assert_eq!(track.entries()[2].gap_before_frames(), 40, "the gap after is as it was");
    assert_track_is_consistent(&track);
}

#[test]
fn cutting_a_sped_up_clip_cuts_it_at_the_source_frame_that_is_shown_there() {
    let mut track = track_of(vec![(0, video(0, 100, 1)), (0, video(100, 300, 4))]);
    // Timeline frame 107 is the second clip's frame 7, which shows source frame 100 + 7 * 4 = 128.
    track.split_at(frame(107)).unwrap();
    let source_ranges: Vec<(u64, u64)> = track
        .entries()
        .iter()
        .map(|entry| {
            let media = entry.clip().as_media().unwrap();
            (media.first_source_frame().value(), media.end_source_frame().value())
        })
        .collect();
    assert_eq!(source_ranges, vec![(0, 100), (100, 128), (128, 300)]);
    assert_eq!(track.total_frame_count(), 100 + 50, "the length did not change");
}

#[test]
fn cutting_a_text_clip_makes_two_text_clips() {
    let mut track = track_of(vec![(10, text(90))]);
    track.split_at(frame(40)).unwrap();
    assert_eq!(spans(&track), vec![(10, 40), (40, 100)]);
    assert!(track.entries().iter().all(|entry| entry.clip().as_text().is_some()));
}

#[test]
fn a_cut_that_cannot_be_made_is_refused_and_changes_nothing() {
    let mut track = track_of(vec![(0, video(0, 100, 1)), (50, video(100, 200, 1))]);
    let before = track.clone();
    assert_eq!(
        track.split_at(frame(0)),
        Err(SplitError::AlreadyAtClipBoundary {
            timeline_frame: frame(0)
        })
    );
    assert_eq!(
        track.split_at(frame(150)),
        Err(SplitError::AlreadyAtClipBoundary {
            timeline_frame: frame(150)
        })
    );
    assert_eq!(
        track.split_at(frame(120)),
        Err(SplitError::NoClipThere {
            timeline_frame: frame(120)
        }),
        "in the gap"
    );
    assert_eq!(
        track.split_at(frame(500)),
        Err(SplitError::NoClipThere {
            timeline_frame: frame(500)
        }),
        "past the end"
    );
    assert_eq!(track, before);
}

#[test]
fn cutting_a_sped_up_clip_never_changes_its_length_even_when_the_speed_does_not_divide_it() {
    for clip_speed in [2u32, 3, 4, 5, 7, 16] {
        for track_speed in [1u32, 2, 3] {
            let reference = track_of(vec![(0, video(10, 10 + 101, clip_speed))]).with_speed(speed(track_speed));
            for split_frame in 1..reference.total_frame_count() {
                let mut track = reference.clone();
                track.split_at(frame(split_frame)).unwrap();
                assert_eq!(
                    track.total_frame_count(),
                    reference.total_frame_count(),
                    "101 frames at {clip_speed}x on a {track_speed}x track cut at {split_frame}"
                );
            }
        }
    }
}

// --- changing a clip's length: what moves ---

#[test]
fn clips_that_touch_move_up_together_when_one_before_them_gets_shorter() {
    // A video made of three clips touching: 100, 100 and 100 frames.
    let mut track = track_of(vec![
        (0, video(0, 100, 1)),
        (0, video(100, 200, 1)),
        (0, video(200, 300, 1)),
    ]);
    let middle = ids(&track)[1];

    track.set_clip_speed(middle, speed(4)).unwrap();

    assert_eq!(
        spans(&track),
        vec![(0, 100), (100, 125), (125, 225)],
        "the last one followed"
    );
    assert_eq!(track.total_frame_count(), 225);
    assert_track_is_consistent(&track);
}

#[test]
fn a_gap_stops_the_movement_so_what_comes_after_it_stays_where_it_is() {
    // 100 frames, 20 empty, 100 frames, 30 empty, 50 frames.
    let mut track = track_of(vec![
        (0, video(0, 100, 1)),
        (20, video(100, 200, 1)),
        (30, video(200, 250, 1)),
    ]);
    let first = ids(&track)[0];

    track.set_clip_speed(first, speed(10)).unwrap(); // 10 frames now

    assert_eq!(
        spans(&track),
        vec![(0, 10), (120, 220), (250, 300)],
        "the clips after the gap did not move, and the gap grew to fill what was left"
    );
    assert_eq!(track.entries()[1].gap_before_frames(), 110);
    assert_track_is_consistent(&track);
}

#[test]
fn a_clip_that_grows_pushes_the_next_one_on_only_as_far_as_it_has_to() {
    // 100 frames at 4x is 25; then 10 empty; then another clip.
    let mut track = track_of(vec![
        (0, video(0, 100, 4)),
        (10, video(100, 200, 1)),
        (0, video(200, 300, 1)),
    ]);
    assert_eq!(spans(&track), vec![(0, 25), (35, 135), (135, 235)]);
    let first = ids(&track)[0];

    // Back to normal speed it is 100 frames: it runs 65 past where the next one began.
    track.set_clip_speed(first, speed(1)).unwrap();

    assert_eq!(
        spans(&track),
        vec![(0, 100), (100, 200), (200, 300)],
        "pushed, and the one after follows"
    );
    assert_track_is_consistent(&track);

    // Growing by less than the gap leaves the next clip alone.
    let mut track = track_of(vec![(0, video(0, 100, 10)), (50, video(100, 200, 1))]);
    assert_eq!(spans(&track), vec![(0, 10), (60, 160)]);
    let first = ids(&track)[0];
    track.set_clip_speed(first, speed(5)).unwrap(); // 20 frames
    assert_eq!(spans(&track), vec![(0, 20), (60, 160)]);
}

#[test]
fn text_stays_exactly_where_it_was_put_when_the_clip_before_it_changes_length() {
    // A video of 100 frames with a text clip right after it, then a video clip after that.
    let mut track = track_of(vec![(0, video(0, 100, 1)), (0, text(30)), (0, video(100, 200, 1))]);
    let video_id = ids(&track)[0];

    track.set_clip_speed(video_id, speed(4)).unwrap(); // 25 frames

    assert_eq!(
        spans(&track),
        vec![(0, 25), (100, 130), (130, 230)],
        "the text did not move, and the video that touches the text stays with it"
    );
    assert_track_is_consistent(&track);
}

#[test]
fn the_speed_of_the_track_shortens_every_video_and_sound_but_not_text_or_the_gaps() {
    let mut track = track_of(vec![
        (0, video(0, 100, 1)),
        (0, sound(0, 100, 1)),
        (20, text(30)),
        (10, video(0, 60, 1)),
    ]);
    assert_eq!(spans(&track), vec![(0, 100), (100, 200), (220, 250), (260, 320)]);

    track.set_speed(speed(2));

    assert_eq!(
        spans(&track),
        vec![(0, 50), (50, 100), (220, 250), (260, 290)],
        "the text and the clip after it are anchored: they stay where they were"
    );
    assert_eq!(track.speed(), speed(2));
    assert_track_is_consistent(&track);

    track.set_speed(speed(1));
    assert_eq!(
        spans(&track),
        vec![(0, 100), (100, 200), (220, 250), (260, 320)],
        "and back again"
    );
}

#[test]
fn changing_the_speed_to_what_it_already_is_does_nothing() {
    let mut track = track_of(vec![(0, video(0, 100, 3))]);
    let before = track.clone();
    track.set_speed(SpeedFactor::NORMAL);
    assert_eq!(track, before);
}

#[test]
fn the_gain_and_muting_of_a_clip_change_nothing_about_where_things_are() {
    let mut track = track_of(vec![(0, video(0, 100, 1)), (0, sound(0, 100, 2))]);
    let spans_before = spans(&track);
    let first = ids(&track)[0];
    track.set_clip_gain(first, Gain::from_decibels(6.0)).unwrap();
    track.set_clip_muted(first, true).unwrap();
    assert_eq!(spans(&track), spans_before);
    let media = track.entries()[0].clip().as_media().unwrap();
    assert_eq!(media.gain(), Gain::from_decibels(6.0));
    assert!(media.is_muted());
}

#[test]
fn speed_gain_and_muting_apply_to_videos_and_sounds_and_not_to_text() {
    let mut track = track_of(vec![(0, text(30)), (0, video(0, 30, 1)), (0, sound(0, 30, 1))]);
    let ids = ids(&track);
    assert_eq!(
        track.set_clip_speed(ids[0], speed(2)),
        Err(ClipEditError::NotAMediaClip { clip: ids[0] })
    );
    assert_eq!(
        track.set_clip_gain(ids[0], Gain::UNCHANGED),
        Err(ClipEditError::NotAMediaClip { clip: ids[0] })
    );
    assert_eq!(
        track.set_clip_muted(ids[0], true),
        Err(ClipEditError::NotAMediaClip { clip: ids[0] })
    );
    assert!(track.set_clip_speed(ids[1], speed(2)).is_ok());
    assert!(track.set_clip_speed(ids[2], speed(2)).is_ok());
    assert_eq!(
        track.set_clip_speed(ClipId::new(99), speed(2)),
        Err(ClipEditError::UnknownClip { clip: ClipId::new(99) })
    );
}

// --- deleting ---

#[test]
fn deleting_a_video_closes_up_the_clips_touching_it() {
    let mut track = track_of(vec![
        (0, video(0, 100, 1)),
        (0, video(100, 200, 1)),
        (0, video(200, 300, 1)),
    ]);
    let ids_before = ids(&track);

    track.delete_clip(ids_before[1]).unwrap();

    assert_eq!(spans(&track), vec![(0, 100), (100, 200)], "no hole where it was");
    assert_eq!(ids(&track), vec![ids_before[0], ids_before[2]]);
    assert_track_is_consistent(&track);
}

#[test]
fn deleting_a_video_that_has_empty_time_before_it_keeps_that_empty_time() {
    let mut track = track_of(vec![(30, video(0, 100, 1)), (0, video(100, 200, 1))]);
    let first = ids(&track)[0];
    track.delete_clip(first).unwrap();
    assert_eq!(
        spans(&track),
        vec![(30, 130)],
        "the clip that was behind it is where the deleted one began"
    );
}

#[test]
fn deleting_a_video_does_not_move_what_is_after_a_gap() {
    let mut track = track_of(vec![
        (0, video(0, 100, 1)),
        (0, video(100, 200, 1)),
        (50, video(200, 300, 1)),
    ]);
    let second = ids(&track)[1];
    track.delete_clip(second).unwrap();
    assert_eq!(
        spans(&track),
        vec![(0, 100), (250, 350)],
        "the last clip stayed put and the gap grew"
    );
    assert_track_is_consistent(&track);
}

#[test]
fn deleting_text_leaves_a_hole_and_moves_nothing_even_a_video_that_touched_it() {
    let mut track = track_of(vec![(10, text(20)), (0, video(0, 50, 1)), (0, text(10))]);
    assert_eq!(spans(&track), vec![(10, 30), (30, 80), (80, 90)]);
    let first_text = ids(&track)[0];

    track.delete_clip(first_text).unwrap();

    assert_eq!(spans(&track), vec![(30, 80), (80, 90)], "the video is where it was");
    assert_track_is_consistent(&track);
}

#[test]
fn deleting_the_half_of_a_cut_text_leaves_the_other_half_where_it_was() {
    // This is how a caption is made to start later or end sooner.
    let mut track = track_of(vec![(10, text(90))]);
    track.split_at(frame(40)).unwrap();
    let first_half = ids(&track)[0];
    track.delete_clip(first_half).unwrap();
    assert_eq!(spans(&track), vec![(40, 100)]);
}

#[test]
fn deleting_the_last_or_only_clip_leaves_a_shorter_or_empty_track() {
    let mut track = track_of(vec![(0, video(0, 100, 1)), (20, video(100, 200, 1))]);
    let second = ids(&track)[1];
    track.delete_clip(second).unwrap();
    assert_eq!(track.total_frame_count(), 100);
    let first = ids(&track)[0];
    track.delete_clip(first).unwrap();
    assert!(track.is_empty());
    assert_eq!(track.total_frame_count(), 0);
}

#[test]
fn deleting_a_clip_that_is_not_there_is_an_error_and_changes_nothing() {
    let mut track = track_of(vec![(0, video(0, 100, 1))]);
    let before = track.clone();
    assert_eq!(
        track.delete_clip(ClipId::new(5)),
        Err(ClipEditError::UnknownClip { clip: ClipId::new(5) })
    );
    assert_eq!(track, before);
}

// --- text ---

#[test]
fn text_can_be_put_in_empty_time_and_nothing_else_moves() {
    let mut track = track_of(vec![(0, video(0, 100, 1)), (200, video(100, 200, 1))]);
    let before = spans(&track);

    let (id, duration) = track.insert_text_at(frame(150), TextObject::default(), 60).unwrap();

    assert_eq!(duration, 60);
    assert_eq!(spans(&track), vec![before[0], (150, 210), before[1]]);
    assert_eq!(track.placement_of(id).unwrap().first_timeline_frame, frame(150));
    assert_track_is_consistent(&track);
}

#[test]
fn text_put_in_a_small_space_lasts_only_as_long_as_the_space() {
    let mut track = track_of(vec![(0, video(0, 100, 1)), (30, video(100, 200, 1))]);
    // Frames 100..130 are free: asking for 90 frames at 110 leaves room for 20.
    let (_, duration) = track.insert_text_at(frame(110), TextObject::default(), 90).unwrap();
    assert_eq!(duration, 20);
    assert_eq!(spans(&track), vec![(0, 100), (110, 130), (130, 230)]);
    assert_track_is_consistent(&track);
}

#[test]
fn text_cannot_be_put_where_a_clip_is() {
    let mut track = track_of(vec![(0, video(0, 100, 1))]);
    let before = track.clone();
    assert!(track.insert_text_at(frame(50), TextObject::default(), 30).is_none());
    assert_eq!(track, before);
    assert_eq!(track.longest_new_text_clip_at(frame(50), 30), None);
    assert_eq!(track.longest_new_text_clip_at(frame(100), 30), Some(30));
}

#[test]
fn a_clip_that_does_not_fit_is_refused_with_how_much_room_there_was() {
    let mut track = track_of(vec![(0, video(0, 100, 1)), (30, video(100, 200, 1))]);
    let before = track.clone();
    assert_eq!(
        track.insert_clip_at(frame(110), video(0, 50, 1)),
        Err(InsertError::NotEnoughRoom {
            frames_of_room: 20,
            needed: 50
        })
    );
    assert_eq!(
        track.insert_clip_at(frame(50), video(0, 10, 1)),
        Err(InsertError::Occupied {
            timeline_frame: frame(50)
        })
    );
    assert_eq!(track, before);
    // Exactly the room there is fits.
    assert!(track.insert_clip_at(frame(110), video(0, 20, 1)).is_ok());
    assert_track_is_consistent(&track);
}

#[test]
fn a_clip_put_on_the_track_is_given_a_name_no_other_clip_has_ever_had() {
    let mut track = track_of(vec![(0, text(10))]);
    let first = ids(&track)[0];
    track.delete_clip(first).unwrap();
    let (new_id, _) = track.insert_text_at(frame(0), TextObject::default(), 10).unwrap();
    assert_ne!(new_id, first, "a deleted clip's name is not used again");
}

#[test]
fn changing_what_a_text_clip_says_changes_nothing_about_where_it_is() {
    let mut track = track_of(vec![(10, text(30))]);
    let id = ids(&track)[0];
    let object = TextObject {
        text: "Hello".to_owned(),
        font_family: "Arial".to_owned(),
        ..TextObject::default()
    };
    track.set_text_object(id, object.clone()).unwrap();
    assert_eq!(track.entries()[0].clip().as_text().unwrap().object, object);
    assert_eq!(spans(&track), vec![(10, 40)]);
    let mut track_with_video = track_of(vec![(0, video(0, 10, 1))]);
    let video_id = ids(&track_with_video)[0];
    assert_eq!(
        track_with_video.set_text_object(video_id, object),
        Err(ClipEditError::NotATextClip { clip: video_id })
    );
}

#[test]
fn a_text_clip_can_be_made_longer_up_to_the_next_clip_and_shorter_and_the_next_clip_stays() {
    let mut track = track_of(vec![(0, text(30)), (20, video(0, 100, 1))]);
    let text_id = ids(&track)[0];
    assert_eq!(spans(&track), vec![(0, 30), (50, 150)]);

    assert_eq!(track.set_text_duration(text_id, 45).unwrap(), 45);
    assert_eq!(spans(&track), vec![(0, 45), (50, 150)]);

    assert_eq!(
        track.set_text_duration(text_id, 500).unwrap(),
        50,
        "as long as there is room, and no longer"
    );
    assert_eq!(spans(&track), vec![(0, 50), (50, 150)]);

    assert_eq!(track.set_text_duration(text_id, 10).unwrap(), 10);
    assert_eq!(
        spans(&track),
        vec![(0, 10), (50, 150)],
        "the video after it did not move"
    );
    assert_eq!(
        track.set_text_duration(text_id, 0).unwrap(),
        1,
        "it lasts a frame at least"
    );
    assert_track_is_consistent(&track);
}

#[test]
fn the_last_text_clip_can_be_made_as_long_as_anyone_likes() {
    let mut track = track_of(vec![(5, text(30))]);
    let id = ids(&track)[0];
    assert_eq!(track.set_text_duration(id, 100_000).unwrap(), 100_000);
    assert_eq!(track.total_frame_count(), 100_005);
}

// --- properties that must hold whatever is done ---

#[derive(Debug)]
enum Operation {
    Cut { frame: u64 },
    ChangeSpeed { clip_number: usize, speed: u32 },
    ChangeTrackSpeed { speed: u32 },
    Delete { clip_number: usize },
    AddText { frame: u64, duration: u64 },
    ChangeTextDuration { clip_number: usize, duration: u64 },
}

fn random_operation(random: &mut DeterministicRandomNumbers, track: &Track) -> Operation {
    let clip_count = track.entries().len().max(1) as u64;
    let clip_number = random.next_below(clip_count) as usize;
    let end = track.total_frame_count() + 60;
    match random.next_below(10) {
        0..=2 => Operation::Cut {
            frame: random.next_below(end),
        },
        3..=4 => Operation::ChangeSpeed {
            clip_number,
            speed: [1u32, 1, 2, 3, 4, 7, 16][random.next_below(7) as usize],
        },
        5 => Operation::ChangeTrackSpeed {
            speed: [1u32, 1, 2, 3][random.next_below(4) as usize],
        },
        6 => Operation::Delete { clip_number },
        7..=8 => Operation::AddText {
            frame: random.next_below(end),
            duration: 1 + random.next_below(80),
        },
        _ => Operation::ChangeTextDuration {
            clip_number,
            duration: random.next_below(100),
        },
    }
}

/// What each clip (named by id) looks like on the timeline.
fn spans_by_id(track: &Track) -> Vec<(ClipId, u64, u64)> {
    track
        .placements()
        .map(|placement| {
            (
                placement.id,
                placement.first_timeline_frame.value(),
                placement.end_timeline_frame.value(),
            )
        })
        .collect()
}

#[test]
fn whatever_is_done_to_a_track_it_stays_consistent_and_the_clips_that_should_not_move_do_not() {
    for seed in 1..=40u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed);
        let mut track = track_of(vec![
            (0, video(0, 300, 1)),
            (0, video(300, 500, 1)),
            (15, sound(0, 400, 1)),
            (0, text(40)),
            (25, video(500, 700, 1)),
        ]);
        for step in 0..120 {
            let before = spans_by_id(&track);
            let total_before = track.total_frame_count();
            let operation = random_operation(&mut random, &track);
            let context = format!("seed {seed}, step {step}: {operation:?} on {before:?}");
            let changed_clip = match &operation {
                Operation::Cut { frame: at } => {
                    let covering = track
                        .placement_covering(TimelineFrameIndex::new(*at))
                        .map(|placement| placement.id);
                    if track.split_at(TimelineFrameIndex::new(*at)).is_ok() {
                        assert_eq!(
                            track.total_frame_count(),
                            total_before,
                            "a cut changes no length: {context}"
                        );
                        // Nothing but the cut clip changed: the others are exactly where they were.
                        let after = spans_by_id(&track);
                        let others_before: Vec<_> = before.iter().filter(|item| Some(item.0) != covering).collect();
                        let others_after: Vec<_> = after
                            .iter()
                            .filter(|item| Some(item.0) == covering || before.iter().any(|old| old.0 == item.0))
                            .filter(|item| Some(item.0) != covering)
                            .collect();
                        assert_eq!(others_before, others_after, "{context}");
                    }
                    None
                }
                Operation::ChangeSpeed {
                    clip_number,
                    speed: factor,
                } => {
                    let Some(entry) = track.entries().get(*clip_number) else {
                        continue;
                    };
                    let id = entry.id();
                    if track.set_clip_speed(id, speed(*factor)).is_ok() {
                        Some(id)
                    } else {
                        None
                    }
                }
                Operation::ChangeTrackSpeed { speed: factor } => {
                    track.set_speed(speed(*factor));
                    None
                }
                Operation::Delete { clip_number } => {
                    let Some(entry) = track.entries().get(*clip_number) else {
                        continue;
                    };
                    let id = entry.id();
                    let was_text = entry.clip().as_text().is_some();
                    track.delete_clip(id).unwrap();
                    if was_text {
                        // Text goes without anything else moving.
                        let after = spans_by_id(&track);
                        let expected: Vec<_> = before.iter().filter(|item| item.0 != id).copied().collect();
                        assert_eq!(after, expected, "{context}");
                    }
                    Some(id)
                }
                Operation::AddText { frame: at, duration } => {
                    if let Some((id, _)) =
                        track.insert_text_at(TimelineFrameIndex::new(*at), TextObject::default(), *duration)
                    {
                        let after = spans_by_id(&track);
                        let others: Vec<_> = after.iter().filter(|item| item.0 != id).copied().collect();
                        assert_eq!(others, before, "adding text moves nothing: {context}");
                    }
                    None
                }
                Operation::ChangeTextDuration { clip_number, duration } => {
                    let Some(entry) = track.entries().get(*clip_number) else {
                        continue;
                    };
                    let id = entry.id();
                    if track.set_text_duration(id, *duration).is_ok() {
                        let after = spans_by_id(&track);
                        let others_before: Vec<_> = before.iter().filter(|item| item.0 != id).collect();
                        let others_after: Vec<_> = after.iter().filter(|item| item.0 != id).collect();
                        assert_eq!(
                            others_before, others_after,
                            "a text clip's length moves nothing: {context}"
                        );
                    }
                    None
                }
            };
            assert_track_is_consistent(&track);

            // Whatever was done to a clip, the clips before it did not move.
            if let Some(changed) = changed_clip {
                for (id, start, end) in &before {
                    if *id == changed {
                        break;
                    }
                    let now = track.placement_of(*id).map(|placement| {
                        (
                            placement.first_timeline_frame.value(),
                            placement.end_timeline_frame.value(),
                        )
                    });
                    assert_eq!(
                        now,
                        Some((*start, *end)),
                        "a clip before the changed one moved: {context}"
                    );
                }
            }
            // And no clip ever moves earlier than it was, unless it moved along with a media clip
            // that got shorter or went (so a text clip never does).
            for placement in track.placements() {
                if placement.clip.as_text().is_some()
                    && let Some(old) = before.iter().find(|item| item.0 == placement.id)
                    && !matches!(operation, Operation::Cut { .. })
                {
                    assert!(
                        placement.first_timeline_frame.value() >= old.1,
                        "text moved earlier: {context}"
                    );
                }
            }
        }
    }
}
