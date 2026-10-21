// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Laying out the window: the menu bar along the top, and under it the panels: the properties panel at the
//! left, the preview at the right of it, and along the bottom the timeline with the play buttons, the time and the
//! slider on one line over it.

use std::collections::HashMap;
use std::time::Instant;

use egui::{Frame, Margin, Ui};

use super::{FastForwardApplication, FrameState, SourceFileState};
use crate::application::commands::Command;
use crate::application::menu_bar::{self, MenuAvailability};
use crate::application::preview_panel::{self, HowTheClockGoes, PreviewContent, VideoInThePicture};
use crate::application::properties_panel::{self, PropertiesContent};
use crate::application::theme;
use crate::application::timeline_geometry::TimelineGeometry;
use crate::application::timeline_panel::{self, SourceLook, TimelineContent};
use crate::application::transport_bar::{self, TransportBarState};
use crate::composition::{Layer, PictureSection, VisibleVideo};
use crate::frame_scene::{
    colour_of_a_picture_file, colour_of_the_preview, groups_of_a_section, layers_of_a_section, symbol_of_a_section,
};
use crate::media::thumbnail_second_for_source_frame;
use crate::playback::{FfmpegPictureStreamStarter, PreviewPictures};
use crate::project::{ClipId, Project, SourceFrameIndex, SourceMediaIdentifier, SourceMediaKind, TrackHeight};

impl FastForwardApplication {
    /// Draws the whole window, adding to `commands` whatever the user asks for with it.
    ///
    /// The window is a menu bar along the top, and under it three panels with a gap of the darker
    /// background round each of them and between them: the timeline panel along the bottom, the
    /// properties panel at the left above it, and the preview panel, which is the rest.
    pub(super) fn draw(&mut self, ui: &mut Ui, frame_state: &FrameState, commands: &mut Vec<Command>) {
        let availability = self.menu_availability();
        self.find_out_the_fonts_when_a_text_object_is_selected();
        egui::Panel::top("menu bar")
            .show_separator_line(false)
            .frame(
                Frame::new()
                    .fill(theme::PANEL_BACKGROUND)
                    .inner_margin(Margin::symmetric(4, 2)),
            )
            .show(ui, |ui| menu_bar::show(ui, &availability, commands));

        let geometry = self.timeline_geometry();
        let can_edit = !self.is_blocked();
        let source_looks = source_looks_of(&self.document.project, &self.sources.files);
        let timeline_content = TimelineContent {
            project: &self.document.project,
            geometry,
            current_frame: frame_state.current_frame,
            selection: &self.interface.selection,
            source_looks: &source_looks,
            can_edit,
        };
        timeline_panel::prepare(ui.ctx(), &timeline_content, &mut self.interface.timeline_view);

        // (The gaps are made by the margins round the panels: half of a gap between two panels is round each
        // of them, and the whole of it between a panel and the edge of the window.)
        egui::Panel::bottom(timeline_panel::PANEL_ID)
            .resizable(false)
            .show_separator_line(false)
            .frame(panel_frame(Margin::symmetric(8, 6), PanelSides::AlongTheBottom))
            .show(ui, |ui| {
                transport_bar::show(
                    ui,
                    &TransportBarState {
                        is_playing: frame_state.is_playing,
                        current_frame: frame_state.current_frame,
                        total_frames: frame_state.total_frames,
                        frame_rate: frame_state.frame_rate,
                    },
                    commands,
                );
                ui.add_space(6.0);
                timeline_panel::show(
                    ui,
                    &timeline_content,
                    &mut self.interface.timeline_view,
                    &mut self.interface.textures,
                    commands,
                );
            });

        egui::Panel::left("properties panel")
            .resizable(false)
            .show_separator_line(false)
            .exact_size(theme::PROPERTIES_PANEL_WIDTH)
            .frame(panel_frame(Margin::same(10), PanelSides::AtTheLeft))
            .show(ui, |ui| {
                properties_panel::show(
                    ui,
                    &PropertiesContent {
                        project: &self.document.project,
                        selection: &self.interface.selection,
                        current_frame: frame_state.current_frame,
                        source_looks: &source_looks,
                        font_families: self.environment.font_families.as_deref(),
                        can_edit,
                        why_an_effect_does_nothing: &|kind| {
                            self.environment
                                .plugin_search
                                .why_a_plugin_effect_does_nothing(kind, &self.environment.plugin_host)
                        },
                    },
                    commands,
                );
            });

        let content = preview_content(
            &self.document.project,
            &self.playback.preview_pictures,
            self.interface.selection.single(),
            frame_state,
            &source_looks,
        );
        // (What the frames of the engine are put together in and encoded as comes from the settings, which are read each time: a changed setting is another frame.)
        let colour_of_the_frames = colour_of_the_preview(self.effective_settings());
        let preview = egui::CentralPanel::default()
            .frame(panel_frame(Margin::ZERO, PanelSides::AtTheRight))
            .show(ui, |ui| match &mut self.interface.engine_preview {
                Some(engine_preview) => {
                    engine_preview.set_the_colour(colour_of_the_frames);
                    preview_panel::show_drawn_by_the_engine(
                        ui,
                        &content,
                        &mut self.interface.preview_view,
                        engine_preview,
                        &mut self.environment.text_renderer,
                        HowTheClockGoes::new(frame_state.is_playing, frame_state.frame_rate, Instant::now()),
                        commands,
                    )
                }
                None => preview_panel::show(
                    ui,
                    &content,
                    &mut self.interface.preview_view,
                    &mut self.interface.textures,
                    &mut self.environment.text_renderer,
                    commands,
                ),
            });
        self.interface.preview_panel_in_the_last_frame = Some(preview.response.rect);
    }

