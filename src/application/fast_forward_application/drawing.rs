//! Laying out the window: the menu bar along the top, the preview in the middle, and along the
//! bottom the play buttons, the video track and the slider.

use egui::{Frame, Margin, Ui};

use super::{FastForwardApplication, FrameState, SourceStatus};
use crate::application::commands::Command;
use crate::application::menu_bar::{self, MenuAvailability};
use crate::application::preview_panel::{self, PreviewContent};
use crate::application::timeline_geometry::TimelineGeometry;
use crate::application::timeline_panel::{self, TimelineContent};
use crate::application::transport_bar::{self, TransportBarState};
use crate::application::{scrub_bar, theme};
use crate::media::thumbnail_second_for_source_frame;
use crate::playback::wanted_picture;

impl FastForwardApplication {
    /// Draws the whole window, adding to `commands` whatever the user asks for with it.
    pub(super) fn draw(&mut self, ui: &mut Ui, frame_state: &FrameState, commands: &mut Vec<Command>) {
        let availability = self.menu_availability();
        egui::Panel::top("menu bar")
            .frame(
                Frame::new()
                    .fill(theme::WINDOW_BACKGROUND)
                    .inner_margin(Margin::symmetric(4, 2)),
            )
            .show(ui, |ui| menu_bar::show(ui, &availability, commands));

        let geometry = self.timeline_geometry();
        let can_edit = !self.is_blocked();

        egui::Panel::bottom("timeline area")
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(theme::WINDOW_BACKGROUND)
                    .inner_margin(Margin::symmetric(8, 6)),
            )
            .show(ui, |ui| {
                transport_bar::show(
                    ui,
                    &TransportBarState {
                        is_playing: frame_state.is_playing,
                        has_something_to_play: frame_state.total_frames > 0,
                        current_frame: frame_state.current_frame,
                        frame_rate: frame_state.frame_rate,
                    },
                    commands,
                );
                ui.add_space(6.0);
                timeline_panel::show(
                    ui,
                    &TimelineContent {
                        project: &self.project,
                        geometry,
                        current_frame: frame_state.current_frame,
                        thumbnails: &self.thumbnails.strip,
                        can_edit,
                    },
                    &mut self.timeline_view,
                    &mut self.textures,
                    commands,
                );
                ui.add_space(2.0);
                scrub_bar::show(ui, frame_state.current_frame, frame_state.total_frames, commands);
            });

        // What the preview shows. The picture is the one for the current frame whenever it is ready.
        // While it is not, a playing video keeps its last picture (a sharp one that is a moment old
        // looks better than a blurry one), but a video that is being moved around shows the
        // enlarged thumbnail at once and swaps in the sharp picture when it arrives.
        let video_track = self.project.video_track();
        let wanted = wanted_picture(video_track, frame_state.current_frame);
        let placeholder_second =
            wanted.map(|wanted| thumbnail_second_for_source_frame(frame_state.frame_rate, wanted.source_frame));
        let thumbnail_is_ready =
            placeholder_second.is_some_and(|second| self.thumbnails.strip.picture_for_second(second).is_some());
        let supplied_picture = self.picture_supplier.picture();
        let (picture, placeholder_thumbnail_second) = if frame_state.preview_is_up_to_date {
            (supplied_picture, None)
        } else if frame_state.is_playing || !thumbnail_is_ready {
            (supplied_picture, placeholder_second)
        } else {
            (None, placeholder_second)
        };
        let accelerated_speed = video_track
            .locate(frame_state.current_frame)
            .map(|location| video_track.clips()[location.clip_index].speed_factor())
            .filter(|speed_factor| speed_factor.is_accelerated());

        let content = PreviewContent {
            video_display_size: self.project.source_videos().first().map(|video| {
                (
                    video.properties.display_width_in_pixels,
                    video.properties.display_height_in_pixels,
                )
            }),
            picture,
            placeholder_thumbnail_second,
            thumbnails: &self.thumbnails.strip,
            accelerated_speed,
            indicator_layout: self.project.speed_indicator_layout(),
        };
        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::PREVIEW_BACKGROUND))
            .show(ui, |ui| preview_panel::show(ui, &content, &mut self.textures, commands));
    }

    /// The scale of the video track, once the project has a video to take the shape of the pictures from.
    pub(super) fn timeline_geometry(&self) -> Option<TimelineGeometry> {
        self.project.source_videos().first().map(|video| {
            TimelineGeometry::for_video(
                video.properties.frame_rate,
                video.properties.display_width_in_pixels,
                video.properties.display_height_in_pixels,
                theme::TRACK_HEIGHT,
            )
        })
    }

    /// Which File menu items can be used right now.
    fn menu_availability(&self) -> MenuAvailability {
        let source_is_available = matches!(self.source_status, SourceStatus::Available { .. });
        MenuAvailability {
            can_save: !self.project.source_videos().is_empty(),
            can_import: matches!(
                self.source_status,
                SourceStatus::NoVideo | SourceStatus::Unavailable { .. }
            ),
            can_export: source_is_available && self.toolchain.is_some() && !self.project.video_track().is_empty(),
            busy: self.export.is_some() || self.opening.is_some() || self.dialog.is_some(),
        }
    }
}
