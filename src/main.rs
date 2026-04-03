use axum::{
    extract::{Path, Query},
    routing::{get},
    http::{header, StatusCode},
    response::IntoResponse,
    Router
};
use serde::Deserialize;
use tokio::net::TcpListener;
use libvips::{ops };
mod masks;

#[derive(Deserialize)]
struct ImagePath {
    id: String,
    file: String,
    width: u32
}

#[derive(Deserialize)]
struct ImageQuery {
    radius: Option<u32>,
    gradient: Option<u32>
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(health_check))
        .route("/image/{id}/{file}/{width}", get(get_image));
    let listener = TcpListener::bind("0.0.0.0:3333").await.unwrap();
    println!("Server is running on http://localhost:3333");

    axum::serve(listener, app).await.unwrap();

}

async fn health_check() -> &'static str {
    "Image Resizer Service is online!"
}

async fn get_image(
    Path(path): Path<ImagePath>,
    Query(query): Query<ImageQuery>,
) -> impl IntoResponse {
    let radius = query.radius.unwrap_or(0);

    let file_path = "public/template.png";

    println!("Requested image {} {} with width {}, radius {}", path.id, path.file, path.width, radius);

    let bytes = match tokio::fs::read(file_path).await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            return (StatusCode::NOT_FOUND, "Image not found on disk.").into_response();
        }
    };
    let width = path.width;

    let resize_task = tokio::task::spawn_blocking(move || {
        let mut image = ops::thumbnail_buffer(&bytes, width as i32)
            .map_err(|_| "Failed to thumbnail image")?;

        let w = image.get_width();
        let h = image.get_height();

        let gradient_value = query.gradient.unwrap_or(0).clamp(0, 100);
        if gradient_value > 0 {

            let grad_img = masks::create_svg_gradient(w, h, gradient_value as f64 / 100.0)
                .map_err(|_| "Failed to create SVG gradient")?;

            image = ops::composite_2(&image, &grad_img, libvips::ops::BlendMode::Over)
                .map_err(|_| "Failed to composite gradient")?;
        }

        if radius > 0 {
            let mask_alpha = masks::create_svg_mask(w, h, radius as f64)
                .map_err(|_| "Failed to calculate SVG mask")?;

            // Force image into sRGB to prevent CMYK color inversion issues
            let srgb_image = ops::colourspace(&image, libvips::ops::Interpretation::Srgb)
                .unwrap_or(image);

            // Strip any existing alpha channel by explicitly extracting R, G, B
            let r = ops::extract_band(&srgb_image, 0).map_err(|_| "Failed to extract R")?;
            let g = ops::extract_band(&srgb_image, 1).map_err(|_| "Failed to extract G")?;
            let b = ops::extract_band(&srgb_image, 2).map_err(|_| "Failed to extract B")?;

            // Join clean RGB bands together
            let mut rgb_bands = vec![r, g, b];
            let clean_rgb = ops::bandjoin(&mut rgb_bands).map_err(|_| "Failed to join RGB")?;

            // Join clean RGB with our perfect SVG alpha mask
            let mut final_bands = vec![clean_rgb, mask_alpha];
            image = ops::bandjoin(&mut final_bands).map_err(|_| "Failed to apply alpha mask")?;

        }
        let border_img = masks::create_svg_border(w, h, radius as f64)
            .map_err(|_| "Failed to create SVG border")?;

        // Base image is first, overlay (border) is second
        image = ops::composite_2(&image, &border_img, libvips::ops::BlendMode::Over)
            .map_err(|_| "Failed to composite border")?;

        let webp_bytes = ops::webpsave_buffer(&image)
            .map_err(|_| "Failed to encode to WebP")?;

        Ok::<Vec<u8>, &'static str>(webp_bytes)
    });

    match resize_task.await.unwrap() {
        Ok(resized_bytes) => {
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "image/webp")],
                resized_bytes,
            ).into_response()
        }
        Err(err_msg) => {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                err_msg,
            ).into_response()
        }
    }
}

