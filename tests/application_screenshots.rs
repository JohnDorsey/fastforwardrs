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

use egui::Pos2;
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
    save_what_is_drawn(journey, folder, name);
}

/// Saves the picture of the frame that was drawn last, without drawing any more frames first.
fn save_what_is_drawn(journey: &mut Journey, folder: &Path, name: &str) {
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

    // The properties of a clip, in the panel at the left.
    journey.select_the_clip_at(30);
    save(&mut journey, &folder, "4_the_properties_of_a_clip");
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
    save(
        &mut journey,
        &folder,
        "13_a_text_object_and_its_properties_in_the_panel",
    );

    // The text object is selected, with a handle at each of its corners on the preview.
    journey.settle();
    save(&mut journey, &folder, "14_the_text_object_selected_on_the_preview");

    // The properties of a clip, and of a track, in the panel.
    journey.move_playhead_to_frame(30);
    let on_the_video = journey.point_on_the_strip_at_frame(2, 60);
    journey.click(on_the_video);
    save(&mut journey, &folder, "15_the_properties_of_a_clip");
    journey.select_the_track_at(1);
    save(&mut journey, &folder, "16_the_properties_of_a_track");

    // Several things selected: a list in a yellow outline.
    journey.click_with_control(on_the_video);
    save(&mut journey, &folder, "16_b_a_track_and_a_clip_selected");

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

    // Its clip says so, and its properties offer to choose the file again. (Its track is below the
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
fn write_screenshots_of_things_that_reach_out_past_the_edges_of_the_picture() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let video = colourful_video(&toolchain, &folder);
    let mut journey = Journey::importing(&toolchain, &video);
    journey.move_playhead_to_frame(60);
    journey.wait_for_the_preview_to_show_the_current_frame();
    let handle = |journey: &Journey, which: &str| {
        journey
            .rectangle_of(&format!("Resize handle at the {which} corner"))
            .center()
    };

    // The video, made smaller and then taken to the left until half of it is past the edge: its box
    // reaches over the properties panel, where its corners can be taken hold of as well.
    let on_the_video = journey.rectangle_of("Video, clip 0 of track 0").center();
    journey.click(on_the_video);
    let (top_left, bottom_right) = (handle(&journey, "top left"), handle(&journey, "bottom right"));
    journey.drag(
        egui::PointerButton::Primary,
        bottom_right,
        top_left + (bottom_right - top_left) * 0.6,
    );
    journey.wait_until_idle();
    let (top_left, bottom_right) = (handle(&journey, "top left"), handle(&journey, "bottom right"));
    let middle = top_left + (bottom_right - top_left) * 0.5;
    journey.drag(egui::PointerButton::Primary, middle, middle - egui::vec2(380.0, 0.0));
    journey.wait_until_idle();
    save(&mut journey, &folder, "36_a_video_taken_past_the_left_edge");

    // And a corner of it, over the properties panel, dragged on.
    let top_left = handle(&journey, "top left");
    journey.drag(
        egui::PointerButton::Primary,
        top_left,
        top_left + egui::vec2(60.0, 40.0),
    );
    journey.wait_until_idle();
    save(&mut journey, &folder, "37_its_corner_over_the_properties_dragged");

    // Text past the bottom right corner of the picture.
    journey.add_a_track();
    journey.make_a_text_object_at(0, 0);
    journey.replace_text_in_the_field_showing("Text", "Half of this is not seen");
    journey.settle();
    let (top_left, bottom_right) = (handle(&journey, "top left"), handle(&journey, "bottom right"));
    let middle = top_left + (bottom_right - top_left) * 0.5;
    journey.drag(egui::PointerButton::Primary, middle, middle + egui::vec2(500.0, 160.0));
    journey.wait_until_idle();
    save(&mut journey, &folder, "38_text_taken_past_the_bottom_right_corner");
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

    // Its properties in the panel.
    let on_the_picture_clip = journey.point_on_the_strip_at_frame(0, 90);
    journey.click(on_the_picture_clip);
    save(&mut journey, &folder, "42_the_properties_of_a_picture_clip");
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

#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_screenshots_of_a_cut_flashing() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let video = colourful_video(&toolchain, &folder);
    let mut journey = Journey::start_with_frames_lasting(&toolchain, 1.0 / 60.0);
    journey.import(&video);
    journey.move_playhead_to_frame(150);
    journey.wait_for_the_preview_to_show_the_current_frame();
    journey.settle();

    // The frames that follow the cut, as they are drawn: just after it, and then every five frames (a flash is
    // over in a third of a second, which is eighteen of them).
    journey.harness.key_press(egui::Key::S);
    journey.harness.step();
    save_what_is_drawn(&mut journey, &folder, "60_a_cut_flashing_1");
    for number in 2..=4 {
        journey.harness.run_steps(5);
        save_what_is_drawn(&mut journey, &folder, &format!("60_a_cut_flashing_{number}"));
    }
}

