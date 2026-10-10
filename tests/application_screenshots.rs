//! Pictures of the real window in several states, for looking at the interface without opening it
//! on a desktop. These are not checks, so they do nothing unless asked:
//!
//! ```text
//! FASTFORWARDRS_SCREENSHOT_FOLDER=some/folder cargo test --test application_screenshots -- --ignored
//! ```
//!
//! They need ffmpeg and a graphics adapter (a software one will do).

mod journey;
mod support;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard};

use egui::{PointerButton, vec2};
use fastforwardrs::media::FfmpegToolchain;
use journey::Journey;

const SCREENSHOT_FOLDER_ENVIRONMENT_VARIABLE: &str = "FASTFORWARDRS_SCREENSHOT_FOLDER";

/// Drawing a window for a picture sets up a graphics device, and making two at once in one process
/// crashes on some systems (it did on the development machine). So the tests take turns.
fn take_turn_at_drawing() -> MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn screenshot_folder() -> PathBuf {
    let folder = std::env::var_os(SCREENSHOT_FOLDER_ENVIRONMENT_VARIABLE)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {SCREENSHOT_FOLDER_ENVIRONMENT_VARIABLE} to the folder the pictures go in"));
    std::fs::create_dir_all(&folder).expect("could not make the screenshot folder");
    folder
}

/// A colourful 16 second video (ffmpeg's `testsrc2`) with sound, made in `folder`.
fn colourful_video(toolchain: &FfmpegToolchain, folder: &Path) -> PathBuf {
    let path = folder.join("colourful_16_seconds.mp4");
    if !path.exists() {
        let status = Command::new(toolchain.ffmpeg_path())
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args(["-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=30:duration=16"])
            .args(["-f", "lavfi", "-i", "sine=frequency=330:sample_rate=48000:duration=16"])
            .args([
                "-c:v", "libx264", "-preset", "veryfast", "-crf", "20", "-pix_fmt", "yuv420p", "-g", "30",
            ])
            .args(["-c:a", "aac", "-shortest"])
            .arg(&path)
            .status()
            .expect("could not run ffmpeg");
        assert!(status.success());
    }
    path
}

fn save(journey: &mut Journey, folder: &Path, name: &str) {
    journey.settle();
    let picture = journey.harness.render().expect("the window could be drawn");
    picture
        .save(folder.join(format!("{name}.png")))
        .expect("could not save the picture");
}

#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_screenshots_of_the_window() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let video = colourful_video(&toolchain, &folder);
    let mut journey = Journey::importing(&toolchain, &video);

    save(&mut journey, &folder, "1_just_imported");

    // Cut, and speed up the middle part.
    journey.cut_at_frame(60);
    journey.cut_at_frame(150);
    journey.drag_the_speed_slider_up(105, 150.0);
    journey.click_the_scrub_slider_at(0.1);
    journey.wait_for_the_preview_to_show_the_current_frame();
    save(&mut journey, &folder, "2_playhead_before_the_sped_up_clip");

    // In the sped up clip (timeline frames 60 to 75 of 405): the symbol is on the preview.
    journey.click_the_scrub_slider_at(0.166);
    journey.wait_for_the_preview_to_show_the_current_frame();
    save(&mut journey, &folder, "3_inside_the_sped_up_clip");

    // The speed slider while it is being dragged (the button is held down).
    let press = journey.point_on_the_track_at_frame(30);
    journey.move_pointer_to(press);
    journey.harness.event(egui::Event::PointerButton {
        pos: press,
        button: PointerButton::Secondary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    journey.harness.step();
    for step in 1..=12 {
        journey.move_pointer_to(press - vec2(0.0, 12.0 * step as f32));
    }
    save(&mut journey, &folder, "4_dragging_the_speed_slider");
}

#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_screenshots_of_the_missing_video_warning() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let video = colourful_video(&toolchain, &folder);
    let moved_video = folder.join("colourful_moved.mp4");
    std::fs::copy(&video, &moved_video).unwrap();
    let mut journey = Journey::importing(&toolchain, &moved_video);
    journey.cut_at_frame(90);
    journey
        .application_mut()
        .save_project_to(folder.join("missing_video.fastforwardrs.json"));
    drop(journey);
    std::fs::remove_file(&moved_video).unwrap();

    let mut reopened = Journey::start(&toolchain);
    reopened
        .application_mut()
        .load_project_from(folder.join("missing_video.fastforwardrs.json"));
    reopened.wait_until_idle();

    save(&mut reopened, &folder, "5_missing_video_warning");
}
