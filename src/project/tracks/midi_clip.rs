// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A MIDI clip (phase 13, piece 13.3): a stretch of one track of a Standard MIDI File. **It makes no sound of its own**: its notes travel the road that sound travels (the effects of the
//! clip, of its track, of the groups it is in), and the first synthesizer on that road plays them (see `docs/engine-phase-13-midi-design.md`).
//!
//! The file is a **resource** of the project (see [`Resource`](crate::project::Resource): kept in the project file or linked, read once), and the clip names it and says which of the file's
//! tracks it plays. Like a clip of a sound file it covers a run of **source frames** (a frame of the music is as long as a frame of the project's video; the first is where the file begins),
//! plays at a speed (**which changes when the notes are played and not what they are**: a MIDI clip at twice the speed plays the same notes in half the time), has a volume and a mute switch.

use std::sync::Arc;

use super::gain::Gain;
use super::source_span::{EmptyClip, SourceSpan};
use crate::project::{FrameRate, SourceFrameIndex, SpeedFactor};

/// A run of one track of a MIDI file, played at a single speed.
#[derive(Clone, Debug, PartialEq)]
pub struct MidiClip {
    /// The name of the resource that has the file.
    resource: Arc<str>,
    /// Which track of the file it plays (the place of the track in the file, counted from 0).
    midi_track: u16,
    span: SourceSpan,
    gain: Gain,
    muted: bool,
}

impl MidiClip {
    /// A clip of `first_source_frame` up to `end_source_frame` of the track `midi_track` of the file that the resource `resource` has, at `speed`, at the volume the file has.
    pub fn new(
        resource: impl Into<Arc<str>>,
        midi_track: u16,
        first_source_frame: SourceFrameIndex,
        end_source_frame: SourceFrameIndex,
        speed: SpeedFactor,
    ) -> Result<Self, EmptyClip> {
        Ok(Self {
            resource: resource.into(),
            midi_track,
            span: SourceSpan::new(first_source_frame, end_source_frame, speed)?,
            gain: Gain::UNCHANGED,
            muted: false,
        })
    }

    /// The name of the resource that has the file.
    pub fn resource(&self) -> &str {
        &self.resource
    }

    /// Which track of the file the clip plays.
    pub fn midi_track(&self) -> u16 {
        self.midi_track
    }

    pub fn first_source_frame(&self) -> SourceFrameIndex {
        self.span.first_source_frame()
    }

    /// The frame just after the clip's last frame (the end is exclusive).
    pub fn end_source_frame(&self) -> SourceFrameIndex {
        self.span.end_source_frame()
    }

    /// How much faster than normal the clip itself plays, not counting its track's speed.
    pub fn speed(&self) -> SpeedFactor {
        self.span.speed()
    }

    pub fn gain(&self) -> Gain {
        self.gain
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }

    #[must_use]
    pub fn with_speed(&self, speed: SpeedFactor) -> Self {
        Self {
            span: self.span.with_speed(speed),
            ..self.clone()
        }
    }

    #[must_use]
    pub fn with_gain(&self, gain: Gain) -> Self {
        Self { gain, ..self.clone() }
    }

    #[must_use]
    pub fn with_muted(&self, muted: bool) -> Self {
        Self { muted, ..self.clone() }
    }

    /// The same clip, playing the file of the resource `resource` (what is done when a project is made to use another name for a file).
    #[must_use]
    pub fn with_resource(&self, resource: impl Into<Arc<str>>) -> Self {
        Self {
            resource: resource.into(),
            ..self.clone()
        }
    }

    /// The clip as it is when its project runs at the frame rate `to` and not `from`: it covers the same stretch of the music, with its first and its end frame counted again at the
    /// new rate (and at least one frame of it). Pieces cut from one file that touch each other still do.
    #[must_use]
    pub fn retimed(&self, from: FrameRate, to: FrameRate) -> Self {
        Self {
            span: self.span.retimed(from, to),
            ..self.clone()
        }
    }

    /// How many source frames the clip covers.
    pub fn source_frame_count(&self) -> u64 {
        self.span.source_frame_count()
    }

    /// How many frames the clip occupies on the timeline when it plays at `effective_speed`.
    pub fn timeline_frame_count(&self, effective_speed: SpeedFactor) -> u64 {
        self.span.timeline_frame_count(effective_speed)
    }

    /// The source frame at the clip's frame number `offset_within_clip` (0 is the clip's first frame), or `None` if the clip is shorter than that.
    pub fn source_frame_at_timeline_offset(
        &self,
        effective_speed: SpeedFactor,
        offset_within_clip: u64,
    ) -> Option<SourceFrameIndex> {
        self.span
            .source_frame_at_timeline_offset(effective_speed, offset_within_clip)
    }

