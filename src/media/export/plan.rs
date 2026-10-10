//! Working out what an export has to do, from a project.

use std::collections::BTreeMap;

use thiserror::Error;

use super::filter_graph::{ClipToExport, FilterGraph, FilterGraphSpecification, IndicatorOverlay, build_filter_graph};
use crate::project::{Clip, FrameRate, Project, Settings, SpeedFactor};
use crate::speed_indicator::{IndicatorPicture, indicator_for_frame};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ExportPlanError {
    #[error("there is nothing to export: the project has no video")]
    NothingToExport,
    #[error("this version of fastforwardrs can only export a project that is made from one source video")]
    SeveralSourceVideos,
    #[error(
        "there is nothing to export: every clip is past the end of the source video, which has only {source_frame_count} frames"
    )]
    EverythingIsPastTheEndOfTheSourceVideo { source_frame_count: u64 },
}

/// One fast-forward symbol picture the export needs, to be given to ffmpeg as an input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolInput {
    pub speed_factor: SpeedFactor,
    pub picture: IndicatorPicture,
}

/// Everything an export is going to do, worked out in advance.
#[derive(Clone, Debug)]
pub struct ExportPlan {
    pub filter_graph: FilterGraph,
    /// The clips the finished video is made of, cut short to the frames the source video has (see
    /// [`Self::frames_left_out_past_the_end_of_the_source`]). A clip with no such frames is not here.
    pub clips: Vec<ClipToExport>,
    /// The symbol pictures, in the order they are ffmpeg inputs 1, 2, ... (input 0 is the video).
    pub symbol_inputs: Vec<SymbolInput>,
    pub frame_rate: FrameRate,
    /// How many frames the finished video will have; used to show progress.
    pub total_frame_count: u64,
    /// How many frames of the project's timeline the finished video leaves out, because their
    /// clips reach past the end of the source video. That only happens when the project was
    /// pointed at a shorter video than the one it was edited on (which keeps every edit).
    pub frames_left_out_past_the_end_of_the_source: u64,
    pub picture_width_in_pixels: u32,
    pub picture_height_in_pixels: u32,
    pub has_audio: bool,
}

/// The clips as ffmpeg is asked to make them: cut short to the frames the source video has, with
/// those that have none left out.
///
/// Frames the source video does not have cannot be made. Asking ffmpeg for them would make the
/// sound (which is padded to each clip's planned length) run on past the end of the picture. This
/// only matters when a project has been pointed at a shorter video than the one it was edited on.
pub fn clips_to_export(clips: &[Clip], source_frame_count: u64) -> Vec<ClipToExport> {
    clips
        .iter()
        .filter_map(|clip| {
            let first_source_frame = clip.first_source_frame().value();
            let end_source_frame = clip.end_source_frame().value().min(source_frame_count);
            (first_source_frame < end_source_frame).then_some(ClipToExport {
                first_source_frame,
                end_source_frame,
                speed_factor: clip.speed_factor(),
            })
        })
        .collect()
}

/// Plans the export of `project`.
pub fn plan_export(project: &Project, settings: Settings) -> Result<ExportPlan, ExportPlanError> {
    let [source_video] = project.source_videos() else {
        return Err(if project.source_videos().is_empty() {
            ExportPlanError::NothingToExport
        } else {
            ExportPlanError::SeveralSourceVideos
        });
    };
    if project.video_track().is_empty() {
        return Err(ExportPlanError::NothingToExport);
    }

    let properties = source_video.properties;
    let picture_width_in_pixels = even(properties.display_width_in_pixels);
    let picture_height_in_pixels = even(properties.display_height_in_pixels);

    let clips = clips_to_export(project.video_track().clips(), properties.total_frame_count);
    if clips.is_empty() {
        return Err(ExportPlanError::EverythingIsPastTheEndOfTheSourceVideo {
            source_frame_count: properties.total_frame_count,
        });
    }
    let total_frame_count: u64 = clips.iter().map(ClipToExport::timeline_frame_count).sum();

    let mut symbol_inputs = Vec::new();
    let mut indicator_overlays = BTreeMap::new();
    let accelerated_speeds: std::collections::BTreeSet<SpeedFactor> = clips
        .iter()
        .map(|clip| clip.speed_factor)
        .filter(|speed_factor| speed_factor.is_accelerated())
        .collect();
    for speed_factor in accelerated_speeds {
        let indicator = indicator_for_frame(
            speed_factor,
            project.speed_indicator_layout(),
            picture_width_in_pixels,
            picture_height_in_pixels,
        );
        indicator_overlays.insert(
            speed_factor,
            IndicatorOverlay {
                left_in_pixels: indicator.left_in_pixels,
                top_in_pixels: indicator.top_in_pixels,
            },
        );
        symbol_inputs.push(SymbolInput {
            speed_factor,
            picture: indicator.picture,
        });
    }

    let specification = FilterGraphSpecification {
        frame_rate: properties.frame_rate,
        clips,
        picture_width_in_pixels,
        picture_height_in_pixels,
        source_has_audio: properties.has_audio,
        mute_sped_up_clips: settings.mute_sped_up_clips,
        indicator_overlays,
    };
    let filter_graph = build_filter_graph(&specification);

    Ok(ExportPlan {
        filter_graph,
        clips: specification.clips,
        symbol_inputs,
        frame_rate: properties.frame_rate,
        total_frame_count,
        frames_left_out_past_the_end_of_the_source: project.video_track().total_frame_count() - total_frame_count,
        picture_width_in_pixels,
        picture_height_in_pixels,
        has_audio: properties.has_audio,
    })
}

