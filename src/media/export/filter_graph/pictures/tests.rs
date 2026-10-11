use super::*;

fn frame_rate_30() -> FrameRate {
    FrameRate::new(30, 1).unwrap()
}

fn speed(factor: u32) -> SpeedFactor {
    SpeedFactor::new(factor).unwrap()
}

fn video(source_input: usize, first: u64, end: u64, factor: u32) -> VideoToExport {
    VideoToExport {
        source_input,
        first_source_frame: first,
        end_source_frame: end,
        speed_factor: speed(factor),
    }
}

/// A section that is the clip of source frames `first` up to `end` of the first source, at `factor`x.
fn clip_section(first: u64, end: u64, factor: u32) -> SectionToExport {
    SectionToExport {
        frame_count: (end - first).div_ceil(u64::from(factor)),
        video: Some(video(0, first, end, factor)),
        overlays: Vec::new(),
    }
}

fn black_section(frame_count: u64) -> SectionToExport {
    SectionToExport {
        frame_count,
        video: None,
        overlays: Vec::new(),
    }
}

fn overlay(picture_input: usize, left: u32, top: u32) -> OverlayToExport {
    OverlayToExport {
        picture_input,
        left_in_pixels: left,
        top_in_pixels: top,
    }
}

/// A 320x180 picture made from one source that is shown at that size.
fn specification(sections: Vec<SectionToExport>) -> PictureGraphSpecification {
    PictureGraphSpecification {
        frame_rate: frame_rate_30(),
        picture_width_in_pixels: 320,
        picture_height_in_pixels: 180,
        sources: vec![SourceToExport {
            display_size_in_pixels: Some((320, 180)),
        }],
        sections,
    }
}

fn statements_of(specification: &PictureGraphSpecification) -> Vec<String> {
    picture_statements(specification)
}

#[test]
fn a_single_normal_speed_clip_is_scaled_resampled_cut_by_picture_number_and_renumbered() {
    assert_eq!(
        statements_of(&specification(vec![clip_section(0, 90, 1)])),
        [
            "[0:v:0]scale=320:180,setsar=1,fps=30/1:round=near,split=1[clip_source_video_0]",
            "[clip_source_video_0]trim=start_frame=0:end_frame=90,setpts=N*1/(30*TB)[finished_clip_0]",
            "[finished_clip_0]concat=n=1:v=1:a=0[joined_video]",
            "[joined_video]setpts=N*1/(30*TB)[finished_video]",
        ]
    );
}

#[test]
fn an_accelerated_clip_keeps_every_nth_picture_and_a_normal_one_keeps_them_all() {
    assert_eq!(
        statements_of(&specification(vec![clip_section(0, 150, 1), clip_section(150, 330, 3)])),
        [
            "[0:v:0]scale=320:180,setsar=1,fps=30/1:round=near,split=2[clip_source_video_0][clip_source_video_1]",
            "[clip_source_video_0]trim=start_frame=0:end_frame=150,setpts=N*1/(30*TB)[finished_clip_0]",
            "[clip_source_video_1]trim=start_frame=150:end_frame=330,select='not(mod(n,3))',setpts=N*1/(30*TB)[finished_clip_1]",
            "[finished_clip_0][finished_clip_1]concat=n=2:v=1:a=0[joined_video]",
            "[joined_video]setpts=N*1/(30*TB)[finished_video]",
        ]
    );
}

/// Regression test: joining clips gave the clip after a single-picture clip the same timestamp as
/// that picture, and the encoder dropped a picture. Renumbering the joined pictures by position
/// removes the problem whatever the join does.
#[test]
fn the_joined_pictures_are_numbered_by_position_at_the_exact_frame_rate() {
    for (numerator, denominator, expected) in [
        (30, 1, "setpts=N*1/(30*TB)"),
        (30000, 1001, "setpts=N*1001/(30000*TB)"),
        (24000, 1001, "setpts=N*1001/(24000*TB)"),
        (60, 1, "setpts=N*1/(60*TB)"),
    ] {
        let mut spec = specification(vec![clip_section(0, 4, 4), clip_section(4, 8, 1)]);
        spec.frame_rate = FrameRate::new(numerator, denominator).unwrap();
        let last_statement = statements_of(&spec).pop().unwrap();
        assert_eq!(last_statement, format!("[joined_video]{expected}[finished_video]"));
    }
}

