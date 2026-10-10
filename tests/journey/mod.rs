//! A person's view of the editor, for the integration tests that run whole journeys: the real
//! window drawn frame by frame (without a screen or a graphics card), used with simulated mouse
//! input, with a real ffmpeg behind it.
//!
//! Nothing here touches the real mouse or opens a file dialog. The things a file dialog would
//! choose are handed to the editor directly (see the "programmatic interface" of the application).
//!
//! A test file that uses this declares both `mod support;` and `mod journey;`.

#![allow(dead_code)] // Each test file uses a different part of this module.

use std::path::Path;
use std::time::{Duration, Instant};

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use fastforwardrs::application::FastForwardApplication;
use fastforwardrs::media::FfmpegToolchain;
use fastforwardrs::project::TimelineFrameIndex;

use crate::support::frame_number_in_picture;

const WINDOW_WIDTH: f32 = 1280.0;
const WINDOW_HEIGHT: f32 = 820.0;

/// How long the editor may take over opening a video, making its pictures or exporting it.
pub const TIME_ALLOWED_FOR_BACKGROUND_WORK: Duration = Duration::from_secs(180);

/// How long the preview may take to show a picture that is wanted. (It normally takes about a
/// tenth of a second.)
pub const TIME_ALLOWED_FOR_A_PREVIEW_PICTURE: Duration = Duration::from_secs(20);

/// How far the ends of the slider along the bottom are from the ends of the groove the knob runs
/// along (the radius of the knob).
const SCRUB_SLIDER_END_MARGIN: f32 = 9.0;

/// A clip as the test sees it: the first source frame, the source frame after the last, and the speed.
pub type ClipSummary = (u64, u64, u32);

/// The editor in a window, used like a person would.
pub struct Journey {
    pub harness: Harness<'static, FastForwardApplication>,
}

impl Journey {
    /// A window showing an editor with an empty project.
    pub fn start(toolchain: &FfmpegToolchain) -> Self {
        let application = FastForwardApplication::with_context(&egui::Context::default(), Ok(toolchain.clone()));
        let mut harness = Harness::builder()
            .with_size(vec2(WINDOW_WIDTH, WINDOW_HEIGHT))
            .build_ui_state(
                |ui, application: &mut FastForwardApplication| application.show(ui),
                application,
            );
        harness.run();
        Self { harness }
    }

    /// A window showing the editor with the video at `video_path` imported, once its pictures are made.
    pub fn importing(toolchain: &FfmpegToolchain, video_path: &Path) -> Self {
        let mut journey = Self::start(toolchain);
        journey.import(video_path);
        journey
    }

    /// Imports the video at `video_path` (what choosing it in the Import dialog does) and waits
    /// until it is fully opened.
    pub fn import(&mut self, video_path: &Path) {
        self.application_mut().import_video_from(video_path.to_owned());
        self.wait_until_idle();
        assert!(
            self.application().source_video_is_available(),
            "the video could not be imported: {:?}",
            self.application().message_showing()
        );
    }

    pub fn application(&self) -> &FastForwardApplication {
        self.harness.state()
    }

    pub fn application_mut(&mut self) -> &mut FastForwardApplication {
        self.harness.state_mut()
    }

    // --- waiting ---

