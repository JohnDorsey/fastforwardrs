// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the picture plan makes of a number of a shader that is read from a mask that is a track (`docs/shader-masks-design.md`): **the mask is the picture of the track on its own**, made
//! whether or not the track's eye is open, and it is in the sections where an effect reads it; a section is cut where the mask changes; a mask can be made of masks; and a ring of tracks that
//! read each other, which an edit cannot make but a clip that is moved can, is cut where it closes.

use super::tests::{frame, long_video, plan, text};
use super::*;
use crate::composition::test_projects::project_of;
use crate::project::{ClipMove, DrivenValue, MaskChannel, Member};

/// A shader with two numbers that have a range: `amount` (0 to 1, 0.25 to begin with) and `strength` (2 to 6, 3 to begin with).
const TINT: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"amount\", \"TYPE\": \"float\", \"DEFAULT\": 0.25 }, { \"NAME\": \"strength\", \"TYPE\": \"float\", \"MIN\": 2.0, \"MAX\": 6.0, \"DEFAULT\": 3.0 } ] }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(inputImage); }\n";

fn the_shader() -> EffectKind {
    let file = crate::isf::read(TINT).unwrap();
    EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "tint.fs", TINT)).unwrap()
}

/// Puts the shader on the clip `clip`, with its number in place `slot` read from the track `source` (as the channel says, and `nothing` where the track has nothing).
fn read_from(project: &mut Project, clip: ClipId, slot: usize, source: TrackId, channel: MaskChannel, nothing: f32) {
    let instance = match project
        .chain_of_the_member(Member::Clip(clip))
        .unwrap()
        .iter()
        .find(|effect| effect.kind() == the_shader())
    {
        Some(effect) => effect.instance(),
        None => project.add_effect(AnimationTarget::Clip(clip), the_shader()).unwrap(),
    };
    project
        .set_driven_value(
            Member::Clip(clip),
            instance,
            slot,
            Some(DrivenValue::from_a_track(source, channel, nothing)),
        )
        .unwrap();
}

fn first_clip_of(project: &Project, position: usize) -> ClipId {
    project.tracks()[position].placement_at_index(0).unwrap().id
}

/// The tracks of the masks of a section, in order.
fn tracks_of_the_masks(section: &PictureSection) -> Vec<TrackId> {
    section.masks.iter().map(|mask| mask.track).collect()
}

fn the_shader_operation(section: &PictureSection, at: u64) -> (String, Vec<f32>, Vec<TrackId>) {
    let operations = section.layers.last().unwrap().operations_at(frame(at));
    match operations.as_slice() {
        [
            PictureOperation::Shader {
                key, numbers, masks, ..
            },
        ] => (key.clone(), numbers.clone(), masks.clone()),
        other => panic!("one operation of a shader was expected: {other:?}"),
    }
}

#[test]
fn the_picture_of_a_track_whose_eye_is_closed_is_a_mask_of_the_sections_where_an_effect_reads_it_and_is_not_a_layer() {
    // A video in front (the mask), and a video behind it (a text in front of both is not needed: the front one is hidden, so the picture has no layer that covers it).
    let mut project = project_of(vec![vec![(0, long_video(0, 100, 1))], vec![(0, long_video(0, 100, 1))]]);
    let (mask_track, host_clip) = (project.tracks()[0].id(), first_clip_of(&project, 1));
    let the_masks_clip = first_clip_of(&project, 0);
    read_from(&mut project, host_clip, 0, mask_track, MaskChannel::Red, 0.75);
    project.set_track_hidden(mask_track, true).unwrap();

    let plan = plan(&project);

    assert_eq!(plan.sections.len(), 1);
    let section = &plan.sections[0];
    assert_eq!(
        section.layers.iter().map(|layer| layer.clip).collect::<Vec<_>>(),
        [host_clip],
        "the hidden track is not in the picture"
    );
    assert_eq!(tracks_of_the_masks(section), [mask_track]);
    let mask = &section.masks[0].section;
    assert_eq!(
        mask.layers.iter().map(|layer| layer.clip).collect::<Vec<_>>(),
        [the_masks_clip]
    );
    assert_eq!((mask.first_timeline_frame, mask.end_timeline_frame), (0, 100));
    assert!(mask.masks.is_empty(), "and it reads no mask");
    // Its video is a video of the plan like any other.
    assert_eq!(mask.video_layers().count(), 1);
    assert_eq!(section.all_video_layers().count(), 2, "the host's and the mask's");
}

