//! Exporting projects of several tracks with the real ffmpeg, checked picture by picture and tone by
//! tone against what the project says the finished video must be: empty time is black, the video in
//! front is the one seen, the sound of every track is mixed, and the speed and volume of a track
//! count.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fastforwardrs::media::FfmpegToolchain;
use fastforwardrs::media::export::{ExportEvent, ExportResult, VideoExport, plan_export};
use fastforwardrs::project::{
    Angle, CanvasArea, ClipLocation, Gain, Placement, Project, Settings, SpeedFactor, TextObject, TimelineFrameIndex,
};
use fastforwardrs::rgba_picture::RgbaPicture;
use support::{
    ClipOfTheVideo, TestVideoSpecification, counter_video, decode_video, decoded_sound, expected_source_frames,
    is_black, loudness, project_with_a_picture_over_a_video, project_with_a_video_and_a_tone_file,
    project_with_tracks_of_videos, red_then_blue_text_picture, scratch_directory, stream_durations, test_video,
    tone_strength, transparent_text_picture,
};

const TIME_ALLOWED_FOR_AN_EXPORT: Duration = Duration::from_secs(180);

/// How loud ffmpeg's `sine` source is, which is the sound of the test videos and tone files.
const TONE_AMPLITUDE: f64 = 0.125;

fn clip(gap_before_frames: u64, first_source_frame: u64, end_source_frame: u64, speed: u32) -> ClipOfTheVideo {
    ClipOfTheVideo {
        gap_before_frames,
        first_source_frame,
        end_source_frame,
        speed,
    }
}

/// Exports `project` to `destination` (reading its files from where its links say they are) and waits.
fn export(toolchain: &FfmpegToolchain, project: &Project, destination: &Path) {
    export_with_text_pictures(toolchain, project, destination, &transparent_text_picture);
}

