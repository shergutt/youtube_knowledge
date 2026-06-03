use axum::extract::State;
use axum::Json;
use serde::Deserialize;

use crate::app_state::AppState;
use crate::errors::{AppError, AppResult};
use crate::models::transcript::ProbeResponse;
use crate::services::youtube_url::extract_video_id;

#[derive(Debug, Deserialize)]
pub struct ProbeRequest {
    pub url: String,
}

pub async fn probe(
    State(state): State<AppState>,
    Json(req): Json<ProbeRequest>,
) -> AppResult<Json<ProbeResponse>> {
    if req.url.len() > state.config.max_url_length {
        return Err(AppError::InvalidUrl(format!(
            "URL exceeds maximum length of {} characters",
            state.config.max_url_length
        )));
    }
    let video_id = extract_video_id(&req.url, state.config.max_url_length)?;
    let resp = state.ytdlp.probe(&video_id).await?;
    Ok(Json(resp))
}
