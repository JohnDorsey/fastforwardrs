//! A person's view of the editor, for the integration tests that run whole journeys: the real
//! window drawn frame by frame (without a screen or a graphics card), used with simulated mouse
//! input, with a real ffmpeg behind it.
//!
//! Nothing here touches the real mouse or opens a file dialog. The things a file dialog would
//! choose are handed to the editor directly (see the "programmatic interface" of the application).
//!
//! A test file that uses this declares both `mod support;` and `mod journey;`.

#![allow(dead_code)] // Each test file uses a different part of this module.

pub mod speakers;

use std::path::Path;
use std::time::{Duration, Instant};

use egui::accesskit::{Action, ActionData, ActionRequest, Role};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use fastforwardrs::application::FastForwardApplication;
use fastforwardrs::media::FfmpegToolchain;
use fastforwardrs::project::{Clip, TimelineFrameIndex};

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

/// How long a frame of the window lasts on its own clock, unless it is asked to be different: a quarter of a second,
/// which is the harness's usual.
const DEFAULT_SECONDS_A_FRAME_LASTS: f32 = 0.25;

/// A clip as the test sees it: the first source frame, the source frame after the last, and the speed.
pub type ClipSummary = (u64, u64, u32);

/// The editor in a window, used like a person would.
pub struct Journey {
    pub harness: Harness<'static, FastForwardApplication>,
}

impl Journey {
    /// A window showing an editor with an empty project.
    pub fn start(toolchain: &FfmpegToolchain) -> Self {
        Self::start_with_frames_lasting(toolchain, DEFAULT_SECONDS_A_FRAME_LASTS)
    }