/// Like [`export`], with `render_text` making the pictures of the text objects.
fn export_with_text_pictures(
    toolchain: &FfmpegToolchain,
    project: &Project,
    destination: &Path,
    render_text: &dyn Fn(&TextObject, u32, u32) -> RgbaPicture,
) {
    let plan = plan_export(project, Settings::DEFAULT, render_text).expect("the project can be exported");
    let source_paths: Vec<PathBuf> = plan
        .source_files
        .iter()
        .map(|identifier| project.source(*identifier).unwrap().link.absolute_path.clone())
        .collect();
    let export = VideoExport::start(toolchain, &plan, &source_paths, destination, Arc::new(|| {})).expect("it starts");
    let started = Instant::now();
    loop {
        match export.try_next_event() {
            Some(ExportEvent::Finished(result)) => {
                assert_eq!(result, ExportResult::Completed);
                return;
            }
            Some(ExportEvent::Progress { .. }) => {}
            None => {
                assert!(
                    started.elapsed() < TIME_ALLOWED_FOR_AN_EXPORT,
                    "the export did not finish in time"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

// --- pictures ---

#[test]
fn empty_time_on_a_track_is_black_for_exactly_as_many_frames_as_it_lasts() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    // Frames 0..60 of the video, then 30 empty frames, then frames 60..120; with 20 empty frames first.
    let project = project_with_tracks_of_videos(
        &toolchain,
        &[&video],
        &[(0, &[clip(20, 0, 60, 1), clip(30, 60, 120, 1)])],
    );
    let scratch = scratch_directory();
    let destination = scratch.path().join("gaps.mp4");
    export(&toolchain, &project, &destination);

    let decoded = decode_video(&toolchain, &destination, 320, 180);
    assert_eq!(decoded.pictures.len(), 20 + 60 + 30 + 60);
    for timeline_frame in 0..decoded.pictures.len() {
        let picture = &decoded.pictures[timeline_frame];
        let expected: Option<u64> = match timeline_frame {
            0..20 => None,
            20..80 => Some(timeline_frame as u64 - 20),
            80..110 => None,
            _ => Some(timeline_frame as u64 - 110 + 60),
        };
        match expected {
            None => assert!(is_black(picture), "timeline frame {timeline_frame} should be black"),
            Some(source_frame) => {
                assert!(
                    !is_black(picture),
                    "timeline frame {timeline_frame} should show a picture"
                );
                assert_eq!(
                    support::frame_number_in_picture(picture, 320, 180, 3),
                    source_frame,
                    "timeline frame {timeline_frame}"
                );
            }
        }
    }
}

#[test]
fn of_two_videos_at_the_same_time_the_one_in_front_is_seen_and_the_one_behind_picks_up_where_it_would_have_been() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    // In front: source frames 300..330, from timeline frame 40. Behind: source frames 0..120 from the start.
    let project = project_with_tracks_of_videos(
        &toolchain,
        &[&video, &video],
        &[(0, &[clip(40, 300, 330, 1)]), (1, &[clip(0, 0, 120, 1)])],
    );
    let scratch = scratch_directory();
    let destination = scratch.path().join("layers.mp4");
    export(&toolchain, &project, &destination);

    let frame_numbers = decode_video(&toolchain, &destination, 320, 180).frame_numbers();
    let expected: Vec<u64> = (0..120u64)
        .map(|timeline_frame| match timeline_frame {
            40..70 => 300 + (timeline_frame - 40),
            _ => timeline_frame,
        })
        .collect();
    assert_eq!(frame_numbers, expected);
}

fn portrait_video(toolchain: &FfmpegToolchain) -> support::TestVideo {
    test_video(
        toolchain,
        &TestVideoSpecification {
            name: "portrait_3_seconds",
            width_in_pixels: 180,
            height_in_pixels: 320,
            duration_in_seconds: 3,
            with_audio: false,
            ..TestVideoSpecification::COUNTER
        },
    )
}

/// Whether the 320x180 `picture` has only black (as near as video gets) at `(x, y)`.
fn is_black_at(picture: &[u8], x: usize, y: usize) -> bool {
    picture[(y * 320 + x) * 3..(y * 320 + x) * 3 + 3]
        .iter()
        .all(|value| *value <= 3)
}

#[test]
fn a_video_of_another_shape_than_the_first_is_fitted_inside_the_picture_with_black_beside_it() {
    let toolchain = toolchain_or_skip!();
    let landscape = counter_video(&toolchain); // 320x180: the shape of the finished picture
    let portrait = portrait_video(&toolchain);
    // The landscape video is the first source, so it gives the picture its shape. The portrait video
    // is on the track in front for the first 30 frames, with nothing behind it; the landscape one
    // comes after, for 60.
    let project = project_with_tracks_of_videos(
        &toolchain,
        &[&landscape, &portrait],
        &[(1, &[clip(0, 0, 30, 1)]), (0, &[clip(30, 0, 60, 1)])],
    );
    assert_eq!(project.picture_size(), (320, 180));
    let scratch = scratch_directory();
    let destination = scratch.path().join("fitted.mp4");
    export(&toolchain, &project, &destination);

    let decoded = decode_video(&toolchain, &destination, 320, 180);
    assert_eq!(decoded.pictures.len(), 90);
    // While the portrait video is on its own: it is 100 pixels wide in the middle (110..210), black either side.
    let picture = &decoded.pictures[10];
    for y in [5, 90, 175] {
        assert!(is_black_at(picture, 5, y), "black on the left at row {y}");
        assert!(is_black_at(picture, 314, y), "black on the right at row {y}");
        // (The strip along the top of the test pictures can be black or white, so look along the row.)
        assert!(
            (112..208).any(|x| !is_black_at(picture, x, y)),
            "the video in the middle at row {y}"
        );
    }
    // After it, the landscape video fills the picture.
    let later = &decoded.pictures[40];
    assert!(!is_black_at(later, 5, 90));
}

/// A behaviour that changed when videos could be put anywhere on the picture: what is behind a video
/// that does not fill the picture is seen where it does not reach, and the bars beside a video of
/// another shape are no longer black over it.
#[test]
fn a_video_behind_one_of_another_shape_shows_through_the_bars_beside_it() {
    let toolchain = toolchain_or_skip!();
    let landscape = counter_video(&toolchain);
    let portrait = portrait_video(&toolchain);
    // The portrait video is in front for 30 frames, over the landscape one, which is there for 60.
    let project = project_with_tracks_of_videos(
        &toolchain,
        &[&landscape, &portrait],
        &[(1, &[clip(0, 0, 30, 1)]), (0, &[clip(0, 0, 60, 1)])],
    );
    let scratch = scratch_directory();
    let destination = scratch.path().join("through_the_bars.mp4");
    export(&toolchain, &project, &destination);

    let decoded = decode_video(&toolchain, &destination, 320, 180);
    assert_eq!(decoded.pictures.len(), 60);
    let picture = &decoded.pictures[10];
    // (Not the strip along the top of the test pictures, which is black where its digits are not.)
    for y in [90, 175] {
        assert!(
            (0..100).any(|x| !is_black_at(picture, x, y)),
            "the landscape video on the left of the portrait one at row {y}"
        );
        assert!(
            (220..320).any(|x| !is_black_at(picture, x, y)),
            "the landscape video on the right of the portrait one at row {y}"
        );
    }
    // The middle is the portrait video, which is as it was whatever is behind it: look at the same
    // frame of a project that has only the portrait video. (The video is compressed, and what is
    // beside it is not the same, so the same pictures come out a shade apart, not exactly alike.)
    let alone = project_with_tracks_of_videos(&toolchain, &[&landscape, &portrait], &[(1, &[clip(0, 0, 30, 1)])]);
    let alone_destination = scratch.path().join("alone.mp4");
    export(&toolchain, &alone, &alone_destination);
    let alone_pictures = decode_video(&toolchain, &alone_destination, 320, 180).pictures;
    for y in [20, 90, 160] {
        for x in 112..208 {
            let index = (y * 320 + x) * 3;
            for channel in 0..3 {
                let (layered, on_its_own) = (picture[index + channel], alone_pictures[10][index + channel]);
                assert!(
                    layered.abs_diff(on_its_own) <= 16,
                    "the middle at ({x}, {y}) is the portrait video: {layered} and {on_its_own}"
                );
            }
        }
    }
}

// --- where videos and text are on the picture, and how far they are turned ---

/// The first clip of the first track of `project`.
fn the_first_clip(project: &Project) -> ClipLocation {
    ClipLocation {
        track: project.tracks()[0].id(),
        clip: project.tracks()[0].entries()[0].id(),
    }
}

/// The colour of the pixel at `(x, y)` of a 320x180 picture.
fn colour_at(picture: &[u8], x: usize, y: usize) -> [u8; 3] {
    let index = (y * 320 + x) * 3;
    [picture[index], picture[index + 1], picture[index + 2]]
}

/// Whether there is something that is not black anywhere in a rectangle of the 320x180 picture
/// (the numbered test pictures are not all one colour, so a single point can be black by chance).
fn something_in(picture: &[u8], columns: std::ops::Range<usize>, rows: std::ops::Range<usize>) -> bool {
    rows.into_iter().any(|y| {
        columns
            .clone()
            .any(|x| colour_at(picture, x, y).iter().any(|value| *value > 20))
    })
}

#[test]
fn a_video_put_in_the_middle_at_half_the_size_is_exported_there_with_black_round_it() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut project = project_with_tracks_of_videos(&toolchain, &[&video], &[(0, &[clip(0, 0, 60, 1)])]);
    project
        .set_clip_placement(
            the_first_clip(&project),
            Placement::upright(CanvasArea::new(0.25, 0.25, 0.5, 0.5)),
        )
        .unwrap();
    let scratch = scratch_directory();
    let destination = scratch.path().join("half_size.mp4");
    export(&toolchain, &project, &destination);

    let decoded = decode_video(&toolchain, &destination, 320, 180);
    assert_eq!(decoded.pictures.len(), 60);
    let picture = &decoded.pictures[10];
    // The video fills 80..240 across and 45..135 down.
    for (x, y) in [(20, 20), (300, 160), (60, 90), (260, 90), (160, 30), (160, 150)] {
        assert!(is_black(&colour_at(picture, x, y)), "({x}, {y}) is outside the video");
    }
    assert!(something_in(picture, 100..220, 60..120), "the video is in the middle");
    assert!(something_in(picture, 82..100, 50..130), "up to its left edge");
    assert!(something_in(picture, 222..238, 50..130), "up to its right edge");
}

#[test]
fn a_video_that_is_turned_a_quarter_is_exported_standing_up_where_its_middle_was() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut project = project_with_tracks_of_videos(&toolchain, &[&video], &[(0, &[clip(0, 0, 60, 1)])]);
    project
        .set_clip_placement(
            the_first_clip(&project),
            Placement::new(CanvasArea::new(0.25, 0.25, 0.5, 0.5), Angle::from_degrees(90.0)),
        )
        .unwrap();
    let scratch = scratch_directory();
    let destination = scratch.path().join("turned.mp4");
    export(&toolchain, &project, &destination);

    let decoded = decode_video(&toolchain, &destination, 320, 180);
    let picture = &decoded.pictures[10];
    // The 160 by 90 video, with its middle at (160, 90), now fills 115..205 across and 10..170 down.
    for (x, y) in [(90, 90), (230, 90), (100, 30), (220, 150), (20, 20)] {
        assert!(
            is_black(&colour_at(picture, x, y)),
            "({x}, {y}) is outside the turned video"
        );
    }
    assert!(
        something_in(picture, 125..195, 15..45),
        "the video reaches up above where it was"
    );
    assert!(something_in(picture, 125..195, 135..165), "and down below");
    assert!(something_in(picture, 125..195, 60..120), "and is in the middle");
}

#[test]
fn text_turned_a_quarter_clockwise_has_what_was_its_left_at_the_top() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut project = project_with_tracks_of_videos(&toolchain, &[&video], &[(0, &[clip(0, 0, 60, 1)])]);
    let front = project.add_track_in_front();
    project
        .insert_text(
            front,
            fastforwardrs::project::TimelineFrameIndex::new(0),
            TextObject {
                area: CanvasArea::new(0.25, 0.25, 0.5, 0.5),
                rotation: Angle::from_degrees(90.0),
                ..TextObject::default()
            },
            60,
        )
        .unwrap();
    let scratch = scratch_directory();
    let destination = scratch.path().join("turned_text.mp4");
    export_with_text_pictures(&toolchain, &project, &destination, &red_then_blue_text_picture);

    let decoded = decode_video(&toolchain, &destination, 320, 180);
    let picture = &decoded.pictures[10];
    // The 160 by 90 picture, red on the left and blue on the right, with its middle at (160, 90),
    // turned a quarter clockwise: red on the top half of what is 90 across and 160 down.
    let [red, green, blue] = colour_at(picture, 160, 40);
    assert!(
        red > 150 && green < 90 && blue < 90,
        "the left of it is at the top: {red} {green} {blue}"
    );
    let [red, green, blue] = colour_at(picture, 160, 140);
    assert!(
        blue > 150 && red < 90 && green < 90,
        "the right of it is at the bottom: {red} {green} {blue}"
    );
    // Where the text was before it was turned, to either side of the middle, it is the video again.
    let [red, _, blue] = colour_at(picture, 100, 90);
    assert!(red < 150 && blue < 150, "(100, 90) is not the text any more");
}

