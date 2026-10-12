//! Working out what an export has to do, from a project.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use thiserror::Error;

use super::animated_sections;
use super::filter_graph::{
    FilterGraph, FilterGraphSpecification, LayerContentToExport, LayerToExport, LoudnessToExport,
    PictureGraphSpecification, PlaceToExport, SectionToExport, SoundClipToExport, SoundGraphSpecification,
    SoundItemToExport, SoundStripToExport, VideoToExport, build_filter_graph,
};
use crate::colour_effects::ColourMatrix;
use crate::composition::{LayerContent, Loudness, PictureSection, SoundItem, SoundPlan, plan_pictures, plan_sound};
use crate::project::{
    ClipId, FrameRate, Placement, Project, Settings, SourceMediaIdentifier, SpedUpClipSound, SpeedFactor, TextObject,
    TimelineFrameIndex,
};
use crate::rgba_picture::RgbaPicture;
use crate::speed_indicator::indicator_for_frame;
use crate::text_rendering::TextAppearance;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ExportPlanError {
    #[error("there is nothing to export: the project has nothing on its tracks")]
    NothingToExport,
    #[error(
        "there is too much animation to export: keyframes make the picture different in more than {most} separate stretches \
         of the video, and the export would take minutes only to get ready. Make what changes change more slowly, or for \
         a shorter time, or cut the clip into shorter ones."
    )]
    TooMuchAnimation { most: usize },
}

impl ExportPlanError {
    /// What a message that says it is headed with.
    pub fn heading(self) -> &'static str {
        match self {
            Self::NothingToExport => "There is nothing to export",
            Self::TooMuchAnimation { .. } => "There is too much animation to export",
        }
    }
}

/// What a still picture laid over the video is a picture of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StillPictureKind {
    /// The fast-forward symbol for this speed.
    FastForwardSymbol(SpeedFactor),
    /// A text object.
    Text(ClipId),
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
    /// How many frames of the finished video are missing the picture of a video, only because the
    /// video file has fewer pictures than its clips need. That only happens when the project was
    /// pointed at a shorter video than the one it was edited on (which keeps every edit).
    pub frames_missing_a_picture: u64,
    pub picture_width_in_pixels: u32,
    pub picture_height_in_pixels: u32,
    pub has_audio: bool,
}

/// Where the fast-forward symbol goes: which ffmpeg input its picture is, and its place on the finished picture.
#[derive(Clone, Copy)]
struct StillPicturePlace {
    picture_input: usize,
    place: PlaceToExport,
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
    let picture_size = (picture_width_in_pixels, picture_height_in_pixels);
    // What changes with keyframes is cut into stretches that are each exported as one thing, so from here on a section
    // is made of layers that are in one place.
    let sections = animated_sections::with_keyframes_resolved(&pictures.sections, picture_size)?;

    // The files ffmpeg reads: every one that a picture or a sound is made from.
    let mut used_sources = sources_of_sound(&sound);
    for layer in sections.iter().flat_map(|section| &section.layers) {
        match &layer.content {
            LayerContent::Video(video) => {
                used_sources.insert(video.source);
            }
            LayerContent::Image { source } => {
                used_sources.insert(*source);
            }
            LayerContent::Text(_) => {}
        }
    }
    let source_files: Vec<SourceMediaIdentifier> = used_sources.into_iter().collect();
    let input_of_source: HashMap<SourceMediaIdentifier, usize> = source_files
        .iter()
        .enumerate()
        .map(|(input, identifier)| (*identifier, input))
        .collect();

