//! What the picture is at every frame: which video is seen, and which text objects are over it.
//!
//! The tracks are in front-to-back order, and a video fills the whole picture, so at any frame the
//! only video that can be seen is the one on the front-most track that has a video there, and
//! the only text objects that can be seen are those on tracks in front of it. (With no video at a
//! frame the picture is black, with all of the text over it.) The timeline is cut into *sections*,
//! stretches of frames over which the same video clip and the same text objects are seen.

use std::collections::BTreeSet;

use crate::project::tracks::{Clip, ClipLocation, TextObject};
use crate::project::{FrameRate, Project, SourceMediaIdentifier, SpeedFactor, TimelineFrameIndex};

/// The video that is seen during a section: the part of one clip that the section covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisibleVideo {
    pub clip: ClipLocation,
    pub source: SourceMediaIdentifier,
    /// The source frame shown by the first frame of the section.
    pub first_source_frame: u64,
    /// The frame just after the last source frame the section needs (the end is exclusive). The
    /// source file has all the frames up to this one, so the section is made entirely of pictures.
    pub end_source_frame: u64,
    /// How much faster than normal the video plays: the clip's speed times its track's. Every
    /// `speed`th source frame is shown, starting with the first. A symbol shows when it is above 1.
    pub speed: SpeedFactor,
}

/// A text object that is seen during a section.
#[derive(Clone, Debug, PartialEq)]
pub struct VisibleText {
    pub clip: ClipLocation,
    pub object: TextObject,
}

/// A stretch of the timeline over which the picture is made of the same things.
#[derive(Clone, Debug, PartialEq)]
pub struct PictureSection {
    /// The first frame of the section.
    pub first_timeline_frame: u64,
    /// The frame just after the last frame of the section (the end is exclusive).
    pub end_timeline_frame: u64,
    /// The video that is seen, or `None` for a black picture.
    pub video: Option<VisibleVideo>,
    /// The text objects over the picture, the one at the back first (the order they are drawn in).
    pub texts: Vec<VisibleText>,
}

impl PictureSection {
    pub fn timeline_frame_count(&self) -> u64 {
        self.end_timeline_frame - self.first_timeline_frame
    }

    /// The source frame the video shows at `timeline_frame`, or `None` if it shows no video there.
    pub fn source_frame_at(&self, timeline_frame: TimelineFrameIndex) -> Option<u64> {
        let video = self.video?;
        let frame = timeline_frame.value();
        if frame < self.first_timeline_frame || frame >= self.end_timeline_frame {
            return None;
        }
        Some(video.first_source_frame + (frame - self.first_timeline_frame) * u64::from(video.speed.value()))
    }
}

/// The picture at every frame of the timeline.
#[derive(Clone, Debug, PartialEq)]
pub struct PicturePlan {
    pub frame_rate: FrameRate,
    /// The size of the finished picture, in pixels.
    pub picture_size: (u32, u32),
    pub total_frame_count: u64,
    /// The sections, in order, with no gaps between them: together they cover the whole timeline.
    pub sections: Vec<PictureSection>,
    /// How many frames are black only because the file that a video is made from does not have
    /// enough pictures for the clip (which happens when a project is pointed at a shorter file than
    /// the one it was edited on).
    pub frames_black_for_lack_of_pictures: u64,
}

impl PicturePlan {
    /// The section that `timeline_frame` is in, if it is on the timeline.
    pub fn section_at(&self, timeline_frame: TimelineFrameIndex) -> Option<&PictureSection> {
        let frame = timeline_frame.value();
        let index = self
            .sections
            .partition_point(|section| section.end_timeline_frame <= frame);
        self.sections
            .get(index)
            .filter(|section| section.first_timeline_frame <= frame)
    }
}

/// A video or text clip as the planning sees it.
struct Candidate<'project> {
    /// Where the clip's track is in the list of tracks: 0 is the front.
    track_position: usize,
    clip: ClipLocation,
    first_timeline_frame: u64,
    end_timeline_frame: u64,
    effective_speed: SpeedFactor,
    content: &'project Clip,
    /// For a video: how many frames the source file has.
    source_frame_count: u64,
}

/// Works out the picture at every frame of the project. A video from a file that is not `usable`
/// is black, as a video is where its file has no more pictures.
pub fn plan_pictures(project: &Project, is_usable: impl Fn(SourceMediaIdentifier) -> bool) -> PicturePlan {
    let mut candidates = Vec::new();
    for (track_position, track) in project.tracks().iter().enumerate() {
        for placement in track.placements() {
            let source_frame_count = match placement.clip {
                Clip::Video(media) if is_usable(media.source()) => {
                    project.frame_count_of_source(media.source()).unwrap_or(0)
                }
                Clip::Video(_) => 0,
                Clip::Text(_) => 0,
                Clip::Audio(_) => continue,
            };
            candidates.push(Candidate {
                track_position,
                clip: ClipLocation {
                    track: track.id(),
                    clip: placement.id,
                },
                first_timeline_frame: placement.first_timeline_frame.value(),
                end_timeline_frame: placement.end_timeline_frame.value(),
                effective_speed: placement.effective_speed,
                content: placement.clip,
                source_frame_count,
            });
        }
    }

    let total_frame_count = project.total_frame_count();
    let boundaries: BTreeSet<u64> = candidates
        .iter()
        .flat_map(|candidate| [candidate.first_timeline_frame, candidate.end_timeline_frame])
        .chain([0, total_frame_count])
        .collect();
    let boundaries: Vec<u64> = boundaries
        .into_iter()
        .filter(|&frame| frame <= total_frame_count)
        .collect();

    // Going from one boundary to the next, the clips that are there are kept track of (they are
    // few: about one for each track), so that the work does not grow with the square of the number of clips.
    let mut by_start: Vec<usize> = (0..candidates.len()).collect();
    by_start.sort_by_key(|&index| candidates[index].first_timeline_frame);
    let mut next_to_start = 0;
    let mut active: Vec<&Candidate<'_>> = Vec::new();

    let mut sections: Vec<PictureSection> = Vec::new();
    let mut frames_black_for_lack_of_pictures = 0;
    for pair in boundaries.windows(2) {
        let (first, end) = (pair[0], pair[1]);
        active.retain(|candidate| candidate.end_timeline_frame > first);
        while let Some(&index) = by_start.get(next_to_start)
            && candidates[index].first_timeline_frame <= first
        {
            active.push(&candidates[index]);
            next_to_start += 1;
        }
        let (new_sections, frames_lacking_pictures) = sections_between(first, end, &active);
        frames_black_for_lack_of_pictures += frames_lacking_pictures;
        for section in new_sections {
            push_merging_with_the_one_before(&mut sections, section);
        }
    }
    PicturePlan {
        frame_rate: project.frame_rate(),
        picture_size: project.picture_size(),
        total_frame_count,
        sections,
        frames_black_for_lack_of_pictures,
    }
}

