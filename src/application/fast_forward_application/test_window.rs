//! The window the interaction tests use, and the ways of using it: it shows the real editor with
//! a project made in memory (from a video that does not exist on disk, so that the tests need
//! neither ffmpeg nor a graphics card), and sends it the mouse and keyboard input a person would
//! (without ever touching the real mouse).

use std::path::PathBuf;

use egui::accesskit::{Action, ActionData, ActionRequest, Role};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Shape, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use super::{FastForwardApplication, SourceFileState, SourceStatus};
use crate::application::theme;
use crate::application::timeline_panel::{gear_label, strip_label};
use crate::media::ToolchainError;
use crate::project::{
    ClipLocation, FrameRate, Project, SourceAudioProperties, SourceMedia, SourceMediaIdentifier, SourceMediaLink,
    SourceVideoProperties, TimelineFrameIndex,
};

pub(super) const WINDOW_SIZE: [f32; 2] = [1280.0, 820.0];
pub(super) const FRAMES_PER_SECOND: u32 = 30;

/// The video that [`project_of`] is made from.
pub(super) const VIDEO: SourceMediaIdentifier = SourceMediaIdentifier::new(1);

/// The sound file that [`sound_source`] is.
pub(super) const SOUND: SourceMediaIdentifier = SourceMediaIdentifier::new(2);

/// A video of `total_frames` frames that is 1280x720 at 30 frames per second, with sound.
pub(super) fn video_source(total_frames: u64) -> SourceMedia {
    SourceMedia::video(
        VIDEO,
        SourceMediaLink::from_absolute_path(PathBuf::from("holiday.mp4")),
        SourceVideoProperties {
            frame_rate: FrameRate::from_whole_frames_per_second(FRAMES_PER_SECOND).unwrap(),
            total_frame_count: total_frames,
            display_width_in_pixels: 1280,
            display_height_in_pixels: 720,
            has_audio: true,
        },
    )
}

/// A sound file that lasts `seconds` seconds.
pub(super) fn sound_source(seconds: u64) -> SourceMedia {
    SourceMedia::audio(
        SOUND,
        SourceMediaLink::from_absolute_path(PathBuf::from("song.mp3")),
        SourceAudioProperties {
            duration_in_microseconds: seconds * 1_000_000,
        },
    )
}

/// A project of `total_frames` frames from a video that is 1280x720 at 30 frames per second, as
/// one clip on its one track.
pub(super) fn project_of(total_frames: u64) -> Project {
    let mut project = Project::empty();
    project
        .import_source(video_source(total_frames), TimelineFrameIndex::new(0))
        .unwrap();
    project
}

/// A project of a video of `video_frames` frames cut into three clips of equal length that touch
/// one another, all on its one track.
pub(super) fn project_cut_into_three(video_frames: u64) -> Project {
    let mut project = project_of(video_frames);
    for third in 1..=2 {
        project.split_at(TimelineFrameIndex::new(video_frames / 3 * third), None);
    }
    project
}

/// A project with a video of `video_frames` frames on one track and, in front of it, a sound file
/// of `sound_seconds` seconds on another.
pub(super) fn project_with_a_video_and_a_sound(video_frames: u64, sound_seconds: u64) -> Project {
    let mut project = project_of(video_frames);
    let imported = project
        .import_source(sound_source(sound_seconds), TimelineFrameIndex::new(0))
        .unwrap();
    assert!(imported.created_track, "the sound goes on a track of its own, in front");
    project
}

/// A window showing the editor, and a way to use it like a person.
pub(super) struct Window {
    pub(super) harness: Harness<'static, FastForwardApplication>,
}

impl Window {
    /// A window showing `project`, all of whose source files are there and can be used.
    pub(super) fn showing(project: Project) -> Self {
        let mut application = FastForwardApplication::with_context(
            &egui::Context::default(),
            Err(ToolchainError::NotFound {
                places_searched: "nowhere (in a test)".to_owned(),
            }),
        );
        application.dialog = None; // the "ffmpeg was not found" message
        let sources: Vec<(SourceMediaIdentifier, PathBuf)> = project
            .sources()
            .iter()
            .map(|source| (source.identifier, source.link.absolute_path.clone()))
            .collect();
        application.replace_project(project, None);
        for (identifier, path) in sources {
            application
                .source_files
                .insert(identifier, SourceFileState::new(SourceStatus::Available { path }));
        }
        let mut harness = Harness::builder()
            .with_size(vec2(WINDOW_SIZE[0], WINDOW_SIZE[1]))
            // A frame is a sixtieth of a second long, as on a screen, so that the two clicks of a
            // double click (which have a frame to themselves each) are as close together as a hand makes them.
            .with_step_dt(1.0 / 60.0)
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
        self.click_with(position, PointerButton::Primary);
    }

    pub(super) fn right_click(&mut self, position: Pos2) {
        self.click_with(position, PointerButton::Secondary);
    }

    /// Clicks twice in a row, quickly enough to be a double click.
    pub(super) fn double_click(&mut self, position: Pos2) {
        self.move_pointer_to(position);
        for _ in 0..2 {
            self.press(position, PointerButton::Primary);
            self.release(position, PointerButton::Primary);
        }
        self.settle();
    }

