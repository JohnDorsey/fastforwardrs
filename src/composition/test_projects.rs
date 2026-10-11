//! Projects for the tests of the plans, made up at random, so that a plan can be checked against
//! a plain answer worked out one frame at a time.

use std::path::PathBuf;

use crate::project::tracks::{Clip, MediaClip, TextClip, TextObject, Track, TrackId};
use crate::project::{
    FrameRate, Project, SourceAudioProperties, SourceFrameIndex, SourceMedia, SourceMediaIdentifier, SourceMediaLink,
    SourceVideoProperties, SpeedFactor, SpeedIndicatorLayout,
};
use crate::test_support::DeterministicRandomNumbers;

pub(super) const LONG_VIDEO: SourceMediaIdentifier = SourceMediaIdentifier::new(1);
/// A video that is short and has no sound.
pub(super) const SHORT_SILENT_VIDEO: SourceMediaIdentifier = SourceMediaIdentifier::new(2);
pub(super) const SOUND_FILE: SourceMediaIdentifier = SourceMediaIdentifier::new(3);

pub(super) const LONG_VIDEO_FRAME_COUNT: u64 = 300;
pub(super) const SHORT_VIDEO_FRAME_COUNT: u64 = 90;
/// The sound file lasts 8 seconds: 240 frames at 30 frames per second.
pub(super) const SOUND_FILE_FRAME_COUNT: u64 = 240;

pub(super) fn the_sources() -> Vec<SourceMedia> {
    let video = |identifier, total_frame_count, has_audio| {
        SourceMedia::video(
            identifier,
            SourceMediaLink::from_absolute_path(PathBuf::from("/files/video.mp4")),
            SourceVideoProperties {
                frame_rate: FrameRate::from_whole_frames_per_second(30).unwrap(),
                total_frame_count,
                display_width_in_pixels: 640,
                display_height_in_pixels: 360,
                has_audio,
            },
        )
    };
    vec![
        video(LONG_VIDEO, LONG_VIDEO_FRAME_COUNT, true),
        video(SHORT_SILENT_VIDEO, SHORT_VIDEO_FRAME_COUNT, false),
        SourceMedia::audio(
            SOUND_FILE,
            SourceMediaLink::from_absolute_path(PathBuf::from("/files/sound.mp3")),
            SourceAudioProperties {
                duration_in_microseconds: 8_000_000,
            },
        ),
    ]
}

pub(super) fn media_clip(source: SourceMediaIdentifier, first: u64, end: u64, speed: u32) -> MediaClip {
    MediaClip::new(
        source,
        SourceFrameIndex::new(first),
        SourceFrameIndex::new(end),
        SpeedFactor::new(speed).unwrap(),
    )
    .unwrap()
}

/// A project of `tracks` (the one in front first), each a list of clips with the empty frames
/// before each.
pub(super) fn project_of(tracks: Vec<Vec<(u64, Clip)>>) -> Project {
    let tracks = tracks
        .into_iter()
        .enumerate()
        .map(|(number, clips)| Track::new(TrackId::new(number as u32), format!("track {number}")).with_clips(clips))
        .collect();
    Project::from_parts(the_sources(), tracks, SpeedIndicatorLayout::DEFAULT).unwrap()
}

/// A project with a few tracks of videos, sounds and text, laid out at random, where some clips
/// reach past the end of their file.
pub(super) fn random_project(random: &mut DeterministicRandomNumbers) -> Project {
    let track_count = 1 + random.next_below(4);
    let mut tracks = Vec::new();
    for number in 0..track_count {
        let mut clips = Vec::new();
        for _ in 0..random.next_below(6) {
            let gap = if random.next_below(3) == 0 {
                random.next_below(40)
            } else {
                0
            };
            let clip_speed = [1u32, 1, 2, 3, 5][random.next_below(5) as usize];
            let clip = match random.next_below(10) {
                0..=4 => {
                    let source = [LONG_VIDEO, SHORT_SILENT_VIDEO][random.next_below(2) as usize];
                    let file_length = if source == LONG_VIDEO {
                        LONG_VIDEO_FRAME_COUNT
                    } else {
                        SHORT_VIDEO_FRAME_COUNT
                    };
                    let first = random.next_below(file_length);
                    // Up to half as long again as the file, so that some clips run out of pictures.
                    let end = first + 1 + random.next_below(file_length * 3 / 2);
                    Clip::Video(media_clip(source, first, end, clip_speed))
                }
                5..=7 => {
                    let first = random.next_below(SOUND_FILE_FRAME_COUNT);
                    let end = first + 1 + random.next_below(SOUND_FILE_FRAME_COUNT * 3 / 2);
                    Clip::Audio(media_clip(SOUND_FILE, first, end, clip_speed))
                }
                _ => Clip::Text(TextClip::new(text_object(random), 1 + random.next_below(80)).unwrap()),
            };
            clips.push((gap, clip));
        }
        let speed = [1u32, 1, 2][random.next_below(3) as usize];
        let mut track = Track::new(TrackId::new(number as u32), format!("track {number}"))
            .with_speed(SpeedFactor::new(speed).unwrap())
            .with_clips(clips);
        if random.next_below(6) == 0 {
            track = track.with_muted(true);
        }
        tracks.push(track);
    }
    Project::from_parts(the_sources(), tracks, SpeedIndicatorLayout::DEFAULT).unwrap()
}

fn text_object(random: &mut DeterministicRandomNumbers) -> TextObject {
    TextObject {
        text: format!("text {}", random.next_below(1000)),
        ..TextObject::default()
    }
}
