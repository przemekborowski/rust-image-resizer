use axum::{
    extract::{Path, Query},
    routing::{get},
    http::{header, StatusCode},
    response::IntoResponse,
    Router
};
use serde::Deserialize;
use tokio::net::TcpListener;

#[derive(Deserialize)]
struct ImagePath {
    id: String,
    file: String,
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
struct ImageQuery {
    radius: Option<u32>,
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(health_check))
        .route("/image/{id}/{file}/{width}/{height}", get(get_image));
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
    let radius = match query.radius {
        Some(r) => format!("and a border radius of {}", r),
        None => "with no border radius".to_string(),
    };

    let file_path = "public/template.png";

    println!("Requested image {} {} with dimensions {}/{}, radius {}", path.id, path.file, path.width, path.height, radius);

    match tokio::fs::read(file_path).await {
        Ok(bytes) => {
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "image/png")],
                bytes,
            ).into_response()
        }
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            (
                StatusCode::NOT_FOUND,
                "Image not found on disk.",
            ).into_response()
        }
    }
}