#[test]
fn neighbouring_clips_share_their_boundary_picture_number_so_none_is_lost_or_doubled() {
    let graph = statements_of(&specification(vec![
        clip_section(0, 100, 1),
        clip_section(100, 250, 2),
        clip_section(250, 300, 1),
    ]))
    .join(";");
    assert!(graph.contains("trim=start_frame=0:end_frame=100,"), "{graph}");
    assert!(graph.contains("trim=start_frame=100:end_frame=250,"), "{graph}");
    assert!(graph.contains("trim=start_frame=250:end_frame=300,"), "{graph}");
}

/// Regression test: with timestamps rounded to milliseconds (as in Matroska files) a third of the
/// pictures landed in the wrong frame slot when the source was resampled with `round=up`.
/// `round=near` puts every picture in its own slot.
#[test]
fn the_source_is_resampled_to_the_project_frame_rate_to_the_nearest_slot() {
    for (numerator, denominator) in [(30, 1), (30000, 1001), (24000, 1001), (60, 1)] {
        let mut spec = specification(vec![clip_section(0, 100, 1)]);
        spec.frame_rate = FrameRate::new(numerator, denominator).unwrap();
        let graph = statements_of(&spec).join(";");
        assert!(
            graph.contains(&format!("fps={numerator}/{denominator}:round=near,split=1")),
            "{graph}"
        );
        assert!(!graph.contains("round=up"), "{graph}");
    }
}

#[test]
fn clips_are_cut_by_picture_number_so_the_frame_rate_does_not_matter_for_the_cuts() {
    let mut spec = specification(vec![clip_section(30000, 60000, 1)]);
    spec.frame_rate = FrameRate::new(30000, 1001).unwrap();
    let graph = statements_of(&spec).join(";");
    assert!(
        graph.contains("trim=start_frame=30000:end_frame=60000,setpts=N*1001/(30000*TB)"),
        "{graph}"
    );
}

#[test]
fn normal_speed_clips_have_no_select_filter_and_the_select_counts_from_the_clips_own_first_picture() {
    let graph = statements_of(&specification(vec![clip_section(0, 100, 1), clip_section(100, 250, 5)])).join(";");
    assert_eq!(graph.matches("select=").count(), 1, "{graph}");
    assert!(
        graph.contains("trim=start_frame=100:end_frame=250,select='not(mod(n,5))',"),
        "{graph}"
    );
}

#[test]
fn the_picture_is_scaled_to_the_requested_size_with_square_pixels_before_anything_else() {
    let mut spec = specification(vec![clip_section(0, 90, 1)]);
    spec.picture_width_in_pixels = 1280;
    spec.picture_height_in_pixels = 720;
    spec.sources[0].display_size_in_pixels = Some((1280, 720));
    assert!(
        statements_of(&spec)[0].starts_with("[0:v:0]scale=1280:720,setsar=1,fps="),
        "{}",
        statements_of(&spec)[0]
    );
}

// --- still pictures laid over ---

#[test]
fn an_accelerated_clip_gets_its_symbol_laid_over_it_and_a_normal_clip_does_not() {
    let mut accelerated = clip_section(150, 330, 3);
    accelerated.overlays = vec![overlay(1, 12, 8)];
    assert_eq!(
        statements_of(&specification(vec![clip_section(0, 150, 1), accelerated])),
        [
            "[0:v:0]scale=320:180,setsar=1,fps=30/1:round=near,split=2[clip_source_video_0][clip_source_video_1]",
            "[1:v]split=1[still_picture_1_copy_0]",
            "[clip_source_video_0]trim=start_frame=0:end_frame=150,setpts=N*1/(30*TB)[finished_clip_0]",
            "[clip_source_video_1]trim=start_frame=150:end_frame=330,select='not(mod(n,3))',setpts=N*1/(30*TB)[clip_picture_1]",
            "[clip_picture_1][still_picture_1_copy_0]overlay=x=12:y=8:format=auto[finished_clip_1]",
            "[finished_clip_0][finished_clip_1]concat=n=2:v=1:a=0[joined_video]",
            "[joined_video]setpts=N*1/(30*TB)[finished_video]",
        ]
    );
}

