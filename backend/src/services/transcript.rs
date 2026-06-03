use std::path::{Path, PathBuf};

use crate::errors::{AppError, AppResult};
use crate::models::api::{CaptionSource, OutputFormat};
use crate::models::transcript::{TranscriptJson, TranscriptSegment};
use crate::services::ytdlp::{SubtitleMode, YtDlpService};
use crate::utils::slug::themed_base;

#[derive(Debug, Clone, Copy)]
pub enum SubtitleSourceResult {
    Manual,
    Auto,
}

pub struct TranscriptService {
    pub ytdlp: YtDlpService,
}

impl TranscriptService {
    pub fn new(ytdlp: YtDlpService) -> Self {
        Self { ytdlp }
    }

    /// Run yt-dlp based on the requested caption source and produce a transcript
    /// file inside `job_dir`. Returns the produced file path and the source that
    /// succeeded.
    pub async fn generate(
        &self,
        video_id: &str,
        title: &str,
        language: &str,
        source: CaptionSource,
        format: OutputFormat,
        job_dir: &Path,
    ) -> AppResult<(PathBuf, SubtitleSourceResult)> {
        match source {
            CaptionSource::Manual => {
                let (path, src) = self
                    .try_one(
                        video_id,
                        title,
                        language,
                        SubtitleMode::Manual,
                        format,
                        job_dir,
                    )
                    .await?;
                Ok((path, src))
            }
            CaptionSource::Auto => {
                let (path, src) = self
                    .try_one(
                        video_id,
                        title,
                        language,
                        SubtitleMode::Auto,
                        format,
                        job_dir,
                    )
                    .await?;
                Ok((path, src))
            }
            CaptionSource::Best => {
                match Box::pin(self.try_one(
                    video_id,
                    title,
                    language,
                    SubtitleMode::Manual,
                    format,
                    job_dir,
                ))
                .await
                {
                    Ok(v) => Ok(v),
                    Err(AppError::NoCaptionsFound) => {
                        let (path, _) = self
                            .try_one(
                                video_id,
                                title,
                                language,
                                SubtitleMode::Auto,
                                format,
                                job_dir,
                            )
                            .await?;
                        Ok((path, SubtitleSourceResult::Auto))
                    }
                    Err(e) => Err(e),
                }
            }
        }
    }

    async fn try_one(
        &self,
        video_id: &str,
        title: &str,
        language: &str,
        mode: SubtitleMode,
        format: OutputFormat,
        job_dir: &Path,
    ) -> AppResult<(PathBuf, SubtitleSourceResult)> {
        let sub_format_hint = match format {
            OutputFormat::Vtt => "vtt",
            OutputFormat::Srt => "srt",
            OutputFormat::Txt | OutputFormat::Json => "vtt",
        };

        // Clean any prior run
        if job_dir.exists() {
            let _ = std::fs::remove_dir_all(job_dir);
        }
        std::fs::create_dir_all(job_dir)
            .map_err(|e| AppError::Internal(format!("create job dir: {}", e)))?;

        let files = self
            .ytdlp
            .download_subtitles(video_id, language, mode, sub_format_hint, job_dir)
            .await?;

        if files.is_empty() {
            return Err(AppError::NoCaptionsFound);
        }

        // Pick a file. Prefer: requested format, then VTT, then SRT, then others.
        let primary = files
            .iter()
            .find(|f| {
                f.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case(format.extension()))
                    .unwrap_or(false)
            })
            .cloned()
            .or_else(|| {
                files
                    .iter()
                    .find(|f| {
                        f.extension()
                            .and_then(|e| e.to_str())
                            .map(|e| e.eq_ignore_ascii_case("vtt"))
                            .unwrap_or(false)
                    })
                    .cloned()
            })
            .or_else(|| {
                files
                    .iter()
                    .find(|f| {
                        f.extension()
                            .and_then(|e| e.to_str())
                            .map(|e| e.eq_ignore_ascii_case("srt"))
                            .unwrap_or(false)
                    })
                    .cloned()
            })
            .unwrap_or_else(|| files[0].clone());

        let base = themed_base(title, video_id);
        let ext = format.extension();