    // The still pictures: a symbol for every speed that shows one (the speed of the video that is in
    // front), and every text object that is seen, once for each way it looks.
    let mut still_pictures: Vec<StillPictureInput> = Vec::new();
    let mut symbol_places: BTreeMap<SpeedFactor, StillPicturePlace> = BTreeMap::new();
    let accelerated_speeds: BTreeSet<SpeedFactor> = sections
        .iter()
        .filter_map(PictureSection::front_video)
        .map(|(_, video)| video.speed)
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
                place: PlaceToExport {
                    left_in_pixels: i32::try_from(indicator.left_in_pixels).unwrap_or(i32::MAX),
                    top_in_pixels: i32::try_from(indicator.top_in_pixels).unwrap_or(i32::MAX),
                    width_in_pixels: indicator.picture.width_in_pixels,
                    height_in_pixels: indicator.picture.height_in_pixels,
                    turn_in_degrees: 0.0,
                },
            },
        );
        still_pictures.push(StillPictureInput {
            kind: StillPictureKind::FastForwardSymbol(speed_factor),
            picture: indicator.picture,
        });
    }
    // Which input the picture of a text object is, for each text clip and each way it looks (a text object that has
    // keyframes for what it says or how it looks is a picture for each, and one that only moves is one picture).
    let mut text_inputs: HashMap<(ClipId, TextAppearance), usize> = HashMap::new();
    for layer in sections.iter().flat_map(|section| &section.layers) {
        let LayerContent::Text(object) = &layer.content else {
            continue;
        };
        let appearance = TextAppearance::of(object, picture_width_in_pixels, picture_height_in_pixels);
        if text_inputs.contains_key(&(layer.clip, appearance.clone())) {
            continue;
        }
        // The picture of the text is of the text upright in its area, which is as many pixels as the
        // place is, and is turned, if it is, when it is laid over.
        let picture = render_text(object, picture_width_in_pixels, picture_height_in_pixels);
        text_inputs.insert((layer.clip, appearance), source_files.len() + still_pictures.len());
        still_pictures.push(StillPictureInput {
            kind: StillPictureKind::Text(layer.clip),
            picture,
        });
    }

    let sections: Vec<SectionToExport> = sections
        .iter()
        .map(|section| section_to_export(section, picture_size, &input_of_source, &symbol_places, &text_inputs))
        .collect();
    let specification = FilterGraphSpecification {
        pictures: PictureGraphSpecification {
            frame_rate: pictures.frame_rate,
            picture_width_in_pixels,
            picture_height_in_pixels,
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
        frames_missing_a_picture: pictures.frames_missing_a_picture,
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
                            loudness: match &clip.loudness {
                                Loudness::Constant(factor) => LoudnessToExport::Constant(*factor),
                                Loudness::Changing(points) => LoudnessToExport::Changing(
                                    points.iter().map(|point| (point.frame, point.decibels)).collect(),
                                ),
                            },
                        }),
                    })
                    .collect(),
            })
            .collect(),
        sped_up_clip_sound,
    }
}

/// Where `placement` is on a picture `picture_size` pixels across and down.
pub(super) fn place_to_export(placement: Placement, picture_size: (u32, u32)) -> PlaceToExport {
    let pixels = placement.area.in_pixels(picture_size);
    PlaceToExport {
        left_in_pixels: pixels.left,
        top_in_pixels: pixels.top,
        width_in_pixels: pixels.width,
        height_in_pixels: pixels.height,
        // (A turn of more than half a turn is the angle it looks like, and a whole number of turns is none at all: nothing
        // is turned for nothing.)
        turn_in_degrees: placement.rotation.within_a_turn().degrees(),
    }
}

fn section_to_export(
    section: &PictureSection,
    picture_size: (u32, u32),
    input_of_source: &HashMap<SourceMediaIdentifier, usize>,
    symbol_places: &BTreeMap<SpeedFactor, StillPicturePlace>,
    text_inputs: &HashMap<(ClipId, TextAppearance), usize>,
) -> SectionToExport {
    let still = |place: &StillPicturePlace, colour: ColourMatrix| LayerToExport {
        content: LayerContentToExport::Still {
            picture_input: place.picture_input,
            scaled_to_its_place: false,
        },
        place: place.place,
        colour,
    };
    // The symbol belongs to the video that is in front, so it goes right over it, and what is
    // in front of that video goes over both.
    let clip_of_the_front_video = section.front_video().map(|(layer, _)| layer.clip);
    let mut layers = Vec::with_capacity(section.layers.len() + 1);
    for layer in &section.layers {
        match &layer.content {
            LayerContent::Video(video) => layers.push(LayerToExport {
                content: LayerContentToExport::Video(VideoToExport {
                    source_input: input_of_source[&video.source],
                    first_source_frame: video.first_source_frame,
                    end_source_frame: video.end_source_frame,
                    speed_factor: video.speed,
                }),
                place: place_to_export(layer.placement, picture_size),
                colour: layer.colour_matrix(),
            }),
            LayerContent::Image { source } => layers.push(LayerToExport {
                content: LayerContentToExport::Still {
                    picture_input: input_of_source[source],
                    scaled_to_its_place: true,
                },
                place: place_to_export(layer.placement, picture_size),
                colour: layer.colour_matrix(),
            }),
            LayerContent::Text(object) => {
                let appearance = TextAppearance::of(object, picture_size.0, picture_size.1);
                layers.push(still(
                    &StillPicturePlace {
                        picture_input: text_inputs[&(layer.clip, appearance)],
                        place: place_to_export(layer.placement, picture_size),
                    },
                    layer.colour_matrix(),
                ));
            }
        }
        if Some(layer.clip) == clip_of_the_front_video
            && let Some((_, video)) = section.front_video()
            && let Some(symbol) = symbol_places.get(&video.speed)
        {
            layers.push(still(symbol, ColourMatrix::NOTHING_CHANGES));
        }
    }
    SectionToExport {
        frame_count: section.timeline_frame_count(),
        layers,
    }
}

/// The largest even number that is not above `size`, and at least 2. Video encoders for most
/// formats can only make pictures with an even width and height.
fn even(size: u32) -> u32 {
    (size & !1).max(2)
}

#[cfg(test)]
mod animation_tests;
#[cfg(test)]
mod effects_tests;
#[cfg(test)]
mod tests;
