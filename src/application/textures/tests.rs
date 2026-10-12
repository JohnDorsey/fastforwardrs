use egui::Color32;

use super::*;
use crate::project::{Angle, CanvasArea, FontSize, TextColour, TextObject};

fn object() -> TextObject {
    TextObject {
        text: "Hello".to_owned(),
        font_family: String::new(),
        font_size: FontSize::DEFAULT,
        colour: TextColour::WHITE,
        area: CanvasArea::new(0.1, 0.2, 0.5, 0.25),
        rotation: Angle::ZERO,
    }
}

fn key(object: &TextObject) -> TextAppearance {
    TextAppearance::of(object, 1920, 1080)
}

fn picture_of(shade: u8) -> crate::media::CompressedPicture {
    crate::media::CompressedPicture::from_rgb_pixels(
        &[shade; 8 * 8 * 3],
        crate::media::PictureSize {
            width_in_pixels: 8,
            height_in_pixels: 8,
        },
    )
    .unwrap()
}

const VIDEO: SourceMediaIdentifier = SourceMediaIdentifier::new(1);

fn thumbnail_key(level: u8, index: usize) -> ThumbnailKey {
    ThumbnailKey { level, index }
}

#[test]
fn a_thumbnail_that_has_been_made_is_given_and_nothing_is_wanted() {
    let context = Context::default();
    let mut textures = Textures::default();
    let mut library = ThumbnailLibrary::with_room_for(10);
    library.insert(thumbnail_key(2, 9), picture_of(40));

    assert!(
        textures
            .thumbnail(&context, VIDEO, &library, thumbnail_key(2, 9))
            .is_some()
    );

    assert!(textures.take_finer_thumbnails_wanted().is_empty());
}

#[test]
fn a_finer_thumbnail_that_is_not_made_gives_the_coarser_one_of_about_the_same_moment_and_is_wanted() {
    let context = Context::default();
    let mut textures = Textures::default();
    let mut library = ThumbnailLibrary::with_room_for(10);
    library.insert(thumbnail_key(0, 2), picture_of(40)); // 2.0 s

    // 2.25 s at four a second: there is the picture of 2 s, which is shown until the finer one is made.
    assert!(
        textures
            .thumbnail(&context, VIDEO, &library, thumbnail_key(2, 9))
            .is_some()
    );

    let wanted = textures.take_finer_thumbnails_wanted();
    assert_eq!(wanted.len(), 1);
    assert_eq!(wanted[&VIDEO], BTreeSet::from([thumbnail_key(2, 9)]));
    assert!(
        textures.take_finer_thumbnails_wanted().is_empty(),
        "taken: it is not handed over twice"
    );
}

#[test]
fn a_thumbnail_of_a_second_that_is_not_made_yet_is_not_wanted_because_all_of_them_are_being_made() {
    let context = Context::default();
    let mut textures = Textures::default();
    let library = ThumbnailLibrary::with_room_for(10);

    assert!(
        textures
            .thumbnail(&context, VIDEO, &library, thumbnail_key(0, 3))
            .is_none()
    );

    assert!(textures.take_finer_thumbnails_wanted().is_empty());
}

#[test]
fn nothing_at_all_made_gives_no_texture_but_the_finer_picture_is_wanted() {
    let context = Context::default();
    let mut textures = Textures::default();
    let library = ThumbnailLibrary::with_room_for(10);

    assert!(
        textures
            .thumbnail(&context, VIDEO, &library, thumbnail_key(3, 17))
            .is_none()
    );

    assert_eq!(
        textures.take_finer_thumbnails_wanted()[&VIDEO],
        BTreeSet::from([thumbnail_key(3, 17)])
    );
}