    /// Cuts the clip in two just before `split_source_frame`; both halves keep everything else about the clip (the same file and track). `None` if the split point is not strictly
    /// inside the clip.
    pub fn split_before_source_frame(&self, split_source_frame: SourceFrameIndex) -> Option<(Self, Self)> {
        let (first_span, second_span) = self.span.split_before_source_frame(split_source_frame)?;
        Some((
            Self {
                span: first_span,
                ..self.clone()
            },
            Self {
                span: second_span,
                ..self.clone()
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speed(factor: u32) -> SpeedFactor {
        SpeedFactor::new(factor).unwrap()
    }

    fn clip(first: u64, end: u64, factor: u32) -> MidiClip {
        MidiClip::new(
            "tune.mid",
            2,
            SourceFrameIndex::new(first),
            SourceFrameIndex::new(end),
            speed(factor),
        )
        .unwrap()
    }

    #[test]
    fn a_midi_clip_names_its_file_and_its_track_and_covers_at_least_a_frame() {
        let clip = clip(10, 40, 1);

        assert_eq!((clip.resource(), clip.midi_track()), ("tune.mid", 2));
        assert_eq!(clip.source_frame_count(), 30);
        assert!(MidiClip::new("a.mid", 0, SourceFrameIndex::new(5), SourceFrameIndex::new(5), speed(1)).is_err());
        assert!(MidiClip::new("a.mid", 0, SourceFrameIndex::new(6), SourceFrameIndex::new(5), speed(1)).is_err());
    }

    #[test]
    fn a_new_midi_clip_is_at_the_volume_of_the_file_and_not_muted() {
        let clip = clip(0, 10, 1);

        assert_eq!(clip.gain(), Gain::UNCHANGED);
        assert!(!clip.is_muted());
    }

    #[test]
    fn a_midi_clip_lasts_as_long_as_a_sound_clip_of_the_same_run_does_at_each_speed() {
        // (The arithmetic is that of `SourceSpan`, which a clip of a video and of a sound file use: every Nth frame, a partial group counted.)
        assert_eq!(clip(0, 300, 1).timeline_frame_count(speed(4)), 75);
        assert_eq!(clip(0, 10, 1).timeline_frame_count(speed(3)), 4);
        assert_eq!(clip(0, 3, 1).timeline_frame_count(speed(100)), 1);
        assert_eq!(
            clip(100, 110, 1).source_frame_at_timeline_offset(speed(3), 3),
            Some(SourceFrameIndex::new(109))
        );
        assert_eq!(clip(100, 110, 1).source_frame_at_timeline_offset(speed(3), 4), None);
    }

    #[test]
    fn changing_the_speed_the_volume_the_muting_or_the_file_changes_only_that() {
        let original = clip(100, 400, 1);

        assert_eq!(original.with_speed(speed(5)), clip(100, 400, 5));
        assert_eq!(
            original.with_gain(Gain::from_decibels(3.0)).gain(),
            Gain::from_decibels(3.0)
        );
        assert_eq!(original.with_gain(Gain::from_decibels(3.0)).speed(), speed(1));
        assert!(original.with_muted(true).is_muted());
        assert_eq!(original.with_muted(true).with_muted(false), original);
        let other = original.with_resource("other.mid");
        assert_eq!((other.resource(), other.midi_track()), ("other.mid", 2));
        assert_eq!(other.source_frame_count(), 300);
        assert_eq!(original.resource(), "tune.mid", "the original is untouched");
    }

    #[test]
    fn splitting_cuts_the_run_in_two_and_keeps_the_file_the_track_the_speed_the_volume_and_the_muting() {
        let original = clip(100, 400, 4).with_gain(Gain::from_decibels(-6.0)).with_muted(true);

        let (first, second) = original.split_before_source_frame(SourceFrameIndex::new(240)).unwrap();

        assert_eq!(
            first,
            clip(100, 240, 4).with_gain(Gain::from_decibels(-6.0)).with_muted(true)
        );
        assert_eq!(
            second,
            clip(240, 400, 4).with_gain(Gain::from_decibels(-6.0)).with_muted(true)
        );
        for frame in [0, 99, 100, 400, 401] {
            assert_eq!(original.split_before_source_frame(SourceFrameIndex::new(frame)), None);
        }
    }

    #[test]
    fn counted_again_at_another_frame_rate_a_midi_clip_covers_the_same_music() {
        let thirty = FrameRate::from_whole_frames_per_second(30).unwrap();
        let sixty = FrameRate::from_whole_frames_per_second(60).unwrap();

        let retimed = clip(100, 400, 3).with_muted(true).retimed(thirty, sixty);

        assert_eq!(
            (retimed.first_source_frame().value(), retimed.end_source_frame().value()),
            (200, 800)
        );
        assert_eq!(
            (
                retimed.resource(),
                retimed.midi_track(),
                retimed.speed(),
                retimed.is_muted()
            ),
            ("tune.mid", 2, speed(3), true)
        );
        // (A clip of one frame is still one frame at a lower rate.)
        assert_eq!(clip(10, 11, 1).retimed(sixty, thirty).source_frame_count(), 1);
    }
}
