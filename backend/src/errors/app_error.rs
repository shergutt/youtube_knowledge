use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiErrorEnvelope {
    pub error: ApiErrorBody,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid url: {0}")]
    InvalidUrl(String),

    #[error("unsupported url type: {0}")]
    UnsupportedUrlType(String),

    #[error("yt-dlp is not installed or not on PATH")]
    YtDlpNotInstalled,

    #[error("yt-dlp timed out after {0} seconds")]
    YtDlpTimeout(u64),

    #[error("yt-dlp failed: {0}")]
    YtDlpFailed(String),

    #[error("yt-dlp metadata parse failed: {0}")]
    YtDlpMetadataParseFailed(String),

    #[error("no captions were found for the selected language")]
    NoCaptionsFound,

    #[error("requested caption format is not available")]
    CaptionFormatUnavailable,

    #[error("transcript parse failed: {0}")]
    TranscriptParseFailed(String),

    #[error("job not found")]
    JobNotFound,

    #[error("file expired or missing")]
    FileExpired,

    #[error("analysis not configured: MINIMAX_API_KEY is not set")]
    AnalysisNotConfigured,

    #[error("invalid analysis request: {0}")]
    InvalidAnalysisRequest(String),

    #[error("MiniMax request failed: {0}")]
    MiniMaxRequestFailed(String),

    #[error("MiniMax request timed out")]
    MiniMaxTimeout,

    #[error("MiniMax returned an empty or invalid response: {0}")]
    MiniMaxInvalidResponse(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidUrl(_) => "INVALID_URL",
            Self::UnsupportedUrlType(_) => "UNSUPPORTED_URL_TYPE",
            Self::YtDlpNotInstalled => "YTDLP_NOT_INSTALLED",
            Self::YtDlpTimeout(_) => "YTDLP_TIMEOUT",
            Self::YtDlpFailed(_) => "YTDLP_FAILED",
            Self::YtDlpMetadataParseFailed(_) => "YTDLP_METADATA_PARSE_FAILED",
            Self::NoCaptionsFound => "NO_CAPTIONS_FOUND",
            Self::CaptionFormatUnavailable => "CAPTION_FORMAT_UNAVAILABLE",
            Self::TranscriptParseFailed(_) => "TRANSCRIPT_PARSE_FAILED",
            Self::JobNotFound => "JOB_NOT_FOUND",
            Self::FileExpired => "FILE_EXPIRED",
            Self::AnalysisNotConfigured => "ANALYSIS_NOT_CONFIGURED",
            Self::InvalidAnalysisRequest(_) => "INVALID_ANALYSIS_REQUEST",
            Self::MiniMaxRequestFailed(_) => "MINIMAX_REQUEST_FAILED",
            Self::MiniMaxTimeout => "MINIMAX_TIMEOUT",
            Self::MiniMaxInvalidResponse(_) => "MINIMAX_INVALID_RESPONSE",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }

    pub fn message(&self) -> String {
        self.to_string()
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::InvalidUrl(_) | Self::UnsupportedUrlType(_) | Self::InvalidAnalysisRequest(_) => {
                StatusCode::BAD_REQUEST
            }
            Self::NoCaptionsFound
            | Self::CaptionFormatUnavailable
            | Self::YtDlpMetadataParseFailed(_)
            | Self::TranscriptParseFailed(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::JobNotFound | Self::FileExpired => StatusCode::NOT_FOUND,
            Self::AnalysisNotConfigured => StatusCode::SERVICE_UNAVAILABLE,
            Self::YtDlpNotInstalled
            | Self::YtDlpTimeout(_)
            | Self::YtDlpFailed(_)
            | Self::MiniMaxRequestFailed(_)
            | Self::MiniMaxTimeout
            | Self::MiniMaxInvalidResponse(_)
            | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let body = ApiErrorEnvelope {
            error: ApiErrorBody {
                code: self.code().to_string(),
                message: self.message(),
            },
        };
        (self.status(), Json(body)).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