        // Convert if needed
        let output_path = match format {
            OutputFormat::Vtt | OutputFormat::Srt => {
                // Reuse yt-dlp's content but write under the themed filename.
                let target = job_dir.join(format!("{base}.{language}.{ext}"));
                let source_data = std::fs::read(&primary)
                    .map_err(|e| AppError::Internal(format!("read subtitle: {}", e)))?;
                std::fs::write(&target, source_data)
                    .map_err(|e| AppError::Internal(format!("write subtitle: {}", e)))?;
                target
            }
            OutputFormat::Txt => {
                let vtt = ensure_vtt(&primary, &files)?;
                let text =
                    vtt_to_text(&std::fs::read_to_string(&vtt).map_err(|e| {
                        AppError::TranscriptParseFailed(format!("read vtt: {}", e))
                    })?)?;
                let out = job_dir.join(format!("{base}.{language}.txt"));
                std::fs::write(&out, text)
                    .map_err(|e| AppError::Internal(format!("write txt: {}", e)))?;
                out
            }
            OutputFormat::Json => {
                let vtt = ensure_vtt(&primary, &files)?;
                let segments =
                    vtt_to_segments(&std::fs::read_to_string(&vtt).map_err(|e| {
                        AppError::TranscriptParseFailed(format!("read vtt: {}", e))
                    })?)?;
                let payload = TranscriptJson {
                    video_id: video_id.to_string(),
                    language: language.to_string(),
                    source: match mode {
                        SubtitleMode::Manual => "manual".to_string(),
                        SubtitleMode::Auto => "auto".to_string(),
                    },
                    segments,
                };
                let out = job_dir.join(format!("{base}.{language}.json"));
                std::fs::write(
                    &out,
                    serde_json::to_string_pretty(&payload)
                        .map_err(|e| AppError::Internal(format!("serialize json: {}", e)))?,
                )
                .map_err(|e| AppError::Internal(format!("write json: {}", e)))?;
                out
            }
        };

        let result_src = match mode {
            SubtitleMode::Manual => SubtitleSourceResult::Manual,
            SubtitleMode::Auto => SubtitleSourceResult::Auto,
        };

        Ok((output_path, result_src))
    }
}

fn ensure_vtt(primary: &Path, files: &[PathBuf]) -> AppResult<PathBuf> {
    let ext = primary
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if ext == "vtt" {
        return Ok(primary.to_path_buf());
    }
    // Try to find any vtt file in the candidate list
    if let Some(vtt) = files.iter().find(|p| {
        p.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("vtt"))
            .unwrap_or(false)
    }) {
        return Ok(vtt.clone());
    }
    Err(AppError::CaptionFormatUnavailable)
}

/// Parse a VTT file into a list of segments (start, end, text).
pub fn vtt_to_segments(input: &str) -> AppResult<Vec<TranscriptSegment>> {
    let normalized = input.replace("\r\n", "\n");
    let mut segments = Vec::new();
    let mut lines = normalized.lines().peekable();
    let mut in_header = true;

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if in_header {
            if trimmed == "WEBVTT" || trimmed.starts_with("WEBVTT ") || trimmed.is_empty() {
                if trimmed.starts_with("WEBVTT") {
                    // skip until first blank line
                }
                continue;
            } else {
                in_header = false;
            }
        }

        if trimmed.is_empty() {
            continue;
        }

        // Skip NOTE blocks
        if trimmed.starts_with("NOTE") {
            // skip until blank line
            while let Some(next) = lines.peek() {
                if next.trim().is_empty() {
                    break;
                }
                lines.next();
            }
            continue;
        }

        // Detect timing line: either this line is the timing, or the next is.
        let timing_line = if trimmed.contains("-->") {
            trimmed.to_string()
        } else {
            // current line is a cue identifier; consume it and read next
            match lines.next() {
                Some(l) if l.trim().contains("-->") => l.trim().to_string(),
                _ => continue,
            }
        };

        let (start, end) = parse_timing(&timing_line)?;
        let mut text_lines: Vec<String> = Vec::new();
        while let Some(next) = lines.peek() {
            if next.trim().is_empty() {
                lines.next();
                break;
            }
            text_lines.push(next.trim().to_string());
            lines.next();
        }
        let text = strip_vtt_tags(text_lines.join(" "));
        if !text.is_empty() {
            segments.push(TranscriptSegment { start, end, text });
        }
    }
    Ok(dedupe_rolling_cues(segments))
}

/// Drop YouTube auto-caption "rolling" cues.
///
/// YouTube's `json3`/`srv3` auto-captions emit a fresh cue for every word
/// boundary. Each new cue starts exactly when the previous cue ends, and its
/// text is the previous text plus the new word(s). After yt-dlp converts these
/// to VTT, the result is unreadable: dozens of 10ms cues that are strict
/// prefixes of the next cue.
///
/// This pass keeps only the longest "stable" cue and drops the rolling
/// predecessors. A cue is dropped when its `end` timestamp equals the next
/// cue's `start` timestamp AND its text is a strict prefix of the next cue's
/// text.
fn dedupe_rolling_cues(segments: Vec<TranscriptSegment>) -> Vec<TranscriptSegment> {
    if segments.len() < 2 {
        return segments;
    }
    let mut out: Vec<TranscriptSegment> = Vec::with_capacity(segments.len());
    for i in 0..segments.len() {
        let cur = &segments[i];
        let dominated = segments
            .get(i + 1)
            .is_some_and(|next| cur.end == next.start && next.text.starts_with(&cur.text));
        if !dominated {
            out.push(cur.clone());
        }
    }
    out
}

