// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The flash of white light where a clip was cut in two: a bar of light across the strip at the cut, which
//! is at its brightest, and has all the light it ever has, at once, and then spreads as it fades away in a moment.
//!
//! What happened (a cut was made) is told to the timeline when it happens ([`CutFlashes::add`]), and the flash
//! begins when the next frame of the window is drawn. Its age is measured on the window's own clock (the
//! seconds of [`egui::InputState::time`]), the same clock the frames are drawn by.
//!
//! The flash is drawn over everything else on the strips, the playhead included: a cut is made at the playhead, so
//! the flash begins right under it, and the playhead's dark outline, which is as wide as the flash is at first,
//! would hide it.

use egui::epaint::Mesh;
use egui::{Color32, Painter, Shape, Stroke, pos2};

use super::strip_view::StripView;
use crate::project::{TimelineFrameIndex, TrackId};

/// How long a flash lasts, in seconds.
pub(super) const FLASH_SECONDS: f64 = 0.3;

/// How far the glow of a flash reaches to either side of the cut, in points, as it would begin and as it ends if it spread freely
/// (see [`glow_reach`]: it does not, at first).
const GLOW_REACH_AT_FIRST: f32 = 6.0;
const GLOW_REACH_AT_LAST: f32 = 28.0;

/// How wide the bright line at the cut is, in points.
const CORE_WIDTH: f32 = 2.0;

/// How bright the middle of the glow is, compared with the line.
const GLOW_BRIGHTNESS: f32 = 0.8;

/// A cut that was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cut {
    track: TrackId,
    frame: TimelineFrameIndex,
}

/// A flash that is showing, and when it began.
#[derive(Clone, Copy, Debug)]
struct ShowingFlash {
    cut: Cut,
    began_at_seconds: f64,
}

/// The flashes of light at the cuts that were just made.
#[derive(Debug, Default)]
pub(super) struct CutFlashes {
    /// Cuts whose flashes have not begun: they begin when the window is next drawn.
    waiting: Vec<Cut>,
    showing: Vec<ShowingFlash>,
}

impl CutFlashes {
    /// Says that the clip on `track` was cut at `frame`.
    pub(super) fn add(&mut self, track: TrackId, frame: TimelineFrameIndex) {
        self.waiting.push(Cut { track, frame });
    }

    /// Begins the flashes that are waiting, as of `now` (seconds on the window's clock), and forgets the ones
    /// that are over. Returns whether any are showing, which is when the window has to be drawn again soon, to
    /// show them moving.
    pub(super) fn update(&mut self, now: f64) -> bool {
        self.showing.extend(self.waiting.drain(..).map(|cut| ShowingFlash {
            cut,
            began_at_seconds: now,
        }));
        self.showing
            .retain(|flash| now - flash.began_at_seconds < FLASH_SECONDS);
        !self.showing.is_empty()
    }

    /// The flashes that are showing on `track` as of `now`: the frame each is at, and how far along it is, from 0
    /// (it has just begun) to just under 1.
    fn on_track(&self, track: TrackId, now: f64) -> impl Iterator<Item = (TimelineFrameIndex, f32)> + '_ {
        self.showing
            .iter()
            .filter(move |flash| flash.cut.track == track)
            .map(move |flash| {
                let progress = ((now - flash.began_at_seconds) / FLASH_SECONDS).clamp(0.0, 1.0);
                (flash.cut.frame, progress as f32)
            })
    }
}

/// How bright a flash is when it is `progress` (0 to 1) of the way through: fully so at the start, and not at all
/// at the end.
fn brightness(progress: f32) -> f32 {
    (1.0 - progress).powi(2)
}

/// How far the glow of a flash would reach to either side of the cut when it is `progress` (0 to 1) of the way through, if it
/// spread freely: it spreads quickly and then slows.
fn glow_reach_when_it_spreads_freely(progress: f32) -> f32 {
    let spread = 1.0 - (1.0 - progress).powi(3);
    GLOW_REACH_AT_FIRST + (GLOW_REACH_AT_LAST - GLOW_REACH_AT_FIRST) * spread
}

/// How far through a flash (0 to 1) the glow that spreads freely has the most light, which is how far it reaches times how bright
/// it is. (Worked out from the two: the light is `reach * (1 - progress)^2`, which is greatest where its slope is nothing.)
fn progress_with_the_most_light() -> f32 {
    let reach_range = GLOW_REACH_AT_LAST - GLOW_REACH_AT_FIRST;
    let how_much_is_left = (2.0 * GLOW_REACH_AT_LAST / (5.0 * reach_range)).cbrt();
    1.0 - how_much_is_left
}

