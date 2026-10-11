//! Laying out the window: the menu bar along the top, the preview in the middle, and along the
//! bottom the play buttons, the timeline and the slider.

use std::collections::HashMap;

use egui::{Frame, Margin, Ui};

use super::{FastForwardApplication, FrameState, SourceFileState};
use crate::application::commands::Command;
use crate::application::menu_bar::{self, MenuAvailability};
use crate::application::preview_panel::{self, PreviewContent, VideoInThePicture};
use crate::application::timeline_geometry::TimelineGeometry;
use crate::application::timeline_panel::{self, SourceLook, TimelineContent};
use crate::application::transport_bar::{self, TransportBarState};
use crate::application::{scrub_bar, theme};
use crate::composition::VisibleText;
use crate::media::thumbnail_second_for_source_frame;
use crate::playback::{FfmpegPictureStreamStarter, PictureSupplier};
use crate::project::{ClipLocation, Project, SourceMediaIdentifier, SourceMediaKind};

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
        let source_looks = source_looks_of(&self.project, &self.source_files);

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
                        selected_clip: self.selected_clip,
                        source_looks: &source_looks,
                        can_edit,
                    },
                    &mut self.timeline_view,
                    &mut self.textures,
                    commands,
                );
                ui.add_space(2.0);
                scrub_bar::show(ui, frame_state.current_frame, frame_state.total_frames, commands);
            });

        let content = preview_content(
            &self.project,
            &self.picture_supplier,
            self.selected_clip,
            frame_state,
            &source_looks,
        );
        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::PREVIEW_BACKGROUND))
            .show(ui, |ui| {
                preview_panel::show(ui, &content, &mut self.textures, &mut self.text_renderer, commands);
            });
    }

    /// The scale of the strips, from the shape of the finished picture.
    pub(super) fn timeline_geometry(&self) -> TimelineGeometry {
        let (picture_width, picture_height) = self.project.picture_size();
        TimelineGeometry::for_picture(
            self.project.frame_rate(),
            picture_width,
            picture_height,
            theme::TRACK_HEIGHT,
        )
    }

    /// Which menu items can be used right now.
    fn menu_availability(&self) -> MenuAvailability {
        MenuAvailability {
            can_save: !self.project.sources().is_empty() || !self.project.has_no_clips(),
            can_export: self.toolchain.is_some()
                && !self.project.has_no_clips()
                && self.sources_in_use_that_are_not_available().is_empty(),
            has_selection: self.selected_clip.is_some(),
            busy: self.export.is_some() || !self.openings.is_empty() || self.dialog.is_some(),
        }
    }
}

/// What the preview shows. The picture is the one for the current frame whenever it is ready.
/// While it is not, a playing video keeps its last picture (a sharp one that is a moment old
/// looks better than a blurry one), but a video that is being moved around shows the
/// enlarged thumbnail at once and swaps in the sharp picture when it arrives.
fn preview_content<'a>(
    project: &Project,
    picture_supplier: &'a PictureSupplier<FfmpegPictureStreamStarter>,
    selected_clip: Option<ClipLocation>,
    frame_state: &'a FrameState,
    source_looks: &HashMap<SourceMediaIdentifier, SourceLook<'a>>,
) -> PreviewContent<'a> {
    let section = frame_state.picture_plan.section_at(frame_state.current_frame);
    let texts: &[VisibleText] = section.map_or(&[], |section| section.texts.as_slice());

    let video = match &frame_state.wanted_picture {
        None => VideoInThePicture::None,
        Some(wanted) => {
            let supplied_picture = picture_supplier.picture();
            let thumbnail = source_looks
                .get(&wanted.source)
                .and_then(|look| look.thumbnails)
                .map(|strip| {
                    (
                        strip,
                        thumbnail_second_for_source_frame(frame_state.frame_rate, wanted.source_frame),
                    )
                })
                .filter(|(strip, second)| strip.picture_for_second(*second).is_some());
            let up_to_date = frame_state.preview_is_up_to_date;
            let moved_around = !up_to_date && !frame_state.is_playing;
            match (supplied_picture, thumbnail) {
                (_, Some((strip, second))) if moved_around => VideoInThePicture::Thumbnail {
                    source: wanted.source,
                    second,
                    strip,
                },
                (Some(picture), _) => VideoInThePicture::Picture(picture),
                (None, Some((strip, second))) => VideoInThePicture::Thumbnail {
                    source: wanted.source,
                    second,
                    strip,
                },
                (None, None) => VideoInThePicture::Waiting,
            }
        }
    };
    PreviewContent {
        picture_size: (frame_state.total_frames > 0).then(|| project.picture_size()),
        video,
        accelerated_speed: section
            .and_then(|section| section.video)
            .map(|video| video.speed)
            .filter(|speed_factor| speed_factor.is_accelerated()),
        indicator_layout: project.speed_indicator_layout(),
        texts,
        selected_clip,
    }
}