#[test]
fn the_same_coarse_picture_is_one_texture_however_many_finer_ones_stand_in_for_it() {
    let context = Context::default();
    let mut textures = Textures::default();
    let mut library = ThumbnailLibrary::with_room_for(10);
    library.insert(thumbnail_key(0, 2), picture_of(40));

    let first = textures
        .thumbnail(&context, VIDEO, &library, thumbnail_key(2, 8))
        .unwrap()
        .id();
    let second = textures
        .thumbnail(&context, VIDEO, &library, thumbnail_key(2, 9))
        .unwrap()
        .id();

    assert_eq!(first, second);
}

fn supplied_picture(serial_number: u64, shade: u8) -> SuppliedPicture {
    SuppliedPicture {
        source: VIDEO,
        source_frame: crate::project::SourceFrameIndex::new(0),
        frame: DecodedFrame {
            width_in_pixels: 1,
            height_in_pixels: 1,
            rgba_pixels: vec![shade, 0, 0, 255],
        },
        serial_number,
    }
}

#[test]
fn the_pictures_of_different_tracks_are_different_textures_and_a_track_keeps_its_own_as_it_changes() {
    let context = Context::default();
    let mut textures = Textures::default();
    let (track_a, track_b) = (TrackId::new(0), TrackId::new(1));

    let a = textures
        .preview_picture(
            &context,
            track_a,
            &supplied_picture(1, 10),
            PictureFilter::Smooth,
            ColourMatrix::NOTHING_CHANGES,
        )
        .id();
    let b = textures
        .preview_picture(
            &context,
            track_b,
            &supplied_picture(2, 20),
            PictureFilter::Smooth,
            ColourMatrix::NOTHING_CHANGES,
        )
        .id();
    assert_ne!(a, b);

    let a_again = textures
        .preview_picture(
            &context,
            track_a,
            &supplied_picture(1, 10),
            PictureFilter::Smooth,
            ColourMatrix::NOTHING_CHANGES,
        )
        .id();
    assert_eq!(a, a_again, "the same picture is not uploaded again");
    let a_next = textures
        .preview_picture(
            &context,
            track_a,
            &supplied_picture(3, 30),
            PictureFilter::Smooth,
            ColourMatrix::NOTHING_CHANGES,
        )
        .id();
    assert_eq!(a, a_next, "a new picture of the track is put in the track's texture");
    assert_eq!(
        textures
            .preview_picture(
                &context,
                track_b,
                &supplied_picture(2, 20),
                PictureFilter::Smooth,
                ColourMatrix::NOTHING_CHANGES
            )
            .id(),
        b,
        "and the other track's is not touched"
    );
}

/// What was uploaded to the graphics card (or uploaded again) in a run of the window's code, apart from what the context
/// makes for its own fonts: how each is put on the screen, and its pixels.
fn pictures_uploaded_while(context: &Context, run: impl FnOnce(&Context)) -> Vec<(TextureOptions, Vec<Color32>)> {
    context.begin_pass(egui::RawInput::default());
    run(context);
    let mut output = context.end_pass();
    let uploaded = output
        .textures_delta
        .set
        .iter()
        .filter(|(id, _)| **id != egui::TextureId::Managed(0))
        .flat_map(|(_, deltas)| {
            deltas.iter().map(|delta| match &delta.image {
                egui::ImageData::Color(image) => (delta.options, image.pixels.clone()),
            })
        })
        .collect();
    // (A window would hand them to the graphics card: they are not to be left unhandled.)
    output.textures_delta.clear();
    uploaded
}

/// The options of the textures that were uploaded (or uploaded again) in a run of the window's code.
fn options_of_the_pictures_uploaded_while(context: &Context, run: impl FnOnce(&Context)) -> Vec<TextureOptions> {
    pictures_uploaded_while(context, run)
        .into_iter()
        .map(|(options, _)| options)
        .collect()
}

fn the_options_that_show_pixels() -> TextureOptions {
    TextureOptions {
        magnification: TextureFilter::Nearest,
        minification: TextureFilter::Linear,
        ..TextureOptions::LINEAR
    }
}

