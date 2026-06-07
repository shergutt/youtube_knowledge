use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::extract::{Path as AxPath, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::{error, info};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::{AppError, AppResult};
use crate::models::analysis::{AnalysisJob, AnalysisPurpose, AnalysisStatus};
use crate::models::job::{JobError, JobStatus};
use crate::models::playlist::AnalysisSpec;
use crate::utils::time::is_expired;

/// Maximum number of analysis specs accepted per request. Mirrors the
/// number of `AnalysisPurpose` variants, which is the natural upper bound
/// (a request asking for the same purpose twice would just duplicate work).
pub const MAX_ANALYSIS_SPECS: usize = 8;

#[derive(Debug, Deserialize)]
pub struct CreateAnalysisRequest {
    pub job_id: Uuid,
    /// One or more analysis goals. Each spec is queued and run in parallel
    /// against the same transcript. The same `purpose` may be requested more
    /// than once (e.g. summary in two languages), but duplicates are not
    /// deduplicated server-side.
    pub specs: Vec<AnalysisSpec>,
    /// Optional override of the output language applied to every spec in
    /// `specs` whose `output_language` is empty. Useful so the frontend can
    /// keep a single "output language" field across a multi-goal request.
    #[serde(default)]
    pub output_language: Option<String>,
    /// Optional override of the custom prompt applied to every spec in
    /// `specs` whose `purpose` is `custom` and whose own `custom_prompt` is
    /// empty. Only used when at least one spec asks for the custom goal.
    #[serde(default)]
    pub custom_prompt: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateAnalysisResponse {
    pub analysis_ids: Vec<Uuid>,
    pub status: AnalysisStatus,
}

pub async fn create_analysis(
    State(state): State<AppState>,
    Json(req): Json<CreateAnalysisRequest>,
) -> AppResult<Json<CreateAnalysisResponse>> {
    if state.analyzer.is_none() {
        return Err(AppError::AnalysisNotConfigured);
    }
    if req.specs.is_empty() {
        return Err(AppError::InvalidAnalysisRequest(
            "at least one analysis spec is required".to_string(),
        ));
    }
    if req.specs.len() > MAX_ANALYSIS_SPECS {
        return Err(AppError::InvalidAnalysisRequest(format!(
            "at most {} analysis specs per request",
            MAX_ANALYSIS_SPECS
        )));
    }

    let default_language = req
        .output_language
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("en")
        .to_string();
    if default_language.len() > 16
        || !default_language
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::InvalidAnalysisRequest(
            "output_language must be alphanumeric".to_string(),
        ));
    }

    // Normalize each spec: fill in default language, resolve custom_prompt.
    let mut normalized: Vec<AnalysisSpec> = Vec::with_capacity(req.specs.len());
    for (i, spec) in req.specs.into_iter().enumerate() {
        if matches!(spec.purpose, AnalysisPurpose::Custom) {
            let prompt = spec
                .custom_prompt
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    req.custom_prompt
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                })
                .unwrap_or_default();
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
        let lang = if spec.output_language.trim().is_empty() {
            default_language.clone()
        } else {
            spec.output_language.trim().to_string()
        };
        if lang.len() > 16
            || !lang
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(AppError::InvalidAnalysisRequest(format!(
                "output_language for spec #{} must be alphanumeric",
                i + 1
            )));
        }
        let resolved_prompt: Option<String> = if matches!(spec.purpose, AnalysisPurpose::Custom)
        {
            spec.custom_prompt
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| {
                    req.custom_prompt
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                })
        } else {
            None
        };
        normalized.push(AnalysisSpec {
            purpose: spec.purpose,
            custom_prompt: resolved_prompt,
            output_language: lang,
        });
    }

    // Look up source job
    let (video_id, title, language, source_status, source_dir) = {
        let jobs = state.jobs.read().await;
        let job = jobs.get(&req.job_id).ok_or(AppError::JobNotFound)?;
        (
            job.video_id.clone(),
            job.title.clone(),
            job.language.clone(),
            job.status,
            job.job_dir.clone(),
        )
    };

    if source_status != JobStatus::Completed {
        return Err(AppError::InvalidAnalysisRequest(
            "transcript job is not completed yet".to_string(),
        ));
    }
    // The transcript job record's `expires_at` is the single source of truth
    // for transcript availability; the analysis is written into a sub-dir of
    // the transcript job dir, so cleanup of the parent removes both.
    let source_expires_at = {
        let jobs = state.jobs.read().await;
        jobs.get(&req.job_id).and_then(|j| j.expires_at)
    };
    if is_expired(source_expires_at) {
        return Err(AppError::FileExpired);
    }

    // Sub-directory inside the transcript job dir, so cleanup deletes both.
    let analysis_dir_rel = format!("{}/analysis", source_dir);

    // Build and register one AnalysisJob per spec, then spawn them all. The
    // analysis semaphore inside `spawn_analysis` will throttle the actual
    // MiniMax calls, but the registration + queuing step is parallel.
    let mut analysis_ids: Vec<Uuid> = Vec::with_capacity(normalized.len());
    {
        let mut map = state.analyses.write().await;
        for spec in normalized {
            let custom_prompt = if matches!(spec.purpose, AnalysisPurpose::Custom) {
                spec.custom_prompt
            } else {
                None
            };
            let job = AnalysisJob::new(
                req.job_id,
                video_id.clone(),
                title.clone(),
                language.clone(),
                spec.purpose,
                custom_prompt,
                spec.output_language,
                analysis_dir_rel.clone(),
                state.config.minimax_model.clone(),
                state.config.file_ttl_minutes,
            );
            let id = job.id;
            map.insert(id, job);
            analysis_ids.push(id);
        }
    }

    for id in &analysis_ids {
        spawn_analysis(state.clone(), *id);
    }

    Ok(Json(CreateAnalysisResponse {
        analysis_ids,
        status: AnalysisStatus::Queued,
    }))
}

