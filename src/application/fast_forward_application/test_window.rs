//! The window the interaction tests use, and the ways of using it: it shows the real editor with
//! a project made in memory (from a video that does not exist on disk, so that the tests need
//! neither ffmpeg nor a graphics card), and sends it the mouse and keyboard input a person would
//! (without ever touching the real mouse).

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use egui::accesskit::{Action, ActionData, ActionRequest, Role};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Shape, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use super::{FastForwardApplication, SourceFileState, SourceStatus};
use crate::application::commands::Command;
use crate::application::file_chooser::ScriptedFileChooser;
use crate::application::theme;
use crate::application::timeline_panel::{GripDotArrangement, VERTICES_IN_A_GRIP_DOT, strip_label};
use crate::media::ToolchainError;
use crate::playback::{SpeakersLog, speakers_that_only_keep_a_log};
use crate::project::{
    ClipId, FrameRate, Project, SourceAudioProperties, SourceMedia, SourceMediaIdentifier, SourceMediaLink,
    SourceVideoProperties, SpeedFactor, TimelineFrameIndex,
};

pub(super) const WINDOW_SIZE: [f32; 2] = [1280.0, 820.0];
pub(super) const FRAMES_PER_SECOND: u32 = 30;

/// What the window of the tests puts round the whole of what the editor draws, in points (the real window does
/// not: the panels are 6 points from its edges, which are these edges' 6 points more than this).
pub(super) const MARGIN_THE_TEST_WINDOW_HAS_ROUND_ITS_EDGES: f32 = 8.0;

/// The video that [`project_of`] is made from.
pub(super) const VIDEO: SourceMediaIdentifier = SourceMediaIdentifier::new(1);

/// The sound file that [`sound_source`] is.
pub(super) const SOUND: SourceMediaIdentifier = SourceMediaIdentifier::new(2);

/// A video of `total_frames` frames that is 1280x720 at 30 frames per second, with sound.
pub(super) fn video_source(total_frames: u64) -> SourceMedia {
    video_source_shown_at(total_frames, 1280, 720)
}

