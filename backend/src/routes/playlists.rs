use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{Path as AxPath, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use tracing::{info, warn};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::{AppError, AppResult};
use crate::models::analysis::{AnalysisJob, AnalysisPurpose, AnalysisStatus};
use crate::models::api::{CaptionSource, OutputFormat};
use crate::models::job::{JobError, JobStatus, TranscriptJob};
use crate::models::playlist::{
    AnalysisSpec, ChildAnalysis, ChildStage, PlaylistChild, PlaylistJob, PlaylistStatus,
};
use crate::services::youtube_url::extract_playlist_id;
use crate::utils::time::is_expired;

use super::analyses::MAX_ANALYSIS_SPECS;

#[derive(Debug, Deserialize)]
pub struct ProbeRequest {
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct ProbeResponse {
    pub playlist_id: String,
    pub title: String,
    pub video_count: usize,
    pub videos: Vec<ProbeVideo>,
}

#[derive(Debug, Serialize)]
pub struct ProbeVideo {
    pub video_id: String,
    pub title: String,
    pub duration_seconds: Option<u64>,
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
    let playlist_id = extract_playlist_id(&req.url, state.config.max_url_length)?;
    let raw = state.ytdlp.list_playlist(&playlist_id).await?;
    let max = state.config.max_playlist_videos;
    let videos: Vec<ProbeVideo> = raw
        .entries
        .into_iter()
        .filter(|e| crate::services::youtube_url::is_valid_video_id_for_playlist(&e.id))
        .take(max)
        .map(|e| ProbeVideo {
            video_id: e.id,
            title: e.title.unwrap_or_else(|| "(untitled)".to_string()),
            duration_seconds: e.duration.map(|d| d as u64),
        })
        .collect();
    let count = videos.len();
    Ok(Json(ProbeResponse {
        playlist_id: raw.id,
        title: raw.title.unwrap_or_else(|| "Playlist".to_string()),
        video_count: count,
        videos,
    }))
}

#[derive(Debug, Deserialize)]
pub struct CreatePlaylistRequest {
    pub url: String,
    pub language: String,
    pub caption_source: CaptionSource,
    pub output_format: OutputFormat,
    /// One or more analysis specs to run on every video of the playlist.
    /// `None` means "transcripts only, no analysis".
    #[serde(default)]
    pub analysis: Option<Vec<AnalysisSpec>>,
}

#[derive(Debug, Serialize)]
pub struct CreatePlaylistResponse {
    pub playlist_id: Uuid,
    pub status: PlaylistStatus,
}

pub async fn create_playlist(
    State(state): State<AppState>,
    Json(req): Json<CreatePlaylistRequest>,
) -> AppResult<Json<CreatePlaylistResponse>> {
    if req.url.len() > state.config.max_url_length {
        return Err(AppError::InvalidUrl(format!(
            "URL exceeds maximum length of {} characters",
            state.config.max_url_length
        )));
    }
    let playlist_id = extract_playlist_id(&req.url, state.config.max_url_length)?;

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
    if let Some(specs) = &req.analysis {
        if specs.is_empty() {
            return Err(AppError::InvalidAnalysisRequest(
                "analysis must be a non-empty list of specs".to_string(),
            ));
        }
        if specs.len() > MAX_ANALYSIS_SPECS {
            return Err(AppError::InvalidAnalysisRequest(format!(
                "at most {} analysis specs per playlist",
                MAX_ANALYSIS_SPECS
            )));
        }
        for (i, spec) in specs.iter().enumerate() {
            if matches!(spec.purpose, AnalysisPurpose::Custom) {
                let prompt = spec.custom_prompt.as_deref().unwrap_or("").trim();
                if prompt.is_empty() {
                    return Err(AppError::InvalidAnalysisRequest(format!(
                        "custom_prompt is required for spec #{} (purpose=custom)",
                        i + 1
                    )));
                }
                if prompt.len() > 2000 {
                    return Err(AppError::InvalidAnalysisRequest(format!(
                        "custom_prompt for spec #{} must be <= 2000 characters",
                        i + 1
                    )));
                }
            }
            if spec.output_language.len() > 16
                || !spec
                    .output_language
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err(AppError::InvalidAnalysisRequest(format!(
                    "output_language for spec #{} must be alphanumeric",
                    i + 1
                )));
            }
        }
    }
    if state.analyzer.is_none() && req.analysis.is_some() {
        return Err(AppError::AnalysisNotConfigured);
    }

    let job_dir_rel = format!("playlists/{}", Uuid::new_v4());
    let job_dir_abs: PathBuf = state.config.storage_dir.join(&job_dir_rel);
    let job = PlaylistJob::new(
        req.url,
        playlist_id,
        language,
        req.caption_source,
        req.output_format,
        req.analysis,
        job_dir_abs.to_string_lossy().to_string(),
        state.config.file_ttl_minutes,
    );
    let id = job.id;
    {
        let mut map = state.playlists.write().await;
        map.insert(id, job);
    }
    spawn_playlist(state.clone(), id);
    Ok(Json(CreatePlaylistResponse {
        playlist_id: id,
        status: PlaylistStatus::Queued,
    }))
}

fn spawn_playlist(state: AppState, playlist_id: Uuid) {
    tokio::spawn(async move {
        let permit = match state.playlist_semaphore.acquire().await {
            Ok(p) => p,
            Err(_) => return,
        };

        let settings = {
            let map = state.playlists.read().await;
            map.get(&playlist_id).map(|p| {
                (
                    p.source_url.clone(),
                    p.playlist_id.clone(),
                    p.language.clone(),
                    p.caption_source,
                    p.output_format,
                    p.analysis.clone(),
                )
            })
        };
        let Some((_, pl_id, language, caption_source, output_format, analysis)) = settings else {
            return;
        };

        // Step 1: list videos
        {
            let mut map = state.playlists.write().await;
            if let Some(p) = map.get_mut(&playlist_id) {
                p.status = PlaylistStatus::Running;
                p.updated_at = Utc::now();
            }
        }
        let raw = match state.ytdlp.list_playlist(&pl_id).await {
            Ok(r) => r,
            Err(e) => {
                mark_playlist_failed(&state, playlist_id, e.code(), &e.message()).await;
                return;
            }
        };
        let max = state.config.max_playlist_videos;
        let entries: Vec<(String, String)> = raw
            .entries
            .into_iter()
            .filter(|e| crate::services::youtube_url::is_valid_video_id_for_playlist(&e.id))
            .take(max)
            .map(|e| {
                let title = e.title.unwrap_or_else(|| "(untitled)".to_string());
                (e.id, title)
            })
            .collect();
        let total = entries.len();

        {
            let mut map = state.playlists.write().await;
            if let Some(p) = map.get_mut(&playlist_id) {
                p.playlist_title = raw.title.unwrap_or_else(|| "Playlist".to_string());
                p.total = total;
                p.children = entries
                    .iter()
                    .map(|(vid, title)| PlaylistChild {
                        video_id: vid.clone(),
                        title: title.clone(),
                        stage: ChildStage::Pending,
                        transcript_job_id: None,
                        transcript_filename: None,
                        transcript_download_url: None,
                        analyses: Vec::new(),
                        error: None,
                    })
                    .collect();
                p.updated_at = Utc::now();
            }
        }

        if total == 0 {
            // Nothing to do — mark complete
            let mut map = state.playlists.write().await;
            if let Some(p) = map.get_mut(&playlist_id) {
                p.status = PlaylistStatus::Completed;
                p.updated_at = Utc::now();
            }
            drop(permit);
            return;
        }

        info!(
            playlist_id = %playlist_id,
            total,
            "playlist worker pool starting"
        );

        // Step 2: process videos with a small worker pool.
        let worker_count = state.config.max_playlist_concurrent_videos.max(1);
        let semaphore = Arc::new(Semaphore::new(worker_count));
        let mut handles = Vec::with_capacity(total);
        for index in 0..total {
            let permit = match semaphore.clone().acquire_owned().await {
                Ok(p) => p,
                Err(_) => break,
            };
            let state_c = state.clone();
            let language_c = language.clone();
            let analysis_c = analysis.clone();
            let handle = tokio::spawn(async move {
                let _p = permit;
                process_one(
                    &state_c,
                    playlist_id,
                    index,
                    &language_c,
                    caption_source,
                    output_format,
                    analysis_c.as_ref(),
                )
                .await;
            });
            handles.push(handle);
        }
        for h in handles {
            let _ = h.await;
        }

        // Step 3: finalize
        let mut map = state.playlists.write().await;
        if let Some(p) = map.get_mut(&playlist_id) {
            p.status = PlaylistStatus::Completed;
            p.updated_at = Utc::now();
        }
        info!(playlist_id = %playlist_id, "playlist complete");
        drop(permit);
    });
}

#[allow(clippy::too_many_arguments)]
async fn process_one(
    state: &AppState,
    playlist_id: Uuid,
    child_index: usize,
    language: &str,
    caption_source: CaptionSource,
    output_format: OutputFormat,
    analysis: Option<&Vec<AnalysisSpec>>,
) {
    let (video_id, title) = {
        let map = state.playlists.read().await;
        let Some(p) = map.get(&playlist_id) else {
            return;
        };
        match p.children.get(child_index) {
            Some(c) => (c.video_id.clone(), c.title.clone()),
            None => return,
        }
    };

    // Mark as running
    {
        let mut map = state.playlists.write().await;
        if let Some(p) = map.get_mut(&playlist_id) {
            if let Some(c) = p.children.get_mut(child_index) {
                c.stage = ChildStage::Running;
            }
            p.updated_at = Utc::now();
        }
    }

    // 1. Create a transcript job and wait for it.
    let transcript_job_id = match create_child_transcript(
        state,
        &video_id,
        &title,
        language,
        caption_source,
        output_format,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            mark_child_failed(state, playlist_id, child_index, e.code(), &e.message()).await;
            return;
        }
    };

    // Wait for it
    let transcript_final = match wait_for_transcript(state, transcript_job_id).await {
        Ok(j) => j,
        Err(e) => {
            mark_child_failed(state, playlist_id, child_index, e.code(), &e.message()).await;
            return;
        }
    };
    if transcript_final.status != JobStatus::Completed {
        let code = transcript_final
            .error
            .as_ref()
            .map(|e| e.code.clone())
            .unwrap_or_else(|| AppError::Internal("unknown".into()).code().to_string());
        let message = transcript_final
            .error
            .as_ref()
            .map(|e| e.message.clone())
            .unwrap_or_else(|| "transcript job failed".to_string());
        mark_child_failed(state, playlist_id, child_index, &code, &message).await;
        return;
    }

    // Update child with transcript info
    let transcript_filename = transcript_final.output_filename.clone();
    let transcript_download_url = Some(format!("/api/downloads/{}", transcript_job_id));
    {
        let mut map = state.playlists.write().await;
        if let Some(p) = map.get_mut(&playlist_id) {
            if let Some(c) = p.children.get_mut(child_index) {
                c.transcript_job_id = Some(transcript_job_id);
                c.transcript_filename = transcript_filename.clone();
                c.transcript_download_url = transcript_download_url.clone();
            }
        }
    }

    // 2. Optionally create analyses (one per spec, in parallel).
    if let Some(specs) = analysis {
        // Create all analysis jobs up front so we have ids to poll.
        let mut per_spec: Vec<(AnalysisSpec, Uuid)> = Vec::with_capacity(specs.len());
        for spec in specs {
            match create_child_analysis(
                state,
                transcript_job_id,
                &video_id,
                &title,
                language,
                spec,
            )
            .await
            {
                Ok(id) => per_spec.push((spec.clone(), id)),
                Err(e) => {
                    warn!(error = %e, video_id = %video_id, "analysis create failed");
                    // Don't abort the whole child; record the failure and
                    // keep going for the remaining specs.
                    per_spec.push((
                        spec.clone(),
                        Uuid::nil(), // sentinel: a nil id marks a creation failure
                    ));
                    let _ = e; // suppress unused warning if the warn! is stripped
                }
            }
        }

        // Wait for each analysis in parallel. `wait_for_analysis` returns
        // the final job record; we only abort the whole child on internal
        // errors (e.g. timeout), per-spec failures are captured on the
        // child record.
        let mut joinset = tokio::task::JoinSet::new();
        for (_, id) in &per_spec {
            if id.is_nil() {
                continue;
            }
            let state_c = state.clone();
            let id_c = *id;
            joinset.spawn(async move { (id_c, wait_for_analysis(&state_c, id_c).await) });
        }
        let mut results: std::collections::HashMap<Uuid, Result<AnalysisJob, AppError>> =
            std::collections::HashMap::new();
        while let Some(res) = joinset.join_next().await {
            if let Ok((id, r)) = res {
                results.insert(id, r);
            }
        }

        let mut child_analyses: Vec<ChildAnalysis> = Vec::with_capacity(per_spec.len());
        let mut hard_fail: Option<(String, String)> = None;
        for (spec, id) in per_spec {
            if id.is_nil() {
                child_analyses.push(ChildAnalysis {
                    analysis_id: Uuid::nil(),
                    purpose: spec.purpose,
                    output_language: spec.output_language.clone(),
                    filename: None,
                    download_url: None,
                    status: "failed".to_string(),
                    error: Some(JobError {
                        code: "INTERNAL_ERROR".to_string(),
                        message: "analysis could not be queued".to_string(),
                    }),
                });
                continue;
            }
            match results.remove(&id) {
                Some(Ok(job)) => {
                    let (status, filename, download_url, error) = match job.status {
                        AnalysisStatus::Completed => (
                            "completed".to_string(),
                            job.output_filename.clone(),
                            Some(format!("/api/analyses/{}/download", id)),
                            None,
                        ),
                        AnalysisStatus::Failed => {
                            let err = job.error.clone().unwrap_or(JobError {
                                code: "INTERNAL_ERROR".to_string(),
                                message: "analysis failed".to_string(),
                            });
                            ("failed".to_string(), None, None, Some(err))
                        }
                        other => (
                            format!("{:?}", other).to_lowercase(),
                            None,
                            None,
                            job.error.clone(),
                        ),
                    };
                    child_analyses.push(ChildAnalysis {
                        analysis_id: id,
                        purpose: spec.purpose,
                        output_language: spec.output_language,
                        filename,
                        download_url,
                        status,
                        error,
                    });
                }
                Some(Err(e)) => {
                    warn!(error = %e, video_id = %video_id, analysis_id = %id, "analysis wait failed");
                    // A wait-time error (timeout, internal) is hard — the
                    // transcript is still on disk and other analyses may
                    // have succeeded, so we record the per-spec error and
                    // keep going. We only short-circuit on truly fatal
                    // errors.
                    if matches!(e, AppError::JobNotFound | AppError::MiniMaxTimeout) {
                        hard_fail = Some((e.code().to_string(), e.message()));
                    }
                    child_analyses.push(ChildAnalysis {
                        analysis_id: id,
                        purpose: spec.purpose,
                        output_language: spec.output_language,
                        filename: None,
                        download_url: None,
                        status: "failed".to_string(),
                        error: Some(JobError {
                            code: e.code().to_string(),
                            message: e.message(),
                        }),
                    });
                }
                None => {
                    child_analyses.push(ChildAnalysis {
                        analysis_id: id,
                        purpose: spec.purpose,
                        output_language: spec.output_language,
                        filename: None,
                        download_url: None,
                        status: "failed".to_string(),
                        error: Some(JobError {
                            code: "INTERNAL_ERROR".to_string(),
                            message: "analysis did not return".to_string(),
                        }),
                    });
                }
            }
        }

        {
            let mut map = state.playlists.write().await;
            if let Some(p) = map.get_mut(&playlist_id) {
                if let Some(c) = p.children.get_mut(child_index) {
                    c.analyses = child_analyses;
                }
            }
        }

        if let Some((code, message)) = hard_fail {
            mark_child_failed(state, playlist_id, child_index, &code, &message).await;
            return;
        }
    }

    // Mark child complete
    {
        let mut map = state.playlists.write().await;
        if let Some(p) = map.get_mut(&playlist_id) {
            if let Some(c) = p.children.get_mut(child_index) {
                c.stage = ChildStage::Completed;
            }
            p.completed += 1;
            p.updated_at = Utc::now();
        }
    }
    info!(
        playlist_id = %playlist_id,
        child = child_index,
        video_id = %video_id,
        "child complete"
    );
}

