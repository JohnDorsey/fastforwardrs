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

use egui_kittest::kittest::Queryable;
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

/// A sound file of 10 seconds whose loudness rises and falls twice a second, so that its waveform
/// has a shape to look at.
fn pulsing_sound(toolchain: &FfmpegToolchain, folder: &Path) -> PathBuf {
    let path = folder.join("pulsing_10_seconds.wav");
    if !path.exists() {
        let status = Command::new(toolchain.ffmpeg_path())
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args(["-f", "lavfi", "-i", "sine=frequency=220:sample_rate=48000:duration=10"])
            .args(["-af", "tremolo=f=1.5:d=0.9,volume=7"])
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
    journey.set_the_speed_of_the_clip_at(105, 10);
    journey.click_the_scrub_slider_at(0.1);
    journey.wait_for_the_preview_to_show_the_current_frame();
    save(&mut journey, &folder, "2_playhead_before_the_sped_up_clip");

    // In the sped up clip (timeline frames 60 to 75 of 405): the symbol is on the preview.
    journey.click_the_scrub_slider_at(0.166);
    journey.wait_for_the_preview_to_show_the_current_frame();
    save(&mut journey, &folder, "3_inside_the_sped_up_clip");

    // The box of settings of a clip, open above it.
    journey.open_the_settings_of_the_clip_at(30);
    save(&mut journey, &folder, "4_the_settings_of_a_clip");
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

#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_screenshots_of_the_edit_menu_and_the_settings_windows() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let video = colourful_video(&toolchain, &folder);
    let mut journey = Journey::importing(&toolchain, &video);
    journey.cut_at_frame(60);

    journey.harness.get_by_label("Edit").click();
    save(&mut journey, &folder, "6_edit_menu");

    // The global settings, with autosave ticked.
    journey.harness.get_by_label("Global settings").click();
    journey.settle();
    journey.harness.get_by_label("Autosave").click();
    save(&mut journey, &folder, "7_global_settings_with_autosave_on");

    // The project's settings next to them: all of them follow the global ones (so are greyed out,
    // with autosave shown as on), then one of them is let go of.
    journey.harness.get_by_label("Edit").click();
    journey.settle();
    journey.harness.get_by_label("Project settings").click();
    save(&mut journey, &folder, "8_both_windows_the_project_follows_everything");
    journey
        .harness
        .get_all_by_label("Use global setting")
        .nth(1)
        .unwrap()
        .click();
    save(&mut journey, &folder, "9_the_project_has_a_setting_of_its_own");
}

#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_screenshots_of_tracks_sound_and_text() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let video = colourful_video(&toolchain, &folder);
    let sound = pulsing_sound(&toolchain, &folder);
    let mut journey = Journey::importing(&toolchain, &video);
    journey.cut_at_frame(120);

    // A sound file goes on a track of its own, in front, and is drawn as its waveform.
    journey.move_playhead_to_frame(0);
    journey.import(&sound);
    save(&mut journey, &folder, "10_a_video_and_a_sound_on_two_tracks");

    // A third track, with a text object made from the menu that opens on an empty part of it.
    journey.add_a_track();
    let empty_part = journey.point_on_the_strip_at_frame(0, 60);
    journey.right_click(empty_part);
    save(&mut journey, &folder, "11_the_menu_on_an_empty_part_of_a_track");
    journey.harness.get_by_label_contains("New").click();
    journey.settle();
    save(&mut journey, &folder, "12_the_new_menu_opened");
    journey.harness.get_by_label("Text object").click();
    journey.settle();
    journey.replace_text_in_the_field_showing("Text", "Chapter one: the beach");
    journey.wait_until_idle();
    save(&mut journey, &folder, "13_a_text_object_and_the_window_it_is_edited_in");

    // The text object is selected, with a handle at each of its corners on the preview.
    journey.harness.get_by_label("Close window").click();
    journey.settle();
    save(&mut journey, &folder, "14_the_text_object_selected_on_the_preview");

    // The box of settings of a clip, above the clip, and the box of a track, above its gear.
    journey.move_playhead_to_frame(30);
    let on_the_video = journey.point_on_the_strip_at_frame(2, 60);
    journey.right_click(on_the_video);
    save(&mut journey, &folder, "15_the_settings_of_a_clip");
    journey.press_key(egui::Key::Escape);
    let gear = journey.rectangle_of("Settings of track 1").center();
    journey.click(gear);
    save(&mut journey, &folder, "16_the_settings_of_a_track");
    journey.press_key(egui::Key::Escape);

    // A clip that is selected has an outline.
    let on_the_sound = journey.point_on_the_strip_at_frame(1, 90);
    journey.click(on_the_sound);
    save(&mut journey, &folder, "17_a_selected_clip");

    // The project, for opening in the real program.
    journey
        .application_mut()
        .save_project_to(folder.join("demo.fastforwardrs.json"));
}

