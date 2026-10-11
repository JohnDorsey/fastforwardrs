//! A picture made by this program rather than by ffmpeg (the fast-forward symbol, text), as pixels.

use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};

/// A picture of RGBA pixels with straight (not premultiplied) alpha, row by row, top row first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaPicture {
    pub width_in_pixels: u32,
    pub height_in_pixels: u32,
    pub rgba_pixels: Vec<u8>,
}

impl RgbaPicture {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_becomes_a_png_that_reads_back_with_the_same_pixels() {
        let picture = RgbaPicture {
            width_in_pixels: 2,
            height_in_pixels: 2,
            rgba_pixels: vec![255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 10, 20, 30, 40],
        };
        let bytes = picture.to_png_bytes().unwrap();
        assert_eq!(
            &bytes[..8],
            &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
            "a PNG file starts with its signature"
        );
        let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .unwrap()
            .to_rgba8();
        assert_eq!((decoded.width(), decoded.height()), (2, 2));
        assert_eq!(decoded.into_raw(), picture.rgba_pixels);
    }
}
