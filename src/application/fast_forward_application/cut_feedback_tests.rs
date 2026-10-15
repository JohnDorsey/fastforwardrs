// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of what a cut makes known: the snip that is heard, and the flash of light that is seen where each clip
//! was cut.

use std::time::Instant;

use egui::epaint::Mesh;
use egui::{Color32, Key, Modifiers, Shape};

use super::test_window::{Window, project_of, project_with_a_video_and_a_sound};
use crate::application::commands::Command;
use crate::application::theme;
use crate::playback::SoundEffect;
use crate::project::TimelineFrameIndex;

/// How the snip sounds for the speakers the tests give the editor (they play 48000 moments a second).
fn the_snip() -> Vec<f32> {
    SoundEffect::Snip.synthesize(48000)
}

/// A flash of light that was drawn in the last frame.
#[derive(Clone, Copy, Debug)]
struct DrawnFlash {
    /// Where, across the screen, the cut it is at is.
    x: f32,
    top: f32,
    bottom: f32,
    /// How bright the light is at the cut, from 0 to 255.
    brightness: u8,
    /// How far the glow reaches to either side of the cut, in points.
    reach: f32,
}

impl DrawnFlash {
    /// How much light there is in all: how far the glow reaches times how bright it is.
    fn light(&self) -> f32 {
        self.reach * f32::from(self.brightness)
    }
}

/// Whether `mesh` is the glow of a flash: two rows of three points, the middle ones white and the outer ones clear.
fn glow_of_a_flash(mesh: &Mesh) -> Option<DrawnFlash> {
    let [first, middle, last, other_first, other_middle, other_last] = mesh.vertices[..] else {
        return None;
    };
    let is_white =
        |color: Color32| color.a() > 0 && color.r() == color.a() && color.g() == color.a() && color.b() == color.a();
    let is_a_glow = first.color == Color32::TRANSPARENT
        && last.color == Color32::TRANSPARENT
        && other_first.color == Color32::TRANSPARENT
        && other_last.color == Color32::TRANSPARENT
        && is_white(middle.color)
        && middle.color == other_middle.color
        && middle.pos.x == other_middle.pos.x
        && first.pos.y == middle.pos.y;
    is_a_glow.then(|| DrawnFlash {
        x: middle.pos.x,
        top: middle.pos.y,
        bottom: other_middle.pos.y,
        brightness: middle.color.a(),
        reach: middle.pos.x - first.pos.x,
    })
}

/// The bright lines at cuts that were drawn in the last frame: where each is across the screen, from where to
/// where it goes up and down, and how bright it is, from 0 to 255.
fn bright_lines_at_cuts_drawn(window: &Window) -> Vec<(f32, (f32, f32), u8)> {
    window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::LineSegment { points, stroke }
                if stroke.width == 2.0
                    && stroke.color.a() > 0
                    && [stroke.color.r(), stroke.color.g(), stroke.color.b()] == [stroke.color.a(); 3] =>
            {
                Some((points[0].x, (points[0].y, points[1].y), stroke.color.a()))
            }
            _ => None,
        })
        .collect()
}

fn flashes_drawn(window: &Window) -> Vec<DrawnFlash> {
    window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Mesh(mesh) => glow_of_a_flash(mesh),
            _ => None,
        })
        .collect()
}

/// Lets the window run for `seconds` (a frame of it is a sixtieth of a second long).
fn let_time_pass(window: &mut Window, seconds: f64) {
    window.harness.run_steps((seconds * 60.0).ceil() as usize);
}

/// Cuts at `frame` as the keyboard does, but does not let the window draw a frame after it.
fn cut_without_drawing_after(window: &mut Window, frame: u64) {
    window.application_mut().execute_all(
        vec![Command::SplitAt {
            frame: TimelineFrameIndex::new(frame),
            only: None,
        }],
        Instant::now(),
        &egui::Context::default(),
    );
}

// --- the snip ---

#[test]
fn cutting_a_clip_with_s_makes_a_snip() {
    let mut window = Window::showing(project_of(300));
    let log = window.log_the_sound_effects();

    window.cut_at_frame(100);

    assert_eq!(window.clips().len(), 2, "the clip was cut");
    let played = log.sounds_played();
    assert_eq!(played.len(), 1, "one snip");
    assert_eq!(*played[0], *the_snip());
}