pub fn spawn_analysis(state: AppState, analysis_id: Uuid) {
    tokio::spawn(async move {
        let permit = match state.analysis_semaphore.acquire().await {
            Ok(p) => p,
            Err(_) => return,
        };

        // Snapshot fields
        let (
            video_id,
            title,
            language,
            purpose,
            custom_prompt,
            output_language,
            job_dir,
            source_job_id,
        ) = {
            let map = state.analyses.read().await;
            match map.get(&analysis_id) {
                Some(j) => (
                    j.video_id.clone(),
                    j.title.clone(),
                    j.language.clone(),
                    j.purpose,
                    j.custom_prompt.clone(),
                    j.output_language.clone(),
                    j.job_dir.clone(),
                    j.source_job_id,
                ),
                None => return,
            }
        };

        {
            let mut map = state.analyses.write().await;
            if let Some(j) = map.get_mut(&analysis_id) {
                j.status = AnalysisStatus::Running;
                j.progress = 10;
                j.updated_at = Utc::now();
            }
        }

        let analyzer = match state.require_analyzer() {
            Ok(a) => a,
            Err(e) => {
                mark_failed(&state, analysis_id, e.code(), &e.message()).await;
                return;
            }
        };

        // The transcript lives in the source job's directory; the analysis
        // job_dir is a sub-directory used for the .md output.
        let job_dir_path = PathBuf::from(&job_dir);
        let transcript_dir = job_dir_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| job_dir_path.clone());
        let transcript = match analyzer.read_transcript(&transcript_dir, &language) {
            Ok(t) => t,
            Err(e) => {
                mark_failed(&state, analysis_id, e.code(), &e.message()).await;
                return;
            }
        };

        // Truncate if too long
        let max_chars = state.config.max_transcript_chars_for_analysis;
        let transcript_to_send: String = if transcript.chars().count() > max_chars {
            let truncated: String = transcript.chars().take(max_chars).collect();
            let mut t = truncated;
            t.push_str("\n\n[... transcript truncated for analysis ...]");
            t
        } else {
            transcript.clone()
        };

        {
            let mut map = state.analyses.write().await;
            if let Some(j) = map.get_mut(&analysis_id) {
                j.progress = 25;
                j.updated_at = Utc::now();
            }
        }

        let result = analyzer
            .analyze(
                purpose,
                custom_prompt.as_deref(),
                &video_id,
                &language,
                &output_language,
                &transcript_to_send,
            )
            .await;

        {
            let mut map = state.analyses.write().await;
            let Some(job) = map.get_mut(&analysis_id) else {
                return;
            };
            job.updated_at = Utc::now();
            match result {
                Ok(out) => {
                    // Save to disk
                    let path = match analyzer.save_markdown(
                        &job_dir_path,
                        &video_id,
                        &title,
                        purpose,
                        &out.markdown,
                    ) {
                        Ok(p) => p,
                        Err(e) => {
                            job.status = AnalysisStatus::Failed;
                            job.error = Some(JobError {
                                code: e.code().to_string(),
                                message: e.message(),
                            });
                            return;
                        }
                    };
                    // Verify file size
                    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    let max = state.config.max_analysis_file_mb * 1024 * 1024;
                    if size > max {
                        let _ = std::fs::remove_file(&path);
                        job.status = AnalysisStatus::Failed;
                        job.error = Some(JobError {
                            code: "INTERNAL_ERROR".to_string(),
                            message: format!(
                                "analysis exceeds {} MB",
                                state.config.max_analysis_file_mb
                            ),
                        });
                        return;
                    }
                    let filename = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    job.output_filename = Some(filename);
                    job.input_tokens = out.input_tokens;
                    job.output_tokens = out.output_tokens;
                    job.progress = 100;
                    job.status = AnalysisStatus::Completed;
                    info!(
                        analysis_id = %analysis_id,
                        source_job_id = %source_job_id,
                        video_id = %video_id,
                        purpose = ?purpose,
                        path = %path.display(),
                        input_tokens = ?out.input_tokens,
                        output_tokens = ?out.output_tokens,
                        "analysis completed"
                    );
                }
                Err(e) => {
                    error!(analysis_id = %analysis_id, error = %e, "analysis failed");
                    job.status = AnalysisStatus::Failed;
                    job.error = Some(JobError {
                        code: e.code().to_string(),
                        message: e.message(),
                    });
                }
            }
        }

        drop(permit);
    });
}