/// The largest even number that is not above `size`, and at least 2. Video encoders for most
/// formats can only make pictures with an even width and height.
fn even(size: u32) -> u32 {
    (size & !1).max(2)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::project::{
        SourceVideo, SourceVideoIdentifier, SourceVideoLink, SourceVideoProperties, SpeedIndicatorLayout,
        TimelineFrameIndex, VideoTrack,
    };

    fn properties(width: u32, height: u32, has_audio: bool) -> SourceVideoProperties {
        SourceVideoProperties {
            frame_rate: FrameRate::new(30, 1).unwrap(),
            total_frame_count: 300,
            display_width_in_pixels: width,
            display_height_in_pixels: height,
            has_audio,
        }
    }

    fn project_with_video(width: u32, height: u32, has_audio: bool) -> Project {
        let mut project = Project::empty();
        project
            .import_source_video(SourceVideo {
                identifier: SourceVideoIdentifier::new(1),
                link: SourceVideoLink::from_absolute_path(PathBuf::from("/videos/holiday.mp4")),
                properties: properties(width, height, has_audio),
            })
            .unwrap();
        project
    }

    #[test]
    fn a_project_without_a_video_cannot_be_exported() {
        assert_eq!(
            plan_export(&Project::empty(), Settings::DEFAULT).unwrap_err(),
            ExportPlanError::NothingToExport
        );
    }

    #[test]
    fn a_project_with_a_video_that_has_no_frames_cannot_be_exported() {
        let mut project = Project::empty();
        let mut empty_properties = properties(320, 180, false);
        empty_properties.total_frame_count = 0;
        project
            .import_source_video(SourceVideo {
                identifier: SourceVideoIdentifier::new(1),
                link: SourceVideoLink::from_absolute_path(PathBuf::from("/a.mp4")),
                properties: empty_properties,
            })
            .unwrap();
        assert_eq!(
            plan_export(&project, Settings::DEFAULT).unwrap_err(),
            ExportPlanError::NothingToExport
        );
    }

    #[test]
    fn a_project_with_two_videos_cannot_be_exported_yet() {
        let make_video = |identifier| SourceVideo {
            identifier: SourceVideoIdentifier::new(identifier),
            link: SourceVideoLink::from_absolute_path(PathBuf::from("/a.mp4")),
            properties: properties(320, 180, false),
        };
        let project = Project::from_parts(
            vec![make_video(1), make_video(2)],
            VideoTrack::default(),
            SpeedIndicatorLayout::DEFAULT,
        )
        .unwrap();
        assert_eq!(
            plan_export(&project, Settings::DEFAULT).unwrap_err(),
            ExportPlanError::SeveralSourceVideos
        );
    }

    #[test]
    fn the_plan_reports_the_length_of_the_finished_video_in_frames() {
        let mut project = project_with_video(320, 180, false);
        project.split_at(TimelineFrameIndex::new(100)).unwrap();
        project.set_clip_speed(1, SpeedFactor::new(4).unwrap()).unwrap();
        let plan = plan_export(&project, Settings::DEFAULT).unwrap();
        assert_eq!(plan.total_frame_count, 100 + 50);
        assert_eq!(plan.frame_rate, FrameRate::new(30, 1).unwrap());
    }

    /// A project edited on a video of 300 frames and then pointed at a shorter one: clips
    /// 0..100 at 1x, 100..200 at 4x and 200..300 at 1x, on a video of `frame_count` frames.
    fn project_on_a_replacement_video_of(frame_count: u64) -> Project {
        let mut project = project_with_video(320, 180, true);
        project.split_at(TimelineFrameIndex::new(100)).unwrap();
        project.split_at(TimelineFrameIndex::new(200)).unwrap();
        project.set_clip_speed(1, SpeedFactor::new(4).unwrap()).unwrap();
        let mut shorter = properties(320, 180, true);
        shorter.total_frame_count = frame_count;
        project
            .relink_source_video(
                SourceVideoIdentifier::new(1),
                SourceVideoLink::from_absolute_path(PathBuf::from("/elsewhere/shorter.mp4")),
                shorter,
            )
            .unwrap();
        project
    }

    #[test]
    fn a_project_on_its_own_video_leaves_nothing_out() {
        let plan = plan_export(&project_with_video(320, 180, true), Settings::DEFAULT).unwrap();
        assert_eq!(plan.total_frame_count, 300);
        assert_eq!(plan.frames_left_out_past_the_end_of_the_source, 0);
    }

    #[test]
    fn clips_reaching_past_the_end_of_a_shorter_replacement_video_are_cut_short_to_what_exists() {
        // The video has 130 frames. Clip 1 (100..200 at 4x) keeps source frames 100, 104, ... 128:
        // 8 of its 25 timeline frames. Clip 2 (200..300) has none.
        let project = project_on_a_replacement_video_of(130);
        assert_eq!(project.video_track().total_frame_count(), 100 + 25 + 100);

        let plan = plan_export(&project, Settings::DEFAULT).unwrap();

        assert_eq!(
            plan.total_frame_count,
            100 + 8,
            "the finished video has only the frames that exist"
        );
        assert_eq!(plan.frames_left_out_past_the_end_of_the_source, 17 + 100);
        let planned_clips: Vec<(u64, u64, u32)> = plan
            .clips
            .iter()
            .map(|clip| {
                (
                    clip.first_source_frame,
                    clip.end_source_frame,
                    clip.speed_factor.value(),
                )
            })
            .collect();
        assert_eq!(
            planned_clips,
            vec![(0, 100, 1), (100, 130, 4)],
            "the second clip is cut short at the end of the video, and the third is left out"
        );
        // The graph asks ffmpeg only for those.
        let text = &plan.filter_graph.text;
        assert!(text.contains("trim=start_frame=100:end_frame=130"), "{text}");
        assert!(text.contains("concat=n=2:v=1:a=0"), "{text}");
    }

    #[test]
    fn the_plan_shows_exactly_the_frames_of_the_clips_that_the_source_video_has() {
        for frame_count in [1, 50, 99, 100, 101, 130, 200, 201, 299, 300, 1000] {
            let project = project_on_a_replacement_video_of(frame_count);
            let plan = plan_export(&project, Settings::DEFAULT).unwrap();

            // What the finished video must show, found one frame at a time: every frame of every
            // clip, if the source video has the source frame it stands for.
            let must_show: Vec<u64> = project
                .video_track()
                .placements()
                .flat_map(|placement| {
                    (0..placement.clip.timeline_frame_count())
                        .map(move |offset| placement.clip.source_frame_at_timeline_offset(offset).unwrap().value())
                })
                .filter(|&source_frame| source_frame < frame_count)
                .collect();
            // What the plan's clips show, found the same way.
            let planned: Vec<u64> = plan
                .clips
                .iter()
                .flat_map(|clip| {
                    (0..clip.timeline_frame_count())
                        .map(move |offset| clip.first_source_frame + offset * u64::from(clip.speed_factor.value()))
                })
                .collect();

            assert_eq!(planned, must_show, "video of {frame_count} frames");
            assert_eq!(
                plan.total_frame_count,
                must_show.len() as u64,
                "video of {frame_count} frames"
            );
            assert_eq!(
                plan.total_frame_count + plan.frames_left_out_past_the_end_of_the_source,
                project.video_track().total_frame_count(),
                "video of {frame_count} frames"
            );
        }
    }

    #[test]
    fn a_video_with_none_of_the_frames_the_clips_need_cannot_be_exported() {
        // (A real video with no frames cannot be opened at all; the plan must cope anyway.)
        assert_eq!(
            plan_export(&project_on_a_replacement_video_of(0), Settings::DEFAULT).unwrap_err(),
            ExportPlanError::EverythingIsPastTheEndOfTheSourceVideo { source_frame_count: 0 }
        );
    }

    #[test]
    fn a_speed_only_used_by_clips_that_are_left_out_gets_no_symbol_picture() {
        // The 4x clip (100..200) is wholly past the end of a video of 100 frames.
        let plan = plan_export(&project_on_a_replacement_video_of(100), Settings::DEFAULT).unwrap();
        assert!(plan.symbol_inputs.is_empty());
        assert_eq!(plan.total_frame_count, 100);
    }

    #[test]
    fn odd_picture_sizes_are_rounded_down_to_even_ones() {
        let plan = plan_export(&project_with_video(321, 181, false), Settings::DEFAULT).unwrap();
        assert_eq!(
            (plan.picture_width_in_pixels, plan.picture_height_in_pixels),
            (320, 180)
        );
        let plan = plan_export(&project_with_video(1, 1, false), Settings::DEFAULT).unwrap();
        assert_eq!((plan.picture_width_in_pixels, plan.picture_height_in_pixels), (2, 2));
    }

    #[test]
    fn only_accelerated_speeds_get_a_symbol_picture_and_each_speed_only_one() {
        let mut project = project_with_video(1280, 720, false);
        for frame in [50, 100, 150, 200] {
            project.split_at(TimelineFrameIndex::new(frame)).unwrap();
        }
        // Five clips: 1x, 8x, 1x, 8x, 2x.
        for (clip_index, factor) in [(1, 8), (3, 8), (4, 2)] {
            project
                .set_clip_speed(clip_index, SpeedFactor::new(factor).unwrap())
                .unwrap();
        }
        let plan = plan_export(&project, Settings::DEFAULT).unwrap();
        let speeds: Vec<u32> = plan
            .symbol_inputs
            .iter()
            .map(|input| input.speed_factor.value())
            .collect();
        assert_eq!(
            speeds,
            vec![2, 8],
            "ordered by speed, 1x has none, 8x is shared by two clips"
        );
        assert!(
            plan.symbol_inputs
                .iter()
                .all(|input| input.picture.height_in_pixels > 0)
        );
    }

    #[test]
    fn a_project_with_no_accelerated_clips_needs_no_symbol_pictures() {
        let mut project = project_with_video(1280, 720, false);
        project.split_at(TimelineFrameIndex::new(100)).unwrap();
        assert!(
            plan_export(&project, Settings::DEFAULT)
                .unwrap()
                .symbol_inputs
                .is_empty()
        );
    }

    #[test]
    fn the_symbol_is_sized_and_placed_for_the_finished_picture() {
        let mut project = project_with_video(1920, 1080, false);
        project.set_speed_indicator_layout(SpeedIndicatorLayout::new(0.1, 0.2, 0.1));
        project.set_clip_speed(0, SpeedFactor::new(4).unwrap()).unwrap();
        let plan = plan_export(&project, Settings::DEFAULT).unwrap();
        assert_eq!(plan.symbol_inputs[0].picture.height_in_pixels, 108);
        assert!(
            plan.filter_graph.text.contains("overlay=x=192:y=216:format=auto"),
            "{}",
            plan.filter_graph.text
        );
    }

    #[test]
    fn the_mute_setting_reaches_the_graph_and_only_changes_the_sound_of_sped_up_clips() {
        let mut project = project_with_video(320, 180, true);
        project.split_at(TimelineFrameIndex::new(100)).unwrap();
        project.set_clip_speed(1, SpeedFactor::new(4).unwrap()).unwrap();
        let muting = Settings {
            mute_sped_up_clips: true,
            ..Settings::DEFAULT
        };

        let loud = plan_export(&project, Settings::DEFAULT).unwrap().filter_graph.text;
        let quiet = plan_export(&project, muting).unwrap().filter_graph.text;

        assert!(loud.contains("atempo=4"), "{loud}");
        assert!(!quiet.contains("atempo"), "{quiet}");
        assert!(quiet.contains("volume=0"), "{quiet}");
        assert_eq!(
            loud.matches("setpts=").count(),
            quiet.matches("setpts=").count(),
            "the pictures are the same"
        );
    }

    #[test]
    fn sound_is_part_of_the_plan_only_when_the_source_has_it() {
        assert!(
            plan_export(&project_with_video(320, 180, true), Settings::DEFAULT)
                .unwrap()
                .filter_graph
                .audio_output_label
                .is_some()
        );
        assert!(
            plan_export(&project_with_video(320, 180, false), Settings::DEFAULT)
                .unwrap()
                .filter_graph
                .audio_output_label
                .is_none()
        );
    }
}