/// How far the glow of a flash reaches to either side of the cut when it is `progress` (0 to 1) of the way through.
///
/// A glow that spreads freely, from a small one at the start, has more and more light for the first few frames (it spreads faster
/// than the flash fades) before it has less and less, which looks like a flash that fades in and then out. A flash is to have the
/// most light it has at once, so until the light of the glow that spreads freely is at its most, the glow reaches just as far as
/// it needs to for the light of it to stay what it will be then (which is a little more every frame, as the brightness falls, and
/// is what the glow that spreads freely is, at that point). From then on it spreads as it always did, and the flash fades out as
/// it always did.
fn glow_reach(progress: f32) -> f32 {
    let most_light_at = progress_with_the_most_light();
    if progress >= most_light_at {
        return glow_reach_when_it_spreads_freely(progress);
    }
    glow_reach_when_it_spreads_freely(most_light_at) * brightness(most_light_at) / brightness(progress)
}

/// White at `brightness` (0 to 1) of full strength, which shows what is under it less as it is more.
fn white_at(brightness: f32) -> Color32 {
    Color32::from_white_alpha((brightness.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// Draws the flashes that are showing, each on the strip of its track (`strips` says where each is, and a flash is
/// cut off at the edges of its strip).
pub(super) fn draw_over<'a>(
    painter: &Painter,
    strips: impl IntoIterator<Item = (TrackId, &'a StripView)>,
    flashes: &CutFlashes,
    now: f64,
) {
    for (track, view) in strips {
        draw_on_a_strip(&painter.with_clip_rect(view.rectangle), view, track, flashes, now);
    }
}

fn draw_on_a_strip(painter: &Painter, view: &StripView, track: TrackId, flashes: &CutFlashes, now: f64) {
    let strip = view.rectangle;
    for (frame, progress) in flashes.on_track(track, now) {
        let x = view.screen_x_of_frame(frame.value());
        let brightness = brightness(progress);
        let reach = glow_reach(progress);
        let (clear, bright) = (Color32::TRANSPARENT, white_at(GLOW_BRIGHTNESS * brightness));

        // The glow: the light is brightest at the cut and falls to nothing at `reach` to either side.
        let mut glow = Mesh::default();
        for y in [strip.top(), strip.bottom()] {
            glow.colored_vertex(pos2(x - reach, y), clear);
            glow.colored_vertex(pos2(x, y), bright);
            glow.colored_vertex(pos2(x + reach, y), clear);
        }
        for (top_left, bottom_left) in [(0, 3), (1, 4)] {
            glow.add_triangle(top_left, top_left + 1, bottom_left + 1);
            glow.add_triangle(top_left, bottom_left + 1, bottom_left);
        }
        painter.add(Shape::mesh(glow));

        painter.line_segment(
            [pos2(x, strip.top()), pos2(x, strip.bottom())],
            Stroke::new(CORE_WIDTH, white_at(brightness)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(number: u32) -> TrackId {
        TrackId::new(number)
    }

    fn frame(number: u64) -> TimelineFrameIndex {
        TimelineFrameIndex::new(number)
    }

    #[test]
    fn a_cut_that_is_told_of_begins_to_flash_when_the_window_is_next_drawn() {
        let mut flashes = CutFlashes::default();
        flashes.add(track(1), frame(40));
        assert_eq!(
            flashes.on_track(track(1), 10.0).count(),
            0,
            "nothing shows until the window is drawn"
        );

        assert!(
            flashes.update(10.0),
            "there is a flash showing, so the window must be drawn again soon"
        );
        let showing: Vec<_> = flashes.on_track(track(1), 10.0).collect();
        assert_eq!(showing, vec![(frame(40), 0.0)], "it has just begun");
    }

    #[test]
    fn a_flash_goes_on_from_begin_to_end_and_is_then_forgotten() {
        let mut flashes = CutFlashes::default();
        flashes.add(track(1), frame(40));
        flashes.update(10.0);

        assert!(flashes.update(10.0 + FLASH_SECONDS / 2.0));
        let (_, progress) = flashes.on_track(track(1), 10.0 + FLASH_SECONDS / 2.0).next().unwrap();
        assert!((progress - 0.5).abs() < 1e-6, "{progress}");

        assert!(
            !flashes.update(10.0 + FLASH_SECONDS),
            "over: nothing shows and nothing needs the window drawn"
        );
        assert_eq!(flashes.on_track(track(1), 10.0 + FLASH_SECONDS).count(), 0);
    }

    #[test]
    fn flashes_are_shown_on_the_tracks_they_were_made_on_and_each_goes_its_own_way() {
        let mut flashes = CutFlashes::default();
        flashes.add(track(1), frame(40));
        flashes.add(track(2), frame(40));
        flashes.update(5.0);
        flashes.add(track(1), frame(90));
        flashes.update(5.1);

        let on_the_first: Vec<_> = flashes.on_track(track(1), 5.1).map(|(frame, _)| frame).collect();
        assert_eq!(on_the_first, vec![frame(40), frame(90)]);
        let on_the_second: Vec<_> = flashes.on_track(track(2), 5.1).map(|(frame, _)| frame).collect();
        assert_eq!(on_the_second, vec![frame(40)]);
        assert_eq!(flashes.on_track(track(3), 5.1).count(), 0);

        // The first two are over (0.3 s after 5.0) while the one that began at 5.1 still has time.
        assert!(flashes.update(5.0 + FLASH_SECONDS + 0.01));
        assert_eq!(
            flashes
                .on_track(track(1), 5.31)
                .map(|(frame, _)| frame)
                .collect::<Vec<_>>(),
            vec![frame(90)]
        );
        assert_eq!(flashes.on_track(track(2), 5.31).count(), 0);
    }

    #[test]
    fn a_flash_is_brightest_at_the_start_and_gone_at_the_end() {
        assert_eq!(brightness(0.0), 1.0);
        assert_eq!(brightness(1.0), 0.0);
        let mut last = brightness(0.0);
        for step in 1..=20 {
            let now = brightness(step as f32 / 20.0);
            assert!(now < last, "it dims all the way: {now} after {last}");
            last = now;
        }
    }

    #[test]
    fn the_light_of_a_flash_is_the_most_it_ever_is_when_it_begins_and_never_grows_after_that() {
        // (The light is how far the glow reaches times how bright it is: a glow that spreads faster than it fades is a flash
        // that seems to swell for a moment before it dies away, which is not one that begins at full strength.)
        let light = |progress: f32| glow_reach(progress) * brightness(progress);
        let mut last = light(0.0);
        for step in 1..=1000 {
            let now = light(step as f32 / 1000.0);
            assert!(
                now <= last * 1.000_001,
                "at {step} thousandths of the way through: {now} after {last}"
            );
            last = now;
        }
        assert!(light(1.0) < 0.001 * light(0.0), "and it is gone at the end");
    }

    #[test]
    fn a_flash_has_all_of_the_light_it_will_have_at_the_start_and_keeps_it_while_the_glow_that_spreads_freely_would_have_more()
     {
        let light = |progress: f32| glow_reach(progress) * brightness(progress);
        let most_light_at = progress_with_the_most_light();
        // (Measured: the glow that spreads freely has the most light a fifth of the way through, which is 60 milliseconds.)
        assert!((most_light_at - 0.2014).abs() < 0.001, "{most_light_at}");
        let most_light = glow_reach_when_it_spreads_freely(most_light_at) * brightness(most_light_at);
        for step in 0..=20 {
            let progress = most_light_at * step as f32 / 20.0;
            assert!(
                (light(progress) - most_light).abs() < 1e-3,
                "{progress}: {} and {most_light}",
                light(progress)
            );
        }
        assert!(
            (0..=1000).all(|step| {
                let progress = step as f32 / 1000.0;
                glow_reach_when_it_spreads_freely(progress) * brightness(progress) <= most_light + 1e-3
            }),
            "that is the most that the glow that spreads freely has, anywhere"
        );
    }

    #[test]
    fn from_the_time_the_light_of_a_flash_is_at_its_most_the_glow_spreads_and_the_flash_fades_as_they_always_did() {
        let most_light_at = progress_with_the_most_light();
        for step in 0..=100 {
            let progress = most_light_at + (1.0 - most_light_at) * step as f32 / 100.0;
            assert_eq!(
                glow_reach(progress),
                glow_reach_when_it_spreads_freely(progress),
                "{progress}"
            );
        }
        assert_eq!(
            brightness(0.5),
            0.25,
            "the fade is the same: the brightness is what is left, squared"
        );
    }

    #[test]
    fn the_glow_spreads_from_a_small_one_to_a_wide_one() {
        assert!(
            glow_reach(0.0) > GLOW_REACH_AT_FIRST,
            "it begins with the reach that gives it all its light"
        );
        assert!(glow_reach(0.0) < glow_reach(progress_with_the_most_light()));
        assert_eq!(glow_reach(1.0), GLOW_REACH_AT_LAST);
        assert_eq!(glow_reach_when_it_spreads_freely(0.0), GLOW_REACH_AT_FIRST);
        let mut last = glow_reach(0.0);
        for step in 1..=20 {
            let now = glow_reach(step as f32 / 20.0);
            assert!(now > last, "{now} after {last}");
            last = now;
        }
    }

    #[test]
    fn white_at_a_brightness_is_that_much_of_white() {
        assert_eq!(white_at(0.0), Color32::TRANSPARENT);
        assert_eq!(white_at(1.0), Color32::WHITE);
        assert_eq!(white_at(2.0), Color32::WHITE, "never brighter than white");
        assert_eq!(white_at(-1.0), Color32::TRANSPARENT);
    }
}