#[test]
fn a_preview_picture_is_blended_to_begin_with_and_uploaded_again_as_blocks_when_the_pixels_are_to_be_seen_and_back_again()
 {
    let context = Context::default();
    let mut textures = Textures::default();
    let track = TrackId::new(0);
    let picture = supplied_picture(1, 10);

    let uploaded = options_of_the_pictures_uploaded_while(&context, |context| {
        textures.preview_picture(
            context,
            track,
            &picture,
            PictureFilter::Smooth,
            ColourMatrix::NOTHING_CHANGES,
        );
    });
    assert_eq!(uploaded, vec![TextureOptions::LINEAR]);

    let uploaded = options_of_the_pictures_uploaded_while(&context, |context| {
        textures.preview_picture(
            context,
            track,
            &picture,
            PictureFilter::Smooth,
            ColourMatrix::NOTHING_CHANGES,
        );
    });
    assert_eq!(uploaded, vec![], "the same picture the same way is not uploaded again");

    let uploaded = options_of_the_pictures_uploaded_while(&context, |context| {
        textures.preview_picture(
            context,
            track,
            &picture,
            PictureFilter::Pixels,
            ColourMatrix::NOTHING_CHANGES,
        );
    });
    assert_eq!(
        uploaded,
        vec![the_options_that_show_pixels()],
        "the same picture, now as blocks"
    );

    let uploaded = options_of_the_pictures_uploaded_while(&context, |context| {
        textures.preview_picture(
            context,
            track,
            &picture,
            PictureFilter::Pixels,
            ColourMatrix::NOTHING_CHANGES,
        );
    });
    assert_eq!(uploaded, vec![]);

    let uploaded = options_of_the_pictures_uploaded_while(&context, |context| {
        textures.preview_picture(
            context,
            track,
            &supplied_picture(2, 20),
            PictureFilter::Pixels,
            ColourMatrix::NOTHING_CHANGES,
        );
    });
    assert_eq!(
        uploaded,
        vec![the_options_that_show_pixels()],
        "a new picture of the track keeps being shown as blocks"
    );

    let uploaded = options_of_the_pictures_uploaded_while(&context, |context| {
        textures.preview_picture(
            context,
            track,
            &supplied_picture(2, 20),
            PictureFilter::Smooth,
            ColourMatrix::NOTHING_CHANGES,
        );
    });
    assert_eq!(uploaded, vec![TextureOptions::LINEAR], "and blended again");
}

#[test]
fn the_picture_of_an_image_file_is_uploaded_again_when_the_way_it_is_shown_changes_and_the_timeline_leaves_the_way_as_it_is()
 {
    let context = Context::default();
    let mut textures = Textures::default();
    let source = SourceMediaIdentifier::new(1);
    let picture = RgbaPicture {
        width_in_pixels: 1,
        height_in_pixels: 1,
        rgba_pixels: vec![255, 0, 0, 255],
    };

    let uploaded = options_of_the_pictures_uploaded_while(&context, |context| {
        textures.image_picture(
            context,
            source,
            &picture,
            PictureFilter::Pixels,
            ColourMatrix::NOTHING_CHANGES,
        );
    });
    assert_eq!(uploaded, vec![the_options_that_show_pixels()]);

    let uploaded = options_of_the_pictures_uploaded_while(&context, |context| {
        textures.image_picture_for_the_timeline(context, source, &picture);
    });
    assert_eq!(
        uploaded,
        vec![],
        "the timeline takes the one there is, whichever way it is shown"
    );

    let uploaded = options_of_the_pictures_uploaded_while(&context, |context| {
        textures.image_picture(
            context,
            source,
            &picture,
            PictureFilter::Smooth,
            ColourMatrix::NOTHING_CHANGES,
        );
    });
    assert_eq!(uploaded, vec![TextureOptions::LINEAR]);

    let other = SourceMediaIdentifier::new(2);
    let uploaded = options_of_the_pictures_uploaded_while(&context, |context| {
        textures.image_picture_for_the_timeline(context, other, &picture);
    });
    assert_eq!(
        uploaded,
        vec![TextureOptions::LINEAR],
        "a picture that the timeline is first to want is blended"
    );
}

