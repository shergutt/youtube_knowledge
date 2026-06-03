use url::Url;

use crate::errors::{AppError, AppResult};

const VIDEO_ID_LEN: usize = 11;

pub fn canonical_url(video_id: &str) -> String {
    format!("https://www.youtube.com/watch?v={}", video_id)
}

pub fn canonical_playlist_url(playlist_id: &str) -> String {
    format!("https://www.youtube.com/playlist?list={}", playlist_id)
}

pub fn extract_video_id(input: &str, max_url_length: usize) -> AppResult<String> {
    if input.len() > max_url_length {
        return Err(AppError::InvalidUrl(format!(
            "URL exceeds maximum length of {} characters",
            max_url_length
        )));
    }

    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidUrl("URL is empty".to_string()));
    }

    // Reject obviously unsafe schemes
    if trimmed.contains("://")
        && !trimmed.starts_with("http://")
        && !trimmed.starts_with("https://")
    {
        return Err(AppError::InvalidUrl(
            "only http(s) URLs are supported".to_string(),
        ));
    }
    if trimmed.starts_with("file://") {
        return Err(AppError::InvalidUrl(
            "file:// URLs are not allowed".to_string(),
        ));
    }

    let lower = trimmed.to_lowercase();
    if lower.starts_with("localhost") || lower.starts_with("127.") || lower.starts_with("0.0.0.0") {
        return Err(AppError::InvalidUrl(
            "localhost URLs are not allowed".to_string(),
        ));
    }

    let url = Url::parse(trimmed).map_err(|e| AppError::InvalidUrl(e.to_string()))?;
    let host = url
        .host_str()
        .ok_or_else(|| AppError::InvalidUrl("URL has no host".to_string()))?
        .to_lowercase();

    if !is_youtube_host(&host) {
        return Err(AppError::InvalidUrl(format!(
            "URL is not a YouTube domain: {}",
            host
        )));
    }

    // Reject unsupported YouTube URL types
    let path = url.path();
    let lower_path = path.to_lowercase();
    if lower_path.starts_with("/channel/")
        || lower_path.starts_with("/c/")
        || lower_path.starts_with("/user/")
        || lower_path.starts_with("/@")
    {
        return Err(AppError::UnsupportedUrlType(
            "channels and handles are not supported".to_string(),
        ));
    }
    if lower_path.starts_with("/results") || lower_path.starts_with("/feed") {
        return Err(AppError::UnsupportedUrlType(
            "search and feed URLs are not supported".to_string(),
        ));
    }

    // watch?v=
    if host == "www.youtube.com" || host == "youtube.com" {
        if lower_path == "/watch" {
            let v = url
                .query_pairs()
                .find(|(k, _)| k == "v")
                .map(|(_, v)| v.into_owned());
            return match v {
                Some(id) if is_valid_video_id(&id) => Ok(id),
                _ => Err(AppError::InvalidUrl(
                    "missing or invalid v parameter".to_string(),
                )),
            };
        }
        if lower_path.starts_with("/shorts/") {
            let id = path
                .trim_start_matches('/')
                .trim_start_matches("shorts/")
                .trim_end_matches('/');
            let id = id.split('/').next().unwrap_or("");
            return check_video_id(id);
        }
        if lower_path.starts_with("/embed/") {
            let id = path
                .trim_start_matches('/')
                .trim_start_matches("embed/")
                .trim_end_matches('/');
            let id = id.split('/').next().unwrap_or("");
            return check_video_id(id);
        }
    }

    // youtu.be/<id>
    if host == "youtu.be" {
        let id = path.trim_start_matches('/').trim_end_matches('/');
        let id = id.split('/').next().unwrap_or("");
        return check_video_id(id);
    }

    Err(AppError::InvalidUrl(format!(
        "unrecognized YouTube URL pattern: {}{}",
        host, path
    )))
}