#[test]
fn a_still_picture_used_in_several_places_is_copied_once_for_each_and_every_copy_is_used_once() {
    let with_symbol = |first, end| {
        let mut section = clip_section(first, end, 4);
        section.overlays = vec![overlay(1, 10, 10)];
        section
    };
    let graph = statements_of(&specification(vec![
        with_symbol(0, 90),
        clip_section(90, 180, 1),
        with_symbol(180, 270),
        with_symbol(270, 360),
    ]))
    .join(";");
    assert!(
        graph.contains("[1:v]split=3[still_picture_1_copy_0][still_picture_1_copy_1][still_picture_1_copy_2]"),
        "{graph}"
    );
    for copy in 0..3 {
        let label = format!("[still_picture_1_copy_{copy}]");
        assert_eq!(
            graph.matches(&label).count(),
            2,
            "{label} is made once and used once in {graph}"
        );
    }
    assert_eq!(graph.matches("overlay=").count(), 3);
}

#[test]
fn different_still_pictures_are_different_inputs_and_each_is_laid_over_where_it_says() {
    let mut first = clip_section(0, 90, 8);
    first.overlays = vec![overlay(2, 5, 5)];
    let mut second = clip_section(90, 180, 2);
    second.overlays = vec![overlay(1, 7, 7)];
    let graph = statements_of(&specification(vec![first, second])).join(";");
    assert!(graph.contains("[1:v]split=1[still_picture_1_copy_0]"), "{graph}");
    assert!(graph.contains("[2:v]split=1[still_picture_2_copy_0]"), "{graph}");
    assert!(
        graph.contains("[clip_picture_0][still_picture_2_copy_0]overlay=x=5:y=5"),
        "{graph}"
    );
    assert!(
        graph.contains("[clip_picture_1][still_picture_1_copy_0]overlay=x=7:y=7"),
        "{graph}"
    );
}

#[test]
fn a_still_picture_that_no_section_lays_over_is_left_out_of_the_graph() {
    let graph = statements_of(&specification(vec![clip_section(0, 90, 1)])).join(";");
    assert!(!graph.contains("overlay"), "{graph}");
    assert!(!graph.contains("[1:v]"), "{graph}");
}

#[test]
fn several_still_pictures_over_one_section_are_laid_on_one_after_the_other_the_back_one_first() {
    let mut section = clip_section(0, 90, 2);
    section.overlays = vec![overlay(1, 10, 20), overlay(2, 30, 40), overlay(3, 50, 60)];
    let statements = statements_of(&specification(vec![section]));
    let overlays: Vec<&String> = statements
        .iter()
        .filter(|statement| statement.contains("overlay="))
        .collect();
    assert_eq!(
        overlays,
        [
            "[clip_picture_0][still_picture_1_copy_0]overlay=x=10:y=20:format=auto[clip_picture_0_with_1]",
            "[clip_picture_0_with_1][still_picture_2_copy_0]overlay=x=30:y=40:format=auto[clip_picture_0_with_2]",
            "[clip_picture_0_with_2][still_picture_3_copy_0]overlay=x=50:y=60:format=auto[finished_clip_0]",
        ]
    );
}

// --- black, and several sources ---

