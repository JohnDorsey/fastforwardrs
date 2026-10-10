//! Drawing the fast-forward symbol: two filled triangles and the speed factor, such as `▶▶ 4x`,
//! in white with a dark outline so that it can be read on top of any footage.
//!
//! The symbol is drawn into an image by this code (rather than by the user interface toolkit or by
//! ffmpeg), so that the live preview and the exported video get exactly the same picture.

use std::sync::OnceLock;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};

use crate::project::SpeedFactor;

/// The typeface for the speed number. See `assets/fonts/Hack-Regular-license.txt`.
const FONT_BYTES: &[u8] = include_bytes!("../../assets/fonts/Hack-Regular.ttf");

// All sizes below are fractions of the symbol's height, so the symbol looks the same at any size.

/// Empty space kept all the way round the shapes, which is where the outline is drawn.
const PADDING: f32 = 0.15;
/// How far the dark outline reaches out from the white shapes.
const OUTLINE_RADIUS: f32 = 0.06;
/// How dark the outline is, from 0 (invisible) to 1 (solid black).
const OUTLINE_STRENGTH: f32 = 0.9;
/// How tall the triangles are, as a fraction of the height of the digits.
const TRIANGLE_HEIGHT_PER_DIGIT_HEIGHT: f32 = 0.9;
/// How wide each triangle is, as a fraction of its own height.
const TRIANGLE_WIDTH_PER_TRIANGLE_HEIGHT: f32 = 0.8;
/// The space between the two triangles, as a fraction of a triangle's height.
const GAP_BETWEEN_TRIANGLES_PER_TRIANGLE_HEIGHT: f32 = 0.12;
/// The space between the triangles and the number, as a fraction of the height of the digits.
const GAP_BEFORE_NUMBER_PER_DIGIT_HEIGHT: f32 = 0.4;
/// The smallest symbol that is drawn; anything smaller could not be read anyway.
pub const SMALLEST_HEIGHT_IN_PIXELS: u32 = 8;
/// How many sample points per pixel (along each side) are used to smooth the triangles' edges.
const TRIANGLE_SAMPLES_PER_PIXEL_SIDE: u32 = 4;

/// A picture of the symbol: RGBA pixels with straight (not premultiplied) alpha, row by row, top
/// row first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndicatorPicture {
    pub width_in_pixels: u32,
    pub height_in_pixels: u32,
    pub rgba_pixels: Vec<u8>,
}

impl IndicatorPicture {
    /// The picture as the bytes of a PNG file, which is how ffmpeg is given it for the export.
    pub fn to_png_bytes(&self) -> Result<Vec<u8>, image::ImageError> {
        let mut png_bytes = Vec::new();
        PngEncoder::new(&mut png_bytes).write_image(
            &self.rgba_pixels,
            self.width_in_pixels,
            self.height_in_pixels,
            ExtendedColorType::Rgba8,
        )?;
        Ok(png_bytes)
    }
}

fn font() -> &'static FontRef<'static> {
    static FONT: OnceLock<FontRef<'static>> = OnceLock::new();
    FONT.get_or_init(|| FontRef::try_from_slice(FONT_BYTES).expect("the bundled font file is a valid font"))
}

/// Where everything goes, in units of the symbol's height.
struct SymbolMetrics {
    aspect_ratio: f32,
    digit_height: f32,
    triangle_height: f32,
    triangle_width: f32,
    gap_between_triangles: f32,
    number_left: f32,
    /// The size of the typeface, in units of height per em, chosen so that digits are exactly
    /// `digit_height` tall.
    font_size: f32,
    text: String,
}

