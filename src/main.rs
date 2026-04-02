use axum::{
    extract::{Path, Query},
    routing::{get},
    http::{header, StatusCode},
    response::IntoResponse,
    Router
};
use serde::Deserialize;
use tokio::net::TcpListener;
use libvips::{ops, VipsImage };

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

            let grad_img = create_svg_gradient(w, h, gradient_value as f64 / 100.0)
                .map_err(|_| "Failed to create SVG gradient")?;

            image = ops::composite_2(&image, &grad_img, libvips::ops::BlendMode::Over)
                .map_err(|_| "Failed to composite gradient")?;
        }

        if radius > 0 {
            let mask_alpha = create_svg_mask(w, h, radius as f64)
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
        let border_img = create_svg_border(w, h, radius as f64)
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

fn create_svg_mask(width: i32, height: i32, radius: f64) -> Result<VipsImage, libvips::error::Error> {
    // Generate a perfectly anti-aliased vector shape
    let svg = format!(
        "<svg viewBox=\"0 0 {w} {h}\">
            <rect rx=\"{r}\" ry=\"{r}\" x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\" fill=\"#fff\"/>
        </svg>",
        w = width,
        h = height,
        r = radius
    );

    // Load the SVG vector into libvips
    let mask = VipsImage::new_from_buffer(svg.as_bytes(), "")?;

    // Because the SVG is white on a transparent background, its Alpha channel (Band 3)
    // is exactly the flawless rounded mask we need. Extract it:
    let mask_alpha = ops::extract_band(&mask, 3)?;

    Ok(mask_alpha)
}

fn create_svg_border(
    width: i32,
    height: i32,
    radius: f64,
) -> Result<VipsImage, libvips::error::Error> {
    let stroke_width = 2.0;
    let half_sw = 1.0;

    let w = width as f64 - stroke_width;
    let h = height as f64 - stroke_width;

    let mut r = radius - half_sw;
    if r < 0.0 { r = 0.0; }

    // Fix: We use standard 6-digit hex and control the 50% alpha using stroke-opacity
    let svg = format!(
        "<svg viewBox=\"0 0 {width} {height}\">
            <rect rx=\"{r}\" ry=\"{r}\"
                  x=\"{half_sw}\" y=\"{half_sw}\"
                  width=\"{w}\" height=\"{h}\"
                  fill=\"none\"
                  stroke=\"rgb(242, 243, 243)\" stroke-opacity=\"0.3\" stroke-width=\"{stroke_width}\"/>
        </svg>",
        width = width,
        height = height,
        r = r,
        half_sw = half_sw,
        w = w,
        h = h,
        stroke_width = stroke_width
    );

    VipsImage::new_from_buffer(svg.as_bytes(), "")
}


fn create_svg_gradient(width: i32, height: i32, gradient: f64) -> Result<VipsImage, libvips::error::Error> {
    let grad_height = height as f64 * gradient;
    let start_y = height as f64 - grad_height;

    let svg = format!(
        "<svg viewBox=\"0 0 {width} {height}\">
            <defs>
                <linearGradient id=\"bottom_grad\" x1=\"0%\" y1=\"0%\" x2=\"0%\" y2=\"100%\">
                    <stop offset=\"0%\" stop-color=\"rgb(10, 21, 31)\" stop-opacity=\"0\" />
                    <stop offset=\"100%\" stop-color=\"rgb(10, 21, 31)\" stop-opacity=\"0.95\" />
                </linearGradient>
            </defs>
            <rect x=\"0\" y=\"{start_y}\" width=\"{width}\" height=\"{grad_height}\" fill=\"url(#bottom_grad)\" />
        </svg>",
        width = width,
        height = height,
        start_y = start_y,
        grad_height = grad_height
    );

    VipsImage::new_from_buffer(svg.as_bytes(), "")
}
