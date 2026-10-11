//! The bars of the waveform of a sound clip.

use super::TimelineGeometry;
use crate::media::Waveform;
use crate::project::ClipPlacement;

/// One thin upright bar of the waveform of a sound clip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaveformColumn {
    /// Where the bar starts along the strip.
    pub left: f64,
    pub width: f64,
    /// How loud the sound is in the bar: 0 for silence (or where the file has no sound), 1 for the loudest it can be.
    pub loudness: f32,
}

impl TimelineGeometry {
    /// The bars of the waveform of the sound clip `placement`, the ones that are in the view
    /// `view_width` wide that starts `scroll_offset` along the strip, each `column_width` points
    /// wide, in order from left to right. A bar is as loud as the loudest sound in the stretch of
    /// the file that it stands for, which is longer for a clip that plays faster.
    pub fn waveform_columns(
        &self,
        placement: &ClipPlacement<'_>,
        waveform: &Waveform,
        scroll_offset: f64,
        view_width: f64,
        column_width: f64,
    ) -> Vec<WaveformColumn> {
        let Some(media) = placement.clip.as_media() else {
            return Vec::new();
        };
        if column_width <= 0.0 || column_width.is_nan() {
            return Vec::new();
        }
        let clip_left = self.x_of_frame(placement.first_timeline_frame.value());
        let clip_right = self.x_of_frame(placement.end_timeline_frame.value());
        let visible_left = clip_left.max(scroll_offset);
        let visible_right = clip_right.min(scroll_offset + view_width);
        if visible_right <= visible_left {
            return Vec::new();
        }
        let speed = f64::from(placement.effective_speed.value());
        let seconds_into_the_file_at_the_clip_start =
            self.frame_rate.seconds_at_frame(media.first_source_frame().value());
        let seconds_into_the_file_at =
            |x: f64| seconds_into_the_file_at_the_clip_start + (x - clip_left) / self.points_per_second * speed;

        // Bars are laid from the clip's left edge, so that they do not shimmer while the view scrolls.
        let first_column = ((visible_left - clip_left) / column_width).floor() as u64;
        let end_column = ((visible_right - clip_left) / column_width).ceil() as u64;
        (first_column..end_column)
            .map(|column| {
                let left = clip_left + column as f64 * column_width;
                let right = (left + column_width).min(clip_right);
                let loudness = waveform
                    .loudest_between(seconds_into_the_file_at(left), seconds_into_the_file_at(right))
                    .map_or(0.0, |peak| f32::from(peak) / 255.0);
                WaveformColumn {
                    left,
                    width: right - left,
                    loudness,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{assert_close, geometry_at, media};
    use super::*;
    use crate::project::{Clip, SpeedFactor, TextClip, TextObject, Track, TrackId};

    // --- waveform columns ---

    /// A waveform in which every peak (a hundredth of a second) is as loud as its number, so that the
    /// answer says which part of the file a column was made from.
    fn ramp_waveform() -> Waveform {
        Waveform::from_peaks((0..=255).collect::<Vec<u8>>().repeat(10))
    }

    fn sound_placement_track(first: u64, end: u64, speed: u32) -> Track {
        Track::new(TrackId::new(0), "track 0").with_clips([(0, Clip::Audio(media(first, end, speed)))])
    }

    #[test]
    fn the_columns_of_a_sound_clip_fill_it_exactly_from_its_left_edge() {
        let geometry = geometry_at(30, 1, 100.0);
        let track = sound_placement_track(0, 90, 1); // 3 seconds: 300 points
        let placement = track.placement_at_index(0).unwrap();
        let columns = geometry.waveform_columns(&placement, &ramp_waveform(), 0.0, 10_000.0, 2.0);
        assert_eq!(columns.len(), 150);
        assert_close(columns[0].left, 0.0);
        let mut expected_left = 0.0;
        for column in &columns {
            assert_close(column.left, expected_left);
            expected_left += column.width;
        }
        assert_close(expected_left, 300.0);
    }

    #[test]
    fn a_last_column_that_does_not_fit_is_cut_short_at_the_end_of_the_clip() {
        let geometry = geometry_at(30, 1, 100.0);
        let track = sound_placement_track(0, 90, 1);
        let placement = track.placement_at_index(0).unwrap();
        let columns = geometry.waveform_columns(&placement, &ramp_waveform(), 0.0, 10_000.0, 7.0);
        let last = columns.last().unwrap();
        assert_close(last.left + last.width, 300.0);
        assert!(last.width < 7.0);
    }

    #[test]
    fn each_column_is_as_loud_as_the_part_of_the_file_it_stands_for() {
        let geometry = geometry_at(30, 1, 100.0);
        // The clip is the file's second second (source frames 30 to 90): columns start at 1.0 s into the file.
        let track = sound_placement_track(30, 90, 1);
        let placement = track.placement_at_index(0).unwrap();
        let columns = geometry.waveform_columns(&placement, &ramp_waveform(), 0.0, 10_000.0, 10.0);
        // 10 points is a tenth of a second: 10 peaks. The first column covers peaks 100..110, so its loudest is 109.
        assert_eq!(columns[0].loudness, 109.0 / 255.0);
        assert_eq!(columns[1].loudness, 119.0 / 255.0);
    }

    #[test]
    fn a_sound_clip_that_plays_faster_stands_for_more_of_the_file_in_every_column() {
        let geometry = geometry_at(30, 1, 100.0);
        // 300 source frames at 5x: 60 timeline frames, 200 points. Ten points are a tenth of a second
        // on the timeline, which is half a second of the file, 50 peaks: 0..50, so the loudest is 49.
        let track = sound_placement_track(0, 300, 5);
        let placement = track.placement_at_index(0).unwrap();
        let columns = geometry.waveform_columns(&placement, &ramp_waveform(), 0.0, 10_000.0, 10.0);
        assert_eq!(columns.len(), 20);
        assert_eq!(columns[0].loudness, 49.0 / 255.0);
        assert_eq!(columns[1].loudness, 99.0 / 255.0);
    }

    #[test]
    fn a_track_that_plays_faster_has_the_same_effect_on_the_columns_as_a_clip_that_does() {
        let geometry = geometry_at(30, 1, 100.0);
        let faster_clip = sound_placement_track(0, 300, 5);
        let faster_track = sound_placement_track(0, 300, 1).with_speed(SpeedFactor::new(5).unwrap());
        let columns_of = |track: &Track| {
            let placement = track.placement_at_index(0).unwrap();
            geometry.waveform_columns(&placement, &ramp_waveform(), 0.0, 10_000.0, 10.0)
        };
        assert_eq!(columns_of(&faster_clip), columns_of(&faster_track));
    }

    #[test]
    fn only_the_columns_inside_the_view_are_made_and_they_do_not_move_when_the_view_does() {
        let geometry = geometry_at(30, 1, 100.0);
        let track = sound_placement_track(0, 900, 1); // 30 seconds: 3000 points
        let placement = track.placement_at_index(0).unwrap();
        let waveform = ramp_waveform();
        let whole = geometry.waveform_columns(&placement, &waveform, 0.0, 10_000.0, 4.0);
        let in_view = geometry.waveform_columns(&placement, &waveform, 1001.0, 200.0, 4.0);
        assert!(!in_view.is_empty() && in_view.len() < 60, "{}", in_view.len());
        assert!(in_view.first().unwrap().left <= 1001.0);
        assert!(in_view.last().unwrap().left + in_view.last().unwrap().width >= 1201.0);
        for column in &in_view {
            assert!(
                whole.contains(column),
                "the same column as when the whole clip is made: {column:?}"
            );
        }
    }

    #[test]
    fn a_view_beside_the_clip_has_no_columns() {
        let geometry = geometry_at(30, 1, 100.0);
        let track = sound_placement_track(0, 90, 1);
        let placement = track.placement_at_index(0).unwrap();
        let waveform = ramp_waveform();
        assert!(
            geometry
                .waveform_columns(&placement, &waveform, 400.0, 200.0, 4.0)
                .is_empty()
        );
        assert!(
            geometry
                .waveform_columns(&placement, &waveform, 0.0, 0.0, 4.0)
                .is_empty()
        );
        assert!(
            geometry
                .waveform_columns(&placement, &waveform, 0.0, 100.0, 0.0)
                .is_empty()
        );
        assert!(
            geometry
                .waveform_columns(&placement, &waveform, 0.0, 100.0, f64::NAN)
                .is_empty()
        );
    }

    #[test]
    fn where_the_file_has_no_sound_yet_the_columns_are_silent() {
        let geometry = geometry_at(30, 1, 100.0);
        let track = sound_placement_track(0, 90, 1);
        let placement = track.placement_at_index(0).unwrap();
        let columns = geometry.waveform_columns(&placement, &Waveform::default(), 0.0, 10_000.0, 10.0);
        assert_eq!(columns.len(), 30);
        assert!(columns.iter().all(|column| column.loudness == 0.0));
    }

    #[test]
    fn a_text_clip_has_no_waveform() {
        let geometry = geometry_at(30, 1, 100.0);
        let track = Track::new(TrackId::new(0), "track 0")
            .with_clips([(0, Clip::Text(TextClip::new(TextObject::default(), 60).unwrap()))]);
        let placement = track.placement_at_index(0).unwrap();
        assert!(
            geometry
                .waveform_columns(&placement, &ramp_waveform(), 0.0, 10_000.0, 4.0)
                .is_empty()
        );
    }
}
