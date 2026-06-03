use std::path::PathBuf;

use axum::body::Body;
use axum::extract::{Path as AxPath, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;
use tokio::fs;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::{AppError, AppResult};
use crate::models::job::JobStatus;
use crate::utils::time::is_expired;

pub async fn download(
    State(state): State<AppState>,
    AxPath(job_id): AxPath<Uuid>,
) -> AppResult<Response> {
    let (job_dir_rel, output_filename, output_format, status, expires_at) = {
        let jobs = state.jobs.read().await;
        let job = jobs.get(&job_id).ok_or(AppError::JobNotFound)?;
        (
            job.job_dir.clone(),
            job.output_filename.clone(),
            job.output_format,
            job.status,
            job.expires_at,
        )
    };

    if is_expired(expires_at) {
        return Err(AppError::FileExpired);
    }
    if status != JobStatus::Completed {
        return Err(AppError::FileExpired);
    }

    let filename = output_filename.ok_or(AppError::FileExpired)?;
    let safe_filename = sanitize_filename(&filename);

    let job_dir = PathBuf::from(&job_dir_rel);
    let file_path = job_dir.join(&safe_filename);

    let storage_dir = state
        .config
        .storage_dir
        .canonicalize()
        .map_err(|e| AppError::Internal(format!("canonicalize storage: {}", e)))?;
    let resolved = file_path
        .canonicalize()
        .map_err(|_| AppError::FileExpired)?;
    if !resolved.starts_with(&storage_dir) {
        return Err(AppError::FileExpired);
    }
    if !resolved.is_file() {
        return Err(AppError::FileExpired);
    }

    let bytes = fs::read(&resolved)
        .await
        .map_err(|_| AppError::FileExpired)?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(output_format.mime_type())
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    let disposition = format!(
        "attachment; filename=\"{}\"",
        safe_filename.replace('"', "_")
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition).unwrap_or(HeaderValue::from_static("attachment")),
    );
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from(bytes.len() as u64),
    );

    let response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(bytes))
        .expect("response builder");
    let mut response = response;
    response.headers_mut().extend(headers);
    Ok(response)
}

/// Strip path separators and other risky characters from a stored filename
/// before using it on disk. The filename is also constrained to a known set
/// of transcript extensions to avoid serving arbitrary files.
fn sanitize_filename(name: &str) -> String {
    let allowed_ext: &[&str] = &["vtt", "srt", "txt", "json"];
    let stem = name
        .chars()
        .filter(|c| !c.is_control() && *c != '/' && *c != '\\' && *c != '\0')
        .take(200)
        .collect::<String>();
    let ext = PathBuf::from(&stem)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if !allowed_ext.contains(&ext.as_str()) {
        return format!("{stem}.txt");
    }
    stem
}