    fn click_with(&mut self, position: Pos2, button: PointerButton) {
        self.move_pointer_to(position);
        self.press(position, button);
        self.release(position, button);
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

    /// Sets a slider (found by its label) to `value`, the way a screen reader does.
    pub(super) fn set_slider(&mut self, label: &str, value: f64) {
        let (target_node, target_tree) = self.harness.get_by_label(label).accesskit_node().locate();
        self.harness.event(Event::AccessKitActionRequest(ActionRequest {
            action: Action::SetValue,
            target_tree,
            target_node,
            data: Some(ActionData::NumericValue(value)),
        }));
        self.settle();
    }

    /// Whether the checkbox with this label is ticked.
    pub(super) fn is_checked(&self, label: &str) -> bool {
        self.harness.get_by_label(label).accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)
    }

    /// Clicks the text field that shows `shown` and types `text` into it in place of what it has.
    pub(super) fn replace_text_in_the_field_showing(&mut self, shown: &str, text: &str) {
        let field = self.text_field_showing(shown).rect().center();
        self.click(field);
        self.harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
        self.harness.step();
        self.text_field_showing(shown).type_text(text);
        self.settle();
    }

    // --- looking at the result ---

    /// The title of the track at `position` in the list of tracks (0 is the front).
    pub(super) fn title_of_track(&self, position: usize) -> String {
        self.application().project.tracks()[position].title().to_owned()
    }

    /// The clips of the track at `position` that come from a file, as the source frames they play
    /// and how fast.
    pub(super) fn clips_of_track(&self, position: usize) -> Vec<(u64, u64, u32)> {
        self.application().project.tracks()[position]
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

    /// The clips of the front-most track, which is the only one in a project of one track.
    pub(super) fn clips(&self) -> Vec<(u64, u64, u32)> {
        self.clips_of_track(0)
    }

    /// The `index`th clip of the track at `position`.
    pub(super) fn clip(&self, position: usize, index: usize) -> ClipLocation {
        let track = &self.application().project.tracks()[position];
        ClipLocation {
            track: track.id(),
            clip: track
                .placement_at_index(index)
                .expect("the track has that many clips")
                .id,
        }
    }

    pub(super) fn current_frame(&self) -> u64 {
        let total = self.application().project.total_frame_count();
        self.application()
            .clock
            .position(std::time::Instant::now(), total)
            .value()
    }

    /// The strip of the track at `position`.
    pub(super) fn strip_of_track(&self, position: usize) -> Rect {
        self.rectangle_of(&strip_label(&self.title_of_track(position)))
    }

    /// The text field (single line or several) that shows `shown`.
    pub(super) fn text_field_showing(&self, shown: &str) -> egui_kittest::Node<'_> {
        [Role::TextInput, Role::MultilineTextInput]
            .into_iter()
            .flat_map(|role| self.harness.query_all_by_role(role))
            .find(|node| node.value().as_deref() == Some(shown))
            .unwrap_or_else(|| panic!("no text field shows {shown:?}"))
    }

    /// The box with the title of the track at `position` in it.
    pub(super) fn title_box_of_track(&self, position: usize) -> Rect {
        self.text_field_showing(&self.title_of_track(position)).rect()
    }

    /// The settings gear of the track at `position`.
    pub(super) fn gear_of_track(&self, position: usize) -> Rect {
        self.rectangle_of(&gear_label(&self.title_of_track(position)))
    }

    /// Where, on screen, the start of timeline frame `frame` is along the strip of the track at
    /// `position`, allowing for how far the strips have been scrolled.
    pub(super) fn x_of_frame_on_the_strip(&self, position: usize, frame: u64) -> f32 {
        self.strip_of_track(position).left() + self.application().strip_offset_of_frame(TimelineFrameIndex::new(frame))
    }

    /// The point in the middle of the height of the strip of the track at `position`, at timeline
    /// frame `frame`.
    pub(super) fn middle_of_the_strip_at(&self, position: usize, frame: u64) -> Pos2 {
        Pos2::new(
            self.x_of_frame_on_the_strip(position, frame),
            self.strip_of_track(position).center().y,
        )
    }

    /// Like [`Self::middle_of_the_strip_at`] for the front-most track.
    pub(super) fn middle_of_the_track_at(&self, frame: u64) -> Pos2 {
        self.middle_of_the_strip_at(0, frame)
    }

    /// A point on the ruler just inside timeline frame `frame`, allowing for how far the strips
    /// have been scrolled.
    pub(super) fn point_on_the_ruler_at(&self, frame: u64) -> Pos2 {
        let ruler = self.rectangle_of("Ruler");
        Pos2::new(
            ruler.left() + self.application().strip_offset_of_frame(TimelineFrameIndex::new(frame)) + 1.0,
            ruler.center().y,
        )
    }

    /// Where on the screen the red line of the playhead was drawn in the last frame (as the x it is
    /// at), or `None` if it was not drawn, which is when the current frame is scrolled out of view.
    pub(super) fn x_of_the_drawn_playhead(&self) -> Option<f32> {
        fn red_line_in(shape: &Shape) -> Option<f32> {
            match shape {
                Shape::LineSegment { points, stroke } if stroke.color == theme::PLAYHEAD => Some(points[0].x),
                Shape::Vec(shapes) => shapes.iter().find_map(red_line_in),
                _ => None,
            }
        }
        self.harness
            .output()
            .shapes
            .iter()
            .find_map(|clipped| red_line_in(&clipped.shape))
    }

    /// Makes the current frame `frame`, as if the playhead had been moved there.
    pub(super) fn move_current_frame_to(&mut self, frame: u64) {
        let total = self.application().project.total_frame_count();
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