    /// Looks for the names of the fonts the computer has once a text object is selected, which it
    /// takes a moment to do (the properties panel lists them to choose from when it has them).
    fn find_out_the_fonts_when_a_text_object_is_selected(&mut self) {
        if self.environment.font_families.is_some() {
            return;
        }
        let a_text_object_is_selected = self
            .interface
            .selection
            .single()
            .and_then(|clip| self.document.project.placement_of(clip))
            .is_some_and(|placement| placement.clip.as_text().is_some());
        if a_text_object_is_selected && let Some(renderer) = self.environment.text_renderer.get_if_ready() {
            self.environment.font_families = Some(renderer.font_family_names());
        }
    }

    /// The scale the strips start at, from the shape of the finished picture. (The user can zoom in
    /// and out from it: see [`crate::application::timeline_panel::TimelineViewState::geometry`].)
    pub(super) fn timeline_geometry(&self) -> TimelineGeometry {
        let (picture_width, picture_height) = self.document.project.picture_size();
        TimelineGeometry::for_picture(
            self.document.project.frame_rate(),
            picture_width,
            picture_height,
            TrackHeight::DEFAULT_IN_POINTS,
        )
    }

    /// Which menu items can be used right now.
    fn menu_availability(&self) -> MenuAvailability {
        MenuAvailability {
            can_save: !self.document.project.sources().is_empty() || !self.document.project.has_no_clips(),
            can_export: self.environment.toolchain.is_some()
                && !self.document.project.has_no_clips()
                && self.sources_in_use_that_are_not_available().is_empty(),
            has_selection: !self.interface.selection.is_empty(),
            can_undo: self.document.history.can_undo(),
            can_redo: self.document.history.can_redo(),
            busy: self.jobs.export.is_some()
                || self.jobs.running_save.is_some()
                || !self.sources.openings.is_empty()
                || self.interface.dialog.is_some(),
        }
    }
}

