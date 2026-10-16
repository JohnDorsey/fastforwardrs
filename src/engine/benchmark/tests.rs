// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The benchmark does what it says: its scenes are a ladder with the passes they are said to have, it runs on the software adapter at a size that a processor
//! can make, and its report says what it found and whether the gate was made only when it is the gate.

use super::*;
use crate::engine::testing::software_graphics;

fn small() -> BenchmarkSettings {
    BenchmarkSettings {
        frame_size: (96, 54),
        frames_of_each_scene: 3,
        frames_to_leave_out_first: 1,
        frame_timeout: Duration::from_secs(60),
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
        vec![2, 2, 2, 3, 4, 3, 8],
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
    let Some(_graphics) = software_graphics() else { return };
    let settings = small();
    let report = run(&AdapterChoice::SoftwareOnly, &settings).unwrap();
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
}

#[test]
#[ignore = "takes minutes: the benchmark of the gate at 1080p on the software adapter, with its numbers printed (the example does the same on a graphics card)"]
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
fn the_gate_is_made_or_not_only_at_1080p_on_a_card_and_is_not_said_to_be_when_it_is_not_the_gate() {
    let gate = "the gate is 60 at 1080p on the integrated graphics: ";
    assert!(
        report_of((1920, 1080), false, 16)
            .describe()
            .contains(&format!("{gate}MADE"))
    );
    assert!(
        report_of((1920, 1080), false, 17)
            .describe()
            .contains(&format!("{gate}NOT MADE")),
        "58.8 to the second is not 60"
    );
    assert!(
        report_of((1920, 1080), true, 16)
            .describe()
            .contains(&format!("{gate}a software adapter is not the gate"))
    );
    assert!(
        report_of((1280, 720), false, 16)
            .describe()
            .contains(&format!("{gate}not at that size, so not the gate"))
    );
}