#[test]
fn cutting_clips_on_several_tracks_at_once_makes_one_snip_and_not_one_for_each() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(300, 10));
    let log = window.log_the_sound_effects();

    window.cut_at_frame(100);

    assert_eq!(window.clips_of_track(1).len(), 2, "the clip of the video was cut");
    assert_eq!(
        window.application().project().tracks()[0].placements().count(),
        2,
        "and that of the sound"
    );
    assert_eq!(log.count_of_sounds_played(), 1);
}

#[test]
fn every_cut_makes_its_own_snip() {
    let mut window = Window::showing(project_of(300));
    let log = window.log_the_sound_effects();

    window.cut_at_frame(100);
    window.cut_at_frame(200);

    assert_eq!(window.clips().len(), 3);
    assert_eq!(log.count_of_sounds_played(), 2);
}

#[test]
fn a_cut_that_cuts_nothing_makes_no_snip() {
    let mut window = Window::showing(project_of(300));
    let log = window.log_the_sound_effects();

    window.cut_at_frame(0); // where the clip begins: there is nothing to cut
    assert_eq!(window.clips().len(), 1);
    assert_eq!(log.count_of_sounds_played(), 0);

    window.cut_at_frame(100);
    assert_eq!(log.count_of_sounds_played(), 1);

    window.cut_at_frame(100); // where the cut already is
    assert_eq!(window.clips().len(), 2);
    assert_eq!(
        log.count_of_sounds_played(),
        1,
        "only the one cut that cut something was heard"
    );
}

#[test]
fn cutting_only_the_selected_clip_makes_one_snip() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(300, 10));
    let log = window.log_the_sound_effects();
    window.click(window.middle_of_the_strip_at(0, 150));
    assert_eq!(window.application().selected_clips().len(), 1);

    window.cut_at_frame(100);

    assert_eq!(window.clips_of_track(1).len(), 1, "the video clip was left alone");
    assert_eq!(log.count_of_sounds_played(), 1);
}

#[test]
fn cutting_when_no_speakers_were_given_works_and_is_silent() {
    let mut window = Window::showing(project_of(300));
    window.cut_at_frame(100);
    assert_eq!(window.clips().len(), 2);
}

#[test]
fn an_editor_is_silent_until_it_is_given_the_default_speakers_and_is_then_given_them_for_all_its_sounds() {
    let mut window = Window::showing(project_of(300));
    assert!(
        !window
            .application()
            .playback
            .sound_effects
            .is_given_speakers_to_play_through()
    );
    assert!(
        !window
            .application()
            .playback
            .audio_supplier
            .is_given_speakers_to_play_through()
    );

    window.application_mut().use_the_default_speakers();

    assert!(
        window
            .application()
            .playback
            .sound_effects
            .is_given_speakers_to_play_through()
    );
    assert!(
        window
            .application()
            .playback
            .audio_supplier
            .is_given_speakers_to_play_through()
    );
}

#[test]
fn undoing_and_redoing_a_cut_make_no_snip() {
    let mut window = Window::showing(project_of(300));
    let log = window.log_the_sound_effects();
    window.cut_at_frame(100);
    assert_eq!(log.count_of_sounds_played(), 1);

    window.do_this(Command::Undo);
    window.do_this(Command::Redo);

    assert_eq!(window.clips().len(), 2, "the cut was done again");
    assert_eq!(log.count_of_sounds_played(), 1);
}

#[test]
fn the_snip_is_made_whatever_the_playhead_is_doing() {
    let mut window = Window::showing(project_of(300));
    let log = window.log_the_sound_effects();
    window.move_current_frame_to(100);
    window.do_this(Command::Play);
    assert!(window.application().is_playing());

    window.press_key_with(Modifiers::SHIFT, Key::S);

    assert_eq!(log.count_of_sounds_played(), 1);
}

// --- the flash ---