/// What the timeline draws of each source file: whether it can be used, and its pictures or waveform.
fn source_looks_of<'a>(
    project: &'a Project,
    source_files: &'a HashMap<SourceMediaIdentifier, SourceFileState>,
) -> HashMap<SourceMediaIdentifier, SourceLook<'a>> {
    project
        .sources()
        .iter()
        .map(|source| {
            let state = source_files.get(&source.identifier);
            let is_available = state.is_some_and(|state| state.path_if_available().is_some());
            let look = SourceLook {
                is_available,
                thumbnails: state
                    .filter(|_| source.kind() == SourceMediaKind::Video)
                    .map(|state| &state.thumbnails.strip),
                waveform: state
                    .filter(|_| source.kind() == SourceMediaKind::Audio)
                    .map(|state| &state.waveform.waveform),
                file_name: source
                    .link
                    .absolute_path
                    .file_name()
                    .map_or_else(|| "a file".to_owned(), |name| name.to_string_lossy().into_owned()),
            };
            (source.identifier, look)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use super::*;
    use crate::composition::plan_pictures;
    use crate::playback::wanted_picture;
    use crate::project::{
        Clip, FrameRate, MediaClip, SourceFrameIndex, SourceMedia, SourceMediaLink, SourceVideoProperties, SpeedFactor,
        SpeedIndicatorLayout, TimelineFrameIndex, Track, TrackId,
    };

    /// A project with one video clip of 600 frames at `clip_speed` on a track that plays at `track_speed`.
    fn project_of_a_sped_up_clip(clip_speed: u32, track_speed: u32) -> Project {
        let identifier = SourceMediaIdentifier::new(1);
        let source = SourceMedia::video(
            identifier,
            SourceMediaLink::from_absolute_path(PathBuf::from("holiday.mp4")),
            SourceVideoProperties {
                frame_rate: FrameRate::new(30, 1).unwrap(),
                total_frame_count: 600,
                display_width_in_pixels: 1280,
                display_height_in_pixels: 720,
                has_audio: false,
            },
        );
        let clip = Clip::Video(
            MediaClip::new(
                identifier,
                SourceFrameIndex::new(0),
                SourceFrameIndex::new(600),
                SpeedFactor::new(clip_speed).unwrap(),
            )
            .unwrap(),
        );
        let track = Track::new(TrackId::new(0), "track 0")
            .with_clips([(0, clip)])
            .with_speed(SpeedFactor::new(track_speed).unwrap());
        Project::from_parts(vec![source], vec![track], SpeedIndicatorLayout::DEFAULT).unwrap()
    }

    /// The speed the preview says the video plays at, with the playhead at the first frame.
    fn speed_shown_by_the_preview(project: &Project) -> Option<SpeedFactor> {
        let picture_plan = plan_pictures(project, |_| true);
        let current_frame = TimelineFrameIndex::new(0);
        let frame_state = FrameState {
            current_frame,
            total_frames: project.total_frame_count(),
            frame_rate: project.frame_rate(),
            is_playing: false,
            wanted_picture: wanted_picture(&picture_plan, current_frame),
            picture_plan,
            preview_is_up_to_date: false,
        };
        let supplier = PictureSupplier::new(FfmpegPictureStreamStarter::new(None, Arc::new(|| {})));
        preview_content(project, &supplier, None, &frame_state, &HashMap::new()).accelerated_speed
    }

    #[test]
    fn the_symbol_shows_how_fast_the_picture_really_plays_which_is_the_speed_of_the_clip_times_the_speed_of_its_track()
    {
        let shown =
            |clip_speed, track_speed| speed_shown_by_the_preview(&project_of_a_sped_up_clip(clip_speed, track_speed));
        assert_eq!(shown(1, 1), None, "normal speed has no symbol");
        assert_eq!(shown(4, 1), SpeedFactor::new(4));
        assert_eq!(
            shown(1, 5),
            SpeedFactor::new(5),
            "a sped-up track has the symbol though the clip is at normal speed"
        );
        assert_eq!(shown(3, 2), SpeedFactor::new(6), "the two are multiplied");
    }
}