    /// Like [`Self::start`], with each frame of the window lasting `seconds` on the window's own clock, which is
    /// the one animations go by (not real time, which is as fast as the computer goes).
    pub fn start_with_frames_lasting(toolchain: &FfmpegToolchain, seconds: f32) -> Self {
        let application = FastForwardApplication::with_context(&egui::Context::default(), Ok(toolchain.clone()));
        let mut harness = Harness::builder()
            .with_size(vec2(WINDOW_WIDTH, WINDOW_HEIGHT))
            .with_step_dt(seconds)
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

    /// Imports the file at `path` (what choosing it in the Import dialog does) and waits until it
    /// is fully opened. It goes on the timeline at the current frame.
    pub fn import(&mut self, path: &Path) {
        self.application_mut().import_file_from(path.to_owned());
        self.wait_until_idle();
        assert!(
            !self.application().project().sources().is_empty() && self.application().every_source_file_is_available(),
            "the file could not be imported: {:?}",
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
            let wanted = self.the_video_in_front_at(application.current_frame());
            let shown = application
                .preview_picture()
                .map(|picture| (picture.source, picture.source_frame));
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

    /// Goes on drawing frames, as a window that is open does, for `duration` of real time.
    pub fn keep_drawing_for(&mut self, duration: Duration) {
        let started = Instant::now();
        while started.elapsed() < duration {
            self.harness.step();
            std::thread::sleep(Duration::from_millis(4));
        }
    }

    /// Draws a few frames, so that whatever the last action set going has been carried out.
    pub fn settle(&mut self) {
        self.harness.run_steps(3);
    }

    // --- looking ---

    /// The clips of the front-most track that come from a file, in order.
    pub fn clips(&self) -> Vec<ClipSummary> {
        self.clips_of_track(0)
    }

    /// The clips of the track at `position` in the list of tracks (0 is the front) that come from a
    /// file, in order.
    pub fn clips_of_track(&self, position: usize) -> Vec<ClipSummary> {
        self.application().project().tracks()[position]
            .placements()
            .filter_map(|placement| placement.clip.as_media())
            .map(|media| {
                (
                    media.first_source_frame().value(),
                    media.end_source_frame().value(),
                    media.speed().value(),
                )
            })
            .collect()
    }

    /// Where the clips of the track at `position` in the list of tracks (0 is the front) are on the
    /// timeline, in order: the first frame of each, and the frame after its last.
    pub fn timeline_spans_of_track(&self, position: usize) -> Vec<(u64, u64)> {
        self.application().project().tracks()[position]
            .placements()
            .map(|placement| {
                (
                    placement.first_timeline_frame.value(),
                    placement.end_timeline_frame.value(),
                )
            })
            .collect()
    }

    /// Which file, and which frame of it, the video that is in front shows at timeline frame `frame`.
    pub fn the_video_in_front_at(
        &self,
        frame: TimelineFrameIndex,
    ) -> Option<(
        fastforwardrs::project::SourceMediaIdentifier,
        fastforwardrs::project::SourceFrameIndex,
    )> {
        self.application().project().tracks().iter().find_map(|track| {
            let placement = track.placement_covering(frame)?;
            let Clip::Video(media) = placement.clip else {
                return None;
            };
            Some((media.source(), placement.source_frame_at(frame)?))
        })
    }

    /// The frame of its file that the video that is in front shows at timeline frame `frame`.
    pub fn source_frame_of_the_video_in_front_at(
        &self,
        frame: TimelineFrameIndex,
    ) -> Option<fastforwardrs::project::SourceFrameIndex> {
        self.the_video_in_front_at(frame).map(|(_, source_frame)| source_frame)
    }

    /// The timeline frame at which the picture that is `source_frame` of the file `source` is on
    /// screen, in the clips of the front-most track.
    pub fn timeline_frame_showing_source_frame(
        &self,
        source: fastforwardrs::project::SourceMediaIdentifier,
        source_frame: fastforwardrs::project::SourceFrameIndex,
    ) -> Option<TimelineFrameIndex> {
        self.application().project().tracks()[0]
            .placements()
            .find_map(|placement| {
                let media = placement.clip.as_media().filter(|media| media.source() == source)?;
                let offset = media.timeline_offset_of_source_frame(placement.effective_speed, source_frame)?;
                Some(TimelineFrameIndex::new(placement.first_timeline_frame.value() + offset))
            })
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
        self.click_with(position, PointerButton::Primary);
    }

    pub fn right_click(&mut self, position: Pos2) {
        self.click_with(position, PointerButton::Secondary);
    }

    /// Clicks with Ctrl held, which adds what is clicked to what is selected.
    pub fn click_with_control(&mut self, position: Pos2) {
        self.click_holding(position, Modifiers::COMMAND);
    }

    /// Clicks with Shift held, which opens a drawer over the rows under it instead of pushing them down.
    pub fn click_with_shift(&mut self, position: Pos2) {
        self.click_holding(position, Modifiers::SHIFT);
    }

    fn click_holding(&mut self, position: Pos2, modifiers: Modifiers) {
        self.harness.event(Event::ModifiersChanged(modifiers));
        self.harness.step();
        self.move_pointer_to(position);
        for pressed in [true, false] {
            self.harness.event(Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed,
                modifiers,
            });
            self.harness.step();
        }
        self.harness.event(Event::ModifiersChanged(Modifiers::NONE));
        self.harness.step();
        self.settle();
    }

    fn click_with(&mut self, position: Pos2, button: PointerButton) {
        self.move_pointer_to(position);
        self.press_or_release(position, button, true);
        self.press_or_release(position, button, false);
        self.settle();
    }

    /// Turns the mouse wheel by `delta` points (positive is up) with the pointer at `position` and
    /// `modifiers` held (Ctrl zooms the timeline), and lets the smooth scrolling that follows play out.
    pub fn scroll_the_wheel(&mut self, position: Pos2, delta: egui::Vec2, modifiers: Modifiers) {
        self.move_pointer_to(position);
        self.harness.event(Event::ModifiersChanged(modifiers));
        self.harness.step();
        self.harness.event(Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta,
            phase: egui::TouchPhase::Move,
            modifiers,
        });
        for _ in 0..31 {
            self.harness.step();
        }
        self.harness.event(Event::ModifiersChanged(Modifiers::NONE));
        self.harness.step();
        self.settle();
    }

    /// Where the boundary under the title box of the track at `position` is: a place to drag it
    /// from to make the track taller or shorter.
    pub fn boundary_under_the_track(&self, position: usize) -> Pos2 {
        let strip = self.strip_of_track(position);
        // Below the strip's gap's middle, at the left of the strips where the title box is.
        Pos2::new(strip.left() - 100.0, strip.bottom() + 3.0)
    }

    /// Presses `button` at `from`, moves in small steps to `to`, and lets go there.
    pub fn drag(&mut self, button: PointerButton, from: Pos2, to: Pos2) {
        self.drag_and_hold(button, from, to);
        self.release(button, to);
        self.settle();
    }

    /// Like [`Self::drag`], but keeps the button down at the end.
    pub fn drag_and_hold(&mut self, button: PointerButton, from: Pos2, to: Pos2) {
        self.move_pointer_to(from);
        self.press_or_release(from, button, true);
        let steps = 12;
        for step in 1..=steps {
            self.move_pointer_to(from + (to - from) * (step as f32 / steps as f32));
        }
    }

    /// Lets go of `button` with the pointer at `position`.
    pub fn release(&mut self, button: PointerButton, position: Pos2) {
        self.press_or_release(position, button, false);
    }

    /// Ticks (or unticks) a setting by its label in the global settings window, which is opened
    /// from the Edit menu.
    pub fn change_global_setting(&mut self, label: &str) {
        self.harness.get_by_label("Edit").click();
        self.settle();
        self.harness.get_by_label("Global settings").click();
        self.settle();
        self.harness.get_by_label(label).click();
        self.settle();
    }

    /// Opens the global settings window and, in the drop-down list that shows `shown`, picks `option`.
    pub fn choose_in_a_global_setting_dropdown(&mut self, shown: &str, option: &str) {
        self.choose_in_a_dropdown_of_the_window_opened_by("Global settings", shown, option);
    }

    /// Opens the project settings window and, in the drop-down list of frame rates, which shows
    /// `shown`, picks `option`.
    pub fn choose_the_frame_rate_of_the_project(&mut self, shown: &str, option: &str) {
        self.choose_in_a_dropdown_of_the_window_opened_by("Project settings", shown, option);
    }

    /// Opens the window that the item `menu_item` of the Edit menu opens and, in the drop-down list in it
    /// that shows `shown`, picks `option`.
    fn choose_in_a_dropdown_of_the_window_opened_by(&mut self, menu_item: &str, shown: &str, option: &str) {
        self.harness.get_by_label("Edit").click();
        self.settle();
        self.harness.get_by_label(menu_item).click();
        self.settle();
        self.harness.get_by_value(shown).click();
        self.settle();
        self.harness.get_by_label(option).click();
        self.settle();
    }

    /// Presses one of the buttons next to the time ("Pause", "Play" or "Stop").
    pub fn press_button(&mut self, label: &str) {
        self.harness.get_by_label(label).click();
        self.harness.step();
    }

    // --- the tracks and the slider ---

    /// The strip of the track at `position` in the list of tracks (0 is the front).
    pub fn strip_of_track(&self, position: usize) -> Rect {
        let title = self.application().project().tracks()[position].title().to_owned();
        self.rectangle_of(&format!("Strip of {title}"))
    }

    /// A point on the strip of the front-most track (the only one, while the project has only one)
    /// just inside timeline frame `frame`, which must be in view.
    pub fn point_on_the_track_at_frame(&self, frame: u64) -> Pos2 {
        self.point_on_the_strip_at_frame(0, frame)
    }

    /// A point on the strip of the track at `position` just inside timeline frame `frame`, which
    /// must be in view.
    pub fn point_on_the_strip_at_frame(&self, position: usize, frame: u64) -> Pos2 {
        let strip = self.strip_of_track(position);
        let offset = self.application().strip_offset_of_frame(TimelineFrameIndex::new(frame));
        let point = Pos2::new(strip.left() + offset + 1.0, strip.center().y);
        assert!(
            strip.contains(point),
            "frame {frame} is scrolled out of view: {point:?} is not in {strip:?}"
        );
        point
    }

    /// A point on the ruler above the tracks just inside timeline frame `frame`, which must be in view.
    pub fn point_on_the_ruler_at_frame(&self, frame: u64) -> Pos2 {
        let ruler = self.rectangle_of("Ruler");
        let offset = self.application().strip_offset_of_frame(TimelineFrameIndex::new(frame));
        let point = Pos2::new(ruler.left() + offset + 1.0, ruler.center().y);
        assert!(
            ruler.contains(point),
            "frame {frame} is scrolled out of view: {point:?} is not in {ruler:?}"
        );
        point
    }

    /// Whether timeline frame `frame` is in the part of the timeline that can be seen.
    pub fn frame_is_in_view(&self, frame: u64) -> bool {
        let ruler = self.rectangle_of("Ruler");
        let offset = self.application().strip_offset_of_frame(TimelineFrameIndex::new(frame));
        ruler.contains(Pos2::new(ruler.left() + offset + 1.0, ruler.center().y))
    }

    /// Moves the playhead to `frame` by clicking the ruler there. If that place is out of view the
    /// slider along the bottom is used first, to bring the view near it.
    pub fn move_playhead_to_frame(&mut self, frame: u64) {
        if !self.frame_is_in_view(frame) {
            let last_frame = self
                .application()
                .project()
                .total_frame_count()
                .saturating_sub(1)
                .max(1);
            self.click_the_scrub_slider_at((frame as f32 / last_frame as f32).clamp(0.0, 1.0));
        }
        let point = self.point_on_the_ruler_at_frame(frame);
        self.click(point);
    }

    /// Presses and lets go of a key, and draws a few frames.
    pub fn press_key(&mut self, key: Key) {
        self.harness.key_press(key);
        self.harness.step();
        self.settle();
    }

    /// Presses and lets go of a key with `modifiers` held down, and draws a few frames.
    pub fn press_key_with(&mut self, modifiers: Modifiers, key: Key) {
        self.harness.key_press_modifiers(modifiers, key);
        self.harness.step();
        self.settle();
    }

    /// Cuts at `frame` the way a person does: puts the playhead there (by clicking the ruler) and
    /// presses S.
    pub fn cut_at_frame(&mut self, frame: u64) {
        self.move_playhead_to_frame(frame);
        self.press_key(Key::S);
    }

    /// Clicks the text field that shows `shown` and types `text` into it in place of what it has.
    pub fn replace_text_in_the_field_showing(&mut self, shown: &str, text: &str) {
        let field = |journey: &Self| {
            [Role::TextInput, Role::MultilineTextInput]
                .into_iter()
                .flat_map(|role| journey.harness.query_all_by_role(role).collect::<Vec<_>>())
                .find(|node| node.value().as_deref() == Some(shown))
                .unwrap_or_else(|| panic!("no text field shows {shown:?}"))
                .rect()
        };
        let centre = field(self).center();
        self.click(centre);
        self.harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
        self.harness.step();
        self.harness.event(Event::Text(text.to_owned()));
        self.settle();
    }

    /// Sets a slider (found by its label) to `value`, the way a screen reader does.
    pub fn set_slider(&mut self, label: &str, value: f64) {
        let (target_node, target_tree) = self.harness.get_by_label(label).accesskit_node().locate();
        self.harness.event(Event::AccessKitActionRequest(ActionRequest {
            action: Action::SetValue,
            target_tree,
            target_node,
            data: Some(ActionData::NumericValue(value)),
        }));
        self.settle();
    }

    /// Presses the round button that adds a track.
    pub fn add_a_track(&mut self) {
        let button = self.rectangle_of("Add a track");
        self.click(button.center());
    }

    /// Makes a text object the way a person does: right-clicks the empty part of the strip of the
    /// track at `position` where timeline frame `frame` is, and chooses New, then Text object.
    pub fn make_a_text_object_at(&mut self, position: usize, frame: u64) {
        let point = self.point_on_the_strip_at_frame(position, frame);
        self.right_click(point);
        self.harness.get_by_label_contains("New").click();
        self.settle();
        self.harness.get_by_label("Text object").click();
        self.settle();
    }

    /// Selects the clip that is showing `frame` on the front-most track, by clicking it there: its
    /// properties are then in the properties panel.
    pub fn select_the_clip_at(&mut self, frame: u64) {
        let point = self.point_on_the_track_at_frame(frame);
        self.click(point);
    }

    /// Selects the track at `position` in the list of tracks (0 is the front), by clicking its box beside
    /// the title (the title itself is typed in when it is clicked).
    pub fn select_the_track_at(&mut self, position: usize) {
        let title = self.application().project().tracks()[position].title().to_owned();
        let field = self
            .harness
            .query_all_by_role(egui::accesskit::Role::TextInput)
            .find(|node| node.value().as_deref() == Some(title.as_str()))
            .unwrap_or_else(|| panic!("no text field shows {title:?}"))
            .rect();
        self.click(Pos2::new(field.left() - 4.0, field.center().y));
    }

    /// Sets the speed of the clip that is showing `frame` to `speed`: selects it, and sets the speed slider
    /// that the properties panel then has. (The clip stays selected.)
    pub fn set_the_speed_of_the_clip_at(&mut self, frame: u64, speed: u32) {
        self.select_the_clip_at(frame);
        self.set_slider("Speed", f64::from(speed));
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