#[test]
fn the_picture_of_text_is_made_again_for_the_other_way_of_showing_it_and_each_is_kept() {
    let context = Context::default();
    let mut textures = Textures::default();
    let mut made = 0;
    let mut text_picture = |textures: &mut Textures, context: &Context, filter| {
        textures
            .text_picture(context, key(&object()), filter, ColourMatrix::NOTHING_CHANGES, || {
                made += 1;
                Some(RgbaPicture {
                    width_in_pixels: 1,
                    height_in_pixels: 1,
                    rgba_pixels: vec![255, 255, 255, 255],
                })
            })
            .map(|handle| handle.id())
    };
    let smooth = text_picture(&mut textures, &context, PictureFilter::Smooth).unwrap();
    let blocks = text_picture(&mut textures, &context, PictureFilter::Pixels).unwrap();
    assert_ne!(smooth, blocks);
    assert_eq!(
        text_picture(&mut textures, &context, PictureFilter::Smooth),
        Some(smooth)
    );
    assert_eq!(
        text_picture(&mut textures, &context, PictureFilter::Pixels),
        Some(blocks)
    );
    assert_eq!(made, 2, "one of each, and no more");
}

// --- colours changed by effects ---

/// An orange (200, 100, 50) and the green that turning its hue by a third of the way round makes of it (18, 151, 80), worked out
/// apart from the code (with the matrix of the hue rotation of CSS, in a few lines of arithmetic).
const ORANGE: [u8; 4] = [200, 100, 50, 255];
const ORANGE_TURNED_BY_120_DEGREES: [u8; 4] = [18, 151, 80, 255];

fn turned_by_120_degrees() -> ColourMatrix {
    ColourMatrix::hue_rotation(120.0)
}

fn a_pixel(rgba: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3])
}

fn supplied_orange_picture(serial_number: u64) -> SuppliedPicture {
    SuppliedPicture {
        frame: DecodedFrame {
            rgba_pixels: ORANGE.to_vec(),
            ..supplied_picture(serial_number, 0).frame
        },
        ..supplied_picture(serial_number, 0)
    }
}

fn orange_picture() -> RgbaPicture {
    RgbaPicture {
        width_in_pixels: 1,
        height_in_pixels: 1,
        rgba_pixels: ORANGE.to_vec(),
    }
}

#[test]
fn the_pixels_of_a_picture_have_their_colours_changed_as_the_matrix_says_and_how_clear_they_are_is_not() {
    let frame = DecodedFrame {
        width_in_pixels: 2,
        height_in_pixels: 1,
        rgba_pixels: vec![200, 100, 50, 255, 200, 100, 50, 128],
    };

    let turned = color_image_of(&frame, turned_by_120_degrees());
    let as_they_were = color_image_of(&frame, ColourMatrix::NOTHING_CHANGES);

    assert_eq!(turned.size, [2, 1]);
    assert_eq!(
        turned.pixels,
        vec![a_pixel([18, 151, 80, 255]), a_pixel([18, 151, 80, 128])]
    );
    assert_eq!(
        as_they_were.pixels,
        vec![a_pixel([200, 100, 50, 255]), a_pixel([200, 100, 50, 128])]
    );
}

