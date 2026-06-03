use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::job::JobError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisPurpose {
    /// Short summary, key points, conclusion.
    Summary,
    /// Detailed study notes with definitions, Q&A, flashcards.
    StudyNotes,
    /// Bullet list of insights organized by theme.
    KeyTakeaways,
    /// Tasks, owners, deadlines, decisions.
    ActionItems,
    /// Convert into an engaging blog article.
    BlogPost,
    /// Step-by-step tutorial with prerequisites and exercises.
    Tutorial,
    /// Free-form goal driven by `custom_prompt`.
    Custom,
}

impl AnalysisPurpose {
    /// Build the per-purpose user instruction that is appended to the system
    /// prompt. The shared system message supplies output-format requirements.
    pub fn instruction(&self) -> &'static str {
        match self {
            Self::Summary => "Produce a concise summary of the video. Include: \
                (1) a 2-3 sentence overview, (2) a 'Key points' section with 5-10 bullets, \
                (3) a 'Notable quotes' section with up to 3 verbatim excerpts, \
                (4) a 'Conclusion' section with the main takeaway.",
            Self::StudyNotes => "Produce thorough study notes. Include: \
                (1) a 'Topics covered' outline, (2) for each major topic a 'Definition' \
                and 'Explanation' subsection, (3) a 'Q&A' section with 5-8 review questions \
                and short answers, (4) a 'Flashcards' section formatted as a table with \
                'Term' and 'Definition' columns, (5) a 'Further study' section.",
            Self::KeyTakeaways => "Extract the most important insights and group them by theme. \
                Include: (1) a 'Headline takeaway' (1 sentence), (2) a 'Themes' section with \
                3-5 named themes, each containing 2-4 supporting bullets, (3) a 'Counterpoints' \
                section listing any caveats, objections, or limitations mentioned.",
            Self::ActionItems => "Extract concrete action items. Include: \
                (1) a 'Decisions made' section, (2) a 'Tasks' table with columns 'Task', 'Owner' \
                (if mentioned, else 'Unassigned'), and 'Deadline' (if mentioned, else '—'), \
                (3) a 'Follow-ups' section listing open questions and next investigations.",
            Self::BlogPost => "Rewrite the transcript as an engaging blog article. \
                Include: (1) a punchy title and 1-2 sentence lede, (2) an 'Introduction' that \
                hooks the reader, (3) 3-5 '## Section' headings organizing the content, \
                (4) inline emphasis where appropriate, (5) a 'Conclusion' with a clear takeaway \
                and a 'What to read next' pointer if relevant.",
            Self::Tutorial => "Convert into a hands-on tutorial. Include: (1) a 'What you will build' \
                overview, (2) a 'Prerequisites' list, (3) numbered 'Steps' each with a one-line \
                goal and short explanation, (4) code or command examples when the speaker gives them, \
                (5) a 'Verify it works' checklist, (6) a 'Troubleshooting' section for common mistakes \
                mentioned in the video.",
            Self::Custom => "",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisJob {
    pub id: Uuid,
    pub source_job_id: Uuid,
    pub video_id: String,
    pub title: String,
    pub language: String,
    pub purpose: AnalysisPurpose,
    pub custom_prompt: Option<String>,
    pub output_language: String,
    pub status: AnalysisStatus,
    pub progress: u8,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub output_filename: Option<String>,
    pub error: Option<JobError>,
    pub job_dir: String,
    pub model: String,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

impl AnalysisJob {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_job_id: Uuid,
        video_id: String,
        title: String,
        language: String,
        purpose: AnalysisPurpose,
        custom_prompt: Option<String>,
        output_language: String,
        job_dir: String,
        model: String,
        ttl_minutes: i64,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            source_job_id,
            video_id,
            title,
            language,
            purpose,
            custom_prompt,
            output_language,
            status: AnalysisStatus::Queued,
            progress: 0,
            created_at: now,
            updated_at: now,
            expires_at: Some(now + chrono::Duration::minutes(ttl_minutes)),
            output_filename: None,
            error: None,
            job_dir,
            model,
            input_tokens: None,
            output_tokens: None,
        }
    }
}