/// Extract a YouTube playlist id from a `youtube.com/playlist?list=…` URL.
/// Returns the id without any URL prefix.
pub fn extract_playlist_id(input: &str, max_url_length: usize) -> AppResult<String> {
    if input.len() > max_url_length {
        return Err(AppError::InvalidUrl(format!(
            "URL exceeds maximum length of {} characters",
            max_url_length
        )));
    }
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidUrl("URL is empty".to_string()));
    }
    if trimmed.starts_with("file://") {
        return Err(AppError::InvalidUrl(
            "file:// URLs are not allowed".to_string(),
        ));
    }
    if trimmed.contains("://")
        && !trimmed.starts_with("http://")
        && !trimmed.starts_with("https://")
    {
        return Err(AppError::InvalidUrl(
            "only http(s) URLs are supported".to_string(),
        ));
    }
    let lower = trimmed.to_lowercase();
    if lower.starts_with("localhost") || lower.starts_with("127.") || lower.starts_with("0.0.0.0") {
        return Err(AppError::InvalidUrl(
            "localhost URLs are not allowed".to_string(),
        ));
    }

    let url = Url::parse(trimmed).map_err(|e| AppError::InvalidUrl(e.to_string()))?;
    let host = url
        .host_str()
        .ok_or_else(|| AppError::InvalidUrl("URL has no host".to_string()))?
        .to_lowercase();
    if !is_youtube_host(&host) {
        return Err(AppError::InvalidUrl(format!(
            "URL is not a YouTube domain: {}",
            host
        )));
    }
    let path = url.path().to_lowercase();
    if !path.starts_with("/playlist") {
        return Err(AppError::UnsupportedUrlType(
            "only youtube.com/playlist URLs are supported".to_string(),
        ));
    }
    let list = url
        .query_pairs()
        .find(|(k, _)| k == "list")
        .map(|(_, v)| v.into_owned());
    match list {
        Some(id) if is_valid_playlist_id(&id) => Ok(id),
        _ => Err(AppError::InvalidUrl(
            "missing or invalid list parameter".to_string(),
        )),
    }
}