/// The section (or, if the front-most video runs out of source pictures partway, two) for the
/// frames `first` up to `end`, over which no clip starts or ends, and how many of those frames are
/// black because a video has run out of pictures.
fn sections_between(first: u64, end: u64, active: &[&Candidate<'_>]) -> (Vec<PictureSection>, u64) {
    debug_assert!(
        active
            .iter()
            .all(|candidate| candidate.first_timeline_frame <= first && end <= candidate.end_timeline_frame),
        "every clip in `active` is there for the whole of the stretch"
    );
    let front_video = active
        .iter()
        .filter(|candidate| matches!(candidate.content, Clip::Video(_)))
        .min_by_key(|candidate| candidate.track_position);

    // Text in front of the video, or all of it if there is no video, the one at the back first.
    let mut texts: Vec<&&Candidate<'_>> = active
        .iter()
        .filter(|candidate| matches!(candidate.content, Clip::Text(_)))
        .filter(|candidate| front_video.is_none_or(|video| candidate.track_position < video.track_position))
        .collect();
    texts.sort_by_key(|candidate| std::cmp::Reverse(candidate.track_position));
    let texts: Vec<VisibleText> = texts
        .into_iter()
        .filter_map(|candidate| {
            Some(VisibleText {
                clip: candidate.clip,
                object: candidate.content.as_text()?.object.clone(),
            })
        })
        .collect();

    let Some(video) = front_video else {
        let black = PictureSection {
            first_timeline_frame: first,
            end_timeline_frame: end,
            video: None,
            texts,
        };
        return (vec![black], 0);
    };
    let Some(media) = video.content.as_media() else {
        return (Vec::new(), 0);
    };

    // The source frame the section starts at, and how many of its pictures the file has.
    let speed = u64::from(video.effective_speed.value());
    let first_source_frame = media.first_source_frame().value() + (first - video.first_timeline_frame) * speed;
    let limit = media.end_source_frame().value().min(video.source_frame_count);
    let wanted_frame_count = end - first;
    let available_frame_count = if first_source_frame >= limit {
        0
    } else {
        (limit - first_source_frame).div_ceil(speed).min(wanted_frame_count)
    };

    // The range ends where the last picture's group of source frames does, but not beyond what
    // the file (and the clip) have, so that a whole clip is cut exactly as the clip is.
    let seen = VisibleVideo {
        clip: video.clip,
        source: media.source(),
        first_source_frame,
        end_source_frame: (first_source_frame + available_frame_count * speed).min(limit),
        speed: video.effective_speed,
    };
    let mut sections = Vec::new();
    if available_frame_count > 0 {
        sections.push(PictureSection {
            first_timeline_frame: first,
            end_timeline_frame: first + available_frame_count,
            video: Some(seen),
            texts: texts.clone(),
        });
    }
    if available_frame_count < wanted_frame_count {
        // The file ran out: black for the rest, with the same text.
        sections.push(PictureSection {
            first_timeline_frame: first + available_frame_count,
            end_timeline_frame: end,
            video: None,
            texts,
        });
    }
    (sections, wanted_frame_count - available_frame_count)
}

/// Adds `section` after the others, joined to the one before it if they show the same things and
/// run on from one another.
fn push_merging_with_the_one_before(sections: &mut Vec<PictureSection>, section: PictureSection) {
    if let Some(previous) = sections.last_mut()
        && previous.end_timeline_frame == section.first_timeline_frame
        && previous.texts == section.texts
    {
        match (previous.video, section.video) {
            (None, None) => {
                previous.end_timeline_frame = section.end_timeline_frame;
                return;
            }
            (Some(before), Some(after)) if before.clip == after.clip && runs_on(before, previous, after) => {
                previous.video = Some(VisibleVideo {
                    end_source_frame: after.end_source_frame,
                    ..before
                });
                previous.end_timeline_frame = section.end_timeline_frame;
                return;
            }
            _ => {}
        }
    }
    sections.push(section);
}

/// Whether `after` shows the pictures that come right after those `before` shows in `section`.
fn runs_on(before: VisibleVideo, section: &PictureSection, after: VisibleVideo) -> bool {
    before.speed == after.speed
        && after.first_source_frame
            == before.first_source_frame + section.timeline_frame_count() * u64::from(before.speed.value())
}

#[cfg(test)]
mod tests;
