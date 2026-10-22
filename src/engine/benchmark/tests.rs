// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The benchmark does what it says: its scenes are a ladder with the passes they are said to have, it runs on the software adapter at a size that a processor
//! can make, and its report says what it found and whether the gate was made only when it is the gate.

use super::*;
use crate::engine::testing::{really_software_graphics, software_graphics};

fn small() -> BenchmarkSettings {
    BenchmarkSettings {
        frame_size: (96, 54),
        frames_of_each_scene: 3,
        frames_to_leave_out_first: 1,
        frame_timeout: Duration::from_secs(60),
        frames_in_flight: 1,
    }
}

#[test]
fn the_scenes_are_a_ladder_and_each_takes_the_passes_that_it_is_made_of() {
    let known = known_effects();
    let scenes = the_scenes();
    let plans: Vec<_> = scenes
        .iter()
        .map(|scene| plan_frame(&scene.description((64, 36)), &known))
        .collect();
    // A black frame is cleared and made into its output; the layers are all put in their places by one pass; an effect that is a pass of its own is one pass,
    // and a blur is two (along the rows, and down the columns).
    assert_eq!(
        plans.iter().map(|plan| plan.passes_recorded()).collect::<Vec<_>>(),
        vec![2, 2, 2, 3, 4, 3, 8, 8],
        "{:?}",
        scenes.iter().map(|scene| scene.name).collect::<Vec<_>>()
    );
    assert!(
        plans.iter().all(|plan| plan.effects_not_run == 0),
        "every effect of every scene is one that the engine has"
    );
    assert_eq!(
        scenes.last().unwrap().layers.len(),
        4,
        "the whole stack has four layers: two blurs, a colour matrix and a lookup are done to them"
    );
}

#[test]
fn a_rectangle_that_is_not_turned_has_its_corners_in_the_order_the_engine_takes_them() {
    // (Numbers that are exact in binary, so that the corners are exact too.)
    assert_eq!(
        upright(0.25, 0.5, 0.125, 0.25),
        [(0.25, 0.5), (0.375, 0.5), (0.375, 0.75), (0.25, 0.75)],
        "top left, top right, bottom right, bottom left"
    );
}