#[test]
fn a_track_that_is_seen_and_is_a_mask_is_a_layer_and_a_mask() {
    // The mask is a text in front: it does not cover the picture, so the video behind it is seen.
    let mut project = project_of(vec![vec![(0, text(100))], vec![(0, long_video(0, 100, 1))]]);
    let (mask_track, host_clip) = (project.tracks()[0].id(), first_clip_of(&project, 1));
    read_from(&mut project, host_clip, 0, mask_track, MaskChannel::Brightness, 0.0);

    let plan = plan(&project);

    let section = &plan.sections[0];
    assert_eq!(section.layers.len(), 2, "the text is in the picture");
    assert_eq!(tracks_of_the_masks(section), [mask_track]);
    assert_eq!(section.masks[0].section.layers.len(), 1);
}

#[test]
fn a_section_that_no_effect_reads_a_mask_in_has_none_and_nothing_else_of_the_plan_changes() {
    let mut project = project_of(vec![vec![(30, text(30))], vec![(0, long_video(0, 100, 1))]]);
    let (mask_track, host_clip) = (project.tracks()[0].id(), first_clip_of(&project, 1));
    let without = plan(&project);
    project
        .add_effect(AnimationTarget::Clip(host_clip), the_shader())
        .unwrap();
    let with_a_shader_that_reads_a_slider = plan(&project);

    assert!(without.sections.iter().all(|section| section.masks.is_empty()));
    assert!(
        with_a_shader_that_reads_a_slider
            .sections
            .iter()
            .all(|section| section.masks.is_empty())
    );
    assert_eq!(
        with_a_shader_that_reads_a_slider.sections.len(),
        without.sections.len(),
        "a shader whose numbers are on their sliders cuts nothing"
    );
    let _ = mask_track;
}

#[test]
fn a_section_is_cut_where_the_mask_changes_and_each_piece_has_the_mask_as_it_is_there() {
    // The mask track has a text from frame 20 to frame 60; the video behind it lasts 100 frames and is not cut by anything else.
    let mut project = project_of(vec![vec![(20, text(40))], vec![(0, long_video(0, 100, 1))]]);
    let (mask_track, host_clip) = (project.tracks()[0].id(), first_clip_of(&project, 1));
    read_from(&mut project, host_clip, 0, mask_track, MaskChannel::Red, 0.5);
    project.set_track_hidden(mask_track, true).unwrap();

    let plan = plan(&project);

    let found: Vec<(u64, u64, usize)> = plan
        .sections
        .iter()
        .map(|section| {
            (
                section.first_timeline_frame,
                section.end_timeline_frame,
                section.masks[0].section.layers.len(),
            )
        })
        .collect();
    assert_eq!(
        found,
        [(0, 20, 0), (20, 60, 1), (60, 100, 0)],
        "the mask has the text only from 20 to 60"
    );
    // (The video behind is one clip, and the pieces of it run on from one another: the stream of pictures goes on across the cuts.)
    let videos: Vec<(u64, u64)> = plan
        .sections
        .iter()
        .map(|section| {
            let video = section.video_layers().next().unwrap().1;
            (video.first_source_frame, video.end_source_frame)
        })
        .collect();
    assert_eq!(videos, [(0, 20), (20, 60), (60, 100)]);
}

#[test]
fn a_video_that_is_a_mask_keeps_its_one_range_in_every_piece_that_the_host_is_cut_into() {
    // The host is cut by a text on a third track at frames 50 to 70; the mask is a video of 100 frames on its own track.
    let mut project = project_of(vec![
        vec![(50, text(20))],
        vec![(0, long_video(0, 100, 1))],
        vec![(0, long_video(10, 110, 1))],
    ]);
    let (mask_track, host_clip) = (project.tracks()[1].id(), first_clip_of(&project, 2));
    read_from(&mut project, host_clip, 0, mask_track, MaskChannel::Green, 0.0);
    project.set_track_hidden(mask_track, true).unwrap();

    let plan = plan(&project);

    assert_eq!(plan.sections.len(), 3, "cut by the text");
    let ranges: Vec<(u64, u64, u64, u64)> = plan
        .sections
        .iter()
        .map(|section| {
            let mask = &section.masks[0].section;
            let video = mask.video_layers().next().unwrap().1;
            (
                mask.first_timeline_frame,
                mask.end_timeline_frame,
                video.first_source_frame,
                video.end_source_frame,
            )
        })
        .collect();
    assert_eq!(
        ranges,
        [(0, 100, 0, 100); 3],
        "the mask is one stretch of its video in each: nothing makes its stream start again"
    );
}

