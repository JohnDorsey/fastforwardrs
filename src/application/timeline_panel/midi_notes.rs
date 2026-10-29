// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The notes of a MIDI clip drawn inside it on the timeline, as a small read-only piano roll (phase 13, piece 13.6; docs/engine-phase-13-midi-design.md, question 4): a bar for each note, as long as the
//! note is, higher on the clip for a higher key. Nothing can be edited here (the notes come from the file); it is what a waveform is to a sound clip.
//!
//! Which bars there are is worked out by [`note_bars`], which knows nothing of the screen but the rectangles it is given, so that it is tested by numbers.

use egui::{Mesh, Painter, Rect, Shape, pos2};

use super::clips::ClipRectangles;
use crate::application::theme;
use crate::midi_file::MidiNote;
use crate::project::FrameRate;

/// How far the notes keep from the top and the bottom of the clip, in points.
const MARGIN: f32 = 4.0;

/// The fewest keys that the height of the clip is shared among (an octave): a track of one key, or of two near each other, is not drawn as a bar as tall as the clip, but where it is.
const FEWEST_KEYS_SHOWN: u32 = 12;

/// The most a bar is tall, in points.
const MOST_BAR_HEIGHT: f32 = 8.0;

/// The least a bar is tall, and wide, in points: a note that is short or a key that is one of many is still seen.
const LEAST_SIZE: f32 = 1.5;

/// How long before the visible part of a clip a note may begin and still be looked for: a note that is longer than this and begins before the part that is in view is not drawn in it (the notes are in
/// the order of their starts, and not of their ends, so something has to say where to begin looking).
const LONGEST_NOTE_LOOKED_FOR_BEFORE_THE_VIEW_US: f64 = 120_000_000.0;

/// The bars to draw for the notes `notes` (which have keys from `key_range.0` to `key_range.1`: the height of the clip is shared among those, as the track says them) of a clip that plays the frames `first_source_frame` up to `end_source_frame` of its file at `frame_rate`, which is `whole` on the screen, as far as
/// `visible` of it is in view. **A note that began before the clip is not drawn (it is not played), one that goes on past the end is cut at it, and bars that would be on top of each other
/// (the same key in the same column of pixels) are one.**
pub fn note_bars(
    notes: &[MidiNote],
    key_range: (u8, u8),
    frame_rate: FrameRate,
    first_source_frame: u64,
    end_source_frame: u64,
    whole: Rect,
    visible: Rect,
) -> Vec<Rect> {
    if end_source_frame <= first_source_frame || whole.width() <= 0.0 || visible.width() <= 0.0 {
        return Vec::new();
    }
    let frames_a_microsecond = f64::from(frame_rate.numerator()) / (f64::from(frame_rate.denominator()) * 1_000_000.0);
    let points_a_frame = f64::from(whole.width()) / (end_source_frame - first_source_frame) as f64;
    let x_of = |microseconds: u64| -> f64 {
        f64::from(whole.left())
            + (microseconds as f64 * frames_a_microsecond - first_source_frame as f64) * points_a_frame
    };
    let time_at = |x: f32| -> f64 {
        ((f64::from(x) - f64::from(whole.left())) / points_a_frame + first_source_frame as f64) / frames_a_microsecond
    };
    let from_time = time_at(visible.left()) - LONGEST_NOTE_LOOKED_FOR_BEFORE_THE_VIEW_US;
    let first = notes.partition_point(|note| (note.start_us as f64) < from_time);
    let clip_start_us = first_source_frame as f64 / frames_a_microsecond;
    let clip_end_us = end_source_frame as f64 / frames_a_microsecond;
    let top = whole.top() + MARGIN;
    let height = (whole.bottom() - MARGIN - top).max(LEAST_SIZE);
    let (lowest, highest) = (key_range.0.min(key_range.1), key_range.0.max(key_range.1));
    let keys_of_the_track = u32::from(highest - lowest) + 1;
    let keys = keys_of_the_track.max(FEWEST_KEYS_SHOWN) as f32;
    // (The keys of the track are in the middle of the keys that are shown.)
    let highest_shown = f32::from(highest) + (keys - keys_of_the_track as f32) / 2.0;
    let bar_height = (height / keys).clamp(LEAST_SIZE, MOST_BAR_HEIGHT).min(height);
    let mut bars: Vec<Rect> = Vec::new();
    let mut last_drawn: Option<(i64, u8)> = None;
    for note in &notes[first..] {
        let start = note.start_us as f64;
        if start >= clip_end_us || x_of(note.start_us) > f64::from(visible.right()) {
            // (They are in the order of their starts: nothing after this one is in view.)
            break;
        }
        if start < clip_start_us {
            continue;
        }
        let left = x_of(note.start_us) as f32;
        let right = (x_of(note.end_us) as f32).min(whole.right());
        if right < visible.left() {
            continue;
        }
        let key = note.key.clamp(lowest, highest);
        let column = left.floor() as i64;
        if right - left < 1.0 && last_drawn == Some((column, key)) {
            continue;
        }
        last_drawn = Some((column, key));
        let place = (highest_shown - f32::from(key)) / (keys - 1.0);
        let y = top + place * (height - bar_height);
        bars.push(Rect::from_min_max(
            pos2(left, y),
            pos2((left + LEAST_SIZE).max(right), y + bar_height),
        ));
    }
    bars
}

/// Draws the notes `notes` of the MIDI clip that is `rectangles` on the screen (see [`note_bars`]), in the colour of the notes.
pub(super) fn draw_midi_notes(
    painter: &Painter,
    rectangles: ClipRectangles,
    notes: &[MidiNote],
    key_range: (u8, u8),
    frame_rate: FrameRate,
    first_source_frame: u64,
    end_source_frame: u64,
) {
    let bars = note_bars(
        notes,
        key_range,
        frame_rate,
        first_source_frame,
        end_source_frame,
        rectangles.whole,
        rectangles.visible,
    );
    if bars.is_empty() {
        return;
    }
    let mut mesh = Mesh::default();
    for bar in bars {
        mesh.add_colored_rect(bar, theme::MIDI_CLIP_NOTE);
    }
    painter.with_clip_rect(rectangles.visible).add(Shape::mesh(mesh));
}

#[cfg(test)]
mod tests;