#[test]
fn a_preview_picture_is_uploaded_with_its_colours_changed_and_again_when_they_are_to_be_changed_another_way() {
    let context = Context::default();
    let mut textures = Textures::default();
    let track = TrackId::new(0);
    let picture = supplied_orange_picture(1);
    let upload = |textures: &mut Textures, colour| {
        pictures_uploaded_while(&context, |context| {
            textures.preview_picture(context, track, &picture, PictureFilter::Smooth, colour);
        })
    };

    let uploaded = upload(&mut textures, turned_by_120_degrees());
    assert_eq!(
        uploaded,
        vec![(TextureOptions::LINEAR, vec![a_pixel(ORANGE_TURNED_BY_120_DEGREES)])]
    );
    assert_eq!(
        textures.colours_of_the_preview_picture(track),
        Some(turned_by_120_degrees())
    );

    assert_eq!(
        upload(&mut textures, turned_by_120_degrees()),
        vec![],
        "the same again is not uploaded again"
    );

    let uploaded = upload(&mut textures, ColourMatrix::NOTHING_CHANGES);
    assert_eq!(
        uploaded,
        vec![(TextureOptions::LINEAR, vec![a_pixel(ORANGE)])],
        "the same picture, with its own colours again"
    );
    assert_eq!(
        textures.colours_of_the_preview_picture(track),
        Some(ColourMatrix::NOTHING_CHANGES)
    );
    assert_eq!(
        textures.colours_of_the_preview_picture(TrackId::new(5)),
        None,
        "no picture of that track"
    );
}

#[test]
fn a_new_picture_of_a_video_is_changed_the_way_the_colours_are_to_be() {
    let context = Context::default();
    let mut textures = Textures::default();
    let track = TrackId::new(0);
    let turn = |textures: &mut Textures, picture: &SuppliedPicture| {
        pictures_uploaded_while(&context, |context| {
            textures.preview_picture(context, track, picture, PictureFilter::Smooth, turned_by_120_degrees());
        })
    };

    turn(&mut textures, &supplied_orange_picture(1));
    let uploaded = turn(&mut textures, &supplied_orange_picture(2));

    assert_eq!(
        uploaded,
        vec![(TextureOptions::LINEAR, vec![a_pixel(ORANGE_TURNED_BY_120_DEGREES)])],
        "the next picture is turned too"
    );
}

#[test]
fn the_picture_of_an_image_file_is_uploaded_with_its_colours_changed_and_the_timeline_leaves_them_as_they_are() {
    let context = Context::default();
    let mut textures = Textures::default();
    let source = SourceMediaIdentifier::new(1);
    let picture = orange_picture();

    let uploaded = pictures_uploaded_while(&context, |context| {
        textures.image_picture(
            context,
            source,
            &picture,
            PictureFilter::Smooth,
            turned_by_120_degrees(),
        );
    });
    assert_eq!(
        uploaded,
        vec![(TextureOptions::LINEAR, vec![a_pixel(ORANGE_TURNED_BY_120_DEGREES)])]
    );
    assert_eq!(
        textures.colours_of_the_picture_of_the_file(source),
        Some(turned_by_120_degrees())
    );

    let uploaded = pictures_uploaded_while(&context, |context| {
        textures.image_picture_for_the_timeline(context, source, &picture);
    });
    assert_eq!(
        uploaded,
        vec![],
        "the timeline takes the one there is, with the colours it has"
    );
    assert_eq!(
        textures.colours_of_the_picture_of_the_file(source),
        Some(turned_by_120_degrees())
    );

    let uploaded = pictures_uploaded_while(&context, |context| {
        textures.image_picture(
            context,
            source,
            &picture,
            PictureFilter::Smooth,
            ColourMatrix::NOTHING_CHANGES,
        );
    });
    assert_eq!(uploaded, vec![(TextureOptions::LINEAR, vec![a_pixel(ORANGE)])]);
    assert_eq!(
        textures.colours_of_the_picture_of_the_file(source),
        Some(ColourMatrix::NOTHING_CHANGES)
    );
}

