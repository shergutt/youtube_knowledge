use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::analysis::AnalysisPurpose;
use super::api::CaptionSource;
use super::job::JobError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaylistStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildStage {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisSpec {
    pub purpose: AnalysisPurpose,
    #[serde(default)]
    pub custom_prompt: Option<String>,
    pub output_language: String,
}

/// One per-analysis result attached to a playlist child. Multiple analyses
/// can be produced for the same transcript when the playlist request
/// asked for several purposes; one `ChildAnalysis` is recorded per purpose
/// (per spec).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildAnalysis {
    pub analysis_id: Uuid,
    pub purpose: AnalysisPurpose,
    pub output_language: String,
    pub filename: Option<String>,
    pub download_url: Option<String>,
    pub status: String,
    pub error: Option<JobError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistChild {
    pub video_id: String,
    pub title: String,
    pub stage: ChildStage,
    pub transcript_job_id: Option<Uuid>,
    pub transcript_filename: Option<String>,
    pub transcript_download_url: Option<String>,
    /// One entry per analysis spec that ran against this child's transcript.
    /// Empty when analysis is disabled for the playlist.
    pub analyses: Vec<ChildAnalysis>,
    pub error: Option<JobError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistJob {
    pub id: Uuid,
    pub source_url: String,
    pub playlist_id: String,
    pub playlist_title: String,
    pub language: String,
    pub caption_source: CaptionSource,
    pub output_format: crate::models::api::OutputFormat,
    /// One or more analysis specs to run on every video of the playlist.
    /// `None` means "transcripts only, no analysis". Each spec is fanned out
    /// to every child and the resulting per-child `ChildAnalysis` records are
    /// collected in `PlaylistChild::analyses`.
    pub analysis: Option<Vec<AnalysisSpec>>,
    pub status: PlaylistStatus,
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub children: Vec<PlaylistChild>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub job_dir: String,
    pub error: Option<JobError>,
}

impl PlaylistJob {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_url: String,
        playlist_id: String,
        language: String,
        caption_source: CaptionSource,
        output_format: crate::models::api::OutputFormat,
        analysis: Option<Vec<AnalysisSpec>>,
        job_dir: String,
        ttl_minutes: i64,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            source_url,
            playlist_id,
            playlist_title: String::new(),
            language,
            caption_source,
            output_format,
            analysis,
            status: PlaylistStatus::Queued,
            total: 0,
            completed: 0,
            failed: 0,
            children: Vec::new(),
            created_at: now,
            updated_at: now,
            expires_at: Some(now + chrono::Duration::minutes(ttl_minutes)),
            job_dir,
            error: None,
        }
    }
}
