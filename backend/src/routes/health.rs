use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::app_state::AppState;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub analyzer: bool,
    pub analyzer_model: Option<String>,
}

pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let analyzer = state.analyzer.is_some();
    let analyzer_model = if analyzer {
        Some(state.config.minimax_model.clone())
    } else {
        None
    };
    Json(HealthResponse {
        status: "ok",
        analyzer,
        analyzer_model,
    })
}