#[test]
fn something_turned_at_the_edge_of_the_picture_is_cut_off_there_and_the_picture_stays_its_size() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    // The video fills the whole picture and is turned a quarter: its corners reach out of the picture.
    let mut project = project_with_tracks_of_videos(&toolchain, &[&video], &[(0, &[clip(0, 0, 30, 1)])]);
    project
        .set_clip_placement(
            the_first_clip(&project),
            Placement::new(CanvasArea::WHOLE_PICTURE, Angle::from_degrees(90.0)),
        )
        .unwrap();
    let scratch = scratch_directory();
    let destination = scratch.path().join("cut_off.mp4");
    export(&toolchain, &project, &destination);

    let decoded = decode_video(&toolchain, &destination, 320, 180);
    assert_eq!(decoded.pictures.len(), 30);
    let picture = &decoded.pictures[5];
    // The 320 by 180 video turned a quarter is 180 across and 320 down, in the middle: 70..250 across.
    assert!(is_black(&colour_at(picture, 30, 90)), "left of the turned video");
    assert!(is_black(&colour_at(picture, 290, 90)), "right of the turned video");
    assert!(
        something_in(picture, 100..220, 5..175),
        "the turned video fills the picture from top to bottom"
    );
}

// --- pictures from files ---

fn is_red(colour: [u8; 3]) -> bool {
    colour[0] > 200 && colour[1] < 60 && colour[2] < 60
}

