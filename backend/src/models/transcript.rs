use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptionTrack {
    pub language: String,
    pub name: String,
    pub formats: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResponse {
    pub video_id: String,
    pub title: String,
    pub duration_seconds: Option<u64>,
    pub thumbnail_url: Option<String>,
    pub manual_captions: Vec<CaptionTrack>,
    pub automatic_captions: Vec<CaptionTrack>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub start: String,
    pub end: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptJson {
    pub video_id: String,
    pub language: String,
    pub source: String,
    pub segments: Vec<TranscriptSegment>,
}
