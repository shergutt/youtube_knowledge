use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub storage_dir: PathBuf,
    pub max_concurrent_jobs: usize,
    pub job_timeout_seconds: u64,
    pub file_ttl_minutes: i64,
    pub max_url_length: usize,
    pub max_transcript_file_mb: u64,
    pub cors_origin: String,
    // MiniMax (M3) settings for the AI analyzer
    pub minimax_api_key: String,
    pub minimax_base_url: String,
    pub minimax_model: String,
    pub analysis_max_output_tokens: u32,
    pub analysis_timeout_seconds: u64,
    pub analysis_temperature: f32,
    pub analysis_top_p: f32,
    pub max_analysis_file_mb: u64,
    pub max_transcript_chars_for_analysis: usize,
    pub max_playlist_videos: usize,
    pub max_playlist_zip_mb: u64,
    pub max_playlist_concurrent_videos: usize,
    pub yt_dlp_cookies_file: Option<PathBuf>,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            host: env_or("APP_HOST", "0.0.0.0"),
            port: env_parse("APP_PORT", 8080)?,
            storage_dir: PathBuf::from(env_or("STORAGE_DIR", "./storage")),
            max_concurrent_jobs: env_parse("MAX_CONCURRENT_JOBS", 2)?,
            job_timeout_seconds: env_parse("JOB_TIMEOUT_SECONDS", 60)?,
            file_ttl_minutes: env_parse("FILE_TTL_MINUTES", 60)?,
            max_url_length: env_parse("MAX_URL_LENGTH", 2048)?,
            max_transcript_file_mb: env_parse("MAX_TRANSCRIPT_FILE_MB", 10)?,
            cors_origin: env_or("CORS_ORIGIN", "http://localhost:5173"),
            minimax_api_key: env_or("MINIMAX_API_KEY", ""),
            minimax_base_url: env_or("MINIMAX_BASE_URL", "https://api.minimax.io/anthropic"),
            minimax_model: env_or("MINIMAX_MODEL", "MiniMax-M3"),
            analysis_max_output_tokens: env_parse("ANALYSIS_MAX_OUTPUT_TOKENS", 16_384)?,
            analysis_timeout_seconds: env_parse("ANALYSIS_TIMEOUT_SECONDS", 1200)?,
            analysis_temperature: env_parse("ANALYSIS_TEMPERATURE", 1.0_f32)?,
            analysis_top_p: env_parse("ANALYSIS_TOP_P", 0.95_f32)?,
            max_analysis_file_mb: env_parse("MAX_ANALYSIS_FILE_MB", 5)?,
            max_transcript_chars_for_analysis: env_parse(
                "MAX_TRANSCRIPT_CHARS_FOR_ANALYSIS",
                400_000,
            )?,
            max_playlist_videos: env_parse("MAX_PLAYLIST_VIDEOS", 50)?,
            max_playlist_zip_mb: env_parse("MAX_PLAYLIST_ZIP_MB", 200)?,
            max_playlist_concurrent_videos: env_parse("MAX_PLAYLIST_CONCURRENT_VIDEOS", 2)?,
            yt_dlp_cookies_file: env::var("YTDLP_COOKIES_FILE")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
        })
    }

    pub fn jobs_dir(&self) -> PathBuf {
        self.storage_dir.join("jobs")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.storage_dir.join("cache")
    }

    pub fn minimax_configured(&self) -> bool {
        !self.minimax_api_key.trim().is_empty()
    }
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> Result<T, String> {
    match env::var(key) {
        Ok(v) => v
            .parse::<T>()
            .map_err(|_| format!("invalid value for {}: {}", key, v)),
        Err(_) => Ok(default),
    }
}