fn parse_timing(line: &str) -> AppResult<(String, String)> {
    let parts: Vec<&str> = line.splitn(2, "-->").collect();
    if parts.len() < 2 {
        return Err(AppError::TranscriptParseFailed(format!(
            "invalid timing line: {}",
            line
        )));
    }
    let start = parts[0].trim();
    let end_full = parts[1].trim();
    // end may have settings after a space, e.g. "00:00:05.000 line:0%"
    let end = end_full.split_whitespace().next().unwrap_or("");
    Ok((start.to_string(), end.to_string()))
}

fn strip_vtt_tags(s: String) -> String {
    // Remove simple <c>, <i>, <b>, <v>, <00:00:00.000> tags
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        if ch == '<' {
            in_tag = true;
            continue;
        }
        if ch == '>' {
            in_tag = false;
            continue;
        }
        if !in_tag {
            out.push(ch);
        }
    }
    out.trim().to_string()
}

pub fn vtt_to_text(input: &str) -> AppResult<String> {
    let segments = vtt_to_segments(input)?;
    let mut out = String::new();
    let mut prev_blank = true;
    for seg in segments {
        if !prev_blank {
            out.push('\n');
        }
        out.push_str(&seg.text);
        prev_blank = false;
    }
    out.push('\n');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_VTT: &str = "WEBVTT - Sample\n\n\
NOTE This is a comment\n\n\
1\n\
00:00:01.000 --> 00:00:04.000\n\
Hello <c>and</c> welcome.\n\n\
2\n\
00:00:04.500 --> 00:00:07.000\n\
<v Speaker>Second line!</v>\n\n\
3\n\
00:00:08.000 --> 00:00:10.000\n\
Third line here.\n";

    #[test]
    fn parse_vtt_segments() {
        let segs = vtt_to_segments(SAMPLE_VTT).unwrap();
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].text, "Hello and welcome.");
        assert_eq!(segs[0].start, "00:00:01.000");
        assert_eq!(segs[0].end, "00:00:04.000");
        assert_eq!(segs[1].text, "Second line!");
        assert_eq!(segs[2].text, "Third line here.");
    }

    #[test]
    fn parse_vtt_text() {
        let text = vtt_to_text(SAMPLE_VTT).unwrap();
        assert!(text.contains("Hello and welcome."));
        assert!(text.contains("Second line!"));
        assert!(text.contains("Third line here."));
        assert!(!text.contains("-->"));
        assert!(!text.contains("WEBVTT"));
    }

    #[test]
    fn strips_crlf() {
        let vtt = "WEBVTT\r\n\r\n1\r\n00:00:01.000 --> 00:00:02.000\r\nHello\r\n";
        let segs = vtt_to_segments(vtt).unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "Hello");
    }

    #[test]
    fn strips_tags() {
        let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\n<i>Hi</i> there <00:00:01.500>\n";
        let segs = vtt_to_segments(vtt).unwrap();
        assert_eq!(segs[0].text, "Hi there");
    }

    /// YouTube auto-captions emit a chain of rolling cues. Each one ends
    /// exactly where the next one starts and is a strict prefix of the next.
    /// The dedupe pass must drop the rolling predecessors and keep only the
    /// longest stable cues.
    #[test]
    fn dedupes_rolling_cues() {
        let vtt = "WEBVTT\n\n\
00:00:02.270 --> 00:00:02.280\n\
microservices bring a lot of benefits\n\n\
00:00:02.280 --> 00:00:04.630\n\
microservices bring a lot of benefits there's no doubt about that but at the\n\n\
00:00:04.630 --> 00:00:04.640\n\
there's no doubt about that but at the\n\n\
00:00:04.640 --> 00:00:06.430\n\
there's no doubt about that but at the same time it gets kind of tricky when\n";
        let segs = vtt_to_segments(vtt).unwrap();
        assert_eq!(segs.len(), 2);
        assert_eq!(
            segs[0].text,
            "microservices bring a lot of benefits there's no doubt about that but at the"
        );
        assert_eq!(
            segs[1].text,
            "there's no doubt about that but at the same time it gets kind of tricky when"
        );
    }

    /// Real (non-rolling) cues that happen to share a common prefix must not
    /// be merged.
    #[test]
    fn keeps_distinct_cues_with_shared_prefix() {
        let vtt = "WEBVTT\n\n\
00:00:01.000 --> 00:00:02.000\n\
Hello there\n\n\
00:00:03.000 --> 00:00:04.000\n\
Hello world\n";
        let segs = vtt_to_segments(vtt).unwrap();
        assert_eq!(segs.len(), 2);
    }
}
