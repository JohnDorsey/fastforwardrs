//! The timeline: a ruler, and below it a row for every track, the one in front at the top.
//!
//! A row has, at its left, a box with the track's title and its settings gear, and then the strip
//! of the track, which has the clips on it. The strips are wider than the window, and scroll
//! (together) to keep the current frame in view. Under the last row there is a round button that
//! adds a track. With more tracks than fit, the rows scroll up and down.
//!
//! * Clicking or dragging along the ruler moves the playhead there.
//! * Clicking a clip selects it. Right-clicking a clip opens a box above it with its settings.
//!   Right-clicking an empty part of a strip opens a menu to put something new there.
//! * The gear of a track opens a box above it with the settings of everything on the track.
//! * The mouse wheel (or a trackpad) scrolls the strips without moving the current frame.
//!
//! Cutting is done with a key (see the `keyboard` module), not with the mouse.

mod clips;
mod header;
mod icons;
mod playhead;
mod ruler;
mod settings_boxes;
mod strip;
mod strip_view;

use std::collections::HashMap;

use egui::scroll_area::{DragScroll, ScrollSource};
use egui::{Context, Pos2, Rect, ScrollArea, Sense, Ui, WidgetInfo, WidgetType, pos2, vec2};

pub use settings_boxes::SettingsBoxSubject;
// The names the parts of the timeline are known by to screen readers, which the tests look them up by.
#[cfg(test)]
pub use header::gear_label;
#[cfg(test)]
pub use strip::strip_label;

use self::settings_boxes::OpenSettingsBox;
use self::strip_view::StripView;
use super::bubble;
use super::commands::Command;
use super::textures::Textures;
use super::theme;
use super::timeline_geometry::TimelineGeometry;
use crate::media::{ThumbnailStrip, Waveform};
use crate::project::{ClipLocation, Project, SourceMediaIdentifier, TimelineFrameIndex, TrackId};

/// The name the button that adds a track is known by to screen readers (and the tests).
pub const ADD_TRACK_LABEL: &str = "Add a track";

/// The name the ruler is known by.
pub const RULER_LABEL: &str = "Ruler";

/// Where a click on an empty part of a strip happened, for the menu that puts something new there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NewMenuTarget {
    track: TrackId,
    frame: TimelineFrameIndex,
}

/// What the timeline remembers between frames of the window.
#[derive(Default)]
pub struct TimelineViewState {
    /// How far along the strips the left edge of the view is, in points.
    pub(in crate::application) scroll_offset: f64,
    /// The current frame the view last scrolled to follow.
    last_followed_frame: Option<TimelineFrameIndex>,
    /// The frame that was picked on the ruler during the previous frame of the window. The user
    /// chose that place in the view, so the view must not scroll to put it in the middle.
    frame_picked_on_the_ruler: Option<TimelineFrameIndex>,
    /// Where the ruler and the strips were in the last frame, so that the mouse wheel scrolls
    /// them only while the pointer is over them.
    strips_area_in_the_last_frame: Option<Rect>,
    /// The settings box that is open, if one is.
    settings_box: Option<OpenSettingsBox>,
    /// The box that the press of a mouse button that is still down closed, so that letting go of
    /// the button (which is a click on whatever opens that box) does not open it again at once.
    settings_box_closed_by_the_press: Option<SettingsBoxSubject>,
    /// Where the "New" menu was last opened.
    new_menu_target: Option<NewMenuTarget>,
}

impl TimelineViewState {
    /// Forgets the scroll position and any open box (when a different project is loaded).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Whether a settings box is open. (It has sliders and tick boxes of its own, which use the
    /// keys of the keyboard.)
    pub fn settings_box_is_open(&self) -> bool {
        self.settings_box.is_some()
    }

    /// What the open settings box is about, if one is open.
    pub fn settings_box_subject(&self) -> Option<SettingsBoxSubject> {
        self.settings_box.map(|open| open.subject)
    }

    /// Where the open settings box was drawn in the last frame, if one is open and has been drawn.
    #[cfg(test)]
    pub fn settings_box_rectangle(&self) -> Option<Rect> {
        self.settings_box.and_then(|open| open.rect)
    }

    /// Opens the settings box of `subject` (in place of any other), unless a press that is still
    /// going on has just closed it.
    fn open_settings_box(&mut self, subject: SettingsBoxSubject) {
        if self.settings_box_closed_by_the_press == Some(subject) {
            return;
        }
        self.settings_box = Some(OpenSettingsBox {
            subject,
            anchor: None,
            rect: None,
        });
    }

    fn close_settings_box(&mut self) {
        self.settings_box = None;
    }

    /// Says where the settings box of `subject` has to point, if that is the box that is open.
    fn point_settings_box_at(&mut self, subject: SettingsBoxSubject, anchor: Pos2) {
        if let Some(open) = self.settings_box.as_mut()
            && open.subject == subject
        {
            open.anchor = Some(anchor);
        }
    }