#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_screenshots_of_many_tracks_and_missing_files() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let video = colourful_video(&toolchain, &folder);
    let sound = folder.join("pulsing_to_go_missing.wav");
    std::fs::copy(pulsing_sound(&toolchain, &folder), &sound).unwrap();
    let mut journey = Journey::importing(&toolchain, &video);
    journey.import(&sound);
    for _ in 0..5 {
        journey.add_a_track();
    }
    journey.make_a_text_object_at(1, 30);
    journey.wait_until_idle();
    journey.harness.get_by_label("Close window").click();
    save(&mut journey, &folder, "18_seven_tracks_the_rows_scroll");

    // A project whose sound file cannot be found.
    journey
        .application_mut()
        .save_project_to(folder.join("missing_sound.fastforwardrs.json"));
    drop(journey);
    std::fs::remove_file(&sound).unwrap();
    let mut reopened = Journey::start(&toolchain);
    reopened
        .application_mut()
        .load_project_from(folder.join("missing_sound.fastforwardrs.json"));
    reopened.wait_until_idle();
    save(&mut reopened, &folder, "19_a_sound_file_that_cannot_be_found");

    // Its clip says so, and its box offers to choose the file again. (Its track is below the
    // others that are shown, so the rows are scrolled to it first, as a person would with the scroll bar.)
    reopened.harness.get_by_label("Strip of track 1").scroll_to_me();
    reopened.settle();
    save(&mut reopened, &folder, "20_the_rows_scrolled_down_to_the_sound");
    let missing_clip = reopened.point_on_the_strip_at_frame(5, 100);
    reopened.right_click(missing_clip);
    save(&mut reopened, &folder, "21_the_box_of_a_clip_whose_file_is_missing");
}

#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_screenshots_of_zoom_and_track_heights() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let video = colourful_video(&toolchain, &folder);
    let sound = pulsing_sound(&toolchain, &folder);
    let mut journey = Journey::importing(&toolchain, &video);
    journey.cut_at_frame(120);
    journey.move_playhead_to_frame(0);
    journey.import(&sound);
    journey.move_playhead_to_frame(60);
    save(&mut journey, &folder, "20_the_normal_zoom_and_height");

    // A tall video track, 50% taller than a new one, and a short sound track (as short as it goes).
    let boundary = journey.boundary_under_the_track(1);
    journey.drag(egui::PointerButton::Primary, boundary, boundary + egui::vec2(0.0, 80.0));
    let boundary = journey.boundary_under_the_track(0);
    journey.drag(
        egui::PointerButton::Primary,
        boundary,
        boundary - egui::vec2(0.0, 150.0),
    );
    journey.wait_until_idle();
    save(&mut journey, &folder, "21_a_tall_video_track_and_a_short_sound_track");

    // The video track made short too: the pictures get narrower and are taken more often.
    let boundary = journey.boundary_under_the_track(1);
    journey.drag(
        egui::PointerButton::Primary,
        boundary,
        boundary - egui::vec2(0.0, 140.0),
    );
    journey.wait_until_idle();
    save(&mut journey, &folder, "22_both_tracks_short");

    // Zoomed out far: labels in minutes; then zoomed in far: a mark for every frame.
    let over_the_strip = journey.point_on_the_strip_at_frame(1, 60);
    journey.scroll_the_wheel(over_the_strip, egui::vec2(0.0, -300.0), egui::Modifiers::COMMAND);
    save(&mut journey, &folder, "23_zoomed_out");
    journey.scroll_the_wheel(over_the_strip, egui::vec2(0.0, 1500.0), egui::Modifiers::COMMAND);
    journey.wait_until_idle();
    save(&mut journey, &folder, "24_zoomed_all_the_way_in");
}

