// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading the files that are included in a project archive where they are in it, with the real ffmpeg: a stretch of an archive is
//! given to ffmpeg as its `subfile` protocol, and what ffmpeg makes of it must be what it makes of the file that was put in the
//! archive, for everything the editor asks of it: what a file is, its pictures, its sound, the thumbnails, the waveform and the
//! export.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fastforwardrs::composition::plan_sound;
use fastforwardrs::media::export::{
    ExportEvent, ExportResult, VideoExport, plan_export, sound_graph_specification, sources_of_sound,
};
use fastforwardrs::media::{
    AudioFormat, AudioStream, AudioStreamEvent, AudioStreamRequest, DecodedFrame, FfmpegToolchain, FrameStream,
    FrameStreamEvent, FrameStreamRequest, MediaLocation, PEAKS_PER_SECOND, PictureSize, ThumbnailEvent,
    ThumbnailExtraction, Waveform, WaveformEvent, WaveformExtraction, decode_picture, probe_source_media,
};
use fastforwardrs::project::{
    FrameRate, Settings, SourceFrameIndex, SourceMediaIdentifier, SpedUpClipSound, SpeedFactor, TimelineFrameIndex,
};
use fastforwardrs::project_file::archive::{
    EntryPlace, FileToInclude, append_to_archive, entry_place, resource_entry_name, write_archive,
};
use support::{
    counter_video, project_for, red_and_blue_picture, scratch_directory, tone_file, transparent_text_picture,
};

const TIME_ALLOWED: Duration = Duration::from_secs(60);

/// An archive made in `folder` with the files `files` in it (each is entry number 1, 2, ...), and where each of them is in it.
fn archive_of(folder: &Path, archive_name: &str, files: &[&Path]) -> (PathBuf, Vec<MediaLocation>) {
    fs::create_dir_all(folder).unwrap();
    let archive = std::path::absolute(folder.join(archive_name)).unwrap();
    let to_include: Vec<FileToInclude> = files
        .iter()
        .enumerate()
        .map(|(number, file)| FileToInclude {
            entry_name: resource_entry_name(
                SourceMediaIdentifier::new(number as u32 + 1),
                &file.file_name().unwrap().to_string_lossy(),
            ),
            source: (*file).into(),
        })
        .collect();
    write_archive(&archive, "{}", &to_include).unwrap();
    let locations = to_include
        .iter()
        .map(|file| place_in(&archive, &file.entry_name).as_a_location_in(&archive).unwrap())
        .collect();
    (archive, locations)
}

fn place_in(archive: &Path, entry_name: &str) -> EntryPlace {
    entry_place(archive, entry_name).unwrap()
}

