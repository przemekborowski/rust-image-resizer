use axum::{
    extract::{Path, Query, State},
    routing::{get},
    http::{ StatusCode},
    response::{IntoResponse, Response},
    Router
};
use serde::Deserialize;
use tokio::net::TcpListener;
use moka::future::Cache;
use std::time::Instant;

mod masks;
mod processor;

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
    NetworkError(String),
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
            AppError::NetworkError(msg) => {
                println!("Network error: {}", msg);
                (StatusCode::BAD_GATEWAY, format!("Failed to fetch upstream image: {}", msg))
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
        id: id.clone(),
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

    let upstream_url = format!("https://picsum.photos/seed/{}/{}/{}", id.clone(), 1000, (1000.0 * 1.5) as u32);

    let download_start = Instant::now();
    let response = reqwest::get(&upstream_url)
        .await
        .map_err(|e| AppError::NetworkError(e.to_string()))?;


    if !response.status().is_success() {
        return Err(AppError::NetworkError("Image provider returned an error status".to_string()));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::NetworkError(e.to_string()))?;

    let download_time = download_start.elapsed();
    println!("Download took: {:?}", download_time);

    let process_start = Instant::now();
    let image_bytes = tokio::task::spawn_blocking(move || {
        processor::process_image(bytes, width, radius, gradient)
    })
    .await
    .map_err(|_| AppError::TaskFailed)?
    .map_err(|e| AppError::ImageProcessing(e.to_string()))?;
    let process_time = process_start.elapsed();
    println!("libvips Processing took: {:?}", process_time);

    state.cache.insert(cache_key, image_bytes.clone()).await;
    Ok((
        [(axum::http::header::CONTENT_TYPE, "image/webp")],
        image_bytes,
    ).into_response())
}

