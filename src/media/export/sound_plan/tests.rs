// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the sound of a project is made from: which files, in which inputs, and what each clip of it asks for.

use super::*;
use crate::composition::plan_sound;
use crate::project::TimelineFrameIndex;
use crate::video_export::test_projects::{audio_source, clip_location, frame, project_with_video, speed};

fn sound_of(project: &crate::project::Project) -> SoundPlan {
    plan_sound(project, TimelineFrameIndex::new(0), |_| true)
}

#[test]
fn the_files_the_sound_is_made_from_are_those_of_the_clips_that_have_sound_and_not_the_ones_that_have_none() {
    let mut with_sound = project_with_video(320, 180, true);
    with_sound.import_source(audio_source(2, 4), frame(400)).unwrap();
    assert_eq!(
        sources_of_sound(&sound_of(&with_sound)),
        BTreeSet::from([SourceMediaIdentifier::new(1), SourceMediaIdentifier::new(2)])
    );
    let without_sound = project_with_video(320, 180, false);
    assert!(sources_of_sound(&sound_of(&without_sound)).is_empty());
}

#[test]
fn each_clip_of_the_graph_names_the_input_that_its_file_is_and_has_the_frames_the_speed_and_the_length_of_the_clip() {
    let mut project = project_with_video(320, 180, true);
    project.split_at(frame(100), None);
    let second = clip_location(&project, 0, 1);
    project.set_clip_speed(second, speed(4)).unwrap();
    project.import_source(audio_source(7, 4), frame(200)).unwrap();
    let plan = sound_of(&project);
    // The file of the sound file comes first among the inputs, and the video after it.
    let files = [SourceMediaIdentifier::new(7), SourceMediaIdentifier::new(1)];

    let specification = sound_graph_specification(&plan, &files, SpedUpClipSound::Mute);

    assert_eq!(specification.frame_rate, plan.frame_rate);
    assert_eq!(specification.total_frame_count, plan.total_frame_count);
    assert_eq!(specification.sped_up_clip_sound, SpedUpClipSound::Mute);
    let clips: Vec<&SoundClipToExport> = specification
        .strips
        .iter()
        .flat_map(|strip| &strip.items)
        .filter_map(|item| match item {
            SoundItemToExport::Clip(clip) => Some(clip),
            SoundItemToExport::Silence { .. } => None,
        })
        .collect();
    let video_clips: Vec<(usize, u64, u64, u32, u64)> = clips
        .iter()
        .filter(|clip| clip.source_input == 1)
        .map(|clip| {
            (
                clip.source_input,
                clip.first_source_frame,
                clip.end_source_frame,
                clip.speed_factor.value(),
                clip.timeline_frame_count,
            )
        })
        .collect();
    assert_eq!(
        video_clips,
        vec![(1, 0, 100, 1, 100), (1, 100, 300, 4, 50)],
        "the first clip is 100 frames of the video at 1x, and the second the 200 after them at 4x, which last 50"
    );
    assert_eq!(
        clips.iter().filter(|clip| clip.source_input == 0).count(),
        1,
        "the sound file is input 0"
    );
}

#[test]
#[should_panic(expected = "every file the sound is made from is one of the inputs")]
fn a_file_that_is_not_one_of_the_inputs_is_a_mistake_of_the_program() {
    let project = project_with_video(320, 180, true);
    let plan = sound_of(&project);
    let _ = sound_graph_specification(&plan, &[], SpedUpClipSound::Mute);
}