#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_screenshots_of_turned_and_resized_videos_and_text() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let video = colourful_video(&toolchain, &folder);
    let mut journey = Journey::importing(&toolchain, &video);
    journey.move_playhead_to_frame(60);
    journey.wait_for_the_preview_to_show_the_current_frame();
    save(&mut journey, &folder, "30_the_video_before_anything_is_done_to_it");

    // A text object, turned by dragging round from just outside one of its corners.
    journey.add_a_track();
    journey.make_a_text_object_at(0, 0);
    journey.replace_text_in_the_field_showing("Text", "A caption, turned");
    journey.harness.get_by_label("Close window").click();
    journey.settle();
    save(&mut journey, &folder, "31_the_text_selected_with_its_corners");
    let handle = |journey: &Journey, which: &str| {
        journey
            .rectangle_of(&format!("Resize handle at the {which} corner"))
            .center()
    };
    let (top_left, bottom_right) = (handle(&journey, "top left"), handle(&journey, "bottom right"));
    let middle = top_left + (bottom_right - top_left) * 0.5;
    let outside = bottom_right + (bottom_right - middle).normalized() * 18.0;
    let turned = |point: egui::Pos2, degrees: f32| {
        let (sine, cosine) = degrees.to_radians().sin_cos();
        let offset = point - middle;
        middle + egui::vec2(offset.x * cosine - offset.y * sine, offset.x * sine + offset.y * cosine)
    };
    journey.drag(egui::PointerButton::Primary, outside, turned(outside, -12.0));
    journey.wait_until_idle();
    save(&mut journey, &folder, "32_the_text_turned");

    // The video, picked by a click on it, made smaller by a corner, moved, and turned.
    let beside_the_text = journey.rectangle_of("Video, clip 0 of track 0").center() - egui::vec2(0.0, 120.0);
    journey.click(beside_the_text);
    journey.wait_until_idle();
    save(&mut journey, &folder, "33_the_video_picked");
    let (top_left, bottom_right) = (handle(&journey, "top left"), handle(&journey, "bottom right"));
    journey.drag(
        egui::PointerButton::Primary,
        bottom_right,
        top_left + (bottom_right - top_left) * 0.6,
    );
    journey.wait_until_idle();
    save(&mut journey, &folder, "34_the_video_made_smaller");
    let (top_left, bottom_right) = (handle(&journey, "top left"), handle(&journey, "bottom right"));
    let middle = top_left + (bottom_right - top_left) * 0.5;
    journey.drag(egui::PointerButton::Primary, middle, middle + egui::vec2(60.0, 10.0));
    journey.wait_until_idle();
    let (top_right, bottom_left) = (handle(&journey, "top right"), handle(&journey, "bottom left"));
    let middle = top_right + (bottom_left - top_right) * 0.5;
    let outside = top_right + (top_right - middle).normalized() * 18.0;
    let turned_about = |point: egui::Pos2, degrees: f32| {
        let (sine, cosine) = degrees.to_radians().sin_cos();
        let offset = point - middle;
        middle + egui::vec2(offset.x * cosine - offset.y * sine, offset.x * sine + offset.y * cosine)
    };
    journey.drag(egui::PointerButton::Primary, outside, turned_about(outside, 20.0));
    journey.wait_until_idle();
    save(&mut journey, &folder, "35_the_video_moved_and_turned");
}

