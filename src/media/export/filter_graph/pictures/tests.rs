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

fn place(left: u32, top: u32, width: u32, height: u32, turn_in_degrees: f32) -> PlaceToExport {
    PlaceToExport {
        left_in_pixels: left,
        top_in_pixels: top,
        width_in_pixels: width,
        height_in_pixels: height,
        turn_in_degrees,
    }
}

/// All of the 320x180 picture.
fn whole_picture() -> PlaceToExport {
    place(0, 0, 320, 180, 0.0)
}

fn video_layer(source_input: usize, first: u64, end: u64, factor: u32, place: PlaceToExport) -> LayerToExport {
    LayerToExport {
        content: LayerContentToExport::Video(video(source_input, first, end, factor)),
        place,
    }
}

/// A still picture laid over with its top left corner at `(left, top)` that is 40 by 20 pixels.
fn still_layer(picture_input: usize, left: u32, top: u32) -> LayerToExport {
    LayerToExport {
        content: LayerContentToExport::Still {
            picture_input,
            scaled_to_its_place: false,
        },
        place: place(left, top, 40, 20, 0.0),
    }
}

/// A picture from a file, which has to be scaled to `place`.
fn picture_layer(picture_input: usize, place: PlaceToExport) -> LayerToExport {
    LayerToExport {
        content: LayerContentToExport::Still {
            picture_input,
            scaled_to_its_place: true,
        },
        place,
    }
}

/// A section that is the clip of source frames `first` up to `end` of the first source, at `factor`x,
/// filling the picture.
fn clip_section(first: u64, end: u64, factor: u32) -> SectionToExport {
    SectionToExport {
        frame_count: (end - first).div_ceil(u64::from(factor)),
        layers: vec![video_layer(0, first, end, factor, whole_picture())],
    }
}

fn black_section(frame_count: u64) -> SectionToExport {
    SectionToExport {
        frame_count,
        layers: Vec::new(),
    }
}

/// A 320x180 picture made from sources that are shown at that size.
fn specification(sections: Vec<SectionToExport>) -> PictureGraphSpecification {
    PictureGraphSpecification {
        frame_rate: frame_rate_30(),
        picture_width_in_pixels: 320,
        picture_height_in_pixels: 180,
        sections,
    }
}

fn statements_of(specification: &PictureGraphSpecification) -> Vec<String> {
    picture_statements(specification)
}

