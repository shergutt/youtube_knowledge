use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptionSource {
    Manual,
    Auto,
    Best,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Vtt,
    Srt,
    Txt,
    Json,
}

impl OutputFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Vtt => "vtt",
            Self::Srt => "srt",
            Self::Txt => "txt",
            Self::Json => "json",
        }
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Vtt => "text/vtt",
            Self::Srt => "application/x-subrip",
            Self::Txt => "text/plain; charset=utf-8",
            Self::Json => "application/json",
        }
    }
}
