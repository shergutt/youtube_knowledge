use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::Deserialize;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use crate::config::AppConfig;
use crate::errors::{AppError, AppResult};
use crate::models::transcript::{CaptionTrack, ProbeResponse};
use crate::services::youtube_url::canonical_url;

#[derive(Clone)]
pub struct YtDlpService {
    pub binary: String,
    pub config: AppConfig,
}

impl YtDlpService {
    pub fn new(config: AppConfig) -> Self {
        Self {
            binary: std::env::var("YTDLP_BINARY").unwrap_or_else(|_| "yt-dlp".to_string()),
            config,
        }
    }

    /// Append `--cookies <path>` to a Command if a cookies file is configured
    /// and exists. Used for every yt-dlp invocation so all routes (probe,
    /// transcript, playlist) get authenticated equally.
    pub fn apply_cookies(&self, cmd: &mut Command) {
        if let Some(path) = &self.config.yt_dlp_cookies_file {
            if path.exists() {
                cmd.arg("--cookies").arg(path.as_os_str());
            } else {
                tracing::warn!(
                    path = %path.display(),
                    "YTDLP_COOKIES_FILE is set but the file does not exist; \
                     proceeding without cookies"
                );
            }
        }
        // YouTube's n-challenge requires the EJS challenge-solver script to be
        // fetched on first run; without it, only storyboard images are
        // available. This flag tells yt-dlp to download the script from the
        // yt-dlp GitHub repo. Cached after first download.
        cmd.arg("--remote-components").arg("ejs:github");
    }

    pub async fn check_installed(&self) -> AppResult<()> {
        let mut cmd = Command::new(&self.binary);
        cmd.arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = timeout(Duration::from_secs(10), cmd.output())
            .await
            .map_err(|_| AppError::YtDlpNotInstalled)?
            .map_err(|_| AppError::YtDlpNotInstalled)?;
        if !output.status.success() {
            return Err(AppError::YtDlpNotInstalled);
        }
        Ok(())
    }