#[test]
fn a_cut_flashes_where_it_is_across_the_whole_height_of_the_strip_of_the_track_it_was_made_in() {
    let mut window = Window::showing(project_of(300));
    window.cut_at_frame(100);

    let flashes = flashes_drawn(&window);
    assert_eq!(flashes.len(), 1, "{flashes:?}");
    let strip = window.strip_of_track(0);
    let flash = flashes[0];
    let cut_x = window.x_of_frame_on_the_strip(0, 100);
    assert!((flash.x - cut_x).abs() < 0.5, "at {} and not at {cut_x}", flash.x);
    assert_eq!((flash.top, flash.bottom), (strip.top(), strip.bottom()));
    assert!(
        flash.brightness > 60,
        "it is still bright after a few frames: {flash:?}"
    );

    // A line of light at the very cut, which is brighter than the glow round it.
    let lines = bright_lines_at_cuts_drawn(&window);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let (line_x, (line_top, line_bottom), line_brightness) = lines[0];
    assert!(
        (line_x - cut_x).abs() < 0.5,
        "the line is at {line_x} and not at {cut_x}"
    );
    assert_eq!((line_top, line_bottom), (strip.top(), strip.bottom()));
    assert!(
        line_brightness > flash.brightness,
        "the line is brighter ({line_brightness}) than the glow ({})",
        flash.brightness
    );
}

#[test]
fn a_flash_is_cut_off_at_the_edges_of_the_strip_it_is_on_and_does_not_light_up_the_strips_beside_it() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(300, 10));
    window.click(window.middle_of_the_strip_at(0, 150));
    window.cut_at_frame(100);

    let clip_rectangles: Vec<_> = window
        .clipped_shapes_drawn()
        .into_iter()
        .filter(|(_, shape)| matches!(shape, Shape::Mesh(mesh) if glow_of_a_flash(mesh).is_some()))
        .map(|(clip_rectangle, _)| clip_rectangle)
        .collect();
    assert_eq!(clip_rectangles, vec![window.strip_of_track(0)]);
}

#[test]
fn the_flash_is_drawn_over_the_playhead_which_is_where_the_cut_is_so_that_its_outline_does_not_hide_it() {
    let mut window = Window::showing(project_of(300));
    window.move_current_frame_to(100);
    cut_without_drawing_after(&mut window, 100);
    window.harness.step();

    let shapes = window.shapes_drawn();
    let playhead = shapes
        .iter()
        .position(|shape| matches!(shape, Shape::LineSegment { stroke, .. } if stroke.color == theme::PLAYHEAD))
        .expect("the playhead is drawn");
    let outline = shapes
        .iter()
        .position(|shape| matches!(shape, Shape::LineSegment { stroke, .. } if stroke.color == theme::PLAYHEAD_OUTLINE))
        .expect("and its outline");
    let glow = shapes
        .iter()
        .position(|shape| matches!(shape, Shape::Mesh(mesh) if glow_of_a_flash(mesh).is_some()))
        .expect("the flash is drawn");
    assert!(
        glow > playhead && glow > outline,
        "the playhead at {playhead}, its outline at {outline}, the flash at {glow}"
    );
}

#[test]
fn a_flash_is_brightest_at_first_fades_and_is_over_in_about_a_third_of_a_second() {
    let mut window = Window::showing(project_of(300));
    window.move_current_frame_to(100);
    cut_without_drawing_after(&mut window, 100);

    let mut brightnesses = Vec::new();
    loop {
        window.harness.step();
        match flashes_drawn(&window).first() {
            Some(flash) => brightnesses.push(flash.brightness),
            None => break,
        }
        assert!(brightnesses.len() < 60, "it never ends: {brightnesses:?}");
    }
    // A frame is a sixtieth of a second: it was drawn for 0.3 seconds, to the frame.
    assert!((17..=19).contains(&brightnesses.len()), "{brightnesses:?}");
    assert!(brightnesses[0] >= 170, "bright at once: {brightnesses:?}");
    assert!(
        brightnesses.windows(2).all(|pair| pair[1] <= pair[0]),
        "it only gets fainter: {brightnesses:?}"
    );
    assert!(
        brightnesses[brightnesses.len() / 2] < brightnesses[0] / 2,
        "half way, it has lost more than half of it: {brightnesses:?}"
    );
    assert!(
        brightnesses.last().unwrap() < &20,
        "and at the end there is almost none left: {brightnesses:?}"
    );
}