#[test]
fn the_four_layers_of_the_whole_stack_are_the_four_pictures_each_in_a_place_of_its_own() {
    let scenes = the_scenes();
    let whole = scenes.last().unwrap();
    assert_eq!(
        whole.layers.iter().map(|layer| layer.picture).collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    let corners: Vec<_> = whole.layers.iter().map(|layer| layer.corners).collect();
    for (number, one) in corners.iter().enumerate() {
        for other in &corners[number + 1..] {
            assert_ne!(one, other, "two layers in the same place");
        }
    }
    // The ladder never takes a picture away: each scene that has layers has the ones that the one before it had, from the back.
    for scene in &scenes {
        let pictures: Vec<u64> = scene.layers.iter().map(|layer| layer.picture).collect();
        assert!(
            pictures.iter().zip(1..).all(|(picture, expected)| *picture == expected),
            "{}: {pictures:?}",
            scene.name
        );
    }
}

/// The frame that the engine makes of a picture of `picture_rgba` (`size` pixels) with `calls` done to it, on the software adapter, with the two shaders of the
/// benchmark given to the engine as they are given in `run`.
fn frame_of_the_picture_after(
    graphics: &crate::engine::testing::SoftwareGraphics,
    size: (u32, u32),
    picture_rgba: Vec<u8>,
    calls: Vec<EffectCall>,
) -> crate::engine::RenderedFrame {
    let mut renderer = FrameRenderer::new(graphics.device.clone());
    for (key, stage) in [(BLUR_KEY, BLUR), (LOOKUP_KEY, LOOKUP)] {
        let shader = guard_wgsl(&format!("{THE_BINDINGS}\n{stage}"), DEFAULT_MOST_PASSES_OF_A_LOOP).unwrap();
        renderer.install_effect(key, &shader).unwrap();
    }
    renderer
        .upload_picture(PictureId(1), &PictureData::rgba8(size.0, size.1, picture_rgba).unwrap())
        .unwrap();
    let mut description = FrameDescription::black(size);
    description.layers.push(LayerDescription {
        source: LayerSource::Picture(PictureId(1)),
        corners: upright(0.0, 0.0, 1.0, 1.0),
        effects: calls,
    });
    let (frame, statistics) = renderer.render(&description, Duration::from_secs(60)).unwrap();
    assert_eq!(statistics.effects_not_run, 0);
    frame
}

#[test]
fn the_blur_of_the_benchmark_leaves_a_picture_of_one_colour_as_it_is_and_smooths_an_edge_and_only_near_it() {
    let Some(graphics) = software_graphics() else { return };
    let size = (48, 8);
    let flat = [100, 150, 200, 255].repeat((size.0 * size.1) as usize);
    let blurred_both_ways = calls_of(Effect::Blur);
    assert_eq!(blurred_both_ways.len(), 2, "along the rows and down the columns");
    // Each pass is tried alone as well as both together: a pass that made the opposite of every colour would be undone by the other one (the engine keeps
    // colours below nothing as they are between passes).
    for (which, calls) in [
        ("along the rows alone", &blurred_both_ways[..1]),
        ("down the columns alone", &blurred_both_ways[1..]),
        ("both", &blurred_both_ways[..]),
    ] {
        let frame = frame_of_the_picture_after(&graphics, size, flat.clone(), calls.to_vec());
        for pixel in frame.rgba_pixels.as_chunks::<4>().0 {
            assert!(
                pixel[..3]
                    .iter()
                    .zip([100u8, 150, 200])
                    .all(|(made, wanted)| made.abs_diff(wanted) <= 1),
                "weights that add up to what they are divided by, {which}: {pixel:?}"
            );
        }
    }
    // Black on the left half, white on the right half.
    let mut edge = Vec::new();
    for _down in 0..size.1 {
        for across in 0..size.0 {
            let value = if across < size.0 / 2 { 0 } else { 255 };
            edge.extend([value, value, value, 255]);
        }
    }
    let frame = frame_of_the_picture_after(&graphics, size, edge, blurred_both_ways);
    let at = |across: u32| frame.pixel(across, 4)[0];
    assert!(
        at(size.0 / 2 - 1) > 40 && at(size.0 / 2 - 1) < 120,
        "just left of the edge it is part way: {}",
        at(size.0 / 2 - 1)
    );
    assert!(
        at(size.0 / 2) > 135 && at(size.0 / 2) < 215,
        "just right of it too: {}",
        at(size.0 / 2)
    );
    assert!(at(size.0 / 2 - 1) < at(size.0 / 2), "and it goes from dark to light");
    assert_eq!(
        at(size.0 / 2 - 14),
        0,
        "fourteen pixels from the edge nothing of the other side is in it (the blur reaches eight)"
    );
    assert_eq!(at(size.0 / 2 + 13), 255);
}

#[test]
fn the_lookup_stand_in_of_the_benchmark_leaves_a_picture_of_one_colour_as_it_is_whatever_it_mixes() {
    let Some(graphics) = software_graphics() else { return };
    let size = (24, 12);
    let flat = [200, 50, 120, 255].repeat((size.0 * size.1) as usize);
    for amount in [0.0, 0.5, 1.0] {
        let frame = frame_of_the_picture_after(
            &graphics,
            size,
            flat.clone(),
            vec![EffectCall {
                key: LOOKUP_KEY.to_owned(),
                values: vec![amount],
                table: None,
                state: None,
                images: Vec::new(),
            }],
        );
        for pixel in frame.rgba_pixels.as_chunks::<4>().0 {
            assert!(
                pixel[..3]
                    .iter()
                    .zip([200u8, 50, 120])
                    .all(|(made, wanted)| made.abs_diff(wanted) <= 1),
                "eight reads of the same colour, mixed by {amount}: {pixel:?}"
            );
        }
    }
}

#[test]
fn the_pictures_are_what_they_are_said_to_be_and_not_all_one_colour() {
    let pictures = pictures_of(64, 36).unwrap();
    assert_eq!(pictures.len(), 4);
    assert!(matches!(
        pictures[0].pixels,
        super::super::PicturePixels::Yuv420p8 { .. }
    ));
    assert!(matches!(
        pictures[1].pixels,
        super::super::PicturePixels::Yuv420p8 { .. }
    ));
    assert!(matches!(pictures[2].pixels, super::super::PicturePixels::Rgba8 { .. }));
    let super::super::PicturePixels::Rgba8 { straight_alpha } = &pictures[3].pixels else {
        panic!("the fourth is not a picture of bytes")
    };
    assert_eq!(
        (pictures[3].width_in_pixels, pictures[3].height_in_pixels),
        (21, 12),
        "a third of the frame each way"
    );
    assert!(
        straight_alpha.as_chunks::<4>().0.iter().all(|pixel| pixel[3] >= 0x40),
        "the smaller one is see-through, and none of it is all clear"
    );
    assert!(straight_alpha.as_chunks::<4>().0.iter().any(|pixel| pixel[3] < 255));
    let super::super::PicturePixels::Yuv420p8 { y, .. } = &pictures[0].pixels else {
        panic!("not a video picture")
    };
    let mut seen = [false; 256];
    for number in y {
        seen[usize::from(*number)] = true;
    }
    assert!(
        seen.iter().filter(|seen| **seen).count() > 20,
        "a picture has many brightnesses"
    );
    // Two runs make the same pictures, and the two videos are not the same picture.
    assert_eq!(pictures_of(64, 36).unwrap()[0], pictures[0]);
    assert_ne!(pictures[0], pictures[1]);
}

#[test]
fn the_benchmark_runs_every_scene_on_the_software_adapter_and_says_what_it_found() {
    let Some(_graphics) = really_software_graphics() else {
        return;
    };
    for frames_in_flight in [1, 2, 3] {
        let mut settings = small();
        settings.frames_in_flight = frames_in_flight;
        run_every_scene_and_check_the_report(&settings);
    }
}

fn run_every_scene_and_check_the_report(settings: &BenchmarkSettings) {
    let report = run(&AdapterChoice::SoftwareOnly, settings).unwrap();
    assert_eq!(report.frames_in_flight, settings.frames_in_flight);
    assert!(report.is_a_software_adapter);
    assert_eq!(report.frame_size, (96, 54));
    assert_eq!(report.scenes.len(), the_scenes().len());
    let known = known_effects();
    for (scene, made) in the_scenes().iter().zip(&report.scenes) {
        assert_eq!(made.name, scene.name);
        assert_eq!(
            made.frames, 3,
            "{}: the frames that are timed are the ones after the first",
            made.name
        );
        assert_eq!(
            made.passes,
            plan_frame(&scene.description((96, 54)), &known).passes_recorded()
        );
        assert!(made.fastest <= made.mean && made.mean <= made.slowest, "{made:?}");
        assert!(made.mean > Duration::ZERO && made.frames_per_second() > 0.0);
        assert!(made.mean_waiting_for_the_card <= made.slowest, "{made:?}");
    }
    assert!(report.time_to_send_a_video_picture > Duration::ZERO);
    let text = report.describe();
    for scene in the_scenes() {
        assert!(text.contains(scene.name), "{text}");
    }
    assert!(text.contains("software adapter"), "{text}");
    assert!(text.contains("96 by 54"), "{text}");
    assert_eq!(text.contains("one at a time"), settings.frames_in_flight == 1, "{text}");
    assert_eq!(
        text.contains(&format!("{} on their way at once", settings.frames_in_flight)),
        settings.frames_in_flight > 1,
        "{text}"
    );
}

#[test]
fn only_the_last_scene_sends_new_pictures_of_video_and_it_is_the_whole_stack_otherwise() {
    let scenes = the_scenes();
    let (last, others) = scenes.split_last().unwrap();
    assert!(last.sends_video_pictures);
    assert!(others.iter().all(|scene| !scene.sends_video_pictures));
    let whole_stack = &others[others.len() - 1];
    let layers_of = |scene: &Scene| -> Vec<String> {
        scene
            .layers
            .iter()
            .map(|layer| format!("{} {:?} {}", layer.picture, layer.corners, layer.effects.len()))
            .collect()
    };
    assert_eq!(
        layers_of(last),
        layers_of(whole_stack),
        "the same stack, and the video is what is new"
    );
    assert!(last.name.contains("whole stack") && last.name.contains("every frame"));
}

#[test]
fn pictures_that_are_sent_before_every_frame_reuse_the_textures_of_the_ones_before_and_frames_that_overlap_are_timed_between_one_finished_and_the_next()
 {
    let Some(graphics) = software_graphics() else { return };
    let mut renderer = FrameRenderer::new(graphics.device.clone());
    let pictures = pictures_of(96, 54).unwrap();
    let video: Vec<(PictureId, &PictureData)> = pictures
        .iter()
        .enumerate()
        .take(2)
        .map(|(number, picture)| (PictureId(number as u64 + 1), picture))
        .collect();
    for (number, picture) in pictures.iter().enumerate() {
        renderer.upload_picture(PictureId(number as u64 + 1), picture).unwrap();
    }
    let scene = the_scenes().pop().unwrap();
    let description = scene.description((96, 54));
    for frames_in_flight in [1, 2] {
        let mut settings = small();
        settings.frames_in_flight = frames_in_flight;
        settings.frames_of_each_scene = 4;
        // (The first frames make the textures there are to be; the ones after them make none.)
        time_the_frames_of(&mut renderer, &description, Some(&video), &settings).unwrap();
        let made_after_the_first = renderer.pool_statistics().made;
        let reused_after_the_first = renderer.pool_statistics().reused;
        let (times, waiting) = time_the_frames_of(&mut renderer, &description, Some(&video), &settings).unwrap();
        assert_eq!((times.len(), waiting.len()), (4, 4), "{frames_in_flight} on their way");
        assert!(times.iter().all(|time| *time > Duration::ZERO));
        assert_eq!(
            renderer.pool_statistics().made,
            made_after_the_first,
            "{frames_in_flight} on their way: sending pictures again made no new texture"
        );
        assert!(
            renderer.pool_statistics().reused >= reused_after_the_first + 5 * 8,
            "{frames_in_flight} on their way: the pictures were sent again"
        );
    }
    // Without pictures to send, nothing is sent.
    let reused_before = renderer.pool_statistics().reused;
    let mut settings = small();
    settings.frames_in_flight = 1;
    time_the_frames_of(&mut renderer, &description, None, &settings).unwrap();
    let reused_by_frames_alone = renderer.pool_statistics().reused - reused_before;
    assert!(reused_by_frames_alone <= 4 * 6, "{reused_by_frames_alone}");
}

#[test]
#[ignore = "takes a while: the benchmark of the gate (720p) on the software adapter, with its numbers printed (the example does the same on a graphics card)"]
fn the_benchmark_of_the_gate_on_the_software_adapter_with_its_numbers_printed() {
    let Some(_graphics) = software_graphics() else { return };
    let mut settings = BenchmarkSettings::for_the_gate();
    settings.frames_of_each_scene = 10;
    settings.frames_to_leave_out_first = 2;
    println!("{}", run(&AdapterChoice::SoftwareOnly, &settings).unwrap().describe());
}

// ---- the report

fn scene_of(mean_in_milliseconds: u64) -> SceneTimes {
    SceneTimes {
        name: "the whole stack",
        passes: 8,
        frames: 10,
        mean: Duration::from_millis(mean_in_milliseconds),
        fastest: Duration::from_millis(mean_in_milliseconds.saturating_sub(1)),
        slowest: Duration::from_millis(mean_in_milliseconds + 2),
        mean_waiting_for_the_card: Duration::from_millis(mean_in_milliseconds / 2),
    }
}

fn report_of(frame_size: (u32, u32), is_a_software_adapter: bool, mean_in_milliseconds: u64) -> BenchmarkReport {
    BenchmarkReport {
        adapter_name: "A Graphics Adapter".to_owned(),
        backend: "Vulkan".to_owned(),
        is_a_software_adapter,
        frame_size,
        frames_of_each_scene: 10,
        frames_in_flight: 1,
        time_to_send_a_video_picture: Duration::from_micros(12_300),
        scenes: vec![scene_of(mean_in_milliseconds)],
    }
}

#[test]
fn frames_to_the_second_are_one_over_the_mean_time() {
    assert_eq!(scene_of(20).frames_per_second(), 50.0);
    assert_eq!(scene_of(10).frames_per_second(), 100.0);
    assert!(
        scene_of(0).frames_per_second().is_finite(),
        "a frame that took no time does not divide by nothing"
    );
}

#[test]
fn the_report_gives_the_numbers_of_every_scene_in_milliseconds_and_says_what_it_was_run_on() {
    let text = report_of((1920, 1080), false, 20).describe();
    assert!(text.contains("A Graphics Adapter (Vulkan)"), "{text}");
    assert!(!text.contains("a software adapter"), "{text}");
    assert!(text.contains("1920 by 1080 pixels, 10 timed of each scene"), "{text}");
    assert!(text.contains("12.3 ms"), "{text}");
    let row = text
        .lines()
        .find(|line| line.contains("the whole stack  "))
        .unwrap_or_else(|| panic!("{text}"));
    let numbers: Vec<&str> = row.split_whitespace().rev().take(5).collect();
    // waiting, worst, best, mean, frames/s (the last five of the row, from the right)
    assert_eq!(numbers, vec!["10.00", "22.00", "19.00", "20.00", "50.0"], "{row}");
}

#[test]
fn the_gate_is_made_or_not_only_at_720p_on_a_card_and_is_not_said_to_be_when_it_is_not_the_gate() {
    // (The gate was 1080p until 2026-10-16, when the maintainer's measurements on the integrated graphics, 44 to 50 frames to the second at 1080p, led them to
    // make it 720p, the size that the preview is made at: see `FRAME_SIZE_OF_THE_GATE`.)
    assert_eq!(FRAME_SIZE_OF_THE_GATE, (1280, 720));
    assert_eq!(BenchmarkSettings::for_the_gate().frame_size, FRAME_SIZE_OF_THE_GATE);
    let gate = "the gate is 60 at 720p on the integrated graphics: ";
    assert!(
        report_of((1280, 720), false, 16)
            .describe()
            .contains(&format!("{gate}MADE"))
    );
    assert!(
        report_of((1280, 720), false, 17)
            .describe()
            .contains(&format!("{gate}NOT MADE")),
        "58.8 to the second is not 60"
    );
    assert!(
        report_of((1280, 720), true, 16)
            .describe()
            .contains(&format!("{gate}a software adapter is not the gate"))
    );
    assert!(
        report_of((1920, 1080), false, 16)
            .describe()
            .contains(&format!("{gate}not at that size, so not the gate")),
        "1080p is measured with --size, and says what it makes, and is not the gate"
    );
}