#[test]
fn the_picture_of_an_image_file_that_the_timeline_is_first_to_want_has_the_colours_it_has_and_the_preview_changes_them_after()
 {
    let context = Context::default();
    let mut textures = Textures::default();
    let source = SourceMediaIdentifier::new(1);
    let picture = orange_picture();

    let uploaded = pictures_uploaded_while(&context, |context| {
        textures.image_picture_for_the_timeline(context, source, &picture);
    });
    assert_eq!(uploaded, vec![(TextureOptions::LINEAR, vec![a_pixel(ORANGE)])]);

    let uploaded = pictures_uploaded_while(&context, |context| {
        textures.image_picture(
            context,
            source,
            &picture,
            PictureFilter::Pixels,
            turned_by_120_degrees(),
        );
    });
    assert_eq!(
        uploaded,
        vec![(
            the_options_that_show_pixels(),
            vec![a_pixel(ORANGE_TURNED_BY_120_DEGREES)]
        )],
        "turned, and as blocks"
    );
}

#[test]
fn the_picture_of_text_is_made_again_for_each_way_the_colours_are_changed_and_each_is_kept() {
    let context = Context::default();
    let mut textures = Textures::default();
    let mut made = 0;
    let mut text_picture = |textures: &mut Textures, context: &Context, colour| {
        textures
            .text_picture(context, key(&object()), PictureFilter::Smooth, colour, || {
                made += 1;
                Some(orange_picture())
            })
            .map(|handle| handle.id())
    };

    let as_it_is = text_picture(&mut textures, &context, ColourMatrix::NOTHING_CHANGES).unwrap();
    let turned = text_picture(&mut textures, &context, turned_by_120_degrees()).unwrap();
    let turned_the_other_way = text_picture(&mut textures, &context, ColourMatrix::hue_rotation(-120.0)).unwrap();

    assert_ne!(as_it_is, turned);
    assert_ne!(turned, turned_the_other_way);
    assert_ne!(as_it_is, turned_the_other_way);
    assert_eq!(
        text_picture(&mut textures, &context, ColourMatrix::NOTHING_CHANGES),
        Some(as_it_is)
    );
    assert_eq!(
        text_picture(&mut textures, &context, turned_by_120_degrees()),
        Some(turned)
    );
    assert_eq!(made, 3, "one of each, and no more");
}

#[test]
fn the_picture_of_text_is_uploaded_with_its_colours_changed() {
    let context = Context::default();
    let mut textures = Textures::default();

    let uploaded = pictures_uploaded_while(&context, |context| {
        textures.text_picture(
            context,
            key(&object()),
            PictureFilter::Smooth,
            turned_by_120_degrees(),
            || Some(orange_picture()),
        );
    });

    assert_eq!(
        uploaded,
        vec![(TextureOptions::LINEAR, vec![a_pixel(ORANGE_TURNED_BY_120_DEGREES)])]
    );
}

#[test]
fn a_picture_of_text_that_cannot_be_made_yet_is_none_whatever_colours_it_is_to_have() {
    let context = Context::default();
    let mut textures = Textures::default();
    assert!(
        textures
            .text_picture(
                &context,
                key(&object()),
                PictureFilter::Smooth,
                turned_by_120_degrees(),
                || None
            )
            .is_none()
    );
}

/// A thumbnail of one flat orange (a JPEG, which gives a colour near it), as the second `second` of a video.
fn library_with_an_orange_thumbnail(second: usize) -> ThumbnailLibrary {
    let mut library = ThumbnailLibrary::with_room_for(10);
    library.insert(
        thumbnail_key(0, second),
        crate::media::CompressedPicture::from_rgb_pixels(
            &[[200u8, 100, 50]; 8 * 8].concat(),
            crate::media::PictureSize {
                width_in_pixels: 8,
                height_in_pixels: 8,
            },
        )
        .unwrap(),
    );
    library
}

