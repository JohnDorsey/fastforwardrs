// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Projects for the tests of the plans, made up at random, so that a plan can be checked against
//! a plain answer worked out one frame at a time.

use std::path::PathBuf;

use crate::project::tracks::{
    Angle, CanvasArea, Clip, ClipKind, ImageClip, MediaClip, Placement, TextClip, TextColour, TextObject, Track,
    TrackId,
};
use crate::project::{
    AnimationTarget, EffectKind, FrameRate, Project, PropertyId, PropertyValue, SourceAudioProperties,
    SourceFrameIndex, SourceImageProperties, SourceMedia, SourceMediaIdentifier, SourceMediaLink,
    SourceVideoProperties, SpeedFactor, SpeedIndicatorLayout, TimelineFrameIndex, ValueKind,
};
use crate::test_support::DeterministicRandomNumbers;

pub(super) const LONG_VIDEO: SourceMediaIdentifier = SourceMediaIdentifier::new(1);
/// A video that is short and has no sound.
pub(super) const SHORT_SILENT_VIDEO: SourceMediaIdentifier = SourceMediaIdentifier::new(2);
pub(super) const SOUND_FILE: SourceMediaIdentifier = SourceMediaIdentifier::new(3);
/// A video as long as the long one but standing upright (360 by 640, where the picture is 640 by 360),
/// so that it does not fill the picture. It has no sound.
pub(super) const TALL_VIDEO: SourceMediaIdentifier = SourceMediaIdentifier::new(4);
/// A picture twice as wide as it is tall (400 by 200, where the picture is 640 by 360), which fitted
/// inside the picture is 640 by 320, with 20 pixels above it and below it.
pub(super) const WIDE_PICTURE: SourceMediaIdentifier = SourceMediaIdentifier::new(5);

pub(super) const LONG_VIDEO_FRAME_COUNT: u64 = 300;
pub(super) const SHORT_VIDEO_FRAME_COUNT: u64 = 90;
/// The sound file lasts 8 seconds: 240 frames at 30 frames per second.
pub(super) const SOUND_FILE_FRAME_COUNT: u64 = 240;