fn frames_of(toolchain: &FfmpegToolchain, video: &MediaLocation, width: u32, height: u32) -> Vec<DecodedFrame> {
    let request = FrameStreamRequest {
        video: video.clone(),
        project_frame_rate: FrameRate::new(30, 1).unwrap(),
        video_frame_rate: FrameRate::new(30, 1).unwrap(),
        first_source_frame: SourceFrameIndex::new(100),
        source_frame_count: 40,
        speed_factor: SpeedFactor::new(1).unwrap(),
        video_display_size_in_pixels: (width, height),
        picture_width_in_pixels: width,
        picture_height_in_pixels: height,
    };
    let stream = FrameStream::start(toolchain, &request, Arc::new(|| {})).expect("the stream starts");
    let started = Instant::now();
    let mut frames = Vec::new();
    loop {
        match stream.try_next_event() {
            Some(FrameStreamEvent::Frame(frame)) => frames.push(frame),
            Some(FrameStreamEvent::Finished { error_output }) => {
                assert_eq!(error_output, None);
                return frames;
            }
            None => {
                assert!(started.elapsed() < TIME_ALLOWED, "the stream did not finish in time");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}

fn sound_of(toolchain: &FfmpegToolchain, source: &MediaLocation) -> Vec<f32> {
    let project = project_for(toolchain, &counter_video(toolchain));
    let plan = plan_sound(&project, TimelineFrameIndex::new(0), |_| true);
    let sources: Vec<_> = sources_of_sound(&plan).into_iter().collect();
    let request = AudioStreamRequest {
        source_files: vec![source.clone()],
        sound: sound_graph_specification(&plan, &sources, SpedUpClipSound::MaintainPitch),
        format: AudioFormat {
            sample_rate: 48_000,
            channel_count: 2,
        },
    };
    let stream = AudioStream::start(toolchain, &request, Arc::new(|| {})).expect("the stream starts");
    let started = Instant::now();
    let mut samples = Vec::new();
    loop {
        match stream.try_next_event() {
            Some(AudioStreamEvent::Samples(more)) => samples.extend(more),
            Some(AudioStreamEvent::Finished { error_output }) => {
                assert_eq!(error_output, None);
                return samples;
            }
            None => {
                assert!(started.elapsed() < TIME_ALLOWED, "the stream did not finish in time");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}

#[test]
fn a_video_in_an_archive_is_probed_as_the_video_that_was_put_in() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let (_, locations) = archive_of(scratch.path(), "trip.ffrs", &[&video.path]);

    let in_the_archive = probe_source_media(&toolchain, &locations[0]).expect("it opens in the archive");

    assert_eq!(in_the_archive, probe_source_media(&toolchain, &video.path).unwrap());
}

#[test]
fn the_pictures_of_a_video_in_an_archive_are_the_pictures_of_the_video_that_was_put_in() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let (_, locations) = archive_of(scratch.path(), "trip.ffrs", &[&video.path]);
    let (width, height) = (
        video.specification.width_in_pixels,
        video.specification.height_in_pixels,
    );

    let from_the_archive = frames_of(&toolchain, &locations[0], width, height);
    let from_the_file = frames_of(&toolchain, &MediaLocation::from(video.path.as_path()), width, height);

    assert_eq!(from_the_file.len(), 40);
    assert_eq!(from_the_archive.len(), 40);
    for (number, (in_the_archive, in_the_file)) in from_the_archive.iter().zip(&from_the_file).enumerate() {
        assert_eq!(in_the_archive.rgba_pixels, in_the_file.rgba_pixels, "picture {number}");
    }
}

#[test]
fn the_sound_of_a_video_in_an_archive_is_the_sound_of_the_video_that_was_put_in() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let (_, locations) = archive_of(scratch.path(), "trip.ffrs", &[&video.path]);

    let from_the_archive = sound_of(&toolchain, &locations[0]);
    let from_the_file = sound_of(&toolchain, &MediaLocation::from(video.path.as_path()));

    assert_eq!(from_the_file.len(), 20 * 48_000 * 2);
    assert_eq!(from_the_archive, from_the_file);
}

#[test]
fn the_thumbnails_of_a_video_in_an_archive_are_those_of_the_video_that_was_put_in() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let (_, locations) = archive_of(scratch.path(), "trip.ffrs", &[&video.path]);
    let size = PictureSize {
        width_in_pixels: 64,
        height_in_pixels: 36,
    };
    let thumbnails_of = |location: MediaLocation| {
        let extraction = ThumbnailExtraction::start(&toolchain, &location, size, Arc::new(|| {})).unwrap();
        let started = Instant::now();
        let mut pictures = Vec::new();
        loop {
            match extraction.try_next_event() {
                Some(ThumbnailEvent::Thumbnail { index, picture }) => {
                    pictures.push((index, picture.decompress().unwrap().rgba_pixels));
                }
                Some(ThumbnailEvent::Finished { error_output }) => {
                    assert_eq!(error_output, None);
                    return pictures;
                }
                None => {
                    assert!(started.elapsed() < TIME_ALLOWED, "the thumbnails were not made in time");
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        }
    };

    let from_the_archive = thumbnails_of(locations[0].clone());
    let from_the_file = thumbnails_of(MediaLocation::from(video.path.as_path()));

    assert_eq!(from_the_file.len(), 20);
    assert_eq!(from_the_archive, from_the_file);
}

#[test]
fn the_waveform_of_a_sound_in_an_archive_is_that_of_the_sound_that_was_put_in() {
    let toolchain = toolchain_or_skip!();
    let tone = tone_file(&toolchain, 440, 3);
    let scratch = scratch_directory();
    let (_, locations) = archive_of(scratch.path(), "trip.ffrs", &[&tone]);
    let waveform_of = |location: &MediaLocation| {
        let extraction = WaveformExtraction::start(&toolchain, location, Arc::new(|| {})).unwrap();
        let started = Instant::now();
        let mut waveform = Waveform::default();
        loop {
            match extraction.try_next_event() {
                Some(WaveformEvent::Peaks(peaks)) => waveform.extend(&peaks),
                Some(WaveformEvent::Finished { error_output }) => {
                    assert_eq!(error_output, None);
                    return waveform;
                }
                None => {
                    assert!(started.elapsed() < TIME_ALLOWED, "the waveform was not made in time");
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        }
    };

    let from_the_archive = waveform_of(&locations[0]);
    let from_the_file = waveform_of(&MediaLocation::from(tone.as_path()));

    assert!(from_the_file.peak_count() >= 3 * PEAKS_PER_SECOND as usize - 2);
    assert_eq!(from_the_archive.peaks(), from_the_file.peaks());
}

#[test]
fn a_picture_in_an_archive_is_decoded_as_the_picture_that_was_put_in() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let picture = red_and_blue_picture(&toolchain, scratch.path(), "red_and_blue.png", 64, 32);
    let (_, locations) = archive_of(scratch.path(), "trip.ffrs", &[&picture]);
    let (properties_in_the_archive, properties_of_the_file) = (
        probe_source_media(&toolchain, &locations[0]).unwrap(),
        probe_source_media(&toolchain, &picture).unwrap(),
    );

    let from_the_archive = decode_picture(&toolchain, &locations[0], (64, 32)).unwrap();
    let from_the_file = decode_picture(&toolchain, &picture, (64, 32)).unwrap();

    assert_eq!(properties_in_the_archive, properties_of_the_file);
    assert_eq!(from_the_archive.rgba_pixels, from_the_file.rgba_pixels);
}

#[test]
fn a_video_in_an_archive_is_exported_as_the_video_that_was_put_in_is() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let (_, locations) = archive_of(scratch.path(), "trip.ffrs", &[&video.path]);
    let project = project_for(&toolchain, &video);
    let plan = plan_export(&project, Settings::DEFAULT, &transparent_text_picture).unwrap();
    let export_from = |source: MediaLocation, name: &str| {
        let destination = scratch.path().join(name);
        let export =
            VideoExport::start(&toolchain, &plan, &[source], &destination, Arc::new(|| {})).expect("it starts");
        let started = Instant::now();
        loop {
            match export.try_next_event() {
                Some(ExportEvent::Finished(result)) => {
                    assert_eq!(result, ExportResult::Completed);
                    return destination;
                }
                Some(_) => {}
                None => {
                    assert!(
                        started.elapsed() < Duration::from_secs(180),
                        "the export did not finish in time"
                    );
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }
    };

    let from_the_archive = export_from(locations[0].clone(), "from the archive.mp4");
    let from_the_file = export_from(MediaLocation::from(video.path.as_path()), "from the file.mp4");

    let (decoded_from_the_archive, decoded_from_the_file) = (
        support::decode_video(&toolchain, &from_the_archive, 320, 180),
        support::decode_video(&toolchain, &from_the_file, 320, 180),
    );
    assert_eq!(decoded_from_the_file.pictures.len(), 600);
    assert_eq!(
        decoded_from_the_archive.frame_numbers(),
        decoded_from_the_file.frame_numbers()
    );
    assert_eq!(
        support::decoded_sound(&toolchain, &from_the_archive, 2, 48_000).len(),
        support::decoded_sound(&toolchain, &from_the_file, 2, 48_000).len()
    );
}

#[test]
fn a_file_in_an_archive_in_a_folder_with_a_strange_name_is_read() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let folder = scratch.path().join("Ferien, Österreich (1) 'a' 日本語 = &;");

    let (_, locations) = archive_of(&folder, "trip, v2 (final).ffrs", &[&video.path]);

    assert_eq!(
        probe_source_media(&toolchain, &locations[0]).expect("it opens in the archive"),
        probe_source_media(&toolchain, &video.path).unwrap()
    );
}

#[test]
fn what_is_added_to_an_archive_leaves_a_file_in_it_readable_where_it_was_read_from() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let (archive, locations) = archive_of(scratch.path(), "trip.ffrs", &[&video.path]);
    let before = frames_of(&toolchain, &locations[0], 320, 180);
    let entry = resource_entry_name(
        SourceMediaIdentifier::new(1),
        &video.path.file_name().unwrap().to_string_lossy(),
    );

    // A save that adds the project again, many times, as autosave does.
    for round in 0..5 {
        append_to_archive(
            &archive,
            &format!("{{\"round\": {round}}}"),
            std::slice::from_ref(&entry),
            &[],
            &mut |_| Ok(()),
        )
        .unwrap();
    }

    assert_eq!(
        place_in(&archive, &entry).as_a_location_in(&archive).unwrap(),
        locations[0]
    );
    let after = frames_of(&toolchain, &locations[0], 320, 180);
    assert_eq!(before.len(), after.len());
    for (number, (was, is)) in before.iter().zip(&after).enumerate() {
        assert_eq!(was.rgba_pixels, is.rgba_pixels, "picture {number}");
    }
}