#[test]
fn a_thumbnail_is_uploaded_with_its_colours_changed_and_each_way_of_changing_them_is_a_texture_of_its_own() {
    let context = Context::default();
    let mut textures = Textures::default();
    let library = library_with_an_orange_thumbnail(2);
    let key = thumbnail_key(0, 2);

    let uploaded_as_it_is = pictures_uploaded_while(&context, |context| {
        textures.thumbnail(context, VIDEO, &library, key);
    });
    let uploaded_turned = pictures_uploaded_while(&context, |context| {
        textures.thumbnail_with_colours_changed(context, VIDEO, &library, key, turned_by_120_degrees());
    });

    assert_eq!(uploaded_as_it_is.len(), 1);
    assert_eq!(uploaded_turned.len(), 1);
    let (as_it_is, turned) = (uploaded_as_it_is[0].1[0], uploaded_turned[0].1[0]);
    let mut by_the_matrix = [as_it_is.r(), as_it_is.g(), as_it_is.b(), 255];
    turned_by_120_degrees().apply_to_rgba_pixels(&mut by_the_matrix);
    assert_eq!(
        [turned.r(), turned.g(), turned.b()],
        [by_the_matrix[0], by_the_matrix[1], by_the_matrix[2]],
        "its pixels are changed as the matrix says"
    );
    for (found, wanted) in [turned.r(), turned.g(), turned.b()]
        .into_iter()
        .zip(ORANGE_TURNED_BY_120_DEGREES)
    {
        assert!(
            found.abs_diff(wanted) <= 12,
            "{turned:?} is not near the green: the JPEG is a little out"
        );
    }
    assert!(as_it_is.r().abs_diff(200) <= 6, "{as_it_is:?}");

    let plain = textures.thumbnail(&context, VIDEO, &library, key).unwrap().id();
    let coloured = textures
        .thumbnail_with_colours_changed(&context, VIDEO, &library, key, turned_by_120_degrees())
        .unwrap()
        .id();
    let other_colours = textures
        .thumbnail_with_colours_changed(&context, VIDEO, &library, key, ColourMatrix::hue_rotation(-120.0))
        .unwrap()
        .id();
    assert_ne!(plain, coloured);
    assert_ne!(coloured, other_colours);
    assert_ne!(plain, other_colours);
    assert_eq!(textures.thumbnail(&context, VIDEO, &library, key).unwrap().id(), plain);
    assert_eq!(
        textures
            .thumbnail_with_colours_changed(&context, VIDEO, &library, key, turned_by_120_degrees())
            .unwrap()
            .id(),
        coloured,
        "the same thumbnail the same way is the same texture"
    );
}

#[test]
fn a_thumbnail_that_is_not_changed_by_its_colours_is_the_same_texture_as_the_one_the_timeline_has() {
    let context = Context::default();
    let mut textures = Textures::default();
    let library = library_with_an_orange_thumbnail(2);
    let key = thumbnail_key(0, 2);

    let for_the_timeline = textures.thumbnail(&context, VIDEO, &library, key).unwrap().id();
    let for_the_preview = textures
        .thumbnail_with_colours_changed(&context, VIDEO, &library, key, ColourMatrix::NOTHING_CHANGES)
        .unwrap()
        .id();

    assert_eq!(for_the_timeline, for_the_preview);
}