fn is_valid_playlist_id(s: &str) -> bool {
    // YouTube playlist ids are 2 chars + alphanumerics/_/- (PL…, UU…, LL…, FL…)
    let len = s.len();
    (2..=64).contains(&len)
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Public helper used by the playlist flow to filter yt-dlp entries that look
/// like video ids but don't match the standard 11-char format (e.g. some
/// navigation entries, "UP next" placeholders).
pub fn is_valid_video_id_for_playlist(s: &str) -> bool {
    is_valid_video_id(s)
}

fn is_youtube_host(host: &str) -> bool {
    matches!(
        host,
        "youtube.com" | "www.youtube.com" | "youtu.be" | "m.youtube.com"
    )
}

fn is_valid_video_id(s: &str) -> bool {
    s.len() == VIDEO_ID_LEN
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn check_video_id(id: &str) -> AppResult<String> {
    if is_valid_video_id(id) {
        Ok(id.to_string())
    } else {
        Err(AppError::InvalidUrl(format!(
            "invalid YouTube video id: {}",
            id
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ex(s: &str) -> AppResult<String> {
        extract_video_id(s, 2048)
    }

    #[test]
    fn watch_url() {
        assert_eq!(
            ex("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );
    }

    #[test]
    fn watch_url_no_www() {
        assert_eq!(
            ex("https://youtube.com/watch?v=dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );
    }

    #[test]
    fn youtu_be() {
        assert_eq!(ex("https://youtu.be/dQw4w9WgXcQ").unwrap(), "dQw4w9WgXcQ");
    }

    #[test]
    fn youtu_be_with_query() {
        assert_eq!(
            ex("https://youtu.be/dQw4w9WgXcQ?t=42").unwrap(),
            "dQw4w9WgXcQ"
        );
    }

    #[test]
    fn shorts() {
        assert_eq!(
            ex("https://www.youtube.com/shorts/dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );
        assert_eq!(
            ex("https://youtube.com/shorts/dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );
    }

    #[test]
    fn embed() {
        assert_eq!(
            ex("https://www.youtube.com/embed/dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );
    }

    #[test]
    fn rejects_playlist_for_video_endpoint() {
        // Playlists are no longer valid for /api/transcripts; they are
        // accepted only by /api/playlists.
        let err = ex("https://www.youtube.com/playlist?list=PLabc1234567").unwrap_err();
        // Either InvalidUrl (unrecognized pattern) or UnsupportedUrlType
        // is acceptable here; we just want the call to fail.
        assert!(matches!(err.code(), "INVALID_URL" | "UNSUPPORTED_URL_TYPE"));
    }

    #[test]
    fn rejects_channel() {
        let err = ex("https://www.youtube.com/channel/UCabc").unwrap_err();
        assert_eq!(err.code(), "UNSUPPORTED_URL_TYPE");
    }

    #[test]
    fn rejects_handle() {
        let err = ex("https://www.youtube.com/@example").unwrap_err();
        assert_eq!(err.code(), "UNSUPPORTED_URL_TYPE");
    }

    #[test]
    fn rejects_non_youtube() {
        let err = ex("https://example.com/watch?v=abc").unwrap_err();
        assert_eq!(err.code(), "INVALID_URL");
    }

    #[test]
    fn rejects_empty() {
        let err = ex("").unwrap_err();
        assert_eq!(err.code(), "INVALID_URL");
    }

    #[test]
    fn rejects_file_scheme() {
        let err = ex("file:///etc/passwd").unwrap_err();
        assert_eq!(err.code(), "INVALID_URL");
    }

    #[test]
    fn rejects_localhost() {
        let err = ex("http://localhost:8080/foo").unwrap_err();
        assert_eq!(err.code(), "INVALID_URL");
    }

    #[test]
    fn rejects_bad_id() {
        let err = ex("https://youtu.be/short").unwrap_err();
        assert_eq!(err.code(), "INVALID_URL");
    }

    #[test]
    fn rejects_too_long() {
        let err = ex(&format!("https://youtu.be/{}", "a".repeat(20))).unwrap_err();
        // Length is fine for url itself; this should fail as invalid id
        assert_eq!(err.code(), "INVALID_URL");
    }

    #[test]
    fn rejects_search_url() {
        let err = ex("https://www.youtube.com/results?search_query=hi").unwrap_err();
        assert_eq!(err.code(), "UNSUPPORTED_URL_TYPE");
    }

    #[test]
    fn canonical() {
        assert_eq!(
            canonical_url("dQw4w9WgXcQ"),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
    }

    fn pl(s: &str) -> AppResult<String> {
        extract_playlist_id(s, 2048)
    }

    #[test]
    fn playlist_basic() {
        assert_eq!(
            pl("https://www.youtube.com/playlist?list=PLrAXtmErZgOcM7H5OKN7aRGYvFAtJPu9j").unwrap(),
            "PLrAXtmErZgOcM7H5OKN7aRGYvFAtJPu9j"
        );
    }

    #[test]
    fn playlist_no_www() {
        assert_eq!(
            pl("https://youtube.com/playlist?list=PL1234567890").unwrap(),
            "PL1234567890"
        );
    }

    #[test]
    fn playlist_rejects_video_url() {
        let err = pl("https://www.youtube.com/watch?v=abc12345678").unwrap_err();
        assert_eq!(err.code(), "UNSUPPORTED_URL_TYPE");
    }

    #[test]
    fn playlist_rejects_missing_list() {
        let err = pl("https://www.youtube.com/playlist").unwrap_err();
        assert_eq!(err.code(), "INVALID_URL");
    }

    #[test]
    fn playlist_rejects_non_youtube() {
        let err = pl("https://example.com/playlist?list=PLabc").unwrap_err();
        assert_eq!(err.code(), "INVALID_URL");
    }

    #[test]
    fn playlist_rejects_bad_chars() {
        let err = pl("https://www.youtube.com/playlist?list=PL%20bad%20id").unwrap_err();
        assert_eq!(err.code(), "INVALID_URL");
    }
}