#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_screenshots_of_a_track_being_dragged_to_another_place() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let video = colourful_video(&toolchain, &folder);
    let mut journey = Journey::start_with_frames_lasting(&toolchain, 1.0 / 60.0);
    journey.import(&video);
    journey.add_a_track();
    // (A second click right after the first, which frames this short make, would be a double click.)
    journey.harness.run_steps(40);
    journey.add_a_track();
    journey.harness.run_steps(40);
    journey.move_playhead_to_frame(150);
    journey.wait_until_idle();
    save(&mut journey, &folder, "61_three_tracks_the_video_at_the_back");

    // Take hold of the box of the track at the back (the one with the video), a little way in from its left edge,
    // and drag it up over the one in front of it, and a little over the one in front of that.
    let strip = journey.strip_of_track(2);
    let from = Pos2::new(strip.left() - 154.0, strip.center().y);
    journey.drag_and_hold(egui::PointerButton::Primary, from, from - egui::vec2(0.0, 150.0));
    journey.harness.run_steps(30);
    save_what_is_drawn(&mut journey, &folder, "62_the_back_track_dragged_up_over_the_others");

    // Let go: the track goes into the place it is over, which is the first, and the rows settle.
    journey.release(egui::PointerButton::Primary, from - egui::vec2(0.0, 150.0));
    save_what_is_drawn(&mut journey, &folder, "63_just_let_go");
    journey.harness.run_steps(40);
    save_what_is_drawn(&mut journey, &folder, "64_settled_in_its_place");
}

#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_screenshots_of_drawers_on_clips() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let video = colourful_video(&toolchain, &folder);
    let mut journey = Journey::start_with_frames_lasting(&toolchain, 1.0 / 60.0);
    journey.import(&video);
    journey.cut_at_frame(40);
    journey.cut_at_frame(80);
    // A track in front of the video, with a text object on it from frame 20.
    journey.add_a_track();
    journey.make_a_text_object_at(0, 20);
    journey.harness.run_steps(40);
    journey.move_playhead_to_frame(60);
    journey.wait_until_idle();
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save(
        &mut journey,
        &folder,
        "70_no_buttons_for_the_drawers_while_nothing_is_hovered",
    );

    // The pointer over a clip of the video: its button is there, on the bottom edge.
    let video_strip = journey.strip_of_track(1);
    journey.move_pointer_to(egui::pos2(video_strip.left() + 200.0, video_strip.top() + 20.0));
    save_what_is_drawn(&mut journey, &folder, "71_the_button_of_a_clip_that_is_hovered");

    // Open the drawer of the text object, which is on the track in front: it opens over the video under it.
    let text = journey.application().project().tracks()[0]
        .placement_at_index(0)
        .unwrap()
        .id;
    let front_title = journey.application().project().tracks()[0].title().to_owned();
    journey.move_pointer_to(journey.strip_of_track(0).center());
    let button = journey.rectangle_of(&format!("Drawer of clip {} of {front_title}", text.number()));
    journey.click(button.center());
    journey.harness.run_steps(0);
    save_what_is_drawn(&mut journey, &folder, "72_the_drawer_of_the_text_has_begun_to_open");
    journey.harness.run_steps(40);
    save_what_is_drawn(
        &mut journey,
        &folder,
        "73_the_drawer_of_the_text_is_open_over_the_video",
    );

    // The drawer of the track in front, which is opened by the button on the bottom edge of its box.
    let button = journey.rectangle_of(&format!("Drawer of {front_title}"));
    journey.click(button.center());
    journey.harness.run_steps(40);
    save_what_is_drawn(&mut journey, &folder, "74_the_drawer_of_the_track_is_open_too");

    // A track as short as it can be, with the same clips.
    let boundary = journey.boundary_under_the_track(1);
    journey.drag(
        egui::PointerButton::Primary,
        boundary,
        boundary - egui::vec2(0.0, 150.0),
    );
    journey.harness.run_steps(40);
    save_what_is_drawn(&mut journey, &folder, "75_the_same_at_the_least_height_of_a_track");
}

