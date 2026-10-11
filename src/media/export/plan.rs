//! Working out what an export has to do, from a project.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use thiserror::Error;

use super::filter_graph::{
    FilterGraph, FilterGraphSpecification, OverlayToExport, PictureGraphSpecification, SectionToExport,
    SoundClipToExport, SoundGraphSpecification, SoundItemToExport, SoundStripToExport, SourceToExport, VideoToExport,
    build_filter_graph,
};
use crate::composition::{PictureSection, SoundItem, SoundPlan, plan_pictures, plan_sound};
use crate::project::{
    ClipLocation, FrameRate, Project, Settings, SourceMediaIdentifier, SpedUpClipSound, SpeedFactor, TextObject,
    TimelineFrameIndex,
};
use crate::rgba_picture::RgbaPicture;
use crate::speed_indicator::indicator_for_frame;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ExportPlanError {
    #[error("there is nothing to export: the project has nothing on its tracks")]
    NothingToExport,
}

/// What a still picture laid over the video is a picture of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StillPictureKind {
    /// The fast-forward symbol for this speed.
    FastForwardSymbol(SpeedFactor),
    /// A text object.
    Text(ClipLocation),
}

/// One still picture the export needs, to be given to ffmpeg as an input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StillPictureInput {
    pub kind: StillPictureKind,
    pub picture: RgbaPicture,
}

/// Everything an export is going to do, worked out in advance.
#[derive(Clone, Debug)]
pub struct ExportPlan {
    pub filter_graph: FilterGraph,
    /// The source files ffmpeg reads, which are its inputs 0, 1, ... in this order.
    pub source_files: Vec<SourceMediaIdentifier>,
    /// The still pictures, which are the inputs after the source files, in this order.
    pub still_pictures: Vec<StillPictureInput>,
    pub frame_rate: FrameRate,
    /// How many frames the finished video will have; used to show progress.
    pub total_frame_count: u64,
    /// How many frames of the finished video are black only because a video file has fewer pictures
    /// than its clips need. That only happens when the project was pointed at a shorter video than
    /// the one it was edited on (which keeps every edit).
    pub frames_black_for_lack_of_pictures: u64,
    pub picture_width_in_pixels: u32,
    pub picture_height_in_pixels: u32,
    pub has_audio: bool,
}

/// Where a still picture goes: which ffmpeg input it is, and its place on the finished picture.
#[derive(Clone, Copy)]
struct StillPicturePlace {
    picture_input: usize,
    left_in_pixels: u32,
    top_in_pixels: u32,
}

/// Plans the export of `project`. `render_text` makes the picture of a text object for a finished
/// picture of the given size in pixels: the picture of the text's own area.
pub fn plan_export(
    project: &Project,
    settings: Settings,
    render_text: &dyn Fn(&TextObject, u32, u32) -> RgbaPicture,
) -> Result<ExportPlan, ExportPlanError> {
    let pictures = plan_pictures(project, |_| true);
    if pictures.total_frame_count == 0 {
        return Err(ExportPlanError::NothingToExport);
    }
    let sound = plan_sound(project, TimelineFrameIndex::new(0), |_| true);

    let (picture_width, picture_height) = pictures.picture_size;
    let picture_width_in_pixels = even(picture_width);
    let picture_height_in_pixels = even(picture_height);

    // The files ffmpeg reads: every one that a picture or a sound is made from.
    let mut used_sources = sources_of_sound(&sound);
    for section in &pictures.sections {
        used_sources.extend(section.video.map(|video| video.source));
    }
    let source_files: Vec<SourceMediaIdentifier> = used_sources.into_iter().collect();
    let input_of_source: HashMap<SourceMediaIdentifier, usize> = source_files
        .iter()
        .enumerate()
        .map(|(input, identifier)| (*identifier, input))
        .collect();
    let sources: Vec<SourceToExport> = source_files
        .iter()
        .map(|identifier| SourceToExport {
            display_size_in_pixels: project
                .source(*identifier)
                .and_then(|source| source.properties.video())
                .map(|video| (video.display_width_in_pixels, video.display_height_in_pixels)),
        })
        .collect();

    // The still pictures: a symbol for every speed that shows one, and every text object that is seen.
    let mut still_pictures: Vec<StillPictureInput> = Vec::new();
    let mut symbol_places: BTreeMap<SpeedFactor, StillPicturePlace> = BTreeMap::new();
    let accelerated_speeds: BTreeSet<SpeedFactor> = pictures
        .sections
        .iter()
        .filter_map(|section| section.video)
        .map(|video| video.speed)
        .filter(|speed| speed.is_accelerated())
        .collect();
    for speed_factor in accelerated_speeds {
        let indicator = indicator_for_frame(
            speed_factor,
            project.speed_indicator_layout(),
            picture_width_in_pixels,
            picture_height_in_pixels,
        );
        symbol_places.insert(
            speed_factor,
            StillPicturePlace {
                picture_input: source_files.len() + still_pictures.len(),
                left_in_pixels: indicator.left_in_pixels,
                top_in_pixels: indicator.top_in_pixels,
            },
        );
        still_pictures.push(StillPictureInput {
            kind: StillPictureKind::FastForwardSymbol(speed_factor),
            picture: indicator.picture,
        });
    }
    let mut text_places: HashMap<ClipLocation, StillPicturePlace> = HashMap::new();
    for text in pictures.sections.iter().flat_map(|section| &section.texts) {
        if text_places.contains_key(&text.clip) {
            continue;
        }
        let picture = render_text(&text.object, picture_width_in_pixels, picture_height_in_pixels);
        let area = text.object.area;
        let left = (area.left() * picture_width_in_pixels as f32).round() as u32;
        let top = (area.top() * picture_height_in_pixels as f32).round() as u32;
        text_places.insert(
            text.clip,
            StillPicturePlace {
                picture_input: source_files.len() + still_pictures.len(),
                left_in_pixels: left.min(picture_width_in_pixels.saturating_sub(picture.width_in_pixels)),
                top_in_pixels: top.min(picture_height_in_pixels.saturating_sub(picture.height_in_pixels)),
            },
        );
        still_pictures.push(StillPictureInput {
            kind: StillPictureKind::Text(text.clip),
            picture,
        });
    }

    let sections: Vec<SectionToExport> = pictures
        .sections
        .iter()
        .map(|section| section_to_export(section, &input_of_source, &symbol_places, &text_places))
        .collect();
    let specification = FilterGraphSpecification {
        pictures: PictureGraphSpecification {
            frame_rate: pictures.frame_rate,
            picture_width_in_pixels,
            picture_height_in_pixels,
            sources,
            sections,
        },
        sound: sound_graph_specification(&sound, &source_files, settings.sped_up_clip_sound),
    };
    let filter_graph = build_filter_graph(&specification);
    Ok(ExportPlan {
        has_audio: filter_graph.audio_output_label.is_some(),
        filter_graph,
        source_files,
        still_pictures,
        frame_rate: pictures.frame_rate,
        total_frame_count: pictures.total_frame_count,
        frames_black_for_lack_of_pictures: pictures.frames_black_for_lack_of_pictures,
        picture_width_in_pixels,
        picture_height_in_pixels,
    })
}