#[test]
fn a_stretch_with_no_video_is_black_for_exactly_as_many_frames_as_it_lasts() {
    let statements = statements_of(&specification(vec![black_section(45), clip_section(0, 60, 1)]));
    assert_eq!(
        statements,
        [
            "[0:v:0]scale=320:180,setsar=1,fps=30/1:round=near,split=1[clip_source_video_1]",
            "color=c=black:s=320x180:r=30/1,format=yuv420p,setsar=1,trim=end_frame=45,setpts=N*1/(30*TB)[finished_clip_0]",
            "[clip_source_video_1]trim=start_frame=0:end_frame=60,setpts=N*1/(30*TB)[finished_clip_1]",
            "[finished_clip_0][finished_clip_1]concat=n=2:v=1:a=0[joined_video]",
            "[joined_video]setpts=N*1/(30*TB)[finished_video]",
        ]
    );
}

#[test]
fn a_picture_with_no_video_at_all_is_black_with_nothing_read_from_any_file() {
    let graph = statements_of(&specification(vec![black_section(30)])).join(";");
    assert!(!graph.contains("[0:v"), "{graph}");
    assert!(graph.starts_with("color=c=black:s=320x180"), "{graph}");
}

#[test]
fn still_pictures_can_be_laid_over_black() {
    let mut section = black_section(30);
    section.overlays = vec![overlay(1, 100, 120)];
    let graph = statements_of(&specification(vec![section])).join(";");
    assert!(
        graph.contains("[clip_picture_0][still_picture_1_copy_0]overlay=x=100:y=120:format=auto[finished_clip_0]"),
        "{graph}"
    );
    assert!(
        graph.contains("trim=end_frame=30,setpts=N*1/(30*TB)[clip_picture_0]"),
        "{graph}"
    );
}

#[test]
fn each_source_is_made_the_size_of_the_picture_and_split_once_for_its_own_sections() {
    let mut spec = specification(vec![
        clip_section(0, 60, 1),
        SectionToExport {
            frame_count: 30,
            video: Some(video(1, 0, 30, 1)),
            overlays: Vec::new(),
        },
        clip_section(60, 90, 1),
    ]);
    spec.sources.push(SourceToExport {
        display_size_in_pixels: Some((320, 180)),
    });
    let statements = statements_of(&spec);
    assert_eq!(
        statements[..2],
        [
            "[0:v:0]scale=320:180,setsar=1,fps=30/1:round=near,split=2[clip_source_video_0][clip_source_video_2]",
            "[1:v:0]scale=320:180,setsar=1,fps=30/1:round=near,split=1[clip_source_video_1]",
        ]
    );
    assert!(statements.contains(
        &"[clip_source_video_1]trim=start_frame=0:end_frame=30,setpts=N*1/(30*TB)[finished_clip_1]".to_owned()
    ));
}

#[test]
fn a_video_of_another_shape_is_fitted_inside_the_picture_and_put_in_the_middle_of_black() {
    let mut spec = specification(vec![clip_section(0, 60, 1)]);
    spec.sources[0].display_size_in_pixels = Some((180, 320)); // upright, in a wide picture
    // 9:16 in 320x180: as tall as the picture, 101.25 wide, which is made 100 (even); centred.
    assert_eq!(
        statements_of(&spec)[0],
        "[0:v:0]scale=100:180,setsar=1,pad=320:180:110:0,fps=30/1:round=near,split=1[clip_source_video_0]"
    );
}

#[test]
fn a_video_of_the_same_shape_at_another_size_is_scaled_straight_to_the_picture() {
    let mut spec = specification(vec![clip_section(0, 60, 1)]);
    spec.sources[0].display_size_in_pixels = Some((1920, 1080));
    assert_eq!(
        statements_of(&spec)[0],
        "[0:v:0]scale=320:180,setsar=1,fps=30/1:round=near,split=1[clip_source_video_0]"
    );
}

#[test]
fn a_source_whose_shape_is_not_known_is_scaled_straight_to_the_picture() {
    let mut spec = specification(vec![clip_section(0, 60, 1)]);
    spec.sources[0].display_size_in_pixels = None;
    assert_eq!(
        statements_of(&spec)[0],
        "[0:v:0]scale=320:180,setsar=1,fps=30/1:round=near,split=1[clip_source_video_0]"
    );
}