#[test]
fn a_thumbnail_with_its_colours_changed_is_let_go_of_with_the_others_when_it_is_not_used_and_a_finer_one_is_wanted_as_ever()
 {
    let context = Context::default();
    let mut textures = Textures::default();
    let library = library_with_an_orange_thumbnail(2);

    textures.thumbnail(&context, VIDEO, &library, thumbnail_key(0, 2));
    textures.thumbnail_with_colours_changed(&context, VIDEO, &library, thumbnail_key(0, 2), turned_by_120_degrees());
    textures.forget_thumbnails_not_used_this_frame();
    assert_eq!(
        textures.ways_the_colours_of_the_thumbnails_were_changed().len(),
        2,
        "both were used"
    );

    textures.thumbnail_with_colours_changed(&context, VIDEO, &library, thumbnail_key(0, 2), turned_by_120_degrees());
    textures.forget_thumbnails_not_used_this_frame();
    assert_eq!(
        textures.ways_the_colours_of_the_thumbnails_were_changed(),
        vec![turned_by_120_degrees().bit_pattern()],
        "only the one that was used since is kept"
    );

    // A finer picture that is not made is wanted, and the coarser one stands in, with the colours.
    let stands_in = textures.thumbnail_with_colours_changed(
        &context,
        VIDEO,
        &library,
        thumbnail_key(2, 9),
        turned_by_120_degrees(),
    );
    assert!(stands_in.is_some());
    let wanted = textures.take_finer_thumbnails_wanted();
    assert_eq!(wanted[&VIDEO], BTreeSet::from([thumbnail_key(2, 9)]));
}

#[test]
fn moving_a_text_object_does_not_change_its_picture() {
    let mut moved = object();
    moved.area = moved.area.moved_by(0.1, 0.2);
    assert_eq!(key(&object()), key(&moved));
}

/// The picture is of the text upright in its area; turning it is done when it is drawn, so
/// turning a text object round and round makes no new pictures.
#[test]
fn the_picture_of_an_image_file_is_uploaded_once_and_each_file_has_its_own() {
    let context = Context::default();
    let mut textures = Textures::default();
    let red = RgbaPicture {
        width_in_pixels: 1,
        height_in_pixels: 1,
        rgba_pixels: vec![255, 0, 0, 255],
    };
    let blue = RgbaPicture {
        rgba_pixels: vec![0, 0, 255, 255],
        ..red.clone()
    };
    let first = textures
        .image_picture(
            &context,
            SourceMediaIdentifier::new(1),
            &red,
            PictureFilter::Smooth,
            ColourMatrix::NOTHING_CHANGES,
        )
        .id();
    let second = textures
        .image_picture(
            &context,
            SourceMediaIdentifier::new(2),
            &blue,
            PictureFilter::Smooth,
            ColourMatrix::NOTHING_CHANGES,
        )
        .id();
    assert_ne!(first, second);
    assert_eq!(
        textures
            .image_picture(
                &context,
                SourceMediaIdentifier::new(1),
                &red,
                PictureFilter::Smooth,
                ColourMatrix::NOTHING_CHANGES
            )
            .id(),
        first,
        "the same file is the same texture"
    );
    textures.clear();
    assert_ne!(
        textures
            .image_picture(
                &context,
                SourceMediaIdentifier::new(1),
                &red,
                PictureFilter::Smooth,
                ColourMatrix::NOTHING_CHANGES
            )
            .id(),
        first,
        "forgotten when everything is"
    );
}

#[test]
fn turning_a_text_object_does_not_change_its_picture() {
    let mut turned = object();
    turned.rotation = Angle::from_degrees(37.0);
    assert_eq!(key(&object()), key(&turned));
}

#[test]
fn anything_that_changes_how_the_text_looks_changes_its_picture() {
    let base = key(&object());
    let mut changed = object();
    changed.text = "Hello!".to_owned();
    assert_ne!(base, key(&changed), "the words");
    let mut changed = object();
    changed.font_family = "Arial".to_owned();
    assert_ne!(base, key(&changed), "the font");
    let mut changed = object();
    changed.font_size = FontSize::from_points(80.0);
    assert_ne!(base, key(&changed), "the size of the letters");
    let mut changed = object();
    changed.colour = TextColour::new(1, 2, 3, 4);
    assert_ne!(base, key(&changed), "the colour");
    let mut changed = object();
    changed.area = changed
        .area
        .with_corner_dragged_to(crate::project::Corner::BottomRight, 0.9, 0.9);
    assert_ne!(base, key(&changed), "how big the area is");
    assert_ne!(
        base,
        TextAppearance::of(&object(), 1280, 720),
        "how big the finished picture is"
    );
}
