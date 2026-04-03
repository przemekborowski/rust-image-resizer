use axum::{
    extract::{Path, Query, State},
    routing::{get},
    http::{ StatusCode},
    response::{IntoResponse, Response},
    Router
};
use serde::Deserialize;
use tokio::net::TcpListener;
use libvips::{ops};
use moka::future::Cache;

mod masks;

#[derive(Deserialize)]
struct ImageQuery {
    radius: Option<u32>,
    gradient: Option<u32>
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct ImageCacheKey {
    pub id: String,
    pub file: String,
    pub width: u32,
    pub radius: u32,
    pub gradient: u32,
}

#[derive(Clone)]
pub struct AppState {
    pub cache: Cache<ImageCacheKey, Vec<u8>>,
    // You could also put other shared things here later, like:
    // pub db_pool: sqlx::PgPool,
    // pub api_key: String,
}

pub enum AppError {
    ImageProcessing(String),
    TaskFailed,
    FileNotFound,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::ImageProcessing(msg) => {
                println!("Libvips Error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Image Processing Failed: {}", msg))
            }
            AppError::TaskFailed => {
                println!("Tokio Thread Crash!");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server thread failed".to_string())
            }
            AppError::FileNotFound=> {
                println!("File not found!");
                (StatusCode::INTERNAL_SERVER_ERROR, "File not found".to_string())
            }
        };
        (status, error_message).into_response()
    }
}

#[tokio::main]
async fn main() {

    let cache: Cache<ImageCacheKey, Vec<u8>> = Cache::builder()
        .max_capacity(500)
        .build();

    let state = AppState { cache };

    let app = Router::new()
        .route("/", get(health_check))
        .route("/image/{id}/{file}/{width}", get(get_image))
        .with_state(state);
    let listener = TcpListener::bind("0.0.0.0:3333").await.unwrap();
    println!("Server is running on http://localhost:3333");

    axum::serve(listener, app).await.unwrap();

}

async fn health_check() -> &'static str {
    "Image Resizer Service is online!"
}

async fn get_image(
    Path((id, file, width)): Path<(String, String, u32)>,
    Query(query): Query<ImageQuery>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    let radius = query.radius.unwrap_or(0).clamp(0, 50);
    let gradient = query.gradient.unwrap_or(0).clamp(0, 100);

    println!("Requested image {} {} with width {}, radius {}", id, file, width, radius);

    let cache_key = ImageCacheKey {
        id,
        file,
        width,
        radius,
        gradient,
    };

    if let Some(cached_bytes) = state.cache.get(&cache_key).await {
        println!("Hit cache for {:?}", cache_key);
        return Ok((
            [(axum::http::header::CONTENT_TYPE, "image/webp")],
            cached_bytes,
        ).into_response());
    }

    let file_path = "public/template.png";
    let bytes = match tokio::fs::read(file_path).await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            return Err(AppError::FileNotFound);
        }
    };

    let image_bytes = tokio::task::spawn_blocking(move || {
        let mut image = ops::thumbnail_buffer(&bytes, width as i32)
            .map_err(|_| "Failed to thumbnail image")?;

        let w = image.get_width();
        let h = image.get_height();

        if gradient > 0 {

            let grad_img = masks::create_svg_gradient(w, h, gradient as f64 / 100.0)
                .map_err(|_| "Failed to create SVG gradient")?;

            image = ops::composite_2(&image, &grad_img, libvips::ops::BlendMode::Over)
                .map_err(|_| "Failed to composite gradient")?;
        }

        if radius > 0 {
            let mask_alpha = masks::create_svg_mask(w, h, radius as f64)
                .map_err(|_| "Failed to calculate SVG mask")?;

            let srgb_image = ops::colourspace(&image, libvips::ops::Interpretation::Srgb)
                .unwrap_or(image);

            let r = ops::extract_band(&srgb_image, 0).map_err(|_| "Failed to extract R")?;
            let g = ops::extract_band(&srgb_image, 1).map_err(|_| "Failed to extract G")?;
            let b = ops::extract_band(&srgb_image, 2).map_err(|_| "Failed to extract B")?;

            let mut rgb_bands = vec![r, g, b];
            let clean_rgb = ops::bandjoin(&mut rgb_bands).map_err(|_| "Failed to join RGB")?;

            let mut final_bands = vec![clean_rgb, mask_alpha];
            image = ops::bandjoin(&mut final_bands).map_err(|_| "Failed to apply alpha mask")?;

        }
        let border_img = masks::create_svg_border(w, h, radius as f64)
            .map_err(|_| "Failed to create SVG border")?;

        image = ops::composite_2(&image, &border_img, libvips::ops::BlendMode::Over)
            .map_err(|_| "Failed to composite border")?;

        let webp_bytes = ops::webpsave_buffer(&image)
            .map_err(|_| "Failed to encode to WebP")?;

        Ok::<Vec<u8>, &'static str>(webp_bytes)
    })
    .await
    .map_err(|_| AppError::TaskFailed)?
    .map_err(|e| AppError::ImageProcessing(e.to_string()))?;

    state.cache.insert(cache_key, image_bytes.clone()).await;
    Ok((
        [(axum::http::header::CONTENT_TYPE, "image/webp")],
        image_bytes,
    ).into_response())
}

