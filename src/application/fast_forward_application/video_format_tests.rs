// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! How the pictures of the videos are stored, which the preview asks of ffprobe when (and only when) it wants the pictures in planes, and
//! where what it is told goes. A test gives the editor a prober of its own, one that counts how often it is asked and says what the test wants
//! said, so that no ffprobe is run.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::state::VideoFormatProber;
use super::test_window::{SOUND, VIDEO, Window, project_of, project_with_a_video_and_a_sound};
use super::{SourceFileState, SourceStatus};
use crate::media::{MediaLocation, VideoStreamFormat};

fn ordinary_video_format() -> VideoStreamFormat {
    VideoStreamFormat {
        pixel_format: Some("yuv420p".to_owned()),
        colour_space: Some("bt709".to_owned()),
        colour_range: Some("tv".to_owned()),
        ..VideoStreamFormat::default()
    }
}

/// A prober that does what `answer` does, and the places it was asked about, in order.
fn prober_that_answers(
    answer: impl Fn() -> Option<VideoStreamFormat> + Send + Sync + 'static,
) -> (VideoFormatProber, Arc<Mutex<Vec<MediaLocation>>>) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let prober: VideoFormatProber = {
        let asked = Arc::clone(&asked);
        Arc::new(move |location: &MediaLocation| {
            asked.lock().unwrap().push(location.clone());
            answer()
        })
    };
    (prober, asked)
}

fn window_with_a_prober(
    window: &mut Window,
    answer: impl Fn() -> Option<VideoStreamFormat> + Send + Sync + 'static,
) -> Arc<Mutex<Vec<MediaLocation>>> {
    let (prober, asked) = prober_that_answers(answer);
    window.application_mut().environment.video_format_prober = prober;
    asked
}

