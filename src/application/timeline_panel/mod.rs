//! The video track and the ruler above it.
//!
//! The track is a thick strip made of pictures from the video, one for every second, laid side by
//! side. It is wider than the window, and scrolls to keep the current frame in view. Above it is a
//! ruler with a mark for every second.
//!
//! * Clicking or dragging along the ruler moves the playhead there.
//! * Pressing the right mouse button on a clip and dragging up brings up a vertical slider that
//!   sets how many times faster the clip plays; the speed is set when the button is let go.
//! * The mouse wheel (or a trackpad) scrolls the strip without moving the current frame.
//!
//! Cutting the track is done with a key (see the `keyboard` module), not with the mouse.

mod clips;
mod playhead;
mod ruler;
mod speed_slider_popup;

use egui::{Sense, Ui, WidgetInfo, WidgetType, vec2};

use super::commands::Command;
use super::textures::Textures;
use super::theme;
use super::timeline_geometry::TimelineGeometry;
use crate::media::ThumbnailStrip;
use crate::project::{Project, TimelineFrameIndex};
use speed_slider_popup::ActiveSpeedSlider;

/// What the timeline remembers between frames of the window.
#[derive(Default)]
pub struct TimelineViewState {
    /// How far along the strip the left edge of the window is, in points.
    pub(in crate::application) scroll_offset: f64,
    /// The current frame the view last scrolled to follow.
    last_followed_frame: Option<TimelineFrameIndex>,
    /// The frame that was picked on the ruler during the previous frame of the window. The user
    /// chose that place in the view, so the view must not scroll to put it in the middle.
    frame_picked_on_the_ruler: Option<TimelineFrameIndex>,
    speed_slider: Option<ActiveSpeedSlider>,
}

impl TimelineViewState {
    /// Forgets the scroll position and any slider (when a different video is loaded).
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// What the timeline shows.
pub struct TimelineContent<'a> {
    pub project: &'a Project,
    /// The scale of the strip; `None` while the project has no video.
    pub geometry: Option<TimelineGeometry>,
    pub current_frame: TimelineFrameIndex,
    pub thumbnails: &'a ThumbnailStrip,
    /// Whether clicking and dragging may change the project or move the playhead.
    pub can_edit: bool,
}

/// Draws the ruler and the strip and adds a command for each thing the user does to them.
pub fn show(
    ui: &mut Ui,
    content: &TimelineContent<'_>,
    state: &mut TimelineViewState,
    textures: &mut Textures,
    commands: &mut Vec<Command>,
) {
    // The ruler sits right on top of the strip, with nothing between them.
    let ((ruler_response, ruler_painter), (track_response, track_painter)) = ui
        .scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let width = ui.available_width();
            let ruler = ui.allocate_painter(vec2(width, theme::RULER_HEIGHT), Sense::CLICK | Sense::DRAG);
            let track = ui.allocate_painter(vec2(width, theme::TRACK_HEIGHT), Sense::DRAG);
            (ruler, track)
        })
        .inner;
    ruler_response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, content.can_edit, "Ruler"));
    track_response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, content.can_edit, "Video track"));
    let ruler_rectangle = ruler_response.rect;
    let track_rectangle = track_response.rect;
    ruler::draw_background(&ruler_painter, ruler_rectangle);
    clips::draw_background(&track_painter, track_rectangle);

    if let Some(geometry) = content.geometry {
        let track = content.project.video_track();
        let total_frames = track.total_frame_count();
        let view_width = f64::from(track_rectangle.width());

        update_scroll_offset(
            ui,
            ruler_response.hovered() || track_response.hovered(),
            content,
            geometry,
            state,
            view_width,
        );
        let scroll_offset = state.scroll_offset;
        ruler::draw_ticks(&ruler_painter, geometry, total_frames, scroll_offset, ruler_rectangle);
        clips::draw_tiles(
            ui,
            &track_painter,
            content,
            geometry,
            scroll_offset,
            textures,
            track_rectangle,
        );
        clips::draw_clip_boundaries(&track_painter, geometry, track, scroll_offset, track_rectangle);

        if content.can_edit {
            if let Some(frame) = ruler::frame_picked(&ruler_response, geometry, total_frames, scroll_offset) {
                commands.push(Command::SeekTo(frame));
                state.frame_picked_on_the_ruler = Some(frame);
            }
            let to_content_x = |screen_x: f32| f64::from(screen_x - track_rectangle.left()) + scroll_offset;
            speed_slider_popup::handle(ui, &track_response, content, geometry, state, commands, to_content_x);
        }
    }
    clips::draw_border(&track_painter, track_rectangle);
    // The playhead goes over the border, so that it can be seen even on the very first frame.
    if let Some(geometry) = content.geometry {
        playhead::draw(
            &track_painter,
            geometry,
            content.current_frame,
            state.scroll_offset,
            ruler_rectangle,
            track_rectangle,
        );
    }

    if let Some(slider) = &state.speed_slider {
        speed_slider_popup::draw(ui, slider);
    }
}

/// Keeps the strip scrolled so that the current frame is in the middle of the window whenever the
/// current frame changes, and lets the mouse wheel scroll it in between.
///
/// The one exception is a frame the user has just picked on the ruler: they pointed at it where it
/// is, so the strip stays still under their pointer.
fn update_scroll_offset(
    ui: &Ui,
    is_hovered: bool,
    content: &TimelineContent<'_>,
    geometry: TimelineGeometry,
    state: &mut TimelineViewState,
    view_width: f64,
) {
    let total_frames = content.project.video_track().total_frame_count();
    let picked_on_the_ruler = state.frame_picked_on_the_ruler.take() == Some(content.current_frame);
    if state.last_followed_frame != Some(content.current_frame) {
        if !picked_on_the_ruler {
            state.scroll_offset = geometry.scroll_offset_to_centre(content.current_frame, view_width, total_frames);
        }
        state.last_followed_frame = Some(content.current_frame);
    }
    if is_hovered {
        let scrolled = ui.input(|input| input.smooth_scroll_delta);
        // Scrolling either way (a wheel up and down, or a trackpad sideways) moves the strip.
        state.scroll_offset -= f64::from(scrolled.x + scrolled.y);
    }
    state.scroll_offset = geometry.clamp_scroll_offset(state.scroll_offset, view_width, total_frames);
}