impl SymbolMetrics {
    fn for_speed(speed_factor: SpeedFactor) -> Self {
        let text = format!("{}x", speed_factor.value());
        let digit_height = 1.0 - 2.0 * PADDING;
        let triangle_height = digit_height * TRIANGLE_HEIGHT_PER_DIGIT_HEIGHT;
        let triangle_width = triangle_height * TRIANGLE_WIDTH_PER_TRIANGLE_HEIGHT;
        let gap_between_triangles = triangle_height * GAP_BETWEEN_TRIANGLES_PER_TRIANGLE_HEIGHT;
        let number_left =
            PADDING + 2.0 * triangle_width + gap_between_triangles + digit_height * GAP_BEFORE_NUMBER_PER_DIGIT_HEIGHT;

        let font_size = digit_height / digit_height_per_font_size();
        let scaled_font = font().as_scaled(PxScale::from(font_size));
        let text_width: f32 = text
            .chars()
            .map(|character| scaled_font.h_advance(scaled_font.glyph_id(character)))
            .sum();

        Self {
            aspect_ratio: number_left + text_width + PADDING,
            digit_height,
            triangle_height,
            triangle_width,
            gap_between_triangles,
            number_left,
            font_size,
            text,
        }
    }
}

/// How tall the digits are relative to the size the typeface is set at.
fn digit_height_per_font_size() -> f32 {
    static RATIO: OnceLock<f32> = OnceLock::new();
    *RATIO.get_or_init(|| {
        const MEASURING_SIZE: f32 = 1000.0;
        let glyph = font()
            .glyph_id('4')
            .with_scale_and_position(PxScale::from(MEASURING_SIZE), point(0.0, 0.0));
        let bounds = font()
            .outline_glyph(glyph)
            .expect("the bundled font has the digit 4")
            .px_bounds();
        (bounds.max.y - bounds.min.y) / MEASURING_SIZE
    })
}

/// The width of the symbol divided by its height. It depends only on how many digits the speed
/// factor has, so the user interface can place the symbol before drawing it.
pub fn indicator_aspect_ratio(speed_factor: SpeedFactor) -> f32 {
    SymbolMetrics::for_speed(speed_factor).aspect_ratio
}

/// Draws the symbol for `speed_factor`, `height_in_pixels` tall (at least 8 pixels).
pub fn render_speed_indicator(speed_factor: SpeedFactor, height_in_pixels: u32) -> IndicatorPicture {
    let height_in_pixels = height_in_pixels.max(SMALLEST_HEIGHT_IN_PIXELS);
    let height = height_in_pixels as f32;
    let metrics = SymbolMetrics::for_speed(speed_factor);
    let width_in_pixels = ((metrics.aspect_ratio * height).round() as u32).max(1);

    let mut shape = CoverageMask::new(width_in_pixels, height_in_pixels);
    let middle = height / 2.0;

    let triangle_height = metrics.triangle_height * height;
    let triangle_width = metrics.triangle_width * height;
    for triangle_number in 0..2 {
        let left =
            PADDING * height + triangle_number as f32 * (triangle_width + metrics.gap_between_triangles * height);
        shape.fill_triangle(
            (left, middle - triangle_height / 2.0),
            (left, middle + triangle_height / 2.0),
            (left + triangle_width, middle),
        );
    }

    let scaled_font = font().as_scaled(PxScale::from(metrics.font_size * height));
    let baseline = middle + metrics.digit_height * height / 2.0;
    let mut pen_x = metrics.number_left * height;
    for character in metrics.text.chars() {
        let glyph_id = scaled_font.glyph_id(character);
        let glyph = glyph_id.with_scale_and_position(PxScale::from(metrics.font_size * height), point(pen_x, baseline));
        if let Some(outlined_glyph) = font().outline_glyph(glyph) {
            let bounds = outlined_glyph.px_bounds();
            outlined_glyph.draw(|glyph_x, glyph_y, coverage| {
                shape.add(
                    bounds.min.x as i32 + glyph_x as i32,
                    bounds.min.y as i32 + glyph_y as i32,
                    coverage,
                );
            });
        }
        pen_x += scaled_font.h_advance(glyph_id);
    }

    let outline = shape.grown_by(OUTLINE_RADIUS * height);
    compose_white_shape_over_dark_outline(&shape, &outline)
}