#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_screenshots_of_racks_of_keyframes_in_drawers() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let video = colourful_video(&toolchain, &folder);
    let mut journey = Journey::start_with_frames_lasting(&toolchain, 1.0 / 60.0);
    journey.import(&video);
    journey.select_the_clip_at(10);
    journey.move_playhead_to_frame(30);
    journey.wait_until_idle();
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save(&mut journey, &folder, "90_a_video_selected_with_no_keyframes");

    // The stopwatch of the gain: a keyframe under the playhead, and the drawer of the clip opens with a rack for it.
    let stopwatch = journey.rectangle_of("Make a keyframe of gain here");
    journey.click(stopwatch.center());
    journey.harness.run_steps(40);
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save_what_is_drawn(&mut journey, &folder, "91_the_first_keyframe_in_a_new_rack");

    // More keyframes of the gain, and of the position and the size: a rack for each.
    journey.move_playhead_to_frame(90);
    journey.set_slider("Gain", -12.0);
    journey.move_playhead_to_frame(150);
    journey.set_slider("Gain", -3.0);
    for property in ["position", "size", "rotation"] {
        let stopwatch = journey.rectangle_of(&format!("Make a keyframe of {property} here"));
        journey.click(stopwatch.center());
    }
    journey.move_playhead_to_frame(220);
    for property in ["position", "rotation"] {
        let stopwatch = journey.rectangle_of(&format!("Make a keyframe of {property} here"));
        journey.click(stopwatch.center());
    }
    journey.harness.run_steps(40);
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save_what_is_drawn(&mut journey, &folder, "92_four_racks_in_the_drawer_of_a_video");

    // The pointer over a rack: the label to the left of it. And over a keyframe.
    let strip = journey.strip_of_track(0);
    let second_rack = egui::pos2(strip.left() + 120.0, strip.bottom() + 8.0 + 20.0 + 8.0);
    journey.move_pointer_to(second_rack);
    save_what_is_drawn(&mut journey, &folder, "93_the_label_of_a_rack_the_pointer_is_over");

    // A keyframe of the track in front, which opens the drawer of the track.
    journey.select_the_track_at(0);
    journey.move_playhead_to_frame(60);
    let stopwatch = journey.rectangle_of("Make a keyframe of gain here");
    journey.click(stopwatch.center());
    journey.harness.run_steps(40);
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save_what_is_drawn(&mut journey, &folder, "94_the_drawer_of_the_track_has_a_rack_too");
}

