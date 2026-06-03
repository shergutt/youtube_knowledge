use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::api::{CaptionSource, OutputFormat};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptJob {
    pub id: Uuid,
    pub video_id: String,
    pub title: String,
    pub language: String,
    pub caption_source: CaptionSource,
    pub output_format: OutputFormat,
    pub status: JobStatus,
    pub progress: u8,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub output_filename: Option<String>,
    pub error: Option<JobError>,
    pub job_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobError {
    pub code: String,
    pub message: String,
}

impl TranscriptJob {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        video_id: String,
        title: String,
        language: String,
        caption_source: CaptionSource,
        output_format: OutputFormat,
        job_dir: String,
        ttl_minutes: i64,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            video_id,
            title,
            language,
            caption_source,
            output_format,
            status: JobStatus::Queued,
            progress: 0,
            created_at: now,
            updated_at: now,
            expires_at: Some(now + chrono::Duration::minutes(ttl_minutes)),
            output_filename: None,
            error: None,
            job_dir,
        }
    }
}
