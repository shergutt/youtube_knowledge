use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::{AppError, AppResult};
use crate::models::api::{CaptionSource, OutputFormat};
use crate::models::job::{JobError, JobStatus, TranscriptJob};
use crate::services::transcript::{SubtitleSourceResult, TranscriptService};
use crate::services::youtube_url::extract_video_id;
use crate::utils::time::is_expired;

#[derive(Debug, Deserialize)]
pub struct CreateTranscriptRequest {
    pub url: String,
    pub language: String,
    pub caption_source: CaptionSource,
    pub output_format: OutputFormat,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateTranscriptResponse {
    pub job_id: Uuid,
    pub status: JobStatus,
}

pub async fn create_transcript(
    State(state): State<AppState>,
    Json(req): Json<CreateTranscriptRequest>,
) -> AppResult<Json<CreateTranscriptResponse>> {
    if req.url.len() > state.config.max_url_length {
        return Err(AppError::InvalidUrl(format!(
            "URL exceeds maximum length of {} characters",
            state.config.max_url_length
        )));
    }
    let video_id = extract_video_id(&req.url, state.config.max_url_length)?;

    let language = req.language.trim().to_string();
    if language.is_empty() || language.len() > 32 {
        return Err(AppError::InvalidUrl(
            "language must be 1-32 characters".to_string(),
        ));
    }
    if !language
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::InvalidUrl(
            "language contains invalid characters".to_string(),
        ));
    }

    // Title is optional. We trim length and strip control characters; the
    // slugify step converts it into a filename-safe form later.
    let title = req
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.chars()
                .filter(|c| !c.is_control())
                .take(200)
                .collect::<String>()
        })
        .unwrap_or_default();
    // Default title to the video id if the client did not provide one
    let title = if title.is_empty() {
        video_id.clone()
    } else {
        title
    };

    let job_dir_rel = format!("jobs/{}", Uuid::new_v4());
    let job_dir_abs: PathBuf = state.config.storage_dir.join(&job_dir_rel);
    let job_dir_abs_str = job_dir_abs.to_string_lossy().to_string();

    let job = TranscriptJob::new(
        video_id,
        title,
        language,
        req.caption_source,
        req.output_format,
        job_dir_abs_str,
        state.config.file_ttl_minutes,
    );
    let job_id = job.id;
    {
        let mut jobs = state.jobs.write().await;
        jobs.insert(job_id, job);
    }

    spawn_job(state.clone(), job_id);

    Ok(Json(CreateTranscriptResponse {
        job_id,
        status: JobStatus::Queued,
    }))
}

fn spawn_job(state: AppState, job_id: Uuid) {
    tokio::spawn(async move {
        let _permit = match state.semaphore.acquire().await {
            Ok(p) => p,
            Err(_) => {
                mark_failed(&state, job_id, "JOB_NOT_FOUND", "semaphore closed").await;
                return;
            }
        };

        // Read job snapshot
        let (video_id, title, language, caption_source, output_format, job_dir) = {
            let jobs = state.jobs.read().await;
            match jobs.get(&job_id) {
                Some(j) => (
                    j.video_id.clone(),
                    j.title.clone(),
                    j.language.clone(),
                    j.caption_source,
                    j.output_format,
                    PathBuf::from(&j.job_dir),
                ),
                None => return,
            }
        };

        {
            let mut jobs = state.jobs.write().await;
            if let Some(j) = jobs.get_mut(&job_id) {
                j.status = JobStatus::Running;
                j.progress = 5;
                j.updated_at = Utc::now();
            }
        }

        let service = TranscriptService::new(state.ytdlp.clone());
        let result = service
            .generate(
                &video_id,
                &title,
                &language,
                caption_source,
                output_format,
                &job_dir,
            )
            .await;

        {
            let mut jobs = state.jobs.write().await;
            let Some(job) = jobs.get_mut(&job_id) else {
                return;
            };
            job.updated_at = Utc::now();
            match result {
                Ok((path, src)) => {
                    let src_label = match src {
                        SubtitleSourceResult::Manual => "manual",
                        SubtitleSourceResult::Auto => "auto",
                    };
                    let filename = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    let max_bytes = state.config.max_transcript_file_mb * 1024 * 1024;
                    if size > max_bytes {
                        job.status = JobStatus::Failed;
                        job.error = Some(JobError {
                            code: "INTERNAL_ERROR".to_string(),
                            message: format!(
                                "transcript exceeds maximum size of {} MB",
                                state.config.max_transcript_file_mb
                            ),
                        });
                        return;
                    }
                    job.output_filename = Some(filename);
                    job.progress = 100;
                    job.status = JobStatus::Completed;
                    info!(
                        job_id = %job_id,
                        video_id = %video_id,
                        source = src_label,
                        path = %path.display(),
                        "transcript job completed"
                    );
                }
                Err(e) => {
                    error!(job_id = %job_id, error = %e, "transcript job failed");
                    job.status = JobStatus::Failed;
                    job.error = Some(JobError {
                        code: e.code().to_string(),
                        message: e.message(),
                    });
                }
            }
        }
    });
}

async fn mark_failed(state: &AppState, job_id: Uuid, code: &str, message: &str) {
    let mut jobs = state.jobs.write().await;
    if let Some(job) = jobs.get_mut(&job_id) {
        job.status = JobStatus::Failed;
        job.error = Some(JobError {
            code: code.to_string(),
            message: message.to_string(),
        });
        job.updated_at = Utc::now();
    }
}

#[derive(Debug, Serialize)]
pub struct JobResponse {
    pub job_id: Uuid,
    pub video_id: String,
    pub title: String,
    pub status: JobStatus,
    pub progress: u8,
    pub output_filename: Option<String>,
    pub download_url: Option<String>,
    pub error: Option<JobError>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn get_job(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> AppResult<Json<JobResponse>> {
    let jobs = state.jobs.read().await;
    let job = jobs.get(&job_id).ok_or(AppError::JobNotFound)?.clone();
    drop(jobs);

    if is_expired(job.expires_at) {
        return Err(AppError::FileExpired);
    }

    let download_url = if job.status == JobStatus::Completed {
        Some(format!("/api/downloads/{}", job.id))
    } else {
        None
    };

    Ok(Json(JobResponse {
        job_id: job.id,
        video_id: job.video_id,
        title: job.title,
        status: job.status,
        progress: job.progress,
        output_filename: job.output_filename,
        download_url,
        error: job.error,
        expires_at: job.expires_at,
    }))
}