#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_screenshots_of_racks_being_dragged_and_keyframe_crates() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let video = colourful_video(&toolchain, &folder);
    let mut journey = Journey::start_with_frames_lasting(&toolchain, 1.0 / 60.0);
    journey.import(&video);
    // A track in front with a text object on it from frame 10 to frame 100.
    journey.add_a_track();
    journey.make_a_text_object_at(0, 10);
    journey.harness.run_steps(40);
    // Keyframes of the video: its gain at the start of the video, 3 seconds in and 5 seconds in.
    let on_the_video = journey.point_on_the_strip_at_frame(1, 100);
    journey.click(on_the_video);
    for frame in [30, 90, 150] {
        journey.move_playhead_to_frame(frame);
        let stopwatch = journey.rectangle_of("Make a keyframe of gain here");
        journey.click(stopwatch.center());
    }
    for frame in [60, 120] {
        journey.move_playhead_to_frame(frame);
        let stopwatch = journey.rectangle_of("Make a keyframe of position here");
        journey.click(stopwatch.center());
    }
    journey.harness.run_steps(40);
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save_what_is_drawn(&mut journey, &folder, "95_two_racks_in_the_drawer_of_the_video");

    // Taking hold of the position rack and going up with it, over the empty stretch of the front track: a crate fills it.
    let racks_strip = journey.strip_of_track(1);
    let second_rack = egui::pos2(
        racks_strip.left() + 300.0,
        racks_strip.bottom() + 8.0 + 16.0 + 4.0 + 8.0,
    );
    let on_the_front_track = egui::pos2(racks_strip.left() + 700.0, journey.strip_of_track(0).center().y);
    journey.drag_and_hold(egui::PointerButton::Primary, second_rack, on_the_front_track);
    journey.harness.run_steps(10);
    save_what_is_drawn(
        &mut journey,
        &folder,
        "96_a_rack_over_an_empty_stretch_of_a_track_is_in_a_crate",
    );
    journey.release(egui::PointerButton::Primary, on_the_front_track);
    journey.harness.run_steps(20);
    save_what_is_drawn(&mut journey, &folder, "97_the_crate_with_the_rack_in_it");

    // Over the room of the text object, which has no drawer open: the rack goes nowhere. And then the gain rack, which has
    // keyframes two and a half seconds apart, over a crate that is shorter than that.
    let crate_rack = egui::pos2(racks_strip.left() + 700.0, journey.strip_of_track(0).center().y);
    journey.move_pointer_to(crate_rack);
    save_what_is_drawn(&mut journey, &folder, "98_the_label_of_the_rack_in_the_crate");

    // The project, for opening in the real program.
    journey
        .application_mut()
        .save_project_to(folder.join("demo_racks_and_a_crate.fastforwardrs.json"));
}

#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_screenshots_of_the_menus_that_copy_clips_and_tracks() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let video = colourful_video(&toolchain, &folder);
    let mut journey = Journey::start_with_frames_lasting(&toolchain, 1.0 / 60.0);
    journey.import(&video);
    journey.cut_at_frame(60);
    // A track in front with a text object on it, from frame 20 to frame 80 or so, and a keyframe in the drawer of it.
    journey.add_a_track();
    journey.make_a_text_object_at(0, 20);
    journey.harness.run_steps(40);
    journey.move_playhead_to_frame(40);
    let stopwatch = journey.rectangle_of("Make a keyframe of rotation here");
    journey.click(stopwatch.center());
    journey.harness.run_steps(40);
    journey.move_pointer_to(egui::pos2(5.0, 5.0));

    // The menu of the text object: a copy to the right or to the left, and deleting it.
    let on_the_text = journey.point_on_the_strip_at_frame(0, 50);
    journey.right_click(on_the_text);
    journey.harness.run_steps(10);
    save_what_is_drawn(&mut journey, &folder, "99_the_menu_of_a_clip");

    // A copy to the right: the copy has a copy of the keyframe in its drawer, and is selected.
    journey.harness.get_by_label("Duplicate to the right").click();
    journey.settle();
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save_what_is_drawn(&mut journey, &folder, "100_a_copy_to_the_right_selected");

    // Two clips selected, the copy (which was selected by being made) and the clip it is a copy of: the menu says how many.
    let on_the_text = journey.point_on_the_strip_at_frame(0, 50);
    journey.click_with_control(on_the_text);
    journey.right_click(on_the_text);
    journey.harness.run_steps(10);
    save_what_is_drawn(&mut journey, &folder, "101_the_menu_when_two_clips_are_selected");

    // The menu of the box of a track, and the track copied below.
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    let title = journey.application().project().tracks()[0].title().to_owned();
    let field = journey
        .harness
        .query_all_by_role(egui::accesskit::Role::TextInput)
        .find(|node| node.value().as_deref() == Some(title.as_str()))
        .expect("the title of the track is shown")
        .rect();
    journey.right_click(egui::pos2(field.left() - 4.0, field.center().y));
    journey.harness.run_steps(10);
    save_what_is_drawn(&mut journey, &folder, "102_the_menu_of_the_box_of_a_track");
    journey.harness.get_by_label("Duplicate below").click();
    journey.settle();
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save_what_is_drawn(&mut journey, &folder, "103_the_track_copied_below_and_selected");

    // The project, for opening in the real program.
    journey
        .application_mut()
        .save_project_to(folder.join("demo_copies.fastforwardrs.json"));
}