pub(super) fn the_sources() -> Vec<SourceMedia> {
    let video = |identifier, total_frame_count, has_audio, (width, height)| {
        SourceMedia::video(
            identifier,
            SourceMediaLink::from_absolute_path(PathBuf::from("/files/video.mp4")),
            SourceVideoProperties {
                frame_rate: FrameRate::from_whole_frames_per_second(30).unwrap(),
                total_frame_count,
                display_width_in_pixels: width,
                display_height_in_pixels: height,
                has_audio,
            },
        )
    };
    vec![
        video(LONG_VIDEO, LONG_VIDEO_FRAME_COUNT, true, (640, 360)),
        video(SHORT_SILENT_VIDEO, SHORT_VIDEO_FRAME_COUNT, false, (640, 360)),
        video(TALL_VIDEO, LONG_VIDEO_FRAME_COUNT, false, (360, 640)),
        SourceMedia::audio(
            SOUND_FILE,
            SourceMediaLink::from_absolute_path(PathBuf::from("/files/sound.mp3")),
            SourceAudioProperties {
                duration_in_microseconds: 8_000_000,
            },
        ),
        SourceMedia::image(
            WIDE_PICTURE,
            SourceMediaLink::from_absolute_path(PathBuf::from("/files/logo.png")),
            SourceImageProperties {
                display_width_in_pixels: 400,
                display_height_in_pixels: 200,
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
    random_project_of(random, false)
}

/// Like [`random_project`], and, as `with_places`, some of its videos are of another shape than
/// the picture or are put somewhere on it (some over the whole of it) and some of its text is
/// turned.
pub(super) fn random_project_with_places(random: &mut DeterministicRandomNumbers) -> Project {
    random_project_of(random, true)
}

/// Like [`random_project`], and some of the clips that have sound in it have keyframes for how loud they are and whether
/// they are silent, and so do some of the tracks.
pub(super) fn random_project_with_sound_keyframes(random: &mut DeterministicRandomNumbers) -> Project {
    let mut project = random_project(random);
    let clips: Vec<_> = project
        .tracks()
        .iter()
        .flat_map(|track| {
            track
                .placements()
                .filter(|placement| placement.clip.as_media().is_some())
                .map(|placement| {
                    (
                        placement.id,
                        placement.first_timeline_frame.value(),
                        placement.timeline_frame_count(),
                    )
                })
        })
        .collect();
    let make_keyframes = |random: &mut DeterministicRandomNumbers,
                          project: &mut Project,
                          target: AnimationTarget,
                          first_frame: u64,
                          length: u64| {
        for _ in 0..2 + random.next_below(3) {
            let at = TimelineFrameIndex::new(first_frame + random.next_below(length));
            let (property, value) = if random.next_below(5) < 3 {
                (
                    PropertyId::Gain,
                    PropertyValue::Number(random.next_below(41) as f32 - 20.0),
                )
            } else {
                (PropertyId::Muted, PropertyValue::Flag(random.next_below(3) == 0))
            };
            project
                .add_keyframe(target, property, at)
                .expect("the frame is in what it is for");
            project
                .set_property_at(target, property, value, at)
                .expect("a keyframe was just made there");
        }
    };
    for (clip, first_frame, length) in clips {
        if random.next_below(2) == 0 {
            make_keyframes(random, &mut project, AnimationTarget::Clip(clip), first_frame, length);
        }
    }
    let tracks: Vec<_> = project.tracks().iter().map(|track| track.id()).collect();
    let total_frames = project.total_frame_count();
    for track in tracks {
        if total_frames > 0 && random.next_below(3) == 0 {
            make_keyframes(random, &mut project, AnimationTarget::Track(track), 0, total_frames);
        }
    }
    project
}

/// Like [`random_project_with_places`], and some of the videos, pictures and text objects in it have keyframes for where
/// they are, and some text objects for how they look.
pub(super) fn random_project_with_places_and_keyframes(random: &mut DeterministicRandomNumbers) -> Project {
    let mut project = random_project_with_places(random);
    let clips: Vec<_> = project
        .tracks()
        .iter()
        .flat_map(|track| {
            track.placements().map(|placement| {
                (
                    placement.id,
                    placement.clip.kind(),
                    placement.first_timeline_frame.value(),
                    placement.timeline_frame_count(),
                )
            })
        })
        .collect();
    let unit = |random: &mut DeterministicRandomNumbers| random.next_below(1001) as f32 / 1000.0;
    for (clip, kind, first_frame, length) in clips {
        if kind == ClipKind::Audio || random.next_below(2) == 0 {
            continue;
        }
        let target = AnimationTarget::Clip(clip);
        for _ in 0..1 + random.next_below(3) {
            let at = TimelineFrameIndex::new(first_frame + random.next_below(length));
            let (property, value) = match random.next_below(4) {
                0 => (PropertyId::Position, PropertyValue::Pair(unit(random), unit(random))),
                1 => (
                    PropertyId::Size,
                    PropertyValue::Pair(0.1 + unit(random), 0.1 + unit(random)),
                ),
                2 => (
                    PropertyId::Rotation,
                    PropertyValue::Number(unit(random) * 360.0 - 180.0),
                ),
                _ => (
                    PropertyId::Colour,
                    PropertyValue::Colour(TextColour::new(
                        random.next_below(256) as u8,
                        random.next_below(256) as u8,
                        0,
                        255,
                    )),
                ),
            };
            // (A colour is not a property of a video: that one is not made.)
            if project.add_keyframe(target, property, at).is_ok() {
                project
                    .set_property_at(target, property, value, at)
                    .expect("a keyframe was just made there");
            }
        }
    }
    project
}

/// A value of the kind `kind` chosen at random: a turn of the hue of up to a turn and so on either way, for the numbers.
fn random_value_of_the_kind(random: &mut DeterministicRandomNumbers, kind: ValueKind) -> PropertyValue {
    let unit = |random: &mut DeterministicRandomNumbers| random.next_below(1001) as f32 / 1000.0;
    match kind {
        ValueKind::Number => PropertyValue::Number(random.next_below(721) as f32 - 360.0),
        ValueKind::Pair => PropertyValue::Pair(unit(random), unit(random)),
        ValueKind::Colour => PropertyValue::Colour(TextColour::new(
            random.next_below(256) as u8,
            random.next_below(256) as u8,
            random.next_below(256) as u8,
            255,
        )),
        ValueKind::Flag => PropertyValue::Flag(random.next_below(2) == 0),
        ValueKind::Words => PropertyValue::Words(format!("words {}", random.next_below(1000))),
    }
}

/// Like [`random_project_with_places_and_keyframes`], and some of the videos, pictures and text objects in it, and some of the
/// tracks, have some of the effects there are, with values chosen at random (what they begin with for some), some of them with
/// keyframes. (Nothing here names an effect: each of the table is tried.)
pub(crate) fn random_project_with_effects(random: &mut DeterministicRandomNumbers) -> Project {
    let mut project = random_project_with_places_and_keyframes(random);
    let total_frames = project.total_frame_count();
    let mut targets: Vec<(AnimationTarget, u64, u64)> = Vec::new();
    for track in project.tracks() {
        targets.push((AnimationTarget::Track(track.id()), 0, total_frames));
        for placement in track.placements() {
            targets.push((
                AnimationTarget::Clip(placement.id),
                placement.first_timeline_frame.value(),
                placement.timeline_frame_count(),
            ));
        }
    }
    for (target, first_frame, length) in targets {
        for kind in EffectKind::BUILT_IN {
            // (Several of one kind may be put on one target: one more after each, now and then, with values of its own.)
            let mut put_another_on = true;
            while put_another_on {
                // (A sound has none of them, and neither has a track that has nothing on it to put keyframes in.)
                if length == 0 || random.next_below(2) == 0 {
                    break;
                }
                let Ok(instance) = project.add_effect(target, kind) else {
                    break;
                };
                put_another_on = random.next_below(4) == 0;
                let values_begin_as_they_are = random.next_below(4) == 0;
                let properties: Vec<PropertyId> = project
                    .effects_of(target)
                    .and_then(|effects| effects.instance(instance))
                    .map(|effect| effect.properties().collect())
                    .expect("the effect was just put on");
                for property in properties {
                    if !values_begin_as_they_are {
                        let value = random_value_of_the_kind(random, property.kind());
                        project
                            .set_property_at(target, property, value, TimelineFrameIndex::new(first_frame))
                            .expect("the effect was just put on");
                    }
                    if random.next_below(2) == 0 {
                        continue;
                    }
                    for _ in 0..1 + random.next_below(3) {
                        let at = TimelineFrameIndex::new(first_frame + random.next_below(length));
                        let value = random_value_of_the_kind(random, property.kind());
                        project
                            .add_keyframe(target, property, at)
                            .expect("the frame is in what it is for");
                        project
                            .set_property_at(target, property, value, at)
                            .expect("a keyframe was just made there");
                    }
                }
            }
        }
    }
    project
}

fn random_project_of(random: &mut DeterministicRandomNumbers, with_places: bool) -> Project {
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
            let clip = if with_places {
                placed_at_random(random, clip)
            } else {
                clip
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

/// The clip, with a place on the picture chosen at random: a video may be another one that is
/// upright, and may be fitted (no place of its own), over the whole picture (or just not quite
/// because it is turned a little), or anywhere; text may be turned.
fn placed_at_random(random: &mut DeterministicRandomNumbers, clip: Clip) -> Clip {
    let unit = |random: &mut DeterministicRandomNumbers| random.next_below(1001) as f32 / 1000.0;
    let random_place = |random: &mut DeterministicRandomNumbers| {
        Placement::new(
            CanvasArea::new(unit(random), unit(random), 0.1 + unit(random), 0.1 + unit(random)),
            Angle::from_degrees(unit(random) * 360.0 - 180.0),
        )
    };
    match clip {
        Clip::Video(media) => {
            let media = if media.source() == LONG_VIDEO && random.next_below(4) == 0 {
                media_clip(
                    TALL_VIDEO,
                    media.first_source_frame().value(),
                    media.end_source_frame().value(),
                    media.speed().value(),
                )
            } else {
                media
            };
            let place = match random.next_below(6) {
                0..=1 => None,
                2 => Some(Placement::upright(CanvasArea::new(0.0, 0.0, 1.0, 1.0))),
                3 => Some(Placement::new(
                    CanvasArea::new(0.0, 0.0, 1.0, 1.0),
                    Angle::from_degrees(5.0),
                )),
                _ => Some(random_place(random)),
            };
            Clip::Video(media.with_placement(place))
        }
        Clip::Text(text) if random.next_below(3) == 0 => {
            // A picture instead, shown for as long as the text was, somewhere or fitted.
            let picture = ImageClip::new(WIDE_PICTURE, text.duration_in_frames()).expect("it lasted before");
            Clip::Image(picture.with_placement((random.next_below(2) == 0).then(|| random_place(random))))
        }
        Clip::Text(text) => {
            let object = if random.next_below(2) == 0 {
                text.object.with_placement(random_place(random))
            } else {
                text.object.clone()
            };
            Clip::Text(TextClip::new(object, text.duration_in_frames()).expect("it lasted before"))
        }
        other => other,
    }
}

fn text_object(random: &mut DeterministicRandomNumbers) -> TextObject {
    TextObject {
        text: format!("text {}", random.next_below(1000)),
        ..TextObject::default()
    }
}