fn is_blue(colour: [u8; 3]) -> bool {
    colour[2] > 200 && colour[0] < 60 && colour[1] < 60
}

/// Whether the picture file of [`project_with_a_picture_over_a_video`], put in the finished picture
/// as it is (fitted: 320 across and 160 down), is over what is shown: red at the left, blue at the right.
fn the_picture_file_is_over(picture: &[u8]) -> bool {
    is_red(colour_at(picture, 60, 90)) && is_blue(colour_at(picture, 260, 90))
}

/// Whether neither colour of the picture file is where it would be: nothing of it is shown.
fn the_picture_file_is_not_over(picture: &[u8]) -> bool {
    !is_red(colour_at(picture, 60, 90)) && !is_blue(colour_at(picture, 260, 90))
}

#[test]
fn a_picture_over_a_video_that_is_cut_into_pieces_is_over_each_of_them() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let (mut project, _picture) = project_with_a_picture_over_a_video(&toolchain, scratch.path(), 0, 60);
    // Cut the video (and not the picture) in two places: three pieces, the one picture over all of them.
    let video_track = project.tracks().last().unwrap().id();
    for at in [20, 40] {
        let video = ClipLocation {
            track: video_track,
            clip: project
                .tracks()
                .last()
                .unwrap()
                .placement_covering(TimelineFrameIndex::new(at))
                .unwrap()
                .id,
        };
        assert_eq!(project.split_at(TimelineFrameIndex::new(at), Some(&[video])).len(), 1);
    }
    let destination = scratch.path().join("picture_over_pieces.mp4");
    export(&toolchain, &project, &destination);

    let decoded = decode_video(&toolchain, &destination, 320, 180);
    assert_eq!(decoded.pictures.len(), 60);
    for frame in [0, 10, 19, 20, 30, 39, 40, 50, 59] {
        assert!(
            the_picture_file_is_over(&decoded.pictures[frame]),
            "the picture is over frame {frame}"
        );
    }
}