/// Draws frames for a short while, as a window does that nothing is happening in.
fn draw_some_frames(window: &mut Window) {
    for _ in 0..20 {
        window.harness.step();
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Draws frames until `done`, or fails after a while. (What is found out in the background is noticed in the next frame drawn after it is found out.)
fn draw_frames_until(window: &mut Window, what: &str, done: impl Fn(&Window) -> bool) {
    let started = Instant::now();
    while !done(window) {
        assert!(started.elapsed() < Duration::from_secs(20), "it did not happen: {what}");
        window.harness.step();
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn the_format_in_the_setup(window: &Window, video_number: usize) -> Option<VideoStreamFormat> {
    window
        .application()
        .playback
        .preview_pictures
        .setup()
        .expect("the preview has a setup")
        .videos[video_number]
        .format
        .clone()
}

fn the_probe_of_the_video_is_over(window: &Window) -> bool {
    let state = &window.application().sources.files[&VIDEO];
    state.video_format.was_asked && state.video_format.probe.is_none()
}

#[test]
fn while_planes_are_not_wanted_nobody_asks_how_the_pictures_are_stored_and_the_setup_says_so() {
    let mut window = Window::showing(project_of(300));
    let asked = window_with_a_prober(&mut window, || Some(ordinary_video_format()));

    draw_some_frames(&mut window);

    assert_eq!(*asked.lock().unwrap(), Vec::<MediaLocation>::new());
    // (The window drew its first frames before the prober above was given to it: that nobody was asked in those either is in what it kept.)
    let state = &window.application().sources.files[&VIDEO].video_format;
    assert!(!state.was_asked && state.probe.is_none() && state.known.is_none());
    let setup = window
        .application()
        .playback
        .preview_pictures
        .setup()
        .expect("the preview has a setup");
    assert!(!setup.planes_wanted);
    assert_eq!(setup.videos.len(), 1);
    assert_eq!(setup.videos[0].format, None);
}

#[test]
fn while_planes_are_wanted_each_video_is_asked_about_once_and_what_it_says_is_in_the_setup() {
    let mut window = Window::showing(project_of(300));
    let asked = window_with_a_prober(&mut window, || Some(ordinary_video_format()));
    window.application_mut().playback.planes_wanted = true;

    draw_frames_until(&mut window, "what ffprobe said is in the setup", |window| {
        the_format_in_the_setup(window, 0).is_some()
    });
    draw_some_frames(&mut window);

    assert_eq!(
        *asked.lock().unwrap(),
        vec![MediaLocation::from("holiday.mp4")],
        "once, and for the video"
    );
    assert!(
        window
            .application()
            .playback
            .preview_pictures
            .setup()
            .unwrap()
            .planes_wanted
    );
    assert_eq!(the_format_in_the_setup(&window, 0), Some(ordinary_video_format()));
}

#[test]
fn a_video_that_ffprobe_can_say_nothing_about_is_asked_about_once_and_is_left_without_a_format() {
    let mut window = Window::showing(project_of(300));
    let asked = window_with_a_prober(&mut window, || None);
    window.application_mut().playback.planes_wanted = true;

    draw_frames_until(&mut window, "the probe is over", the_probe_of_the_video_is_over);
    draw_some_frames(&mut window);

    assert_eq!(asked.lock().unwrap().len(), 1, "it is not asked again and again");
    assert_eq!(the_format_in_the_setup(&window, 0), None);
    assert!(
        window
            .application()
            .playback
            .preview_pictures
            .setup()
            .unwrap()
            .planes_wanted
    );
}

#[test]
fn a_probe_that_crashes_is_not_started_again_and_leaves_the_video_without_a_format() {
    let mut window = Window::showing(project_of(300));
    let asked = window_with_a_prober(&mut window, || panic!("this crash is expected by the test"));
    window.application_mut().playback.planes_wanted = true;

    draw_frames_until(&mut window, "the probe is over", the_probe_of_the_video_is_over);
    draw_some_frames(&mut window);

    assert_eq!(asked.lock().unwrap().len(), 1, "it is not started again and again");
    assert_eq!(the_format_in_the_setup(&window, 0), None);
}

#[test]
fn only_a_video_that_can_be_found_is_asked_about_and_not_a_sound() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(300, 10));
    {
        // (The sound is there, and can be used: that is what the window shows it with.)
        let files = &window.application().sources.files;
        assert!(files[&SOUND].location_if_available().is_some());
    }
    let asked = window_with_a_prober(&mut window, || Some(ordinary_video_format()));
    window.application_mut().playback.planes_wanted = true;

    draw_frames_until(&mut window, "what ffprobe said is in the setup", |window| {
        the_format_in_the_setup(window, 0).is_some()
    });
    draw_some_frames(&mut window);

    assert_eq!(
        *asked.lock().unwrap(),
        vec![MediaLocation::from("holiday.mp4")],
        "the sound was not asked about"
    );

    // A video that cannot be found now is not asked about either, however long it is wanted.
    let mut window = Window::showing(project_of(300));
    window.application_mut().sources.files.insert(
        VIDEO,
        SourceFileState::new(SourceStatus::Unavailable {
            reason: "it is gone".to_owned(),
        }),
    );
    let asked = window_with_a_prober(&mut window, || Some(ordinary_video_format()));
    window.application_mut().playback.planes_wanted = true;
    draw_some_frames(&mut window);
    assert_eq!(*asked.lock().unwrap(), Vec::<MediaLocation>::new());
}

#[test]
fn a_video_is_asked_about_when_planes_become_wanted_even_though_it_was_found_long_before() {
    let mut window = Window::showing(project_of(300));
    let asked = window_with_a_prober(&mut window, || Some(ordinary_video_format()));
    draw_some_frames(&mut window);
    assert!(asked.lock().unwrap().is_empty(), "planes are not wanted yet");

    window.application_mut().playback.planes_wanted = true;
    draw_frames_until(&mut window, "what ffprobe said is in the setup", |window| {
        the_format_in_the_setup(window, 0).is_some()
    });

    assert_eq!(asked.lock().unwrap().len(), 1);
}