#[test]
fn a_video_that_is_seen_and_is_a_mask_shows_the_same_frame_of_its_source_in_the_picture_and_in_the_mask_in_every_piece_the_host_is_cut_into()
 {
    // The host (a video on the middle track) is cut by a text at frames 50 to 70; the track behind it is a video of 100 frames that is seen, and is the host's mask.
    let mut project = project_of(vec![
        vec![(50, text(20))],
        vec![(0, long_video(10, 110, 1))],
        vec![(0, long_video(0, 100, 1))],
    ]);
    let (mask_track, host_clip) = (project.tracks()[2].id(), first_clip_of(&project, 1));
    read_from(&mut project, host_clip, 0, mask_track, MaskChannel::Green, 0.0);

    let plan = plan(&project);

    assert_eq!(plan.sections.len(), 3, "cut by the text");
    for section in &plan.sections {
        let (_, seen) = section
            .video_layers()
            .find(|(layer, _)| layer.track == mask_track)
            .expect("the track is seen: its video is a layer of the picture");
        let mask = &section.masks[0].section;
        let (_, in_the_mask) = mask.video_layers().next().expect("and the mask is made of it");
        let (first, end) = (section.first_timeline_frame, section.end_timeline_frame);
        for at in [first, first + (end - first) / 2, end - 1] {
            assert_eq!(
                section.source_frame_at(seen, frame(at)),
                mask.source_frame_at(in_the_mask, frame(at)),
                "timeline frame {at} of the piece {first}..{end}: one clip, one frame of its source, wherever it is used"
            );
        }
    }
}

#[test]
fn a_mask_is_made_of_masks_when_the_effects_of_its_track_read_one() {
    // Three tracks: the front one is the mask of the middle one, which is the mask of the back one (a video).
    let mut project = project_of(vec![
        vec![(0, long_video(0, 100, 1))],
        vec![(0, text(100))],
        vec![(0, long_video(0, 100, 1))],
    ]);
    let (front, middle) = (project.tracks()[0].id(), project.tracks()[1].id());
    let (middle_clip, back_clip) = (first_clip_of(&project, 1), first_clip_of(&project, 2));
    read_from(&mut project, middle_clip, 0, front, MaskChannel::Blue, 0.1);
    read_from(&mut project, back_clip, 0, middle, MaskChannel::Red, 0.2);
    project.set_track_hidden(front, true).unwrap();
    project.set_track_hidden(middle, true).unwrap();

    let plan = plan(&project);

    let section = &plan.sections[0];
    assert_eq!(tracks_of_the_masks(section), [middle]);
    let inside = &section.masks[0].section;
    assert_eq!(
        tracks_of_the_masks(inside),
        [front],
        "the mask of the middle track has the front track as its own mask"
    );
    assert_eq!(inside.masks[0].section.layers.len(), 1);
    assert_eq!(
        section.all_video_layers().count(),
        2,
        "the back video and the front one, which is in a mask of a mask"
    );
}