    /// List the videos in a YouTube playlist using `--flat-playlist -J`.
    /// Returns the playlist id, title, and an ordered list of entries.
    pub async fn list_playlist(&self, playlist_id: &str) -> AppResult<RawPlaylist> {
        let url = super::youtube_url::canonical_playlist_url(playlist_id);
        let mut cmd = Command::new(&self.binary);
        cmd.args(["-J", "--flat-playlist", "--skip-download", "--no-warnings"])
            .arg(&url)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        self.apply_cookies(&mut cmd);

        let output = timeout(
            Duration::from_secs(self.config.job_timeout_seconds),
            cmd.output(),
        )
        .await
        .map_err(|_| AppError::YtDlpTimeout(self.config.job_timeout_seconds))?
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::YtDlpNotInstalled
            } else {
                AppError::YtDlpFailed(e.to_string())
            }
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::YtDlpMetadataParseFailed(format!(
                "yt-dlp playlist probe failed (status {}): {}",
                output.status,
                truncate(&stderr, 400)
            )));
        }

        let raw: RawPlaylist = serde_json::from_slice(&output.stdout)
            .map_err(|e| AppError::YtDlpMetadataParseFailed(format!("parse: {}", e)))?;
        Ok(raw)
    }

    pub async fn probe(&self, video_id: &str) -> AppResult<ProbeResponse> {
        let url = canonical_url(video_id);
        let mut cmd = Command::new(&self.binary);
        cmd.args(["-J", "--skip-download", "--no-playlist", "--no-warnings"])
            .arg(&url)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        self.apply_cookies(&mut cmd);

        let output = timeout(
            Duration::from_secs(self.config.job_timeout_seconds),
            cmd.output(),
        )
        .await
        .map_err(|_| AppError::YtDlpTimeout(self.config.job_timeout_seconds))?
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::YtDlpNotInstalled
            } else {
                AppError::YtDlpFailed(e.to_string())
            }
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::YtDlpMetadataParseFailed(format!(
                "yt-dlp exited with status {}: {}",
                output.status, stderr
            )));
        }

        let raw: RawProbe = serde_json::from_slice(&output.stdout)
            .map_err(|e| AppError::YtDlpMetadataParseFailed(e.to_string()))?;

        Ok(build_probe_response(video_id, raw))
    }

    /// Download subtitle files for the given video. The output dir will be created if missing.
    /// Returns the list of files produced in `output_dir`.
    pub async fn download_subtitles(
        &self,
        video_id: &str,
        language: &str,
        mode: SubtitleMode,
        output_format: &str,
        output_dir: &Path,
    ) -> AppResult<Vec<PathBuf>> {
        std::fs::create_dir_all(output_dir)
            .map_err(|e| AppError::Internal(format!("failed to create job dir: {}", e)))?;

        let url = canonical_url(video_id);
        let write_arg = match mode {
            SubtitleMode::Manual => "--write-subs",
            SubtitleMode::Auto => "--write-auto-subs",
        };

        // sub-format: for SRT we use vtt/srt/best, and rely on --convert-subs srt
        // for VTT we use vtt/srt/best as well; yt-dlp prefers the first available
        let sub_format = "vtt/srt/best";
        let convert_subs = if output_format == "srt" {
            Some("srt")
        } else {
            None
        };

        let mut cmd = Command::new(&self.binary);
        cmd.arg("--skip-download")
            .arg(write_arg)
            .arg("--sub-langs")
            .arg(language)
            .arg("--sub-format")
            .arg(sub_format);
        if let Some(fmt) = convert_subs {
            cmd.arg("--convert-subs").arg(fmt);
        }
        cmd.arg("--no-playlist")
            .arg("-P")
            .arg(format!("subtitle:{}", output_dir.display()))
            .arg("-o")
            .arg("subtitle:%(id)s.%(ext)s")
            .arg(&url)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        self.apply_cookies(&mut cmd);

        let output = timeout(
            Duration::from_secs(self.config.job_timeout_seconds),
            cmd.output(),
        )
        .await
        .map_err(|_| AppError::YtDlpTimeout(self.config.job_timeout_seconds))?
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::YtDlpNotInstalled
            } else {
                AppError::YtDlpFailed(e.to_string())
            }
        })?;

        // yt-dlp can exit non-zero for "no subtitles" but still produce stderr noise;
        // we determine success by checking the produced files.
        let _ = output.status;

        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(output_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if matches!(
                            ext,
                            "vtt" | "srt" | "srv3" | "ttml" | "json3" | "ass" | "lrc"
                        ) {
                            files.push(path);
                        }
                    }
                }
            }
        }

        // Filter by language token in filename (yt-dlp uses e.g. "VIDEO_ID.en.vtt")
        // The spec uses "%(id)s.%(ext)s", so we may not get language token.
        // Still keep all candidates and let caller pick.

        Ok(files)
    }
}

#[derive(Clone, Copy)]
pub enum SubtitleMode {
    Manual,
    Auto,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RawPlaylist {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub entries: Vec<RawPlaylistEntry>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RawPlaylistEntry {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default, rename = "url")]
    pub _url: Option<String>,
}

#[derive(Deserialize)]
struct RawProbe {
    id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    thumbnail: Option<String>,
    #[serde(default)]
    subtitles: HashMap<String, Vec<RawSubtitle>>,
    #[serde(default, rename = "automatic_captions")]
    automatic_captions: HashMap<String, Vec<RawSubtitle>>,
}

#[derive(Deserialize)]
struct RawSubtitle {
    #[serde(default)]
    ext: Option<String>,
    #[serde(default, rename = "name")]
    _name: Option<String>,
}

fn build_probe_response(video_id: &str, raw: RawProbe) -> ProbeResponse {
    let manual = convert_tracks(raw.subtitles);
    let auto = convert_tracks(raw.automatic_captions);
    ProbeResponse {
        video_id: raw.id,
        title: raw.title.unwrap_or_else(|| video_id.to_string()),
        duration_seconds: raw.duration.map(|d| d as u64),
        thumbnail_url: raw.thumbnail,
        manual_captions: manual,
        automatic_captions: auto,
    }
}