/// For each pixel, how much of it is covered by a shape: 0 for none and 1 for all.
struct CoverageMask {
    width: u32,
    height: u32,
    coverage: Vec<f32>,
}

impl CoverageMask {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            coverage: vec![0.0; width as usize * height as usize],
        }
    }

    fn index_of(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return None;
        }
        Some(y as usize * self.width as usize + x as usize)
    }

    /// Adds coverage to a pixel. Where shapes overlap, the larger coverage wins.
    fn add(&mut self, x: i32, y: i32, coverage: f32) {
        if let Some(index) = self.index_of(x, y) {
            self.coverage[index] = self.coverage[index].max(coverage.clamp(0.0, 1.0));
        }
    }

    /// The coverage of one pixel (0 outside the mask). Only the tests look at single pixels.
    #[cfg(test)]
    fn at(&self, x: i32, y: i32) -> f32 {
        self.index_of(x, y).map_or(0.0, |index| self.coverage[index])
    }

    /// Fills a triangle, smoothing its edges by testing several sample points inside each pixel.
    fn fill_triangle(&mut self, first: (f32, f32), second: (f32, f32), third: (f32, f32)) {
        let left = first.0.min(second.0).min(third.0).floor().max(0.0) as i32;
        let right = first.0.max(second.0).max(third.0).ceil().min(self.width as f32) as i32;
        let top = first.1.min(second.1).min(third.1).floor().max(0.0) as i32;
        let bottom = first.1.max(second.1).max(third.1).ceil().min(self.height as f32) as i32;

        let samples_per_side = TRIANGLE_SAMPLES_PER_PIXEL_SIDE;
        let total_samples = (samples_per_side * samples_per_side) as f32;
        for y in top..bottom {
            for x in left..right {
                let mut samples_inside = 0;
                for sample_y in 0..samples_per_side {
                    for sample_x in 0..samples_per_side {
                        let point = (
                            x as f32 + (sample_x as f32 + 0.5) / samples_per_side as f32,
                            y as f32 + (sample_y as f32 + 0.5) / samples_per_side as f32,
                        );
                        if triangle_contains(first, second, third, point) {
                            samples_inside += 1;
                        }
                    }
                }
                self.add(x, y, samples_inside as f32 / total_samples);
            }
        }
    }

    /// The same shapes, fattened by `radius` pixels in every direction, with a soft edge.
    ///
    /// This measures, for every pixel, the distance to the nearest part of a shape (a distance
    /// transform, which takes time proportional to the number of pixels however large the radius
    /// is) and keeps the pixels that are within the radius.
    fn grown_by(&self, radius: f32) -> CoverageMask {
        let width = self.width as usize;
        let height = self.height as usize;
        // Squared distance to the nearest shape pixel, starting from "zero inside, far outside".
        let mut squared_distances: Vec<f64> = self
            .coverage
            .iter()
            .map(|&coverage| if coverage >= 0.5 { 0.0 } else { VERY_FAR_SQUARED })
            .collect();

        let mut transform = DistanceTransform::with_capacity(width.max(height));
        let mut column_values = vec![0.0f64; height];
        for column in 0..width {
            for row in 0..height {
                column_values[row] = squared_distances[row * width + column];
            }
            transform.apply(&mut column_values);
            for row in 0..height {
                squared_distances[row * width + column] = column_values[row];
            }
        }
        for row in 0..height {
            transform.apply(&mut squared_distances[row * width..(row + 1) * width]);
        }

        let mut grown = CoverageMask::new(self.width, self.height);
        for (grown_coverage, (&squared_distance, &shape_coverage)) in grown
            .coverage
            .iter_mut()
            .zip(squared_distances.iter().zip(&self.coverage))
        {
            // Fade out over the last pixel of the radius so the outline's edge is smooth too.
            let reach = (f64::from(radius) + 0.5 - squared_distance.sqrt()).clamp(0.0, 1.0) as f32;
            *grown_coverage = reach.max(shape_coverage);
        }
        grown
    }
}