async fn create_child_transcript(
    state: &AppState,
    video_id: &str,
    title: &str,
    language: &str,
    caption_source: CaptionSource,
    output_format: OutputFormat,
) -> AppResult<Uuid> {
    let job = TranscriptJob::new(
        video_id.to_string(),
        title.to_string(),
        language.to_string(),
        caption_source,
        output_format,
        state
            .config
            .storage_dir
            .join(format!("jobs/{}", Uuid::new_v4()))
            .to_string_lossy()
            .to_string(),
        state.config.file_ttl_minutes,
    );
    let id = job.id;
    {
        let mut jobs = state.jobs.write().await;
        jobs.insert(id, job);
    }
    spawn_transcript_for(state.clone(), id);
    Ok(id)
}

fn spawn_transcript_for(state: AppState, job_id: Uuid) {
    tokio::spawn(async move {
        let _permit = match state.semaphore.acquire().await {
            Ok(p) => p,
            Err(_) => return,
        };
        let snap = {
            let jobs = state.jobs.read().await;
            jobs.get(&job_id).map(|j| {
                (
                    j.video_id.clone(),
                    j.title.clone(),
                    j.language.clone(),
                    j.caption_source,
                    j.output_format,
                    PathBuf::from(&j.job_dir),
                )
            })
        };
        let Some((video_id, title, language, caption_source, output_format, job_dir)) = snap else {
            return;
        };
        {
            let mut jobs = state.jobs.write().await;
            if let Some(j) = jobs.get_mut(&job_id) {
                j.status = JobStatus::Running;
                j.progress = 5;
                j.updated_at = Utc::now();
            }
        }
        let service = crate::services::transcript::TranscriptService::new(state.ytdlp.clone());
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
        let mut jobs = state.jobs.write().await;
        if let Some(j) = jobs.get_mut(&job_id) {
            j.updated_at = Utc::now();
            match result {
                Ok((path, _src)) => {
                    let filename = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    let max = state.config.max_transcript_file_mb * 1024 * 1024;
                    if size > max {
                        j.status = JobStatus::Failed;
                        j.error = Some(JobError {
                            code: "INTERNAL_ERROR".to_string(),
                            message: format!(
                                "transcript exceeds {} MB",
                                state.config.max_transcript_file_mb
                            ),
                        });
                        return;
                    }
                    j.output_filename = Some(filename);
                    j.progress = 100;
                    j.status = JobStatus::Completed;
                }
                Err(e) => {
                    j.status = JobStatus::Failed;
                    j.error = Some(JobError {
                        code: e.code().to_string(),
                        message: e.message(),
                    });
                }
            }
        }
    });
}