fn convert_tracks(map: HashMap<String, Vec<RawSubtitle>>) -> Vec<CaptionTrack> {
    let mut tracks: Vec<CaptionTrack> = map
        .into_iter()
        .map(|(lang, subs)| {
            let mut formats: Vec<String> = subs
                .into_iter()
                .filter_map(|s| s.ext)
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            formats.sort();
            let name = localized_language_name(&lang);
            CaptionTrack {
                language: lang,
                name,
                formats,
            }
        })
        .collect();
    tracks.sort_by(|a, b| a.language.cmp(&b.language));
    tracks
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        let mut end = max;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        &s[..end]
    }
}

/// Map a YouTube/BCP-47 caption language code to a human-readable name.
/// Falls back to the raw code for anything not in the table.
pub fn localized_language_name(lang: &str) -> String {
    match lang {
        "en" => "English",
        "en-US" => "English (United States)",
        "en-GB" => "English (United Kingdom)",
        "en-AU" => "English (Australia)",
        "en-IN" => "English (India)",
        "es" => "Spanish",
        "es-ES" => "Spanish (Spain)",
        "es-MX" => "Spanish (Mexico)",
        "es-AR" => "Spanish (Argentina)",
        "es-419" => "Spanish (Latin America)",
        "es-CL" => "Spanish (Chile)",
        "es-CO" => "Spanish (Colombia)",
        "es-PE" => "Spanish (Peru)",
        "es-VE" => "Spanish (Venezuela)",
        "pt" => "Portuguese",
        "pt-BR" => "Portuguese (Brazil)",
        "pt-PT" => "Portuguese (Portugal)",
        "fr" => "French",
        "fr-FR" => "French (France)",
        "fr-CA" => "French (Canada)",
        "de" => "German",
        "it" => "Italian",
        "nl" => "Dutch",
        "pl" => "Polish",
        "ru" => "Russian",
        "tr" => "Turkish",
        "ar" => "Arabic",
        "hi" => "Hindi",
        "bn" => "Bengali",
        "ur" => "Urdu",
        "ja" => "Japanese",
        "ko" => "Korean",
        "zh-Hans" => "Chinese (Simplified)",
        "zh-Hant" => "Chinese (Traditional)",
        "zh-CN" => "Chinese (Simplified, China)",
        "zh-TW" => "Chinese (Traditional, Taiwan)",
        "vi" => "Vietnamese",
        "th" => "Thai",
        "id" => "Indonesian",
        "ms" => "Malay",
        "sv" => "Swedish",
        "no" => "Norwegian",
        "fi" => "Finnish",
        "da" => "Danish",
        "cs" => "Czech",
        "el" => "Greek",
        "he" => "Hebrew",
        "ro" => "Romanian",
        "hu" => "Hungarian",
        "uk" => "Ukrainian",
        other => other,
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localized_name_for_spanish_variants() {
        assert_eq!(localized_language_name("es"), "Spanish");
        assert_eq!(localized_language_name("es-ES"), "Spanish (Spain)");
        assert_eq!(localized_language_name("es-MX"), "Spanish (Mexico)");
        assert_eq!(localized_language_name("es-AR"), "Spanish (Argentina)");
        assert_eq!(localized_language_name("es-419"), "Spanish (Latin America)");
    }

    #[test]
    fn localized_name_keeps_known_languages() {
        assert_eq!(localized_language_name("en"), "English");
        assert_eq!(localized_language_name("en-US"), "English (United States)");
        assert_eq!(localized_language_name("fr-CA"), "French (Canada)");
        assert_eq!(localized_language_name("pt-BR"), "Portuguese (Brazil)");
        assert_eq!(localized_language_name("zh-Hans"), "Chinese (Simplified)");
    }

    #[test]
    fn localized_name_falls_back() {
        assert_eq!(localized_language_name("xx"), "xx");
        assert_eq!(localized_language_name("klingon"), "klingon");
    }
}