/// A squared distance larger than any real one, standing in for "no shape anywhere near". (Not
/// infinity, which would turn into not-a-number in the arithmetic below.)
const VERY_FAR_SQUARED: f64 = 1.0e12;

/// The one-dimensional squared distance transform of Felzenszwalb and Huttenlocher (2012), which
/// takes time proportional to the length of the line. Working memory is kept between lines so
/// that it is allocated only once.
struct DistanceTransform {
    input: Vec<f64>,
    parabola_positions: Vec<usize>,
    parabola_boundaries: Vec<f64>,
}

impl DistanceTransform {
    fn with_capacity(longest_line: usize) -> Self {
        Self {
            input: vec![0.0; longest_line],
            parabola_positions: vec![0; longest_line],
            parabola_boundaries: vec![0.0; longest_line + 1],
        }
    }

    /// Replaces every value `f[q]` of `line` with `min over p of (f[p] + (q - p)^2)`. Given a line
    /// that is 0 inside a shape and very large elsewhere, that is the squared distance from each
    /// position to the nearest part of the shape.
    ///
    /// Each `f[p]` defines a parabola `f[p] + (x - p)^2`. The answer at `q` is the height of the
    /// lowest parabola there, so the work is to find the lower envelope of all the parabolas.
    fn apply(&mut self, line: &mut [f64]) {
        let length = line.len();
        if length == 0 {
            return;
        }
        let input = &mut self.input[..length];
        input.copy_from_slice(line);
        let positions = &mut self.parabola_positions;
        let boundaries = &mut self.parabola_boundaries;

        // positions[0..=last] are the parabolas on the envelope, left to right, and parabola
        // positions[i] is the lowest one from boundaries[i] to boundaries[i + 1].
        let mut last = 0;
        positions[0] = 0;
        boundaries[0] = f64::NEG_INFINITY;
        boundaries[1] = f64::INFINITY;
        for position in 1..length {
            let crossing = loop {
                let rival = positions[last];
                let crossing = ((input[position] + (position * position) as f64)
                    - (input[rival] + (rival * rival) as f64))
                    / (2.0 * position as f64 - 2.0 * rival as f64);
                // The first boundary is minus infinity, so this never pops the first parabola.
                if crossing <= boundaries[last] {
                    last -= 1;
                } else {
                    break crossing;
                }
            };
            last += 1;
            positions[last] = position;
            boundaries[last] = crossing;
            boundaries[last + 1] = f64::INFINITY;
        }

        let mut current = 0;
        for (position, value) in line.iter_mut().enumerate() {
            while boundaries[current + 1] < position as f64 {
                current += 1;
            }
            let nearest = positions[current];
            let offset = position as f64 - nearest as f64;
            *value = offset * offset + input[nearest];
        }
    }
}

fn triangle_contains(first: (f32, f32), second: (f32, f32), third: (f32, f32), point: (f32, f32)) -> bool {
    // The point is inside if it is on the same side of all three edges.
    let side_of =
        |from: (f32, f32), to: (f32, f32)| (to.0 - from.0) * (point.1 - from.1) - (to.1 - from.1) * (point.0 - from.0);
    let sides = [side_of(first, second), side_of(second, third), side_of(third, first)];
    sides.iter().all(|&side| side >= 0.0) || sides.iter().all(|&side| side <= 0.0)
}