/// The files that the sound of `plan` is made from.
pub fn sources_of_sound(plan: &SoundPlan) -> BTreeSet<SourceMediaIdentifier> {
    plan.strips
        .iter()
        .flat_map(|strip| &strip.items)
        .filter_map(|item| match item {
            SoundItem::Clip(clip) => Some(clip.source),
            SoundItem::Silence { .. } => None,
        })
        .collect()
}

/// What the graph of the sound of `plan` is made from, where `source_files` are the files that
/// are the graph's inputs 0, 1, ... in this order (every file the sound is made from must be one
/// of them). The export and the live preview make their sound from this.
pub fn sound_graph_specification(
    plan: &SoundPlan,
    source_files: &[SourceMediaIdentifier],
    sped_up_clip_sound: SpedUpClipSound,
) -> SoundGraphSpecification {
    let input_of_source = |identifier: SourceMediaIdentifier| {
        source_files
            .iter()
            .position(|candidate| *candidate == identifier)
            .expect("every file the sound is made from is one of the inputs")
    };
    SoundGraphSpecification {
        frame_rate: plan.frame_rate,
        total_frame_count: plan.total_frame_count,
        strips: plan
            .strips
            .iter()
            .map(|strip| SoundStripToExport {
                items: strip
                    .items
                    .iter()
                    .map(|item| match item {
                        SoundItem::Silence { frame_count } => SoundItemToExport::Silence {
                            frame_count: *frame_count,
                        },
                        SoundItem::Clip(clip) => SoundItemToExport::Clip(SoundClipToExport {
                            source_input: input_of_source(clip.source),
                            first_source_frame: clip.first_source_frame,
                            end_source_frame: clip.end_source_frame,
                            speed_factor: clip.speed,
                            timeline_frame_count: clip.timeline_frame_count,
                            loudness_factor: clip.loudness_factor,
                        }),
                    })
                    .collect(),
            })
            .collect(),
        sped_up_clip_sound,
    }
}

fn section_to_export(
    section: &PictureSection,
    input_of_source: &HashMap<SourceMediaIdentifier, usize>,
    symbol_places: &BTreeMap<SpeedFactor, StillPicturePlace>,
    text_places: &HashMap<ClipLocation, StillPicturePlace>,
) -> SectionToExport {
    let overlay = |place: &StillPicturePlace| OverlayToExport {
        picture_input: place.picture_input,
        left_in_pixels: place.left_in_pixels,
        top_in_pixels: place.top_in_pixels,
    };
    // The symbol belongs to the video, so it goes right over it, and the text goes over both.
    let symbol = section
        .video
        .and_then(|video| symbol_places.get(&video.speed))
        .map(overlay);
    let texts = section.texts.iter().map(|text| overlay(&text_places[&text.clip]));
    SectionToExport {
        frame_count: section.timeline_frame_count(),
        video: section.video.map(|video| VideoToExport {
            source_input: input_of_source[&video.source],
            first_source_frame: video.first_source_frame,
            end_source_frame: video.end_source_frame,
            speed_factor: video.speed,
        }),
        overlays: symbol.into_iter().chain(texts).collect(),
    }
}

/// The largest even number that is not above `size`, and at least 2. Video encoders for most
/// formats can only make pictures with an even width and height.
fn even(size: u32) -> u32 {
    (size & !1).max(2)
}

#[cfg(test)]
mod tests;
