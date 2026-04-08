use bytes::Bytes;
use libvips::{ops};

use crate::masks;

pub fn process_image(
    bytes: Bytes,
    width: u32,
    radius: u32,
    gradient: u32,
    blur: u32,
) -> Result<Vec<u8>, String> {
        let mut image = ops::thumbnail_buffer(&bytes, width as i32)
            .map_err(|_| "Failed to thumbnail image")?;

        let w = image.get_width();
        let h = image.get_height();

        if blur > 0 {
            let sigma = (blur as f64) / 10.0;
            image = ops::gaussblur(&image, sigma)
                .map_err(|_| "Failed to apply Gaussian blur")?;
        }

        if gradient > 0 {

            let grad_img = masks::create_svg_gradient(w, h, gradient as f64 / 100.0)
                .map_err(|_| "Failed to create SVG gradient")?;

            image = ops::composite_2(&image, &grad_img, libvips::ops::BlendMode::Over)
                .map_err(|_| "Failed to composite gradient")?;
        }

        if radius > 0 {
            let mask_alpha = masks::create_svg_mask(w, h, radius as f64)
                .map_err(|_| "Failed to calculate SVG mask")?;
            let srgb_image = ops::colourspace(&image, libvips::ops::Interpretation::Srgb).unwrap_or(image);

            let clean_rgb = if srgb_image.get_bands() > 3 {
                ops::flatten(&srgb_image).unwrap_or(srgb_image)
            } else {
                srgb_image
            };

            let mut final_bands = vec![clean_rgb, mask_alpha];
            image = ops::bandjoin(&mut final_bands).map_err(|_| "Failed to apply alpha mask")?;

        }
        let border_img = masks::create_svg_border(w, h, radius as f64)
            .map_err(|_| "Failed to create SVG border")?;

        image = ops::composite_2(&image, &border_img, libvips::ops::BlendMode::Over)
            .map_err(|_| "Failed to composite border")?;

        let webp_bytes = ops::webpsave_buffer(&image)
            .map_err(|_| "Failed to encode to WebP")?;

        Ok(webp_bytes)
}
