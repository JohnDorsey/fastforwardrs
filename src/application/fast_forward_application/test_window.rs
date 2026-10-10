//! The window the interaction tests use, and the ways of using it: it shows the real editor with
//! a project made in memory (from a video that does not exist on disk, so that the tests need
//! neither ffmpeg nor a graphics card), and sends it the mouse and keyboard input a person would
//! (without ever touching the real mouse).

use std::path::PathBuf;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use super::{FastForwardApplication, SourceStatus};
use crate::application::theme;
use crate::application::timeline_geometry::TimelineGeometry;
use crate::media::ToolchainError;
use crate::project::{
    FrameRate, Project, SourceVideo, SourceVideoIdentifier, SourceVideoLink, SourceVideoProperties, TimelineFrameIndex,
};

pub(super) const WINDOW_SIZE: [f32; 2] = [1280.0, 820.0];
pub(super) const FRAMES_PER_SECOND: u32 = 30;

/// A project of `total_frames` frames from a video that is 1280x720 at 30 frames per second.
pub(super) fn project_of(total_frames: u64) -> Project {
    let mut project = Project::empty();
    project
        .import_source_video(SourceVideo {
            identifier: SourceVideoIdentifier::new(1),
            link: SourceVideoLink::from_absolute_path(PathBuf::from("holiday.mp4")),
            properties: SourceVideoProperties {
                frame_rate: FrameRate::from_whole_frames_per_second(FRAMES_PER_SECOND).unwrap(),
                total_frame_count: total_frames,
                display_width_in_pixels: 1280,
                display_height_in_pixels: 720,
                has_audio: true,
            },
        })
        .unwrap();
    project
}

/// A window showing the editor, and a way to use it like a person.
pub(super) struct Window {
    pub(super) harness: Harness<'static, FastForwardApplication>,
}

impl Window {
    pub(super) fn showing(project: Project) -> Self {
        let mut application = FastForwardApplication::with_context(
            &egui::Context::default(),
            Err(ToolchainError::NotFound {
                places_searched: "nowhere (in a test)".to_owned(),
            }),
        );
        application.dialog = None; // the "ffmpeg was not found" message
        application.replace_project(project, None);
        application.source_status = SourceStatus::Available {
            path: PathBuf::from("holiday.mp4"),
        };
        let mut harness = Harness::builder()
            .with_size(vec2(WINDOW_SIZE[0], WINDOW_SIZE[1]))
            .build_ui_state(
                |ui, application: &mut FastForwardApplication| application.show(ui),
                application,
            );
        harness.run();
        Self { harness }
    }

    pub(super) fn application(&self) -> &FastForwardApplication {
        self.harness.state()
    }

    pub(super) fn application_mut(&mut self) -> &mut FastForwardApplication {
        self.harness.state_mut()
    }

    pub(super) fn settle(&mut self) {
        self.harness.run_steps(3);
    }

    pub(super) fn rectangle_of(&self, label: &str) -> Rect {
        self.harness.get_by_label(label).rect()
    }

    // --- the mouse ---

    pub(super) fn move_pointer_to(&mut self, position: Pos2) {
        self.harness.event(Event::PointerMoved(position));
        self.harness.step();
    }

    pub(super) fn press(&mut self, position: Pos2, button: PointerButton) {
        self.harness.event(Event::PointerButton {
            pos: position,
            button,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        self.harness.step();
    }

    pub(super) fn release(&mut self, position: Pos2, button: PointerButton) {
        self.harness.event(Event::PointerButton {
            pos: position,
            button,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        self.harness.step();
    }

    pub(super) fn click(&mut self, position: Pos2) {
        self.move_pointer_to(position);
        self.press(position, PointerButton::Primary);
        self.release(position, PointerButton::Primary);
        self.settle();
    }

    /// Presses `button` at `from`, moves in small steps to `to`, and lets go there.
    pub(super) fn drag(&mut self, from: Pos2, to: Pos2, button: PointerButton) {
        self.drag_and_hold(from, to, button);
        self.release(to, button);
        self.settle();
    }

    /// Like [`Self::drag`] but keeps the button down at the end.
    pub(super) fn drag_and_hold(&mut self, from: Pos2, to: Pos2, button: PointerButton) {
        self.move_pointer_to(from);
        self.press(from, button);
        let steps = 12;
        for step in 1..=steps {
            let fraction = step as f32 / steps as f32;
            self.move_pointer_to(from + (to - from) * fraction);
        }
    }

    // --- the keyboard ---

    pub(super) fn press_key(&mut self, key: Key) {
        self.harness.key_press(key);
        self.harness.step();
        self.settle();
    }

    // --- looking at the result ---

    pub(super) fn clips(&self) -> Vec<(u64, u64, u32)> {
        self.application()
            .project
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

    pub(super) fn current_frame(&self) -> u64 {
        let total = self.application().project.video_track().total_frame_count();
        self.application()
            .clock
            .position(std::time::Instant::now(), total)
            .value()
    }

    /// Where, on screen, the start of timeline frame `frame` is, assuming the video track has not
    /// been scrolled (true while the current frame is in the first screenful).
    pub(super) fn x_of_frame_on_the_track(&self, frame: u64) -> f32 {
        let geometry = TimelineGeometry::for_video(
            FrameRate::from_whole_frames_per_second(FRAMES_PER_SECOND).unwrap(),
            1280,
            720,
            theme::TRACK_HEIGHT,
        );
        self.rectangle_of("Video track").left() + geometry.x_of_frame(frame) as f32
    }

    pub(super) fn middle_of_the_track_at(&self, frame: u64) -> Pos2 {
        Pos2::new(
            self.x_of_frame_on_the_track(frame),
            self.rectangle_of("Video track").center().y,
        )
    }

    /// A point on the ruler just inside timeline frame `frame`, allowing for how far the strip has
    /// been scrolled.
    pub(super) fn point_on_the_ruler_at(&self, frame: u64) -> Pos2 {
        let ruler = self.rectangle_of("Ruler");
        let scroll_offset = self.application().timeline_view.scroll_offset as f32;
        Pos2::new(
            ruler.left() + self.x_of_frame_on_the_track(frame)
                - self.rectangle_of("Video track").left()
                - scroll_offset
                + 1.0,
            ruler.center().y,
        )
    }

    /// Makes the current frame `frame`, as if the playhead had been moved there.
    pub(super) fn move_current_frame_to(&mut self, frame: u64) {
        let total = self.application().project.video_track().total_frame_count();
        self.application_mut()
            .clock
            .seek(std::time::Instant::now(), TimelineFrameIndex::new(frame), total);
        self.settle();
    }

    /// Makes the cut a person makes: moves the playhead to `frame` and presses S.
    pub(super) fn cut_at_frame(&mut self, frame: u64) {
        self.move_current_frame_to(frame);
        self.press_key(Key::S);
    }
}