    /// Draws frames until everything the editor is doing in the background has finished.
    pub fn wait_until_idle(&mut self) {
        let started = Instant::now();
        loop {
            self.harness.step();
            if !self.application().is_working_in_the_background() {
                self.settle();
                return;
            }
            assert!(
                started.elapsed() < TIME_ALLOWED_FOR_BACKGROUND_WORK,
                "the editor was still working after {TIME_ALLOWED_FOR_BACKGROUND_WORK:?}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Draws frames until the preview shows the picture of the current frame.
    pub fn wait_for_the_preview_to_show_the_current_frame(&mut self) {
        let started = Instant::now();
        loop {
            self.harness.step();
            let application = self.application();
            let wanted = application
                .project()
                .video_track()
                .locate(application.current_frame())
                .map(|location| location.source_frame);
            let shown = application.preview_picture().map(|picture| picture.source_frame);
            if wanted.is_some() && wanted == shown {
                return;
            }
            assert!(
                started.elapsed() < TIME_ALLOWED_FOR_A_PREVIEW_PICTURE,
                "the preview never showed {wanted:?}: it shows {shown:?}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Draws a few frames, so that whatever the last action set going has been carried out.
    pub fn settle(&mut self) {
        self.harness.run_steps(3);
    }

    // --- looking ---

    /// The clips of the video track, in order.
    pub fn clips(&self) -> Vec<ClipSummary> {
        self.application()
            .project()
            .video_track()
            .clips()
            .iter()
            .map(|clip| {
                (
                    clip.first_source_frame().value(),
                    clip.end_source_frame().value(),
                    clip.speed_factor().value(),
                )
            })
            .collect()
    }

    /// The frame number written into the picture the preview shows (see the support module).
    pub fn frame_number_in_the_preview(&self) -> u64 {
        let picture = self.application().preview_picture().expect("the preview has a picture");
        frame_number_in_picture(
            &picture.frame.rgba_pixels,
            picture.frame.width_in_pixels,
            picture.frame.height_in_pixels,
            4,
        )
    }

    pub fn rectangle_of(&self, label: &str) -> Rect {
        self.harness.get_by_label(label).rect()
    }

    // --- the mouse ---

    pub fn move_pointer_to(&mut self, position: Pos2) {
        self.harness.event(Event::PointerMoved(position));
        self.harness.step();
    }

    fn press_or_release(&mut self, position: Pos2, button: PointerButton, pressed: bool) {
        self.harness.event(Event::PointerButton {
            pos: position,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        });
        self.harness.step();
    }

    pub fn click(&mut self, position: Pos2) {
        self.move_pointer_to(position);
        self.press_or_release(position, PointerButton::Primary, true);
        self.press_or_release(position, PointerButton::Primary, false);
        self.settle();
    }

    /// Presses `button` at `from`, moves in small steps to `to`, and lets go there.
    pub fn drag(&mut self, button: PointerButton, from: Pos2, to: Pos2) {
        self.move_pointer_to(from);
        self.press_or_release(from, button, true);
        let steps = 12;
        for step in 1..=steps {
            self.move_pointer_to(from + (to - from) * (step as f32 / steps as f32));
        }
        self.press_or_release(to, button, false);
        self.settle();
    }

    /// Presses one of the buttons next to the time ("Pause", "Play" or "Stop").
    pub fn press_button(&mut self, label: &str) {
        self.harness.get_by_label(label).click();
        self.harness.step();
    }

    // --- the video track and the slider ---

    /// A point on the video track just inside timeline frame `frame`, which must be in view.
    pub fn point_on_the_track_at_frame(&self, frame: u64) -> Pos2 {
        let track = self.rectangle_of("Video track");
        let offset = self
            .application()
            .video_track_offset_of_frame(TimelineFrameIndex::new(frame))
            .expect("the project has a video");
        let point = Pos2::new(track.left() + offset + 1.0, track.center().y);
        assert!(
            track.contains(point),
            "frame {frame} is scrolled out of view: {point:?} is not in {track:?}"
        );
        point
    }

    /// A point on the ruler above the video track just inside timeline frame `frame`, which must
    /// be in view.
    pub fn point_on_the_ruler_at_frame(&self, frame: u64) -> Pos2 {
        let ruler = self.rectangle_of("Ruler");
        let offset = self
            .application()
            .video_track_offset_of_frame(TimelineFrameIndex::new(frame))
            .expect("the project has a video");
        let point = Pos2::new(ruler.left() + offset + 1.0, ruler.center().y);
        assert!(
            ruler.contains(point),
            "frame {frame} is scrolled out of view: {point:?} is not in {ruler:?}"
        );
        point
    }

    /// Moves the playhead to `frame` by clicking the ruler there.
    pub fn move_playhead_to_frame(&mut self, frame: u64) {
        let point = self.point_on_the_ruler_at_frame(frame);
        self.click(point);
    }

    /// Presses and lets go of a key, and draws a few frames.
    pub fn press_key(&mut self, key: Key) {
        self.harness.key_press(key);
        self.harness.step();
        self.settle();
    }

    /// Cuts the video track at `frame` the way a person does: puts the playhead there (by clicking
    /// the ruler) and presses S.
    pub fn cut_at_frame(&mut self, frame: u64) {
        self.move_playhead_to_frame(frame);
        self.press_key(Key::S);
    }

    /// Sets the speed of the clip that is showing `frame`, by pressing the right mouse button on
    /// it there and dragging up `points_up` points.
    pub fn drag_the_speed_slider_up(&mut self, frame: u64, points_up: f32) {
        let press = self.point_on_the_track_at_frame(frame);
        self.drag(PointerButton::Secondary, press, press - vec2(0.0, points_up));
    }

    /// Clicks the slider along the bottom of the window `fraction` of the way along it (0 is the
    /// left end, 1 the right end).
    pub fn click_the_scrub_slider_at(&mut self, fraction: f32) {
        let slider = self.rectangle_of("Position in the video");
        let left = slider.left() + SCRUB_SLIDER_END_MARGIN;
        let right = slider.right() - SCRUB_SLIDER_END_MARGIN;
        self.click(Pos2::new(left + fraction * (right - left), slider.center().y));
    }
}