/// White shapes drawn over a dark outline, as one picture with straight alpha.
fn compose_white_shape_over_dark_outline(shape: &CoverageMask, outline: &CoverageMask) -> IndicatorPicture {
    let mut rgba_pixels = Vec::with_capacity(shape.coverage.len() * 4);
    for (&shape_alpha, &outline_coverage) in shape.coverage.iter().zip(&outline.coverage) {
        let outline_alpha = outline_coverage * OUTLINE_STRENGTH;
        let combined_alpha = shape_alpha + outline_alpha * (1.0 - shape_alpha);
        // White is 255 and the outline is 0, so only the white shape contributes to the colour.
        let brightness = if combined_alpha > 0.0 {
            255.0 * shape_alpha / combined_alpha
        } else {
            0.0
        };
        rgba_pixels.extend_from_slice(&[
            brightness.round() as u8,
            brightness.round() as u8,
            brightness.round() as u8,
            (combined_alpha * 255.0).round() as u8,
        ]);
    }
    IndicatorPicture {
        width_in_pixels: shape.width,
        height_in_pixels: shape.height,
        rgba_pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::DeterministicRandomNumbers;

    fn speed(factor: u32) -> SpeedFactor {
        SpeedFactor::new(factor).unwrap()
    }

    fn pixel(picture: &IndicatorPicture, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * picture.width_in_pixels + x) * 4) as usize;
        picture.rgba_pixels[offset..offset + 4].try_into().unwrap()
    }

    #[test]
    fn the_picture_has_the_requested_height_and_the_width_the_aspect_ratio_predicts() {
        for factor in [2, 4, 16, 100, 1000] {
            for height in [20u32, 64, 100, 173] {
                let picture = render_speed_indicator(speed(factor), height);
                assert_eq!(picture.height_in_pixels, height);
                let predicted_width = indicator_aspect_ratio(speed(factor)) * height as f32;
                assert!(
                    (picture.width_in_pixels as f32 - predicted_width).abs() <= 0.5,
                    "{factor}x at {height} px: width {} but predicted {predicted_width}",
                    picture.width_in_pixels
                );
                assert_eq!(
                    picture.rgba_pixels.len(),
                    (picture.width_in_pixels * picture.height_in_pixels * 4) as usize
                );
            }
        }
    }

    #[test]
    fn more_digits_make_a_wider_symbol() {
        let widths: Vec<f32> = [4, 16, 100, 1000]
            .map(|factor| indicator_aspect_ratio(speed(factor)))
            .to_vec();
        assert!(widths.windows(2).all(|pair| pair[0] < pair[1]), "{widths:?}");
        // Same number of digits, same width.
        assert_eq!(indicator_aspect_ratio(speed(2)), indicator_aspect_ratio(speed(8)));
    }

    #[test]
    fn symbols_are_never_drawn_smaller_than_the_minimum() {
        assert_eq!(
            render_speed_indicator(speed(4), 1).height_in_pixels,
            SMALLEST_HEIGHT_IN_PIXELS
        );
        assert_eq!(
            render_speed_indicator(speed(4), 0).height_in_pixels,
            SMALLEST_HEIGHT_IN_PIXELS
        );
    }

    #[test]
    fn drawing_is_deterministic() {
        assert_eq!(
            render_speed_indicator(speed(12), 90),
            render_speed_indicator(speed(12), 90)
        );
    }

    #[test]
    fn different_speeds_look_different() {
        assert_ne!(
            render_speed_indicator(speed(4), 90).rgba_pixels,
            render_speed_indicator(speed(8), 90).rgba_pixels
        );
    }

    #[test]
    fn the_padding_around_the_shapes_is_fully_transparent() {
        let picture = render_speed_indicator(speed(4), 100);
        for (x, y) in [
            (0, 0),
            (picture.width_in_pixels - 1, 0),
            (0, 99),
            (picture.width_in_pixels - 1, 99),
        ] {
            assert_eq!(pixel(&picture, x, y)[3], 0, "corner ({x}, {y})");
        }
    }

    #[test]
    fn the_first_triangle_is_solid_white_in_its_middle() {
        let picture = render_speed_indicator(speed(4), 100);
        // The first triangle's left edge is at the padding (15 px); its middle, a bit to the right
        // of that, vertically centred.
        assert_eq!(pixel(&picture, 25, 50), [255, 255, 255, 255]);
    }

    #[test]
    fn the_shapes_are_surrounded_by_a_dark_outline_that_is_partly_transparent() {
        let picture = render_speed_indicator(speed(4), 100);
        // Just left of the first triangle's left edge (at x = 15) is the outline.
        let outline_pixel = pixel(&picture, 13, 50);
        assert_eq!(outline_pixel[0], 0, "the outline is dark: {outline_pixel:?}");
        assert!(
            outline_pixel[3] > 100,
            "the outline is clearly visible: {outline_pixel:?}"
        );
        assert!(
            outline_pixel[3] < 255,
            "the outline is not fully opaque: {outline_pixel:?}"
        );
    }

    #[test]
    fn the_number_is_drawn_to_the_right_of_the_triangles() {
        let picture = render_speed_indicator(speed(4), 100);
        let triangles_end = ((PADDING + 2.0 * 0.72 * 0.9 * 0.8 + 0.72 * 0.9 * 0.12) * 100.0) as u32;
        let white_pixels_in = |from_x: u32, to_x: u32| {
            (0..picture.height_in_pixels)
                .flat_map(|y| (from_x..to_x).map(move |x| (x, y)))
                .filter(|&(x, y)| pixel(&picture, x, y) == [255, 255, 255, 255])
                .count()
        };
        assert!(
            white_pixels_in(triangles_end + 12, picture.width_in_pixels) > 300,
            "the digits and the x are drawn"
        );
        assert!(white_pixels_in(0, triangles_end) > 1000, "the triangles are drawn");
    }

    #[test]
    fn the_symbol_scales_with_its_height() {
        let small = render_speed_indicator(speed(4), 50);
        let large = render_speed_indicator(speed(4), 200);
        let opaque_share = |picture: &IndicatorPicture| {
            picture.rgba_pixels.chunks(4).filter(|pixel| pixel[3] == 255).count() as f32
                / (picture.width_in_pixels * picture.height_in_pixels) as f32
        };
        assert!(
            (opaque_share(&small) - opaque_share(&large)).abs() < 0.03,
            "{} vs {}",
            opaque_share(&small),
            opaque_share(&large)
        );
    }

    #[test]
    fn the_png_form_is_a_real_png_that_reads_back_as_the_same_picture() {
        let picture = render_speed_indicator(speed(8), 64);
        let png_bytes = picture.to_png_bytes().unwrap();
        assert_eq!(&png_bytes[1..4], b"PNG");
        let decoded = image::load_from_memory_with_format(&png_bytes, image::ImageFormat::Png)
            .unwrap()
            .to_rgba8();
        assert_eq!(
            (decoded.width(), decoded.height()),
            (picture.width_in_pixels, picture.height_in_pixels)
        );
        assert_eq!(decoded.into_raw(), picture.rgba_pixels);
    }

    #[test]
    fn the_distance_transform_matches_checking_every_pair_of_positions() {
        let mut random = DeterministicRandomNumbers::with_seed(11);
        let mut transform = DistanceTransform::with_capacity(60);
        for length in [1usize, 2, 3, 10, 37, 60] {
            for _ in 0..40 {
                let original: Vec<f64> = (0..length)
                    .map(|_| match random.next_below(4) {
                        0 => 0.0,
                        1 => random.next_below(50) as f64,
                        _ => VERY_FAR_SQUARED,
                    })
                    .collect();
                let mut line = original.clone();
                transform.apply(&mut line);
                for (position, &result) in line.iter().enumerate() {
                    let expected = original
                        .iter()
                        .enumerate()
                        .map(|(other, &value)| value + ((position as f64) - (other as f64)).powi(2))
                        .fold(f64::INFINITY, f64::min);
                    assert_eq!(result, expected, "position {position} of {original:?}");
                }
            }
        }
    }

    #[test]
    fn growing_a_shape_matches_measuring_the_distance_to_every_shape_pixel() {
        let mut random = DeterministicRandomNumbers::with_seed(5);
        for _ in 0..30 {
            let (width, height) = (3 + random.next_below(30) as u32, 3 + random.next_below(20) as u32);
            let mut shape = CoverageMask::new(width, height);
            for _ in 0..random.next_below(8) {
                shape.add(
                    random.next_below(u64::from(width)) as i32,
                    random.next_below(u64::from(height)) as i32,
                    1.0,
                );
            }
            let radius = 1.0 + random.next_below(6) as f32;

            let grown = shape.grown_by(radius);

            for y in 0..height as i32 {
                for x in 0..width as i32 {
                    let nearest_distance = (0..height as i32)
                        .flat_map(|other_y| (0..width as i32).map(move |other_x| (other_x, other_y)))
                        .filter(|&(other_x, other_y)| shape.at(other_x, other_y) >= 0.5)
                        .map(|(other_x, other_y)| (((x - other_x).pow(2) + (y - other_y).pow(2)) as f32).sqrt())
                        .fold(f32::INFINITY, f32::min);
                    let expected = (radius + 0.5 - nearest_distance).clamp(0.0, 1.0).max(shape.at(x, y));
                    assert!(
                        (grown.at(x, y) - expected).abs() < 1e-4,
                        "pixel ({x}, {y}) of a {width}x{height} mask grown by {radius}: got {}, expected {expected}",
                        grown.at(x, y)
                    );
                }
            }
        }
    }

    #[test]
    fn growing_an_empty_mask_leaves_it_empty() {
        let grown = CoverageMask::new(20, 10).grown_by(4.0);
        assert!(grown.coverage.iter().all(|&coverage| coverage == 0.0));
    }

    /// Regression test: drawing the outline used to check every pixel against every pixel within
    /// the radius, so a symbol sized for a 4K picture took minutes to draw.
    #[test]
    fn a_symbol_as_large_as_a_4k_picture_allows_is_drawn_in_a_reasonable_time() {
        let started = std::time::Instant::now();
        let picture = render_speed_indicator(speed(1000), 1300);
        assert_eq!(picture.height_in_pixels, 1300);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(20),
            "drawing a very large symbol took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_triangle_contains_its_inside_and_not_its_outside() {
        let (first, second, third) = ((0.0, 0.0), (0.0, 10.0), (10.0, 5.0));
        assert!(triangle_contains(first, second, third, (2.0, 5.0)));
        assert!(triangle_contains(first, second, third, (8.0, 5.0)));
        assert!(!triangle_contains(first, second, third, (-1.0, 5.0)));
        assert!(!triangle_contains(first, second, third, (9.0, 1.0)));
        assert!(!triangle_contains(first, second, third, (5.0, 11.0)));
    }

    /// Not a check: a way for a person to look at the symbol. Run with
    /// `FASTFORWARDRS_INDICATOR_PREVIEW_FOLDER=some_folder cargo test --lib write_indicator_previews -- --ignored`
    /// and open the PNG files it writes (they have a transparent background).
    #[test]
    #[ignore = "writes picture files for a person to look at"]
    fn write_indicator_previews() {
        let folder = std::env::var("FASTFORWARDRS_INDICATOR_PREVIEW_FOLDER")
            .expect("set FASTFORWARDRS_INDICATOR_PREVIEW_FOLDER");
        std::fs::create_dir_all(&folder).unwrap();
        for (factor, height) in [
            (2, 120),
            (4, 120),
            (16, 120),
            (100, 120),
            (1000, 120),
            (4, 40),
            (8, 400),
        ] {
            let picture = render_speed_indicator(speed(factor), height);
            std::fs::write(
                std::path::Path::new(&folder).join(format!("indicator_{factor}x_{height}px.png")),
                picture.to_png_bytes().unwrap(),
            )
            .unwrap();
        }
    }
}