#[test]
fn the_light_of_a_flash_is_greatest_in_the_first_frame_and_never_grows_after_that() {
    let mut window = Window::showing(project_of(300));
    window.move_current_frame_to(100);
    cut_without_drawing_after(&mut window, 100);

    let mut lights = Vec::new();
    let mut reaches = Vec::new();
    loop {
        window.harness.step();
        match flashes_drawn(&window).first() {
            Some(flash) => {
                lights.push(flash.light());
                reaches.push(flash.reach);
            }
            None => break,
        }
        assert!(lights.len() < 60, "it never ends: {lights:?}");
    }

    assert!(lights.len() >= 15, "{lights:?}");
    assert!(
        lights.windows(2).all(|pair| pair[1] <= pair[0] * 1.01),
        "no frame has more light than the one before (but for the rounding of the colour to a whole number), so none seems to \
         swell: {lights:?}"
    );
    assert!(
        lights[0] >= 0.99 * lights.iter().copied().fold(0.0, f32::max),
        "the first frame has the most light there is: {lights:?}"
    );
    assert!(
        lights[lights.len() / 2] < 0.6 * lights[0],
        "and it fades away: {lights:?}"
    );
    assert!(
        reaches.windows(2).all(|pair| pair[1] >= pair[0]),
        "(the glow still spreads as it fades: {reaches:?})"
    );
}

#[test]
fn the_window_is_drawn_again_at_once_while_a_flash_is_showing_and_not_when_it_is_over() {
    let mut window = Window::showing(project_of(300));
    window.move_current_frame_to(100);
    cut_without_drawing_after(&mut window, 100);
    window.harness.step();
    assert!(!flashes_drawn(&window).is_empty());
    assert!(
        window.repaint_is_wanted_at_once(),
        "the next frame must come as soon as the screen can show it, to show the flash moving"
    );

    let_time_pass(&mut window, 1.0);
    assert!(flashes_drawn(&window).is_empty());
    assert!(
        !window.repaint_is_wanted_at_once(),
        "a window that shows nothing moving does not spin"
    );
}

#[test]
fn clips_on_several_tracks_cut_at_once_flash_on_each_of_their_tracks_and_not_on_others() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(300, 10));
    window.click(window.middle_of_the_strip_at(0, 150));
    window.cut_at_frame(100);
    let flashes = flashes_drawn(&window);
    assert_eq!(flashes.len(), 1, "only the selected clip was cut: {flashes:?}");
    let sound_strip = window.strip_of_track(0);
    assert_eq!(
        (flashes[0].top, flashes[0].bottom),
        (sound_strip.top(), sound_strip.bottom())
    );

    let_time_pass(&mut window, 1.0);
    assert!(flashes_drawn(&window).is_empty());
    window.do_this(Command::Deselect);
    window.cut_at_frame(200);
    let flashes = flashes_drawn(&window);
    assert_eq!(flashes.len(), 2, "both tracks were cut: {flashes:?}");
    let video_strip = window.strip_of_track(1);
    let mut places: Vec<(f32, f32)> = flashes.iter().map(|flash| (flash.top, flash.bottom)).collect();
    places.sort_by(|first, second| first.0.total_cmp(&second.0));
    assert_eq!(
        places,
        vec![
            (sound_strip.top(), sound_strip.bottom()),
            (video_strip.top(), video_strip.bottom())
        ]
    );
}

#[test]
fn two_cuts_in_a_row_each_flash_at_their_own_place() {
    let mut window = Window::showing(project_of(300));
    window.cut_at_frame(100);
    window.cut_at_frame(200);

    let mut xs: Vec<f32> = flashes_drawn(&window).iter().map(|flash| flash.x).collect();
    xs.sort_by(f32::total_cmp);
    assert_eq!(xs.len(), 2);
    assert!((xs[0] - window.x_of_frame_on_the_strip(0, 100)).abs() < 0.5);
    assert!((xs[1] - window.x_of_frame_on_the_strip(0, 200)).abs() < 0.5);
}

#[test]
fn a_cut_that_cuts_nothing_makes_no_flash() {
    let mut window = Window::showing(project_of(300));
    window.cut_at_frame(0);
    assert!(flashes_drawn(&window).is_empty());
}