/// What the preview shows: every video and text object that is seen at the current frame, one over
/// another. The picture of a video is the one for the current frame whenever it is ready.
/// While it is not, a playing video keeps its last picture (a sharp one that is a moment old
/// looks better than a blurry one), but a video that is being moved around shows the
/// enlarged thumbnail at once and swaps in the sharp picture when it arrives.
fn preview_content<'a>(
    project: &Project,
    preview_pictures: &'a PreviewPictures<FfmpegPictureStreamStarter>,
    selected_clip: Option<ClipId>,
    frame_state: &'a FrameState,
    source_looks: &HashMap<SourceMediaIdentifier, SourceLook<'a>>,
) -> PreviewContent<'a> {
    let section = frame_state.picture_plan.section_at(frame_state.current_frame);
    // (The layers and the symbol are made from the section by the very functions that the export makes them with.)
    let layers = section.map_or_else(Vec::new, |section| {
        layers_of_a_section(
            section,
            frame_state.current_frame,
            |layer, video| video_in_the_picture(section, layer, video, preview_pictures, frame_state, source_looks),
            |source| {
                (
                    source_looks.get(&source).and_then(|look| look.picture),
                    colour_of_a_picture_file(&project.source_colour(source).unwrap_or_default()),
                )
            },
        )
    });
    PreviewContent {
        picture_size: (frame_state.total_frames > 0).then(|| project.picture_size()),
        layers,
        groups: section.map_or_else(Vec::new, |section| {
            groups_of_a_section(section, frame_state.current_frame)
        }),
        symbol: section.and_then(symbol_of_a_section),
        indicator_layout: project.speed_indicator_layout(),
        selected_clip,
    }
}

/// What shows where the video of `layer` is, in `section`, at the current frame.
fn video_in_the_picture<'a>(
    section: &PictureSection,
    layer: &Layer,
    video: &VisibleVideo,
    preview_pictures: &'a PreviewPictures<FfmpegPictureStreamStarter>,
    frame_state: &FrameState,
    source_looks: &HashMap<SourceMediaIdentifier, SourceLook<'a>>,
) -> VideoInThePicture<'a> {
    let track = layer.track;
    // (A picture of another video, that this track had before, is not shown in its place.)
    let supplied_picture = preview_pictures
        .picture_of(track)
        .filter(|picture| picture.source == video.source);
    let source_frame = section
        .source_frame_at(video, frame_state.current_frame)
        .map(SourceFrameIndex::new);
    let thumbnail = source_frame.and_then(|source_frame| {
        source_looks
            .get(&video.source)
            .and_then(|look| look.thumbnails)
            .map(|library| {
                (
                    library,
                    thumbnail_second_for_source_frame(frame_state.frame_rate, source_frame),
                )
            })
            .filter(|(library, second)| library.every_second().picture_for_second(*second).is_some())
    });
    let up_to_date = frame_state.pictures_report.shows_the_wanted_picture_of(track);
    let moved_around = !up_to_date && !frame_state.is_playing;
    match (supplied_picture, thumbnail) {
        (_, Some((library, second))) if moved_around => VideoInThePicture::Thumbnail {
            source: video.source,
            second,
            library,
        },
        (Some(picture), _) => VideoInThePicture::Picture(picture),
        (None, Some((library, second))) => VideoInThePicture::Thumbnail {
            source: video.source,
            second,
            library,
        },
        (None, None) => VideoInThePicture::Waiting,
    }
}