#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_screenshots_of_a_picture_on_the_timeline_and_the_preview() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let video = colourful_video(&toolchain, &folder);
    let picture_file = folder.join("a_picture_800x400.png");
    if !picture_file.exists() {
        let status = Command::new(toolchain.ffmpeg_path())
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args(["-f", "lavfi", "-i", "testsrc=size=800x400:rate=1"])
            .args(["-frames:v", "1"])
            .arg(&picture_file)
            .status()
            .expect("could not run ffmpeg");
        assert!(status.success());
    }
    let mut journey = Journey::importing(&toolchain, &video);
    journey.move_playhead_to_frame(30);
    journey.import(&picture_file);
    journey.move_playhead_to_frame(60);
    journey.wait_for_the_preview_to_show_the_current_frame();
    save(&mut journey, &folder, "40_a_picture_imported_in_front_of_a_video");

    // Picked on the preview, made smaller by a corner, moved and turned.
    let corner = |journey: &Journey, which: &str| {
        journey
            .rectangle_of(&format!("Resize handle at the {which} corner"))
            .center()
    };
    let picture_on_the_preview = journey.rectangle_of("Picture, clip 0 of track 1").center();
    journey.click(picture_on_the_preview);
    let (top_left, bottom_right) = (corner(&journey, "top left"), corner(&journey, "bottom right"));
    journey.drag(
        egui::PointerButton::Primary,
        bottom_right,
        top_left + (bottom_right - top_left) * 0.45,
    );
    journey.wait_until_idle();
    let (top_left, bottom_right) = (corner(&journey, "top left"), corner(&journey, "bottom right"));
    let middle = top_left + (bottom_right - top_left) * 0.5;
    journey.drag(egui::PointerButton::Primary, middle, middle + egui::vec2(120.0, 60.0));
    journey.wait_until_idle();
    let (top_right, bottom_left) = (corner(&journey, "top right"), corner(&journey, "bottom left"));
    let middle = top_right + (bottom_left - top_right) * 0.5;
    let outside = top_right + (top_right - middle).normalized() * 18.0;
    let (sine, cosine) = (-25.0f32).to_radians().sin_cos();
    let offset = outside - middle;
    journey.drag(
        egui::PointerButton::Primary,
        outside,
        middle + egui::vec2(offset.x * cosine - offset.y * sine, offset.x * sine + offset.y * cosine),
    );
    journey.wait_until_idle();
    save(&mut journey, &folder, "41_the_picture_made_smaller_moved_and_turned");

    // Its box on the timeline.
    let on_the_picture_clip = journey.point_on_the_strip_at_frame(0, 90);
    journey.right_click(on_the_picture_clip);
    save(&mut journey, &folder, "42_the_box_of_a_picture_clip");
}

#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_screenshots_of_a_video_of_another_shape_over_one_that_fills_the_picture() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let landscape = colourful_video(&toolchain, &folder);
    let portrait = folder.join("portrait_16_seconds.mp4");
    if !portrait.exists() {
        let status = Command::new(toolchain.ffmpeg_path())
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args(["-f", "lavfi", "-i", "testsrc=size=360x640:rate=30:duration=16"])
            .args([
                "-c:v", "libx264", "-preset", "veryfast", "-crf", "20", "-pix_fmt", "yuv420p", "-g", "30",
            ])
            .arg(&portrait)
            .status()
            .expect("could not run ffmpeg");
        assert!(status.success());
    }
    let mut journey = Journey::importing(&toolchain, &landscape);
    journey.import(&portrait);
    journey.move_playhead_to_frame(60);
    journey.wait_for_the_preview_to_show_the_current_frame();
    save(&mut journey, &folder, "50_a_tall_video_in_front_of_a_wide_one");
}