#[test]
fn a_picture_that_begins_later_is_over_the_video_only_for_as_long_as_it_is_shown() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let (project, _picture) = project_with_a_picture_over_a_video(&toolchain, scratch.path(), 20, 20);
    let destination = scratch.path().join("picture_in_the_middle.mp4");
    export(&toolchain, &project, &destination);

    let decoded = decode_video(&toolchain, &destination, 320, 180);
    assert_eq!(decoded.pictures.len(), 60, "the video is what is as long");
    for frame in [0, 10, 19, 40, 50, 59] {
        assert!(
            the_picture_file_is_not_over(&decoded.pictures[frame]),
            "frame {frame} is only the video"
        );
    }
    for frame in [20, 30, 39] {
        assert!(
            the_picture_file_is_over(&decoded.pictures[frame]),
            "the picture is over frame {frame}"
        );
    }
    // The video itself is as it would be without the picture: the frame number of every picture it is
    // seen alone in (the picture covers the strip with the number while it is there).
    let numbers = decoded.frame_numbers();
    assert_eq!(numbers[..20], (0..20).collect::<Vec<u64>>()[..]);
    assert_eq!(numbers[40..], (40..60).collect::<Vec<u64>>()[..]);
}

#[test]
fn a_picture_from_a_file_that_is_moved_and_turned_a_quarter_is_scaled_to_its_place_and_has_its_left_at_the_top() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let (mut project, picture) = project_with_a_picture_over_a_video(&toolchain, scratch.path(), 0, 60);
    project
        .set_clip_placement(
            picture,
            Placement::new(CanvasArea::new(0.25, 0.25, 0.5, 0.5), Angle::from_degrees(90.0)),
        )
        .unwrap();
    let destination = scratch.path().join("turned_picture.mp4");
    export(&toolchain, &project, &destination);

    let decoded = decode_video(&toolchain, &destination, 320, 180);
    let picture = &decoded.pictures[10];
    // Made 160 by 90 (whatever shape the file is) with its middle at (160, 90), and then turned a quarter
    // clockwise: it is 90 across and 160 down, red on its top half and blue on its bottom half.
    assert!(is_red(colour_at(picture, 160, 40)), "the left of it is at the top");
    assert!(
        is_blue(colour_at(picture, 160, 140)),
        "the right of it is at the bottom"
    );
    assert!(
        !is_red(colour_at(picture, 100, 90)) && !is_blue(colour_at(picture, 100, 90)),
        "(100, 90) is not the picture any more"
    );
    assert!(
        !is_red(colour_at(picture, 220, 90)) && !is_blue(colour_at(picture, 220, 90)),
        "and neither is (220, 90)"
    );
}