#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_screenshots_of_the_keyframe_buttons_and_an_animated_video() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let video = colourful_video(&toolchain, &folder);
    let mut journey = Journey::start_with_frames_lasting(&toolchain, 1.0 / 60.0);
    journey.import(&video);
    journey.select_the_clip_at(10);
    journey.move_playhead_to_frame(0);
    journey.wait_until_idle();
    save(&mut journey, &folder, "80_a_video_selected_with_its_place_in_the_panel");

    // The pointer over the gain: the three buttons come, all grey while there are no keyframes.
    let gain = journey.rectangle_of("Gain");
    journey.move_pointer_to(gain.center());
    save_what_is_drawn(&mut journey, &folder, "81_the_buttons_of_the_gain_with_no_keyframes");

    // A keyframe at the start and one 2 seconds on, at which the video is smaller and further right and quieter.
    let stopwatch = journey.rectangle_of("Make a keyframe of gain here");
    journey.click(stopwatch.center());
    let stopwatch = journey.rectangle_of("Make a keyframe of position here");
    journey.click(stopwatch.center());
    journey.move_playhead_to_frame(60);
    journey.set_slider("w", 40.0);
    journey.set_slider("h", 40.0);
    journey.set_slider("x", 70.0);
    journey.set_slider("Gain", -12.0);
    let gain = journey.rectangle_of("Gain");
    journey.move_pointer_to(gain.center());
    save_what_is_drawn(
        &mut journey,
        &folder,
        "82_on_a_keyframe_the_stopwatch_is_orange_and_the_arrow_back_too",
    );

    // In between: the arrows are both orange, the stopwatch is grey, and the video is half way.
    journey.move_playhead_to_frame(30);
    let gain = journey.rectangle_of("Gain");
    journey.move_pointer_to(gain.center());
    save_what_is_drawn(&mut journey, &folder, "83_half_way_between_two_keyframes");

    // Over the position, to see the buttons of a pair of numbers.
    let position = journey.rectangle_of("x");
    journey.move_pointer_to(position.center());
    save_what_is_drawn(&mut journey, &folder, "84_the_buttons_of_the_position");

    // A text object, with the rows of what it says and how it looks and where it is.
    journey.add_a_track();
    journey.make_a_text_object_at(0, 20);
    journey.settle();
    let colour = journey.rectangle_of("Colour");
    journey.move_pointer_to(colour.center());
    save_what_is_drawn(
        &mut journey,
        &folder,
        "85_the_properties_of_a_text_object_with_its_place",
    );
}