    /// Closes the open settings box if a mouse button was pressed outside it.
    fn close_settings_box_if_pressed_outside(&mut self, context: &Context) {
        if let Some(open) = self.settings_box
            && bubble::pressed_outside(context, open.rect)
        {
            self.settings_box_closed_by_the_press = Some(open.subject);
            self.settings_box = None;
        }
    }
}

/// What the timeline draws of a source file.
pub struct SourceLook<'a> {
    /// Whether the file can be used. The clips made from one that cannot are drawn as missing.
    pub is_available: bool,
    /// The pictures for a video's clips.
    pub thumbnails: Option<&'a ThumbnailStrip>,
    /// The shape of the sound of a sound file, for its clips.
    pub waveform: Option<&'a Waveform>,
    /// The file's name, to say which file a clip is from.
    pub file_name: String,
}

/// What the timeline shows.
pub struct TimelineContent<'a> {
    pub project: &'a Project,
    /// The scale of the strips.
    pub geometry: TimelineGeometry,
    pub current_frame: TimelineFrameIndex,
    pub selected_clip: Option<ClipLocation>,
    /// What to draw of each of the project's source files.
    pub source_looks: &'a HashMap<SourceMediaIdentifier, SourceLook<'a>>,
    /// Whether clicking and dragging may change the project or move the playhead.
    pub can_edit: bool,
}

/// How tall the rows can be together before they scroll, in points.
fn tallest_rows_area() -> f32 {
    let rows = theme::MOST_TRACKS_SHOWN_AT_ONCE as f32;
    rows * theme::TRACK_HEIGHT + (rows - 1.0) * theme::SPACE_BETWEEN_TRACKS
}

/// Draws the ruler, the rows and the button under them, and adds a command for each thing the
/// user does to them.
pub fn show(
    ui: &mut Ui,
    content: &TimelineContent<'_>,
    state: &mut TimelineViewState,
    textures: &mut Textures,
    commands: &mut Vec<Command>,
) {
    let context = ui.ctx().clone();
    state.close_settings_box_if_pressed_outside(&context);
    if let Some(open) = state.settings_box.as_mut() {
        // Where it points is worked out again, from where its subject is drawn in this frame.
        open.anchor = None;
    }

    let width = ui.available_width();
    let strips_offset = theme::TRACK_HEADER_WIDTH + theme::TRACK_HEADER_GAP;
    let strips_width = (width - strips_offset - theme::SCROLL_BAR_ROOM).max(1.0);
    let view_width = f64::from(strips_width);
    update_scroll_offset(ui, content, state, view_width);

    // The ruler sits right on top of the first row, with nothing between them.
    let previous_item_spacing = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = 0.0;
    let (ruler_area, _) = ui.allocate_exact_size(vec2(width, theme::RULER_HEIGHT), Sense::hover());
    let ruler_rectangle = Rect::from_min_size(
        pos2(ruler_area.left() + strips_offset, ruler_area.top()),
        vec2(strips_width, theme::RULER_HEIGHT),
    );
    let ruler_response = ui.interact(ruler_rectangle, ui.id().with("ruler"), Sense::CLICK | Sense::DRAG);
    ruler_response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, content.can_edit, RULER_LABEL));
    // The ruler, the strips and the playhead all show the same stretch of the timeline.
    let ruler_view = StripView::new(content.geometry, state.scroll_offset, ruler_rectangle);
    let ruler_painter = ui.painter_at(ruler_rectangle);
    ruler::draw_background(&ruler_painter, ruler_rectangle);
    let total_frames = content.project.total_frame_count();
    ruler::draw_ticks(&ruler_painter, &ruler_view, total_frames);
    if content.can_edit
        && let Some(frame) = ruler::frame_picked(&ruler_response, &ruler_view, total_frames)
    {
        commands.push(Command::SeekTo(frame));
        state.frame_picked_on_the_ruler = Some(frame);
    }

    // The scroll bar of the rows is a thin one that stays in sight (in the room left for it at the
    // right) while there are more rows than fit, so that it is clear that there are more.
    let rows = ui
        .scope(|ui| {
            let scroll_bar = &mut ui.spacing_mut().scroll;
            scroll_bar.dormant_handle_opacity = 0.55;
            scroll_bar.dormant_background_opacity = 0.25;
            scroll_bar.foreground_color = true;
            ScrollArea::vertical()
                .id_salt("timeline tracks")
                .max_height(tallest_rows_area())
                .auto_shrink([false, true])
                .scroll_source(ScrollSource {
                    scroll_bar: true,
                    drag: DragScroll::Never,
                    // The mouse wheel scrolls the strips along instead.
                    mouse_wheel: false,
                })
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = theme::SPACE_BETWEEN_TRACKS;
                    for track in content.project.tracks() {
                        let (row, _) =
                            ui.allocate_exact_size(vec2(ui.available_width(), theme::TRACK_HEIGHT), Sense::hover());
                        let header_rectangle =
                            Rect::from_min_size(row.min, vec2(theme::TRACK_HEADER_WIDTH, theme::TRACK_HEIGHT));
                        let strip_rectangle = Rect::from_min_size(
                            pos2(row.left() + strips_offset, row.top()),
                            vec2(strips_width, theme::TRACK_HEIGHT),
                        );
                        header::show(ui, header_rectangle, track, content.can_edit, state, commands);
                        let strip_view = ruler_view.shown_in(strip_rectangle);
                        strip::show(ui, &strip_view, track, content, state, textures, commands);
                    }
                })
        })
        .inner;
    ui.spacing_mut().item_spacing.y = previous_item_spacing;

    let strips_rectangle = Rect::from_min_max(
        pos2(ruler_rectangle.left(), ruler_rectangle.bottom()),
        pos2(ruler_rectangle.right(), rows.inner_rect.bottom()),
    );
    state.strips_area_in_the_last_frame = Some(ruler_rectangle.union(strips_rectangle));
    playhead::draw(
        ui.painter(),
        &ruler_view.shown_in(strips_rectangle),
        ruler_rectangle,
        content.current_frame,
    );

    show_add_track_button(ui, content.can_edit, width, commands);
    settings_boxes::show(&context, content, state, commands);
    if !context.input(|input| input.pointer.any_down()) {
        state.settings_box_closed_by_the_press = None;
    }
}