/// What the timeline draws of each source file: whether it can be used, and its pictures or waveform.
pub(super) fn source_looks_of<'a>(
    project: &'a Project,
    source_files: &'a HashMap<SourceMediaIdentifier, SourceFileState>,
) -> HashMap<SourceMediaIdentifier, SourceLook<'a>> {
    project
        .sources()
        .iter()
        .map(|source| {
            let state = source_files.get(&source.identifier);
            let is_available = state.is_some_and(|state| state.location_if_available().is_some());
            let look = SourceLook {
                is_available,
                thumbnails: state
                    .filter(|_| source.kind() == SourceMediaKind::Video)
                    .map(|state| &state.thumbnails.library),
                picture: state
                    .filter(|_| source.kind() == SourceMediaKind::Image)
                    .and_then(|state| state.picture.picture.as_ref()),
                picture_size_in_pixels: source.properties.display_size_in_pixels().unwrap_or((1, 1)),
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
    use crate::colour_effects::ColourMatrix;
    use crate::composition::plan_pictures;
    use crate::playback::PicturesReport;
    use crate::project::{
        AnimationTarget, Clip, EffectKind, FrameRate, MediaClip, PropertyValue, SourceFrameIndex, SourceMedia,
        SourceMediaLink, SourceVideoProperties, SpeedFactor, SpeedIndicatorLayout, TimelineFrameIndex, Track, TrackId,
    };
    use crate::test_support::HUE_SHIFT_AS_THE_FIRST_EFFECT as HUE;
    use crate::test_support::{make_a_keyframe, put_a_hue_shift_on};

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

    /// What the preview shows of `project` with the playhead at the frame `current_frame`, handed to `look`.
    fn look_at_the_preview<R>(project: &Project, current_frame: u64, look: impl FnOnce(PreviewContent<'_>) -> R) -> R {
        let picture_plan = plan_pictures(project, |_| true);
        let frame_state = FrameState {
            current_frame: TimelineFrameIndex::new(current_frame),
            total_frames: project.total_frame_count(),
            frame_rate: project.frame_rate(),
            is_playing: false,
            picture_plan,
            pictures_report: PicturesReport::default(),
        };
        let pictures = PreviewPictures::new(FfmpegPictureStreamStarter::new(None, Arc::new(|| {})));
        look(preview_content(project, &pictures, None, &frame_state, &HashMap::new()))
    }

    /// The speed the preview says the video plays at, with the playhead at the first frame.
    fn speed_shown_by_the_preview(project: &Project) -> Option<SpeedFactor> {
        look_at_the_preview(project, 0, |content| content.symbol.map(|symbol| symbol.speed))
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

    #[test]
    fn a_layer_of_the_preview_has_the_colours_changed_as_the_effects_of_its_clip_and_then_of_its_track_say_at_the_current_frame()
     {
        let mut project = project_of_a_sped_up_clip(1, 1);
        let clip = AnimationTarget::Clip(project.tracks()[0].entries()[0].id());
        let track = AnimationTarget::Track(project.tracks()[0].id());
        put_a_hue_shift_on(&mut project, clip, 90.0);
        project.add_effect(track, EffectKind::HUE_SHIFT).unwrap();
        make_a_keyframe(&mut project, track, HUE, 0, PropertyValue::Number(0.0));
        make_a_keyframe(&mut project, track, HUE, 100, PropertyValue::Number(60.0));

        for (frame, track_turn) in [(0, 0.0), (50, 30.0), (100, 60.0), (400, 60.0)] {
            let colour = look_at_the_preview(&project, frame, |content| {
                assert_eq!(content.layers.len(), 1);
                content.layers[0].colour()
            });

            assert_eq!(
                colour,
                ColourMatrix::hue_rotation(90.0).then(ColourMatrix::hue_rotation(track_turn)),
                "frame {frame}"
            );
        }
    }

    #[test]
    fn a_layer_of_the_preview_with_no_effects_has_its_colours_as_they_are() {
        let project = project_of_a_sped_up_clip(1, 1);

        let colour = look_at_the_preview(&project, 10, |content| content.layers[0].colour());

        assert_eq!(colour, ColourMatrix::NOTHING_CHANGES);
    }
}

/// Where a panel is in the window, which says which of its sides are against the edge of the window and which
/// against another panel.
#[derive(Clone, Copy)]
enum PanelSides {
    /// The panel along the bottom, with all of the window's width.
    AlongTheBottom,
    /// The panel at the left, above the one along the bottom.
    AtTheLeft,
    /// The panel that is the rest: at the right, above the one along the bottom.
    AtTheRight,
}

/// The frame of a panel, in the colour of panels, with room inside it of `inner_margin`, and round it the
/// gap that keeps the panels from the edge of the window and from one another.
fn panel_frame(inner_margin: Margin, sides: PanelSides) -> Frame {
    let gap = theme::PANEL_GAP;
    let (edge, half) = (gap, gap / 2);
    let outer_margin = match sides {
        PanelSides::AlongTheBottom => Margin {
            left: edge,
            right: edge,
            top: half,
            bottom: edge,
        },
        PanelSides::AtTheLeft => Margin {
            left: edge,
            right: half,
            top: edge,
            bottom: half,
        },
        PanelSides::AtTheRight => Margin {
            left: half,
            right: edge,
            top: edge,
            bottom: half,
        },
    };
    Frame::new()
        .fill(theme::PANEL_BACKGROUND)
        .inner_margin(inner_margin)
        .outer_margin(outer_margin)
}