#[test]
fn the_speed_of_the_track_and_of_the_clip_multiply_for_the_pictures() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut project = project_with_tracks_of_videos(&toolchain, &[&video], &[(0, &[clip(0, 0, 360, 3)])]);
    let track = project.tracks()[0].id();
    project.set_track_speed(track, SpeedFactor::new(2).unwrap()).unwrap();
    // 360 source frames at 3 times 2 is 60 frames, showing source frames 0, 6, 12, ...
    let scratch = scratch_directory();
    let destination = scratch.path().join("product.mp4");
    export(&toolchain, &project, &destination);

    let frame_numbers = decode_video(&toolchain, &destination, 320, 180).frame_numbers();
    assert_eq!(frame_numbers, (0..60).map(|number| number * 6).collect::<Vec<u64>>());
    assert_eq!(frame_numbers, expected_source_frames(&project));
}

// --- sound ---

#[test]
fn a_sound_file_is_mixed_with_the_sound_of_the_video_and_both_are_there_at_their_full_size() {
    let toolchain = toolchain_or_skip!();
    let project = project_with_a_video_and_a_tone_file(&toolchain);
    let scratch = scratch_directory();
    let destination = scratch.path().join("mixed.mp4");
    export(&toolchain, &project, &destination);

    let sound = decoded_sound(&toolchain, &destination, 1, 48_000);
    let window = |from: f64, to: f64| &sound[(from * 48_000.0) as usize..(to * 48_000.0) as usize];
    // Before the tone file starts: only the video's 440 hertz.
    let alone = window(1.0, 4.0);
    let strength_of_video_tone = tone_strength(alone, 440.0, 48_000);
    assert!(
        (strength_of_video_tone - TONE_AMPLITUDE).abs() < 0.1 * TONE_AMPLITUDE,
        "{strength_of_video_tone}"
    );
    assert!(tone_strength(alone, 330.0, 48_000) < 0.02, "and not the file's");
    // Both together in the middle, each as strong as it is on its own.
    let both = window(5.5, 8.5);
    assert!(
        (tone_strength(both, 440.0, 48_000) - strength_of_video_tone).abs() < 0.05 * strength_of_video_tone,
        "the video's tone is as strong as it was alone"
    );
    assert!(tone_strength(both, 330.0, 48_000) > 0.1, "the file's tone is there too");
    // After the file ends: the video's tone again.
    let after = window(9.3, 9.9);
    assert!(tone_strength(after, 330.0, 48_000) < 0.02);
    assert!(tone_strength(after, 440.0, 48_000) > 0.1);
    // The finished video is exactly as long as the timeline.
    let (video_seconds, audio_seconds) = stream_durations(&toolchain, &destination);
    assert!((video_seconds.unwrap() - 10.0).abs() < 0.02, "{video_seconds:?}");
    assert!((audio_seconds.unwrap() - 10.0).abs() < 0.05, "{audio_seconds:?}");
}

#[test]
fn the_volume_of_a_clip_and_of_its_track_both_count_and_a_muted_track_is_silent() {
    let toolchain = toolchain_or_skip!();
    let mut project = project_with_a_video_and_a_tone_file(&toolchain);
    let scratch = scratch_directory();
    let measure = |project: &Project, name: &str| {
        let destination = scratch.path().join(name);
        export(&toolchain, project, &destination);
        let sound = decoded_sound(&toolchain, &destination, 1, 48_000);
        // The video's tone alone, between 1 and 4 seconds.
        tone_strength(&sound[48_000..4 * 48_000], 440.0, 48_000)
    };

    let normal = measure(&project, "normal.mp4");
    assert!(normal > 0.1, "{normal}");

    // The video is on the track at the back. Its clip is 6 decibels quieter: about half.
    let video_track = project.tracks()[1].id();
    let video_clip = fastforwardrs::project::ClipLocation {
        track: video_track,
        clip: project.tracks()[1].entries()[0].id(),
    };
    project.set_clip_gain(video_clip, Gain::from_decibels(-6.0)).unwrap();
    let clip_quieter = measure(&project, "clip_quieter.mp4");
    assert!(
        (clip_quieter / normal - 0.501).abs() < 0.02,
        "{}",
        clip_quieter / normal
    );

    // And its track another 6 decibels quieter: about a quarter.
    project.set_track_gain(video_track, Gain::from_decibels(-6.0)).unwrap();
    let both_quieter = measure(&project, "both_quieter.mp4");
    assert!(
        (both_quieter / normal - 0.251).abs() < 0.02,
        "{}",
        both_quieter / normal
    );

    // Muted, the track is silent (the other track's sound is not, which is shown by its 330 hertz).
    project.set_track_muted(video_track, true).unwrap();
    let muted = measure(&project, "muted.mp4");
    assert!(muted < 0.01, "{muted}");
    let destination = scratch.path().join("muted.mp4");
    let sound = decoded_sound(&toolchain, &destination, 1, 48_000);
    assert!(
        tone_strength(
            &sound[(5.5 * 48_000.0) as usize..(8.5 * 48_000.0) as usize],
            330.0,
            48_000
        ) > 0.1
    );
    assert!(
        loudness(&sound[48_000..4 * 48_000]) < 0.01,
        "silent where only the muted track had sound"
    );
}