#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_screenshots_of_selected_racks_the_menus_of_racks_and_keyframes_and_drawers_that_push() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let video = colourful_video(&toolchain, &folder);
    let mut journey = Journey::start_with_frames_lasting(&toolchain, 1.0 / 60.0);
    journey.import(&video);
    // A track in front with a text object on it from frame 10, which has a keyframe of how far it is turned: the drawer that
    // opens for it pushes the track of the video down.
    journey.add_a_track();
    journey.make_a_text_object_at(0, 10);
    journey.harness.run_steps(40);
    let on_the_video = journey.point_on_the_strip_at_frame(1, 100);
    journey.click(on_the_video);
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save(&mut journey, &folder, "104_before_a_drawer_is_opened");
    journey.move_playhead_to_frame(40);
    let on_the_text = journey.point_on_the_strip_at_frame(0, 40);
    journey.click(on_the_text);
    let stopwatch = journey.rectangle_of("Make a keyframe of rotation here");
    journey.click(stopwatch.center());
    journey.harness.run_steps(40);
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save_what_is_drawn(
        &mut journey,
        &folder,
        "105_the_drawer_of_the_text_pushes_the_track_of_the_video_down",
    );

    // The drawer of the video opened with Shift held lies over what is under it (here there is nothing but its own room).
    let on_the_video = journey.point_on_the_strip_at_frame(1, 100);
    journey.click(on_the_video);
    for frame in [30, 90, 150] {
        journey.move_playhead_to_frame(frame);
        let stopwatch = journey.rectangle_of("Make a keyframe of gain here");
        journey.click(stopwatch.center());
    }
    journey.move_playhead_to_frame(60);
    let stopwatch = journey.rectangle_of("Make a keyframe of position here");
    journey.click(stopwatch.center());
    journey.harness.run_steps(40);
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save_what_is_drawn(&mut journey, &folder, "106_two_racks_in_the_drawer_of_the_video");

    // A rack clicked is selected, and another with Ctrl as well.
    let racks_strip = journey.strip_of_track(1);
    let first_rack = egui::pos2(racks_strip.left() + 500.0, racks_strip.bottom() + 8.0 + 8.0);
    let second_rack = egui::pos2(
        racks_strip.left() + 500.0,
        racks_strip.bottom() + 8.0 + 16.0 + 4.0 + 8.0,
    );
    journey.click(first_rack);
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save_what_is_drawn(&mut journey, &folder, "107_a_rack_selected");
    journey.click_with_control(second_rack);
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save_what_is_drawn(
        &mut journey,
        &folder,
        "108_two_racks_selected_and_their_properties_listed",
    );

    // The menu of the racks: they are not of one property, so they cannot be merged.
    journey.right_click(second_rack);
    journey.harness.run_steps(10);
    save_what_is_drawn(
        &mut journey,
        &folder,
        "109_the_menu_of_two_racks_of_different_properties",
    );
    journey.press_key(egui::Key::Escape);

    // The menu of a keyframe.
    journey.click(on_the_video);
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    let first_keyframe = egui::pos2(journey.point_on_the_strip_at_frame(1, 90).x, first_rack.y);
    journey.right_click(first_keyframe);
    journey.harness.run_steps(10);
    save_what_is_drawn(&mut journey, &folder, "110_the_menu_of_a_keyframe");

    // The project, for opening in the real program.
    journey
        .application_mut()
        .save_project_to(folder.join("demo_selected_racks.fastforwardrs.json"));
}

#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_screenshots_of_the_question_asked_before_keyframes_are_lost() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let video = colourful_video(&toolchain, &folder);
    let mut journey = Journey::start_with_frames_lasting(&toolchain, 1.0 / 60.0);
    journey.import(&video);
    // A text object on a track in front, with four keyframes of how far it is turned, which are put at frames of their own
    // when it is made to last less.
    journey.add_a_track();
    journey.make_a_text_object_at(0, 10);
    for frame in [10, 20, 30, 40] {
        journey.move_playhead_to_frame(frame);
        let stopwatch = journey.rectangle_of("Make a keyframe of rotation here");
        journey.click(stopwatch.center());
    }
    journey.harness.run_steps(40);
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save_what_is_drawn(&mut journey, &folder, "111_a_text_with_four_keyframes");

    // Short enough for all the keyframes to be put at a frame of their own: no question.
    journey.set_slider("Shown for", 0.2);
    journey.harness.run_steps(40);
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save_what_is_drawn(
        &mut journey,
        &folder,
        "112_made_so_short_that_the_keyframes_are_a_frame_apart",
    );

    // Shorter than that: the user is asked.
    journey.set_slider("Shown for", 0.1);
    journey.harness.run_steps(10);
    save_what_is_drawn(&mut journey, &folder, "113_the_question_about_losing_a_keyframe");
}