#[test]
fn tracks_that_read_each_other_because_a_clip_was_moved_do_not_make_the_plan_loop() {
    // `first` reads `second`; a clip on `third` reads `first` (which is allowed); then that clip is moved to `second`, which now reads `first`, which reads `second`.
    let mut project = project_of(vec![
        vec![(0, long_video(0, 100, 1))],
        vec![(0, long_video(0, 100, 1))],
        vec![(0, long_video(0, 100, 1))],
    ]);
    let (first, second, third) = (
        project.tracks()[0].id(),
        project.tracks()[1].id(),
        project.tracks()[2].id(),
    );
    let (on_first, on_third) = (first_clip_of(&project, 0), first_clip_of(&project, 2));
    read_from(&mut project, on_first, 0, second, MaskChannel::Red, 0.0);
    read_from(&mut project, on_third, 0, first, MaskChannel::Red, 0.0);
    project
        .move_clips(&[ClipMove {
            clip: on_third,
            to_track: second,
            start: frame(100),
        }])
        .unwrap();
    let _ = third;

    // (The plan is made at all.)
    let plan = plan(&project);

    assert!(!plan.sections.is_empty());
    // Where the ring closes, the track that is being made is a mask of nothing: `first` reads `second`, whose clip (moved there) reads `first`, which reads `second` again, and that is empty.
    let section = plan
        .sections
        .iter()
        .find(|section| !section.masks.is_empty())
        .expect("the first track reads the second");
    assert_eq!(tracks_of_the_masks(section), [second]);
    let second_in_first = &section.masks[0].section;
    for inner in second_in_first.masks.iter() {
        assert_ne!(
            inner.track, second,
            "a track is not its own mask, however far round it is"
        );
    }
}

#[test]
fn an_effect_with_a_number_read_from_a_mask_is_the_operation_of_a_shader_of_its_own_with_the_mask_and_the_number_of_the_call()
 {
    let mut project = project_of(vec![vec![(0, long_video(0, 100, 1))], vec![(0, long_video(0, 100, 1))]]);
    let (mask_track, host_clip) = (project.tracks()[0].id(), first_clip_of(&project, 1));
    project.set_track_hidden(mask_track, true).unwrap();
    let plain = {
        project
            .add_effect(AnimationTarget::Clip(host_clip), the_shader())
            .unwrap();
        let section_plan = super::tests::plan(&project);
        let (key, numbers, masks) = the_shader_operation(&section_plan.sections[0], 10);
        assert!(masks.is_empty());
        assert_eq!(numbers[4..], [0.25, 3.0], "the sliders");
        key
    };
    read_from(&mut project, host_clip, 0, mask_track, MaskChannel::Green, 0.75);

    let plan = plan(&project);

    let (key, numbers, masks) = the_shader_operation(&plan.sections[0], 10);
    assert_ne!(
        key, plain,
        "the shader that reads a mask is another shader to the engine"
    );
    assert!(key.starts_with(&plain), "and its key says which: {key}");
    assert_eq!(masks, [mask_track]);
    assert_eq!(
        numbers[4..],
        [0.75, 3.0],
        "the number of the driven value is what it is where the mask has nothing, and the other is its slider"
    );
    // The same at another frame.
    let (again, _, _) = the_shader_operation(&plan.sections[0], 80);
    assert_eq!(again, key);
}

#[test]
fn two_numbers_of_one_effect_read_from_one_track_are_one_mask_and_from_two_tracks_are_two_in_the_order_of_the_numbers()
{
    let mut project = project_of(vec![
        vec![(0, long_video(0, 100, 1))],
        vec![(0, long_video(0, 100, 1))],
        vec![(0, long_video(0, 100, 1))],
    ]);
    let (first, second) = (project.tracks()[0].id(), project.tracks()[1].id());
    let host_clip = first_clip_of(&project, 2);
    read_from(&mut project, host_clip, 1, first, MaskChannel::Red, 4.0);
    read_from(&mut project, host_clip, 0, second, MaskChannel::Blue, 0.5);
    project.set_track_hidden(first, true).unwrap();
    project.set_track_hidden(second, true).unwrap();
    let one_mask = {
        // (Both numbers from the first track.)
        let mut project = project.clone();
        read_from(&mut project, host_clip, 0, first, MaskChannel::Green, 0.5);
        let plan = super::tests::plan(&project);
        the_shader_operation(&plan.sections[0], 5)
    };

    let plan = plan(&project);

    let (key, numbers, masks) = the_shader_operation(&plan.sections[0], 5);
    assert_eq!(
        masks,
        [second, first],
        "the number in place 0 is read from the second track, so that is the first mask"
    );
    assert_eq!(numbers[4..], [0.5, 4.0]);
    assert_eq!(one_mask.2, [first], "and two numbers of one track are one mask");
    assert_ne!(one_mask.0, key, "and the other shader");
    assert_eq!(tracks_of_the_masks(&plan.sections[0]), [second, first]);
}