/// The big round button under the rows that adds a track, drawn under the title boxes.
fn show_add_track_button(ui: &mut Ui, can_edit: bool, width: f32, commands: &mut Vec<Command>) {
    let (footer, _) = ui.allocate_exact_size(
        vec2(width, theme::ADD_TRACK_BUTTON_DIAMETER + theme::SPACE_BETWEEN_TRACKS),
        Sense::hover(),
    );
    let button_rectangle = Rect::from_center_size(
        pos2(
            footer.left() + theme::TRACK_HEADER_WIDTH / 2.0,
            footer.bottom() - theme::ADD_TRACK_BUTTON_DIAMETER / 2.0,
        ),
        vec2(theme::ADD_TRACK_BUTTON_DIAMETER, theme::ADD_TRACK_BUTTON_DIAMETER),
    );
    // Not focusable, so that the keys of the keyboard keep their meanings as shortcuts of the window.
    let response = ui.interact(
        button_rectangle,
        ui.id().with("add track"),
        if can_edit { Sense::CLICK } else { Sense::hover() },
    );
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, can_edit, ADD_TRACK_LABEL));
    let painter = ui.painter();
    let fill = if !can_edit {
        theme::TRACK_HEADER_BACKGROUND
    } else if response.is_pointer_button_down_on() {
        theme::ICON_BUTTON_PRESSED
    } else if response.hovered() {
        theme::ICON_BUTTON_HOVER
    } else {
        theme::TRACK_HEADER_BACKGROUND
    };
    painter.circle_filled(button_rectangle.center(), theme::ADD_TRACK_BUTTON_DIAMETER / 2.0, fill);
    painter.circle_stroke(
        button_rectangle.center(),
        theme::ADD_TRACK_BUTTON_DIAMETER / 2.0,
        egui::Stroke::new(1.5, theme::TRACK_BORDER),
    );
    icons::draw_plus(
        painter,
        button_rectangle.center(),
        theme::ADD_TRACK_BUTTON_DIAMETER * 0.24,
        3.0,
        if can_edit { theme::ICON } else { theme::ICON_DISABLED },
    );
    if can_edit {
        if response.clicked() {
            commands.push(Command::AddTrack);
        }
        response.on_hover_text(ADD_TRACK_LABEL);
    }
}

/// Keeps the strips scrolled so that the current frame is in the middle of the window whenever the
/// current frame changes, and lets the mouse wheel scroll them in between.
///
/// The one exception is a frame the user has just picked on the ruler: they pointed at it where it
/// is, so the strips stay still under their pointer.
fn update_scroll_offset(ui: &Ui, content: &TimelineContent<'_>, state: &mut TimelineViewState, view_width: f64) {
    let total_frames = content.project.total_frame_count();
    let geometry = content.geometry;
    let picked_on_the_ruler = state.frame_picked_on_the_ruler.take() == Some(content.current_frame);
    if state.last_followed_frame != Some(content.current_frame) {
        if !picked_on_the_ruler {
            state.scroll_offset = geometry.scroll_offset_to_centre(content.current_frame, view_width, total_frames);
        }
        state.last_followed_frame = Some(content.current_frame);
    }
    let pointer_is_over_the_strips = state
        .strips_area_in_the_last_frame
        .zip(ui.input(|input| input.pointer.hover_pos()))
        .is_some_and(|(area, pointer)| area.contains(pointer));
    if pointer_is_over_the_strips {
        let scrolled = ui.input(|input| input.smooth_scroll_delta);
        // Scrolling either way (a wheel up and down, or a trackpad sideways) moves the strips.
        state.scroll_offset -= f64::from(scrolled.x + scrolled.y);
    }
    state.scroll_offset = geometry.clamp_scroll_offset(state.scroll_offset, view_width, total_frames);
}