async fn create_child_analysis(
    state: &AppState,
    source_job_id: Uuid,
    video_id: &str,
    title: &str,
    language: &str,
    spec: &AnalysisSpec,
) -> AppResult<Uuid> {
    let snapshot = {
        let jobs = state.jobs.read().await;
        jobs.get(&source_job_id).map(|j| j.job_dir.clone())
    };
    let Some(source_dir) = snapshot else {
        return Err(AppError::JobNotFound);
    };
    let analysis_dir = format!("{}/analysis", source_dir);
    let custom_prompt = if matches!(spec.purpose, AnalysisPurpose::Custom) {
        spec.custom_prompt.clone()
    } else {
        None
    };
    let job = AnalysisJob::new(
        source_job_id,
        video_id.to_string(),
        title.to_string(),
        language.to_string(),
        spec.purpose,
        custom_prompt,
        spec.output_language.clone(),
        analysis_dir,
        state.config.minimax_model.clone(),
        state.config.file_ttl_minutes,
    );
    let id = job.id;
    {
        let mut map = state.analyses.write().await;
        map.insert(id, job);
    }
    crate::routes::analyses::spawn_analysis(state.clone(), id);
    Ok(id)
}

async fn wait_for_transcript(state: &AppState, id: Uuid) -> AppResult<TranscriptJob> {
    let deadline = Instant::now() + Duration::from_secs(state.config.job_timeout_seconds + 30);
    loop {
        let snap = {
            let jobs = state.jobs.read().await;
            jobs.get(&id).cloned()
        };
        if let Some(j) = snap {
            if matches!(j.status, JobStatus::Completed | JobStatus::Failed) {
                return Ok(j);
            }
        } else {
            return Err(AppError::JobNotFound);
        }
        if Instant::now() > deadline {
            return Err(AppError::Internal(format!(
                "transcript job {id} did not finish in time"
            )));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn wait_for_analysis(state: &AppState, id: Uuid) -> AppResult<AnalysisJob> {
    let deadline = Instant::now() + Duration::from_secs(state.config.analysis_timeout_seconds + 30);
    loop {
        let snap = {
            let map = state.analyses.read().await;
            map.get(&id).cloned()
        };
        if let Some(j) = snap {
            if matches!(j.status, AnalysisStatus::Completed | AnalysisStatus::Failed) {
                return Ok(j);
            }
        } else {
            return Err(AppError::JobNotFound);
        }
        if Instant::now() > deadline {
            return Err(AppError::MiniMaxTimeout);
        }
        tokio::time::sleep(Duration::from_millis(800)).await;
    }
}

async fn mark_child_failed(
    state: &AppState,
    playlist_id: Uuid,
    child_index: usize,
    code: &str,
    message: &str,
) {
    let mut map = state.playlists.write().await;
    if let Some(p) = map.get_mut(&playlist_id) {
        if let Some(c) = p.children.get_mut(child_index) {
            c.stage = ChildStage::Failed;
            c.error = Some(JobError {
                code: code.to_string(),
                message: message.to_string(),
            });
        }
        p.failed += 1;
        p.updated_at = Utc::now();
    }
}

async fn mark_playlist_failed(state: &AppState, id: Uuid, code: &str, message: &str) {
    let mut map = state.playlists.write().await;
    if let Some(p) = map.get_mut(&id) {
        p.status = PlaylistStatus::Failed;
        p.error = Some(JobError {
            code: code.to_string(),
            message: message.to_string(),
        });
        p.updated_at = Utc::now();
    }
}

#[derive(Debug, Serialize)]
pub struct PlaylistResponse {
    pub playlist_id: Uuid,
    pub source_url: String,
    pub playlist_title: String,
    pub language: String,
    pub caption_source: CaptionSource,
    pub output_format: OutputFormat,
    pub analysis: Option<Vec<AnalysisSpec>>,
    pub status: PlaylistStatus,
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub children: Vec<PlaylistChild>,
    pub error: Option<JobError>,
    pub zip_url: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn get_playlist(
    State(state): State<AppState>,
    AxPath(id): AxPath<Uuid>,
) -> AppResult<Json<PlaylistResponse>> {
    let snap = {
        let map = state.playlists.read().await;
        map.get(&id).cloned()
    };
    let job = snap.ok_or(AppError::JobNotFound)?;

    if is_expired(job.expires_at) {
        return Err(AppError::FileExpired);
    }

    let zip_url =
        if job.status == PlaylistStatus::Completed && (job.completed > 0 || job.total == 0) {
            Some(format!("/api/playlists/{}/download", job.id))
        } else {
            None
        };

    Ok(Json(PlaylistResponse {
        playlist_id: job.id,
        source_url: job.source_url,
        playlist_title: job.playlist_title,
        language: job.language,
        caption_source: job.caption_source,
        output_format: job.output_format,
        analysis: job.analysis,
        status: job.status,
        total: job.total,
        completed: job.completed,
        failed: job.failed,
        children: job.children,
        error: job.error,
        zip_url,
        created_at: job.created_at,
        updated_at: job.updated_at,
        expires_at: job.expires_at,
    }))
}

pub async fn download_zip(
    State(state): State<AppState>,
    AxPath(id): AxPath<Uuid>,
) -> AppResult<Response> {
    let snap = {
        let map = state.playlists.read().await;
        map.get(&id).cloned()
    };
    let job = snap.ok_or(AppError::JobNotFound)?;
    if is_expired(job.expires_at) {
        return Err(AppError::FileExpired);
    }
    if job.status != PlaylistStatus::Completed {
        return Err(AppError::InvalidAnalysisRequest(
            "playlist is not completed yet".to_string(),
        ));
    }

    let max_bytes = state.config.max_playlist_zip_mb * 1024 * 1024;
    let buf: Vec<u8> = Vec::new();
    let cursor = std::io::Cursor::new(buf);
    let mut zip = zip::ZipWriter::new(cursor);
    let options: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let mut written: usize = 0;
    let mut added: usize = 0;

    for c in &job.children {
        if let (Some(job_id), Some(name)) = (c.transcript_job_id, &c.transcript_filename) {
            let jobs = state.jobs.read().await;
            if let Some(tj) = jobs.get(&job_id) {
                let path = Path::new(&tj.job_dir).join(name);
                if let Ok(bytes) = std::fs::read(&path) {
                    written += bytes.len();
                    if written > max_bytes as usize {
                        return Err(AppError::Internal(format!(
                            "playlist zip exceeds {} MB",
                            state.config.max_playlist_zip_mb
                        )));
                    }
                    let arc = format!(
                        "transcripts/{:03}-{}.{}",
                        added,
                        sanitize_zip(&tj.title),
                        &name
                    );
                    zip.start_file(&arc, options)
                        .map_err(|e| AppError::Internal(format!("zip: {}", e)))?;
                    use std::io::Write;
                    zip.write_all(&bytes)
                        .map_err(|e| AppError::Internal(format!("zip write: {}", e)))?;
                    added += 1;
                }
            }
        }
        for a in &c.analyses {
            if a.status != "completed" {
                continue;
            }
            let (aid, name) = match (a.analysis_id, a.filename.as_deref()) {
                (aid, Some(n)) if !aid.is_nil() => (aid, n),
                _ => continue,
            };
            let map = state.analyses.read().await;
            if let Some(aj) = map.get(&aid) {
                let path = Path::new(&aj.job_dir).join(name);
                if let Ok(bytes) = std::fs::read(&path) {
                    written += bytes.len();
                    if written > max_bytes as usize {
                        return Err(AppError::Internal(format!(
                            "playlist zip exceeds {} MB",
                            state.config.max_playlist_zip_mb
                        )));
                    }
                    let slug = crate::services::analyzer::purpose_slug(a.purpose);
                    let arc = format!(
                        "analyses/{:03}-{}.{}.md",
                        added,
                        sanitize_zip(&aj.title),
                        slug
                    );
                    zip.start_file(&arc, options)
                        .map_err(|e| AppError::Internal(format!("zip: {}", e)))?;
                    use std::io::Write;
                    zip.write_all(&bytes)
                        .map_err(|e| AppError::Internal(format!("zip write: {}", e)))?;
                    added += 1;
                }
            }
        }
    }

    if added == 0 {
        return Err(AppError::InvalidAnalysisRequest(
            "playlist has no files to download".to_string(),
        ));
    }

    let cursor = zip
        .finish()
        .map_err(|e| AppError::Internal(format!("zip finish: {}", e)))?;
    let bytes = cursor.into_inner();

    let slug = slugify_zip(&job.playlist_title);
    let download_name = format!("{}-{}.zip", slug, &job.id.to_string()[..8]);

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    let disposition = format!(
        "attachment; filename=\"{}\"",
        download_name.replace('"', "_")
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

fn slugify_zip(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_dash = true;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && out.len() < 60 {
            out.push('-');
            last_dash = true;
        }
        if out.len() >= 60 {
            break;
        }
    }
    let trimmed = out.trim_end_matches('-');
    if trimmed.is_empty() {
        "playlist".to_string()
    } else {
        trimmed.to_string()
    }
}

fn sanitize_zip(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => out.push('_'),
            c if c.is_control() => out.push('_'),
            c => out.push(c),
        }
        if out.len() >= 80 {
            break;
        }
    }
    out.trim().to_string()
}