/// A video like [`video_source`] that is shown `width` by `height` pixels.
pub(super) fn video_source_shown_at(total_frames: u64, width: u32, height: u32) -> SourceMedia {
    SourceMedia::video(
        VIDEO,
        SourceMediaLink::from_absolute_path(PathBuf::from("holiday.mp4")),
        SourceVideoProperties {
            frame_rate: FrameRate::from_whole_frames_per_second(FRAMES_PER_SECOND).unwrap(),
            total_frame_count: total_frames,
            display_width_in_pixels: width,
            display_height_in_pixels: height,
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

/// A project of `total_frames` frames from a video that is 720x1280, standing up, at 30 frames per second,
/// as one clip on its one track: the picture is tall, and the preview has room at the sides of it.
pub(super) fn portrait_project_of(total_frames: u64) -> Project {
    let mut project = Project::empty();
    project
        .import_source(
            video_source_shown_at(total_frames, 720, 1280),
            TimelineFrameIndex::new(0),
        )
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

/// A window showing the editor with its one clip sped up four times, the playhead inside it.
pub(super) fn window_inside_a_sped_up_clip() -> Window {
    let mut window = Window::showing(project_of(300));
    let clip = window.clip(0, 0);
    window
        .application_mut()
        .project
        .set_clip_speed(clip, SpeedFactor::new(4).unwrap())
        .unwrap();
    let now = std::time::Instant::now();
    window
        .application_mut()
        .clock
        .seek(now, TimelineFrameIndex::new(10), 75);
    window.settle();
    window
}

/// What the window is told when `button` goes down with the pointer at `position`.
pub(super) fn button_pressed_at(position: Pos2, button: PointerButton) -> Event {
    Event::PointerButton {
        pos: position,
        button,
        pressed: true,
        modifiers: Modifiers::NONE,
    }
}

/// What the window is told when `button` comes up with the pointer at `position`.
pub(super) fn button_released_at(position: Pos2, button: PointerButton) -> Event {
    Event::PointerButton {
        pos: position,
        button,
        pressed: false,
        modifiers: Modifiers::NONE,
    }
}

/// Makes `project` the project of `application`, all of whose source files are there and can be used.
fn replace_project_with_all_its_files_available(application: &mut FastForwardApplication, project: Project) {
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
}

/// A window showing the editor, and a way to use it like a person.
pub(super) struct Window {
    pub(super) harness: Harness<'static, FastForwardApplication>,
    /// How many pictures the frames of the window have put on the graphics card, or changed there, so far (see
    /// [`Self::textures_uploaded_while`]).
    textures_uploaded: Arc<AtomicUsize>,
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
        replace_project_with_all_its_files_available(&mut application, project);
        let mut harness = Harness::builder()
            .with_size(vec2(WINDOW_SIZE[0], WINDOW_SIZE[1]))
            // A frame is a sixtieth of a second long, as on a screen, so that the two clicks of a
            // double click (which have a frame to themselves each) are as close together as a hand makes them.
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                |ui, application: &mut FastForwardApplication| application.show(ui),
                application,
            );
        // (The harness has no graphics card to give the pictures to, so they are counted at the end of each frame, and not given
        // to it.)
        let textures_uploaded = Arc::new(AtomicUsize::new(0));
        harness.ctx.on_end_pass("count the pictures put on the graphics card", {
            let textures_uploaded = Arc::clone(&textures_uploaded);
            Arc::new(move |ui: &mut egui::Ui| {
                let mut changes = ui.ctx().tex_manager().write().take_delta();
                textures_uploaded.fetch_add(changes.set.len(), Ordering::Relaxed);
                changes.clear();
            })
        });
        harness.run();
        let mut window = Self {
            harness,
            textures_uploaded,
        };
        window.settle();
        window
    }

    /// How many pictures were put on the graphics card, or changed there, in the frames drawn while `what` was done to the window.
    /// (A picture that is changed twice in one frame counts once: it is only sent as it is at the end of the frame. A window that is
    /// not changing is meant to put none: what is on the graphics card stays as it is.)
    pub(super) fn textures_uploaded_while(&mut self, what: impl FnOnce(&mut Self)) -> usize {
        self.textures_uploaded.store(0, Ordering::Relaxed);
        what(self);
        self.textures_uploaded.load(Ordering::Relaxed)
    }

    /// Shows another project in the window, as loading one does, all of whose source files are there and can be used.
    pub(super) fn show_another_project(&mut self, project: Project) {
        replace_project_with_all_its_files_available(self.application_mut(), project);
        self.settle();
    }

    pub(super) fn application(&self) -> &FastForwardApplication {
        self.harness.state()
    }

    pub(super) fn application_mut(&mut self) -> &mut FastForwardApplication {
        self.harness.state_mut()
    }

    /// Has the editor play its own little sounds (the snip of a cut) through speakers that make no sound and keep
    /// a log of what they were asked to play, which is what this gives.
    pub(super) fn log_the_sound_effects(&mut self) -> Rc<SpeakersLog> {
        let (opener, log) = speakers_that_only_keep_a_log(48000);
        self.application_mut().play_effects_through(opener);
        log
    }

    /// Makes the editor ask `chooser` for files, in place of the one that fails when it is asked.
    pub(super) fn use_file_chooser(&mut self, chooser: &ScriptedFileChooser) {
        self.application_mut().file_chooser = Box::new(chooser.clone());
    }

    /// Lets a few frames of the window go by, and then as many more as it takes for everything to
    /// stop moving. (The timeline panel grows and shrinks over a few frames when what is in it
    /// changes size, as when a track is added, and the things in it move with it.)
    pub(super) fn settle(&mut self) {
        const MOST_EXTRA_FRAMES_TO_WAIT: usize = 12;
        self.harness.run_steps(3);
        for _ in 0..MOST_EXTRA_FRAMES_TO_WAIT {
            let before = self.harness.query_by_label("Ruler").map(|ruler| ruler.rect());
            self.harness.step();
            if before == self.harness.query_by_label("Ruler").map(|ruler| ruler.rect()) {
                break;
            }
        }
    }

    pub(super) fn rectangle_of(&self, label: &str) -> Rect {
        self.harness.get_by_label(label).rect()
    }

    // --- the panels ---

    /// The rectangles drawn in the colour of panels that are big enough to be panels (and not the menu bar).
    pub(super) fn panels(&self) -> Vec<Rect> {
        self.harness
            .output()
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                Shape::Rect(rectangle)
                    if rectangle.fill == theme::PANEL_BACKGROUND
                        && rectangle.rect.width() > 200.0
                        && rectangle.rect.height() > 100.0 =>
                {
                    Some(rectangle.rect)
                }
                _ => None,
            })
            .collect()
    }

    /// What the timeline is drawn on: the panel along the bottom, which is the widest.
    pub(super) fn the_timeline_panel(&self) -> Rect {
        *self
            .panels()
            .iter()
            .max_by(|first, second| first.width().total_cmp(&second.width()))
            .expect("the panels are drawn")
    }

    /// The two panels above the timeline, the one at the left first: the properties and the preview.
    fn the_panels_above_the_timeline(&self) -> [Rect; 2] {
        let timeline = self.the_timeline_panel();
        let mut above: Vec<Rect> = self.panels().into_iter().filter(|panel| *panel != timeline).collect();
        above.sort_by(|first, second| first.left().total_cmp(&second.left()));
        above.try_into().expect("two panels are above the timeline")
    }

    /// What the properties are drawn on: the left of the two panels above the timeline.
    pub(super) fn the_properties_panel(&self) -> Rect {
        self.the_panels_above_the_timeline()[0]
    }

    /// What the preview is drawn on: the right of the two panels above the timeline.
    pub(super) fn the_preview_panel(&self) -> Rect {
        self.the_panels_above_the_timeline()[1]
    }

    // --- the mouse ---

    pub(super) fn move_pointer_to(&mut self, position: Pos2) {
        self.harness.event(Event::PointerMoved(position));
        self.harness.step();
    }

    pub(super) fn press(&mut self, position: Pos2, button: PointerButton) {
        self.harness.event(button_pressed_at(position, button));
        self.harness.step();
    }

    pub(super) fn release(&mut self, position: Pos2, button: PointerButton) {
        self.harness.event(button_released_at(position, button));
        self.harness.step();
    }

    /// Gives the window all of `events` in a single frame, as a real window gets what happened
    /// between two frames: the tap of a finger on a touch screen, or a click right after the pointer
    /// was moved, has the move and the press in the same frame. (`move_pointer_to`, `press` and the
    /// rest give each event a frame of its own, and so does `harness.event` followed by `step`.)
    pub(super) fn all_in_one_frame(&mut self, events: impl IntoIterator<Item = Event>) {
        self.harness.input_mut().events.extend(events);
        self.harness.step();
    }

    pub(super) fn click(&mut self, position: Pos2) {
        self.click_with(position, PointerButton::Primary);
    }

    pub(super) fn right_click(&mut self, position: Pos2) {
        self.click_with(position, PointerButton::Secondary);
    }

    /// Clicks with the control key (Command on a Mac) held down.
    pub(super) fn click_with_control(&mut self, position: Pos2) {
        self.click_holding(position, Modifiers::COMMAND);
    }

    /// Clicks with the shift key held down.
    pub(super) fn click_with_shift(&mut self, position: Pos2) {
        self.click_holding(position, Modifiers::SHIFT);
    }

    /// Holds the shift key down, until [`Self::let_go_of_shift`].
    pub(super) fn hold_shift(&mut self) {
        self.harness.event(Event::ModifiersChanged(Modifiers::SHIFT));
        self.harness.step();
    }

    pub(super) fn let_go_of_shift(&mut self) {
        self.harness.event(Event::ModifiersChanged(Modifiers::NONE));
        self.harness.step();
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
        self.press(position, button);
        self.release(position, button);
        self.settle();
    }

    /// Turns the mouse wheel by `delta` points (negative is down) with the pointer at `position`
    /// and `modifiers` held, and lets the smooth scrolling that follows play out.
    pub(super) fn scroll_the_wheel(&mut self, position: Pos2, delta: egui::Vec2, modifiers: Modifiers) {
        self.move_pointer_to(position);
        // The keys are held for as long as the wheel is turned, which is for as long as the smooth
        // scrolling that one turn of it makes lasts.
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

    /// Presses `key` after a click on the preview's empty place, which takes the keyboard away from whatever
    /// text was being typed in (the keys are the text's while it is), so that the key is the window's.
    pub(super) fn press_key_outside_the_text(&mut self, key: Key) {
        self.click(Pos2::new(WINDOW_SIZE[0] - 20.0, 120.0));
        self.press_key(key);
    }

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
    pub(super) fn clip(&self, position: usize, index: usize) -> ClipId {
        let track = &self.application().project.tracks()[position];
        track
            .placement_at_index(index)
            .expect("the track has that many clips")
            .id
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

    /// Selects the track at `position` by clicking its box beside the title, which is typed in when it is
    /// clicked (the box has the same room again round the title).
    pub(super) fn select_the_track_at(&mut self, position: usize) {
        let title = self.title_box_of_track(position);
        self.click(Pos2::new(title.left() - 4.0, title.center().y));
    }

    /// The box with the title of the track at `position` in it.
    pub(super) fn title_box_of_track(&self, position: usize) -> Rect {
        self.text_field_showing(&self.title_of_track(position)).rect()
    }

    /// The whole box at the left of the track at `position`: the title in it has the same room again at each side, and
    /// it is as tall as the strip.
    pub(super) fn box_of_track(&self, position: usize) -> Rect {
        let (title, strip) = (self.title_box_of_track(position), self.strip_of_track(position));
        Rect::from_min_max(
            Pos2::new(title.left() - 8.0, strip.top()),
            Pos2::new(title.right() + 8.0, strip.bottom()),
        )
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

    /// Whether the window asked, when it drew the last frame, to be drawn again at once, as it does while
    /// something is moving (and does not while all is still).
    pub(super) fn repaint_is_wanted_at_once(&self) -> bool {
        self.harness
            .output()
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .is_some_and(|viewport| viewport.repaint_delay.is_zero())
    }

    /// Every shape that was drawn in the last frame, in the order they were drawn. (Shapes that
    /// were grouped are listed one by one.)
    pub(super) fn shapes_drawn(&self) -> Vec<&Shape> {
        self.clipped_shapes_drawn()
            .into_iter()
            .map(|(_, shape)| shape)
            .collect()
    }

    /// Like [`Self::shapes_drawn`], with the rectangle each was cut off at when it was drawn.
    pub(super) fn clipped_shapes_drawn(&self) -> Vec<(Rect, &Shape)> {
        fn add_to<'a>(clip_rect: Rect, shape: &'a Shape, shapes: &mut Vec<(Rect, &'a Shape)>) {
            match shape {
                Shape::Vec(inner) => inner.iter().for_each(|shape| add_to(clip_rect, shape, shapes)),
                other => shapes.push((clip_rect, other)),
            }
        }
        let mut shapes = Vec::new();
        for clipped in &self.harness.output().shapes {
            add_to(clipped.clip_rect, &clipped.shape, &mut shapes);
        }
        shapes
    }

    /// The rectangle round each set of grip dots of `arrangement` that was drawn in the last frame, in the order they
    /// were drawn. (Told by how many dots, which is how many vertices, they have: two arrangements that have as many
    /// dots are not told apart.)
    pub(super) fn grip_dots_drawn(&self, arrangement: GripDotArrangement) -> Vec<Rect> {
        self.shapes_drawn()
            .into_iter()
            .filter_map(|shape| match shape {
                Shape::Mesh(mesh) if mesh.vertices.len() == arrangement.count() * VERTICES_IN_A_GRIP_DOT => Some(
                    Rect::from_points(&mesh.vertices.iter().map(|vertex| vertex.pos).collect::<Vec<_>>()),
                ),
                _ => None,
            })
            .collect()
    }

    /// Where on the screen the red line of the playhead was drawn in the last frame (as the x it is
    /// at), or `None` if it was not drawn, which is when the current frame is scrolled out of view.
    pub(super) fn x_of_the_drawn_playhead(&self) -> Option<f32> {
        self.shapes_drawn().into_iter().find_map(|shape| match shape {
            Shape::LineSegment { points, stroke } if stroke.color == theme::PLAYHEAD => Some(points[0].x),
            _ => None,
        })
    }

    /// Does what `command` asks, as the window does with the commands that what a person does in it
    /// gives (they are one edit of the history, if they change the project), and lets the window
    /// settle.
    pub(super) fn do_this(&mut self, command: Command) {
        self.application_mut()
            .execute_all(vec![command], std::time::Instant::now(), &egui::Context::default());
        self.settle();
    }

    /// Does what `command` asks, like [`Self::do_this`], and draws no frame after it: for looking at what the command did by itself,
    /// before anything that the window does every frame has had its turn.
    pub(super) fn do_this_and_draw_nothing(&mut self, command: Command) {
        self.application_mut()
            .execute_all(vec![command], std::time::Instant::now(), &egui::Context::default());
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