#[test]
fn a_single_normal_speed_clip_is_resampled_cut_by_picture_number_renumbered_and_scaled() {
    assert_eq!(
        statements_of(&specification(vec![clip_section(0, 90, 1)])),
        [
            "[0:v:0]fps=30/1:round=near,split=1[clip_source_video_0_0]",
            "[clip_source_video_0_0]trim=start_frame=0:end_frame=90,setpts=N*1/(30*TB),scale=320:180,setsar=1[finished_clip_0]",
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
            "[0:v:0]fps=30/1:round=near,split=2[clip_source_video_0_0][clip_source_video_1_0]",
            "[clip_source_video_0_0]trim=start_frame=0:end_frame=150,setpts=N*1/(30*TB),scale=320:180,setsar=1[finished_clip_0]",
            "[clip_source_video_1_0]trim=start_frame=150:end_frame=330,select='not(mod(n,3))',setpts=N*1/(30*TB),scale=320:180,setsar=1[finished_clip_1]",
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
            graph.contains(&format!("[0:v:0]fps={numerator}/{denominator}:round=near,split=1")),
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
fn a_picture_is_scaled_to_its_place_with_square_pixels_after_the_pictures_that_are_not_kept_are_dropped() {
    let mut spec = specification(vec![clip_section(0, 90, 3)]);
    spec.picture_width_in_pixels = 1280;
    spec.picture_height_in_pixels = 720;
    spec.sections[0].layers[0].place = place(0, 0, 1280, 720, 0.0);
    let video_statement = &statements_of(&spec)[1];
    let selected = video_statement.find("select=").unwrap();
    let scaled = video_statement.find("scale=1280:720,setsar=1").unwrap();
    assert!(
        selected < scaled,
        "so that only the pictures that are kept are scaled: {video_statement}"
    );
}

// --- where a video is on the picture ---

#[test]
fn a_video_that_does_not_fill_the_picture_is_scaled_to_its_place_and_put_in_the_middle_of_black() {
    let mut section = clip_section(0, 60, 1);
    // Upright in a wide picture: as tall as the picture, 100 wide, in the middle.
    section.layers[0].place = place(110, 0, 100, 180, 0.0);
    assert_eq!(
        statements_of(&specification(vec![section]))[1],
        "[clip_source_video_0_0]trim=start_frame=0:end_frame=60,setpts=N*1/(30*TB),scale=100:180,setsar=1,pad=320:180:110:0[finished_clip_0]"
    );
}

#[test]
fn a_video_that_is_the_back_most_layer_and_is_not_turned_is_the_picture_and_the_layers_over_it_go_on_it() {
    let mut section = clip_section(0, 60, 1);
    section.layers[0].place = place(10, 20, 100, 60, 0.0);
    section.layers.push(still_layer(1, 5, 6));
    let statements = statements_of(&specification(vec![section]));
    assert_eq!(
        statements[2..4],
        [
            "[clip_source_video_0_0]trim=start_frame=0:end_frame=60,setpts=N*1/(30*TB),scale=100:60,setsar=1,pad=320:180:10:20[clip_picture_0]",
            "[clip_picture_0][still_picture_1_copy_0]overlay=x=5:y=6:format=auto[finished_clip_0]",
        ]
    );
}

#[test]
fn a_video_laid_over_another_is_cut_and_scaled_to_its_place_and_then_laid_over_on_its_own() {
    let mut section = clip_section(0, 60, 1);
    section
        .layers
        .push(video_layer(1, 100, 160, 1, place(30, 40, 160, 90, 0.0)));
    let statements = statements_of(&specification(vec![section]));
    assert_eq!(
        statements,
        [
            "[0:v:0]fps=30/1:round=near,split=1[clip_source_video_0_0]",
            "[1:v:0]fps=30/1:round=near,split=1[clip_source_video_0_1]",
            "[clip_source_video_0_0]trim=start_frame=0:end_frame=60,setpts=N*1/(30*TB),scale=320:180,setsar=1[clip_picture_0]",
            "[clip_source_video_0_1]trim=start_frame=100:end_frame=160,setpts=N*1/(30*TB),scale=160:90,setsar=1[clip_layer_0_1]",
            "[clip_picture_0][clip_layer_0_1]overlay=x=30:y=40:format=auto[finished_clip_0]",
            "[finished_clip_0]concat=n=1:v=1:a=0[joined_video]",
            "[joined_video]setpts=N*1/(30*TB)[finished_video]",
        ]
    );
}

#[test]
fn a_video_that_is_the_back_most_layer_but_is_turned_is_laid_over_black_like_any_other() {
    let mut section = clip_section(0, 60, 1);
    // 100 by 60, with its top left corner at (100, 50): its middle is at (150, 80).
    section.layers[0].place = place(100, 50, 100, 60, 30.0);
    let statements = statements_of(&specification(vec![section]));
    assert_eq!(
        statements[1..4],
        [
            "color=c=black:s=320x180:r=30/1,format=yuv420p,setsar=1,trim=end_frame=60,setpts=N*1/(30*TB)[clip_picture_0]",
            "[clip_source_video_0_0]trim=start_frame=0:end_frame=60,setpts=N*1/(30*TB),scale=100:60,setsar=1,format=rgba,rotate=a=0.523598776:ow=118:oh=102:c=black@0[clip_layer_0_0]",
            "[clip_picture_0][clip_layer_0_0]overlay=x=91:y=29:format=auto[finished_clip_0]",
        ]
    );
}

#[test]
fn turning_is_clockwise_and_a_turn_the_other_way_is_a_negative_angle() {
    let mut section = clip_section(0, 60, 1);
    section.layers[0].place = place(100, 50, 100, 60, -30.0);
    let graph = statements_of(&specification(vec![section])).join(";");
    assert!(
        graph.contains("rotate=a=-0.523598776:ow=118:oh=102:c=black@0"),
        "{graph}"
    );
}

#[test]
fn a_still_picture_that_is_turned_is_turned_into_a_picture_of_its_own_with_clear_corners_before_it_is_laid_over() {
    let mut section = clip_section(0, 60, 1);
    // 40 by 20, top left corner at (100, 50): the middle is at (120, 60). Turned a quarter, it is 20 by 40.
    section.layers.push(LayerToExport {
        content: LayerContentToExport::Still {
            picture_input: 1,
            scaled_to_its_place: false,
        },
        place: place(100, 50, 40, 20, 90.0),
    });
    let statements = statements_of(&specification(vec![section]));
    assert_eq!(
        statements[2..5],
        [
            "[clip_source_video_0_0]trim=start_frame=0:end_frame=60,setpts=N*1/(30*TB),scale=320:180,setsar=1[clip_picture_0]",
            "[still_picture_1_copy_0]format=rgba,rotate=a=1.570796327:ow=20:oh=40:c=black@0[clip_layer_0_1]",
            "[clip_picture_0][clip_layer_0_1]overlay=x=110:y=40:format=auto[finished_clip_0]",
        ]
    );
}

#[test]
fn a_still_picture_that_is_not_turned_is_laid_over_as_it_is_without_a_statement_of_its_own() {
    let mut section = clip_section(0, 60, 1);
    section.layers.push(still_layer(1, 100, 50));
    let statements = statements_of(&specification(vec![section]));
    assert!(
        statements
            .iter()
            .all(|statement| !statement.contains("rotate") && !statement.contains("clip_layer")),
        "{statements:?}"
    );
}

#[test]
fn the_size_of_the_picture_a_turned_one_fits_in_is_worked_out_in_whole_even_pixels() {
    // Not turned, or turned half way round: the same size.
    assert_eq!(size_when_turned(320, 180, 0.0), (320, 180));
    assert_eq!(size_when_turned(320, 180, 180.0), (320, 180));
    // A quarter: the sides change places, exactly (not a pixel more for a rounding error).
    assert_eq!(size_when_turned(320, 180, 90.0), (180, 320));
    assert_eq!(size_when_turned(320, 180, -90.0), (180, 320));
    assert_eq!(size_when_turned(100, 100, 270.0), (100, 100));
    // An eighth of a turn: a square of 100 is 141.4 across, made 142.
    assert_eq!(size_when_turned(100, 100, 45.0), (142, 142));
    assert_eq!(size_when_turned(100, 60, 30.0), (118, 102));
    // Odd sizes are made even, upwards, so that the middle can be kept.
    assert_eq!(size_when_turned(101, 51, 0.0), (102, 52));
}

#[test]
fn the_picture_laid_over_is_put_so_that_the_middle_of_it_is_the_middle_of_the_place() {
    // Whatever the angle, the turned picture is centred on the middle of the place (within a pixel).
    for degrees in [
        -170.0f32, -90.0, -45.0, -10.0, 0.0, 15.0, 30.0, 45.0, 90.0, 135.0, 180.0,
    ] {
        let place = place(100, 50, 100, 60, degrees);
        let (width, height) = size_when_turned(100, 60, degrees);
        let (left, top) = left_and_top_when_turned(place);
        let middle_x = f64::from(left) + f64::from(width) / 2.0;
        let middle_y = f64::from(top) + f64::from(height) / 2.0;
        assert!((middle_x - 150.0).abs() <= 0.5, "{degrees}: {middle_x}");
        assert!((middle_y - 80.0).abs() <= 0.5, "{degrees}: {middle_y}");
    }
}

// --- still pictures laid over ---

#[test]
fn an_accelerated_clip_gets_its_symbol_laid_over_it_and_a_normal_clip_does_not() {
    let mut accelerated = clip_section(150, 330, 3);
    accelerated.layers.push(still_layer(1, 12, 8));
    assert_eq!(
        statements_of(&specification(vec![clip_section(0, 150, 1), accelerated])),
        [
            "[0:v:0]fps=30/1:round=near,split=2[clip_source_video_0_0][clip_source_video_1_0]",
            "[1:v]split=1[still_picture_1_copy_0]",
            "[clip_source_video_0_0]trim=start_frame=0:end_frame=150,setpts=N*1/(30*TB),scale=320:180,setsar=1[finished_clip_0]",
            "[clip_source_video_1_0]trim=start_frame=150:end_frame=330,select='not(mod(n,3))',setpts=N*1/(30*TB),scale=320:180,setsar=1[clip_picture_1]",
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
        section.layers.push(still_layer(1, 10, 10));
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
    first.layers.push(still_layer(2, 5, 5));
    let mut second = clip_section(90, 180, 2);
    second.layers.push(still_layer(1, 7, 7));
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
    section
        .layers
        .extend([still_layer(1, 10, 20), still_layer(2, 30, 40), still_layer(3, 50, 60)]);
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

#[test]
fn a_still_picture_can_be_behind_a_video_which_is_then_laid_over_it() {
    // Text behind a video that is smaller than the picture, which shows through.
    let section = SectionToExport {
        frame_count: 60,
        layers: vec![
            still_layer(1, 100, 50),
            video_layer(0, 0, 60, 1, place(30, 40, 160, 90, 0.0)),
        ],
    };
    let statements = statements_of(&specification(vec![section]));
    assert_eq!(
        statements[2..6],
        [
            "color=c=black:s=320x180:r=30/1,format=yuv420p,setsar=1,trim=end_frame=60,setpts=N*1/(30*TB)[clip_picture_0]",
            "[clip_picture_0][still_picture_1_copy_0]overlay=x=100:y=50:format=auto[clip_picture_0_with_0]",
            "[clip_source_video_0_1]trim=start_frame=0:end_frame=60,setpts=N*1/(30*TB),scale=160:90,setsar=1[clip_layer_0_1]",
            "[clip_picture_0_with_0][clip_layer_0_1]overlay=x=30:y=40:format=auto[finished_clip_0]",
        ]
    );
}

// --- pictures from files ---

#[test]
fn a_picture_from_a_file_is_scaled_to_its_place_with_square_pixels_and_then_laid_over() {
    let mut section = clip_section(0, 60, 1);
    section.layers.push(picture_layer(1, place(30, 40, 160, 90, 0.0)));
    let statements = statements_of(&specification(vec![section]));
    assert_eq!(
        statements[1..5],
        [
            "[1:v]split=1[still_picture_1_copy_0]",
            "[clip_source_video_0_0]trim=start_frame=0:end_frame=60,setpts=N*1/(30*TB),scale=320:180,setsar=1[clip_picture_0]",
            "[still_picture_1_copy_0]scale=160:90,setsar=1[clip_layer_0_1]",
            "[clip_picture_0][clip_layer_0_1]overlay=x=30:y=40:format=auto[finished_clip_0]",
        ]
    );
}

#[test]
fn a_picture_from_a_file_that_is_turned_is_scaled_first_and_then_turned() {
    let mut section = black_section(30);
    section.layers = vec![picture_layer(0, place(100, 50, 100, 60, 30.0))];
    let statements = statements_of(&specification(vec![section]));
    assert_eq!(
        statements[1..4],
        [
            "color=c=black:s=320x180:r=30/1,format=yuv420p,setsar=1,trim=end_frame=30,setpts=N*1/(30*TB)[clip_picture_0]",
            "[still_picture_0_copy_0]scale=100:60,setsar=1,format=rgba,rotate=a=0.523598776:ow=118:oh=102:c=black@0[clip_layer_0_0]",
            "[clip_picture_0][clip_layer_0_0]overlay=x=91:y=29:format=auto[finished_clip_0]",
        ]
    );
}

#[test]
fn a_picture_used_in_two_sections_is_copied_for_each_and_scaled_to_where_it_is_each_time() {
    let with_picture = |frames: u64, picture_place: PlaceToExport| {
        let mut section = clip_section(0, frames, 1);
        section.layers.push(picture_layer(1, picture_place));
        section
    };
    let graph = statements_of(&specification(vec![
        with_picture(30, place(0, 0, 100, 50, 0.0)),
        with_picture(30, place(10, 10, 200, 100, 0.0)),
    ]))
    .join(";");
    assert!(
        graph.contains("[1:v]split=2[still_picture_1_copy_0][still_picture_1_copy_1]"),
        "{graph}"
    );
    assert!(
        graph.contains("[still_picture_1_copy_0]scale=100:50,setsar=1[clip_layer_0_1]"),
        "{graph}"
    );
    assert!(
        graph.contains("[still_picture_1_copy_1]scale=200:100,setsar=1[clip_layer_1_1]"),
        "{graph}"
    );
}

// --- black, and several sources ---

#[test]
fn a_stretch_with_no_video_is_black_for_exactly_as_many_frames_as_it_lasts() {
    let statements = statements_of(&specification(vec![black_section(45), clip_section(0, 60, 1)]));
    assert_eq!(
        statements,
        [
            "[0:v:0]fps=30/1:round=near,split=1[clip_source_video_1_0]",
            "color=c=black:s=320x180:r=30/1,format=yuv420p,setsar=1,trim=end_frame=45,setpts=N*1/(30*TB)[finished_clip_0]",
            "[clip_source_video_1_0]trim=start_frame=0:end_frame=60,setpts=N*1/(30*TB),scale=320:180,setsar=1[finished_clip_1]",
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
    section.layers = vec![still_layer(1, 100, 120)];
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
fn each_source_is_resampled_once_and_split_for_each_layer_that_uses_it() {
    let spec = specification(vec![
        clip_section(0, 60, 1),
        SectionToExport {
            frame_count: 30,
            layers: vec![video_layer(1, 0, 30, 1, whole_picture())],
        },
        clip_section(60, 90, 1),
    ]);
    let statements = statements_of(&spec);
    assert_eq!(
        statements[..2],
        [
            "[0:v:0]fps=30/1:round=near,split=2[clip_source_video_0_0][clip_source_video_2_0]",
            "[1:v:0]fps=30/1:round=near,split=1[clip_source_video_1_0]",
        ]
    );
    assert!(statements.contains(
        &"[clip_source_video_1_0]trim=start_frame=0:end_frame=30,setpts=N*1/(30*TB),scale=320:180,setsar=1[finished_clip_1]"
            .to_owned()
    ));
}

#[test]
fn a_source_used_by_two_layers_of_one_section_is_split_for_each() {
    let section = SectionToExport {
        frame_count: 60,
        layers: vec![
            video_layer(0, 0, 60, 1, whole_picture()),
            video_layer(0, 100, 160, 1, place(0, 0, 160, 90, 0.0)),
        ],
    };
    let statements = statements_of(&specification(vec![section]));
    assert_eq!(
        statements[0],
        "[0:v:0]fps=30/1:round=near,split=2[clip_source_video_0_0][clip_source_video_0_1]"
    );
}