async fn mark_failed(state: &AppState, id: Uuid, code: &str, message: &str) {
    let mut map = state.analyses.write().await;
    if let Some(j) = map.get_mut(&id) {
        j.status = AnalysisStatus::Failed;
        j.error = Some(JobError {
            code: code.to_string(),
            message: message.to_string(),
        });
        j.updated_at = Utc::now();
    }
}

#[derive(Debug, Serialize)]
pub struct AnalysisJobResponse {
    pub analysis_id: Uuid,
    pub source_job_id: Uuid,
    pub video_id: String,
    pub title: String,
    pub purpose: AnalysisPurpose,
    pub output_language: String,
    pub status: AnalysisStatus,
    pub progress: u8,
    pub output_filename: Option<String>,
    pub download_url: Option<String>,
    pub error: Option<JobError>,
    pub model: String,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn get_analysis(
    State(state): State<AppState>,
    AxPath(id): AxPath<Uuid>,
) -> AppResult<Json<AnalysisJobResponse>> {
    let map = state.analyses.read().await;
    let job = map.get(&id).ok_or(AppError::JobNotFound)?.clone();
    drop(map);

    if is_expired(job.expires_at) {
        return Err(AppError::FileExpired);
    }

    let download_url = if job.status == AnalysisStatus::Completed {
        Some(format!("/api/analyses/{}/download", job.id))
    } else {
        None
    };

    Ok(Json(AnalysisJobResponse {
        analysis_id: job.id,
        source_job_id: job.source_job_id,
        video_id: job.video_id,
        title: job.title,
        purpose: job.purpose,
        output_language: job.output_language,
        status: job.status,
        progress: job.progress,
        output_filename: job.output_filename,
        download_url,
        error: job.error,
        model: job.model,
        input_tokens: job.input_tokens,
        output_tokens: job.output_tokens,
        expires_at: job.expires_at,
    }))
}

pub async fn download_analysis(
    State(state): State<AppState>,
    AxPath(id): AxPath<Uuid>,
) -> AppResult<Response> {
    let (job_dir_rel, output_filename, status, expires_at) = {
        let map = state.analyses.read().await;
        let job = map.get(&id).ok_or(AppError::JobNotFound)?;
        (
            job.job_dir.clone(),
            job.output_filename.clone(),
            job.status,
            job.expires_at,
        )
    };

    if is_expired(expires_at) || status != AnalysisStatus::Completed {
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
    if !is_markdown(&resolved) {
        return Err(AppError::FileExpired);
    }

    let bytes = fs::read(&resolved)
        .await
        .map_err(|_| AppError::FileExpired)?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/markdown; charset=utf-8"),
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

fn sanitize_filename(name: &str) -> String {
    let allowed_ext: &[&str] = &["md"];
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
        return format!("{stem}.md");
    }
    stem
}

fn is_markdown(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("md"))
        .unwrap_or(false)
}