#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_screenshots_of_the_preview_zoomed_in_out_and_in_to_the_pixels() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let video = colourful_video(&toolchain, &folder);
    let mut journey = Journey::start_with_frames_lasting(&toolchain, 1.0 / 60.0);
    journey.import(&video);
    journey.harness.run_steps(40);
    let on_the_preview = egui::pos2(800.0, 220.0);
    let twice_as_big = 200.0 * 2.0_f32.ln();

    // In to twice the size, about a point that is not the middle, which stays where it is.
    journey.scroll_the_wheel(on_the_preview, egui::vec2(0.0, twice_as_big), egui::Modifiers::NONE);
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save_what_is_drawn(&mut journey, &folder, "114_the_preview_zoomed_in_twice");

    // All the way in, to the clock in the corner of the test picture, until its pixels can be seen.
    let the_clock = egui::pos2(520.0, 60.0);
    journey.scroll_the_wheel(the_clock, egui::vec2(0.0, twice_as_big * 5.0), egui::Modifiers::NONE);
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save_what_is_drawn(
        &mut journey,
        &folder,
        "115_the_preview_zoomed_in_until_the_pixels_can_be_seen",
    );

    // Back out as far as it goes: half the size the picture has when it fits.
    journey.scroll_the_wheel(
        on_the_preview,
        egui::vec2(0.0, -twice_as_big * 8.0),
        egui::Modifiers::NONE,
    );
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save_what_is_drawn(&mut journey, &folder, "116_the_preview_zoomed_out_to_half");

    // And the button in the corner brings it back.
    let fit = journey.rectangle_of("Fit");
    journey.click(fit.center());
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save_what_is_drawn(&mut journey, &folder, "117_the_preview_fitting_again");
}

#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_screenshots_of_the_plus_button_that_puts_an_effect_on_and_the_hue_shift() {
    let toolchain = toolchain_or_skip!();
    let _turn = take_turn_at_drawing();
    let folder = screenshot_folder();
    let video = colourful_video(&toolchain, &folder);
    let mut journey = Journey::start_with_frames_lasting(&toolchain, 1.0 / 60.0);
    journey.import(&video);
    journey.harness.run_steps(40);
    journey.select_the_clip_at(100);
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save_what_is_drawn(
        &mut journey,
        &folder,
        "118_a_clip_selected_with_the_plus_button_for_effects",
    );

    // The menu of the button: one row for each effect there is.
    let plus = journey.rectangle_of("Add an effect");
    journey.click(plus.center());
    save_what_is_drawn(&mut journey, &folder, "119_the_menu_of_the_plus_button");

    // The effect is put on: its box, with the slider of its one value.
    let item = journey.rectangle_of("Hue shift");
    journey.click(item.center());
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save_what_is_drawn(&mut journey, &folder, "120_the_box_of_the_hue_shift");

    // Turned a third of the way round: the colours of the picture are different.
    journey.set_slider("Turn", 120.0);
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save_what_is_drawn(&mut journey, &folder, "121_the_colours_turned_by_120_degrees");

    // With keyframes: the colours go round between them.
    let row = journey.rectangle_of("Turn");
    journey.move_pointer_to(row.center());
    let stopwatch = journey.rectangle_of("Make a keyframe of hue shift here");
    journey.click(stopwatch.center());
    journey.move_playhead_to_frame(300);
    journey.set_slider("Turn", 300.0);
    journey.move_playhead_to_frame(200);
    journey.harness.run_steps(40);
    journey.move_pointer_to(egui::pos2(5.0, 5.0));
    save_what_is_drawn(
        &mut journey,
        &folder,
        "122_the_hue_shift_with_keyframes_part_of_the_way_round",
    );

    // The menu of its box takes it off.
    let heading = journey.rectangle_of("Hue shift");
    journey.right_click(heading.center());
    save_what_is_drawn(&mut journey, &folder, "123_the_menu_of_the_box_of_the_effect");
}
