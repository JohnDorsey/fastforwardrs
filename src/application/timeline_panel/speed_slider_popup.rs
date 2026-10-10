//! The pop-up speed slider: pressing the right mouse button on a clip and dragging up brings up a
//! vertical slider that sets how many times faster the clip plays. The speed is set when the
//! button is let go.

use egui::{Align2, FontId, Id, Key, LayerId, Order, PointerButton, Pos2, Rect, Response, Stroke, Ui, pos2};

use super::{TimelineContent, TimelineViewState};
use crate::application::commands::Command;
use crate::application::speed_slider::{SpeedSliderDrag, SpeedSliderScale};
use crate::application::theme;
use crate::application::timeline_geometry::TimelineGeometry;
use crate::project::SpeedFactor;

/// The speeds that are marked on the pop-up slider.
const MARKED_SPEEDS: [u32; 13] = [1, 2, 3, 5, 10, 20, 30, 50, 100, 200, 300, 500, 1000];

/// How far the pop-up slider keeps from the top of the window.
const SPEED_SLIDER_MARGIN: f32 = 16.0;

/// A speed slider that is being dragged.
pub(super) struct ActiveSpeedSlider {
    clip_index: usize,
    drag: SpeedSliderDrag,
    /// Where the right mouse button went down.
    origin: Pos2,
    /// Where the bottom (normal speed) end of the slider is on the screen.
    bottom_y: f32,
}

/// Starts, follows and finishes the drag of the right mouse button that sets a clip's speed.
pub(super) fn handle(
    ui: &Ui,
    response: &Response,
    content: &TimelineContent<'_>,
    geometry: TimelineGeometry,
    state: &mut TimelineViewState,
    commands: &mut Vec<Command>,
    to_content_x: impl Fn(f32) -> f64,
) {
    let track = content.project.video_track();
    if response.drag_started_by(PointerButton::Secondary) {
        // The slider is anchored where the button went down, not where the drag was noticed.
        let origin = ui
            .input(|input| input.pointer.press_origin())
            .or_else(|| response.interact_pointer_pos());
        let clicked_clip = origin.and_then(|origin| {
            let frame = geometry.frame_at_x(to_content_x(origin.x), track.total_frame_count())?;
            let location = track.locate(frame)?;
            Some((
                origin,
                location.clip_index,
                track.clips()[location.clip_index].speed_factor(),
            ))
        });
        if let Some((origin, clip_index, speed_factor)) = clicked_clip {
            let room_above = origin.y - ui.ctx().content_rect().top() - SPEED_SLIDER_MARGIN;
            let scale = SpeedSliderScale::fitting(room_above);
            state.speed_slider = Some(ActiveSpeedSlider {
                clip_index,
                drag: SpeedSliderDrag::begin(scale, speed_factor, origin.y),
                origin,
                bottom_y: origin.y + scale.height_of(speed_factor),
            });
        }
    }

    let Some(slider) = state.speed_slider.as_mut() else {
        return;
    };
    if let Some(pointer) = ui.input(|input| input.pointer.latest_pos()) {
        slider.drag.move_pointer(pointer.y);
        let (up_presses, down_presses, wheel) = ui.input(|input| {
            (
                input.num_presses(Key::ArrowUp),
                input.num_presses(Key::ArrowDown),
                input.smooth_scroll_delta.y,
            )
        });
        let nudge = up_presses as i32 - down_presses as i32 + wheel_steps(wheel);
        if nudge != 0 {
            slider.drag.nudge(nudge, pointer.y);
        }
    }
    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);

    if response.drag_stopped_by(PointerButton::Secondary) {
        let chosen = slider.drag.current();
        let clip_index = slider.clip_index;
        state.speed_slider = None;
        if track
            .clips()
            .get(clip_index)
            .is_some_and(|clip| clip.speed_factor() != chosen)
        {
            commands.push(Command::SetClipSpeed {
                clip_index,
                speed_factor: chosen,
            });
        }
    }
}

/// How many speed steps a mouse wheel movement of `scroll_points` counts as while the slider is up.
fn wheel_steps(scroll_points: f32) -> i32 {
    // About one step for every notch of a mouse wheel (which scrolls around 50 points a notch).
    (scroll_points / 50.0).round() as i32
}

/// Draws the pop-up slider above everything else.
pub(super) fn draw(ui: &Ui, slider: &ActiveSpeedSlider) {
    let painter = ui
        .ctx()
        .layer_painter(LayerId::new(Order::Tooltip, Id::new("speed slider")));
    let scale = slider.drag.scale();
    let x = slider.origin.x;
    let bottom_y = slider.bottom_y;
    let top_y = bottom_y - scale.full_height();

    let backdrop = Rect::from_min_max(pos2(x - 56.0, top_y - 14.0), pos2(x + 104.0, bottom_y + 14.0));
    painter.rect_filled(backdrop, 8.0, theme::SPEED_SLIDER_BACKGROUND);

    let groove = Rect::from_min_max(pos2(x - 3.0, top_y), pos2(x + 3.0, bottom_y));
    painter.rect_filled(groove, 3.0, theme::SLIDER_TRACK);

    for speed in MARKED_SPEEDS {
        let Some(factor) = SpeedFactor::new(speed) else {
            continue;
        };
        let y = bottom_y - scale.height_of(factor);
        painter.line_segment(
            [pos2(x - 12.0, y), pos2(x - 4.0, y)],
            Stroke::new(1.0, theme::SPEED_SLIDER_TICK),
        );
        painter.text(
            pos2(x - 16.0, y),
            Align2::RIGHT_CENTER,
            format!("{speed}x"),
            FontId::proportional(11.0),
            theme::SPEED_SLIDER_TICK,
        );
    }

    let knob_y = bottom_y - scale.height_of(slider.drag.current());
    let filled = Rect::from_min_max(pos2(x - 3.0, knob_y), pos2(x + 3.0, bottom_y));
    painter.rect_filled(filled, 3.0, theme::SLIDER_FILLED);
    painter.circle_filled(pos2(x, knob_y), 9.0, theme::SLIDER_KNOB);
    painter.text(
        pos2(x + 18.0, knob_y),
        Align2::LEFT_CENTER,
        slider.drag.current().to_string(),
        FontId::monospace(22.0),
        theme::SPEED_SLIDER_TEXT,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mouse_wheel_notch_is_one_step_in_either_direction() {
        assert_eq!(wheel_steps(50.0), 1);
        assert_eq!(wheel_steps(-50.0), -1);
        assert_eq!(wheel_steps(100.0), 2);
        assert_eq!(wheel_steps(0.0), 0);
        assert_eq!(wheel_steps(10.0), 0, "a tiny movement is not a step");
    }

    #[test]
    fn every_marked_speed_is_a_real_speed_the_slider_offers() {
        for speed in MARKED_SPEEDS {
            assert!(SpeedFactor::new(speed).is_some());
            assert!(speed <= crate::application::speed_slider::LARGEST_SPEED_FACTOR_ON_THE_SLIDER);
        }
        assert!(
            MARKED_SPEEDS.windows(2).all(|pair| pair[0] < pair[1]),
            "the marks are in order"
        );
    }
}