#[test]
fn the_sound_of_a_track_that_is_muted_everywhere_leaves_no_sound_at_all_in_the_video() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut project = project_with_tracks_of_videos(&toolchain, &[&video], &[(0, &[clip(0, 0, 60, 1)])]);
    let track = project.tracks()[0].id();
    project.set_track_muted(track, true).unwrap();
    let scratch = scratch_directory();
    let destination = scratch.path().join("silent.mp4");
    export(&toolchain, &project, &destination);
    let (video_seconds, audio_seconds) = stream_durations(&toolchain, &destination);
    assert!((video_seconds.unwrap() - 2.0).abs() < 0.05, "{video_seconds:?}");
    assert_eq!(audio_seconds, None, "no sound stream at all");
}

/// Regression test: a clip that was wholly past the end of its file's sound (as happens when a
/// project is pointed at a shorter file) was padded with `apad`, which makes sound without
/// timestamps when it has nothing to pad, and the finished video's sound ended where that clip
/// began (4.5 seconds into a video of 17.8), with everything after it lost.
#[test]
fn a_clip_wholly_past_the_end_of_a_shorter_file_is_silent_and_the_sound_after_it_is_where_it_belongs() {
    let toolchain = toolchain_or_skip!();
    let short_video = test_video(
        &toolchain,
        &TestVideoSpecification {
            name: "counter_5_seconds",
            duration_in_seconds: 5,
            ..TestVideoSpecification::COUNTER
        },
    );
    // The file has 150 frames. The first clip is frames 0..100 of it (3.33 seconds), the second is
    // frames 200..300, which are all past its end, and the third is frames 50..100 again.
    let project = project_with_tracks_of_videos(
        &toolchain,
        &[&short_video],
        &[(0, &[clip(0, 0, 100, 1), clip(0, 200, 300, 1), clip(0, 50, 100, 1)])],
    );
    assert_eq!(project.total_frame_count(), 250);
    let scratch = scratch_directory();
    let destination = scratch.path().join("past_the_end.mp4");
    export(&toolchain, &project, &destination);

    let (video_seconds, audio_seconds) = stream_durations(&toolchain, &destination);
    let (video_seconds, audio_seconds) = (video_seconds.unwrap(), audio_seconds.unwrap());
    assert!((video_seconds - 250.0 / 30.0).abs() < 0.02, "{video_seconds}");
    assert!(
        (audio_seconds - video_seconds).abs() < 0.05,
        "the sound is as long as the picture: {audio_seconds} seconds, not {video_seconds}"
    );
    let sound = decoded_sound(&toolchain, &destination, 1, 48_000);
    let window = |from: f64, to: f64| &sound[(from * 48_000.0) as usize..(to * 48_000.0) as usize];
    assert!(
        tone_strength(window(0.5, 3.0), 440.0, 48_000) > 0.8 * TONE_AMPLITUDE,
        "the first clip is heard"
    );
    assert!(
        loudness(window(3.6, 6.4)) < 0.01,
        "the clip with nothing to play is silent"
    );
    assert!(
        tone_strength(window(7.0, 8.2), 440.0, 48_000) > 0.8 * TONE_AMPLITUDE,
        "the clip after it is heard where it is on the timeline"
    );
}
