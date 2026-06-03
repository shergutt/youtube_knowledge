pub mod analyses;
pub mod downloads;
pub mod health;
pub mod jobs;
pub mod playlists;
pub mod probe;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use crate::app_state::AppState;
use std::time::Duration;

pub fn build_router(state: AppState, max_body_bytes: usize) -> Router {
    let api = Router::new()
        .route("/api/probe", post(probe::probe))
        .route("/api/transcripts", post(jobs::create_transcript))
        .route("/api/jobs/{job_id}", get(jobs::get_job))
        .route("/api/downloads/{job_id}", get(downloads::download))
        .route("/api/analyze", post(analyses::create_analysis))
        .route("/api/analyses/{id}", get(analyses::get_analysis))
        .route(
            "/api/analyses/{id}/download",
            get(analyses::download_analysis),
        )
        .route("/api/playlists/probe", post(playlists::probe))
        .route("/api/playlists", post(playlists::create_playlist))
        .route("/api/playlists/{id}", get(playlists::get_playlist))
        .route("/api/playlists/{id}/download", get(playlists::download_zip));

    Router::new()
        .route("/health", get(health::health))
        .merge(api)
        .layer(DefaultBodyLimit::max(max_body_bytes))
        .layer(RequestBodyLimitLayer::new(max_body_bytes))
        .layer(TraceLayer::new_for_http())
        .layer(cors_for(&state.config.cors_origin))
        .with_state(state)
}

fn cors_for(origin: &str) -> CorsLayer {
    use axum::http::{header, HeaderValue, Method};
    let allow_origin: HeaderValue = origin
        .parse()
        .unwrap_or_else(|_| HeaderValue::from_static("*"));
    CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::ACCEPT])
        .max_age(Duration::from_secs(3600))
}
