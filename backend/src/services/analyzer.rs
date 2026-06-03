use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::json;
use tracing::{info, warn};

use crate::config::AppConfig;
use crate::errors::{AppError, AppResult};
use crate::models::analysis::AnalysisPurpose;
use crate::utils::slug::themed_base;

/// Result of a successful analysis call.
#[derive(Debug, Clone)]
pub struct AnalysisOutput {
    pub markdown: String,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Clone)]
pub struct AnalyzerService {
    pub config: AppConfig,
    pub client: reqwest::Client,
}

impl AnalyzerService {
    pub fn new(config: AppConfig) -> AppResult<Self> {
        if !config.minimax_configured() {
            return Err(AppError::AnalysisNotConfigured);
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(
                config.analysis_timeout_seconds,
            ))
            .build()
            .map_err(|e| AppError::Internal(format!("build http client: {}", e)))?;
        Ok(Self { config, client })
    }

    /// Read the transcript from disk. Prefers the TXT file (cleanest for LLMs).
    pub fn read_transcript(&self, job_dir: &Path, _language: &str) -> AppResult<String> {
        let entries = std::fs::read_dir(job_dir)
            .map_err(|e| AppError::Internal(format!("read transcript dir: {}", e)))?;
        let mut txts: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("txt"))
                        .unwrap_or(false)
            })
            .collect();
        txts.sort();
        match txts.first() {
            Some(p) => std::fs::read_to_string(p)
                .map_err(|e| AppError::Internal(format!("read transcript: {}", e))),
            None => Err(AppError::TranscriptParseFailed(
                "no .txt transcript found for analysis".to_string(),
            )),
        }
    }

    /// Build the system prompt shared by all purposes plus the per-purpose
    /// instruction.
    pub fn build_system_prompt(
        purpose: AnalysisPurpose,
        output_language: &str,
        custom_prompt: Option<&str>,
    ) -> String {
        let purpose_block = match purpose {
            AnalysisPurpose::Custom => custom_prompt.unwrap_or("").trim().to_string(),
            other => other.instruction().to_string(),
        };

        let language_directive = language_directive(output_language);

        format!(
            "You are a senior content analyst who transforms video transcripts into \
            high-quality, structured markdown documents.\n\n\
            # Task\n\
            {purpose_block}\n\n\
            # Output format\n\
            - Output language: {output_language}. {language_directive}\n\
            - Use GitHub-flavored markdown.\n\
            - Use a top-level `#` title that reflects the document's purpose.\n\
            - Use `##` for major sections and `###` for subsections.\n\
            - Use bullet lists, numbered lists, and tables when they improve clarity.\n\
            - Quote short phrases from the transcript when the wording matters.\n\
            - Do not invent facts that are not supported by the transcript.\n\
            - Do not include a preamble, explanation, or code fence around the output.\n\
            - Return ONLY the markdown document.",
            purpose_block = purpose_block,
            output_language = output_language,
            language_directive = language_directive,
        )
    }

    pub fn build_user_prompt(
        video_id: &str,
        language: &str,
        purpose: AnalysisPurpose,
        transcript: &str,
    ) -> String {
        let header = match purpose {
            AnalysisPurpose::Custom => "Analyze the following video transcript.",
            _ => "Analyze the following video transcript and produce the requested document.",
        };
        let purpose_str = match purpose {
            AnalysisPurpose::Summary => "summary",
            AnalysisPurpose::StudyNotes => "study_notes",
            AnalysisPurpose::KeyTakeaways => "key_takeaways",
            AnalysisPurpose::ActionItems => "action_items",
            AnalysisPurpose::BlogPost => "blog_post",
            AnalysisPurpose::Tutorial => "tutorial",
            AnalysisPurpose::Custom => "custom",
        };
        format!(
            "{header}\n\n\
            Video ID: {video_id}\n\
            Transcript language: {language}\n\
            Purpose: {purpose}\n\n\
            <transcript>\n{transcript}\n</transcript>",
            header = header,
            video_id = video_id,
            language = language,
            purpose = purpose_str,
            transcript = transcript,
        )
    }

    /// Call MiniMax-M3 and return the assistant text.
    pub async fn analyze(
        &self,
        purpose: AnalysisPurpose,
        custom_prompt: Option<&str>,
        video_id: &str,
        transcript_language: &str,
        output_language: &str,
        transcript: &str,
    ) -> AppResult<AnalysisOutput> {
        let system = Self::build_system_prompt(purpose, output_language, custom_prompt);
        let user = Self::build_user_prompt(video_id, transcript_language, purpose, transcript);

        let url = format!(
            "{}/v1/messages",
            self.config.minimax_base_url.trim_end_matches('/')
        );
        let body = json!({
            "model": self.config.minimax_model,
            "system": system,
            "messages": [{
                "role": "user",
                "content": [{"type": "text", "text": user}]
            }],
            "max_tokens": self.config.analysis_max_output_tokens,
            "temperature": self.config.analysis_temperature,
            "top_p": self.config.analysis_top_p,
        });

        info!(
            model = %self.config.minimax_model,
            video_id = %video_id,
            purpose = ?purpose,
            max_tokens = self.config.analysis_max_output_tokens,
            temperature = self.config.analysis_temperature,
            top_p = self.config.analysis_top_p,
            timeout_s = self.config.analysis_timeout_seconds,
            "calling MiniMax analyzer"
        );

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.config.minimax_api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AppError::MiniMaxTimeout
                } else {
                    AppError::MiniMaxRequestFailed(e.to_string())
                }
            })?;

        let status = resp.status();
        let raw = resp
            .text()
            .await
            .map_err(|e| AppError::MiniMaxRequestFailed(e.to_string()))?;

        if !status.is_success() {
            warn!(%status, body = %raw, "MiniMax non-success response");
            return Err(AppError::MiniMaxRequestFailed(format!(
                "status {}: {}",
                status,
                truncate(&raw, 400)
            )));
        }

        let parsed: MessagesResponse = serde_json::from_str(&raw)
            .map_err(|e| AppError::MiniMaxInvalidResponse(format!("parse: {}", e)))?;

        let mut text = String::new();
        for block in parsed.content {
            if block.block_type == "text" {
                if let Some(t) = block.text {
                    text.push_str(&t);
                }
            }
        }
        if text.trim().is_empty() {
            return Err(AppError::MiniMaxInvalidResponse(
                "no text blocks in response".to_string(),
            ));
        }

        Ok(AnalysisOutput {
            markdown: text,
            input_tokens: parsed.usage.as_ref().and_then(|u| u.input_tokens),
            output_tokens: parsed.usage.and_then(|u| u.output_tokens),
        })
    }

    /// Save the analysis to disk and return the file path and a safe filename.
    pub fn save_markdown(
        &self,
        job_dir: &Path,
        video_id: &str,
        title: &str,
        purpose: AnalysisPurpose,
        markdown: &str,
    ) -> AppResult<PathBuf> {
        std::fs::create_dir_all(job_dir)
            .map_err(|e| AppError::Internal(format!("mkdir analysis dir: {}", e)))?;
        let base = themed_base(title, video_id);
        let slug = purpose_slug(purpose);
        let filename = format!("{base}.{slug}.md");
        let path = job_dir.join(&filename);
        std::fs::write(&path, markdown)
            .map_err(|e| AppError::Internal(format!("write analysis: {}", e)))?;
        Ok(path)
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        // Find a safe char boundary
        let mut end = max;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        &s[..end]
    }
}

pub fn purpose_slug(purpose: AnalysisPurpose) -> &'static str {
    match purpose {
        AnalysisPurpose::Summary => "summary",
        AnalysisPurpose::StudyNotes => "study-notes",
        AnalysisPurpose::KeyTakeaways => "takeaways",
        AnalysisPurpose::ActionItems => "actions",
        AnalysisPurpose::BlogPost => "blog",
        AnalysisPurpose::Tutorial => "tutorial",
        AnalysisPurpose::Custom => "custom",
    }
}

/// Extra guidance for the model when writing in a specific target language.
/// English gets no special handling. Spanish and other Romance languages get
/// explicit preservation rules to avoid the model silently translating
/// accents, idioms, or proper nouns.
fn language_directive(lang: &str) -> &'static str {
    let lower = lang.to_ascii_lowercase();
    let primary = lower.split(['-', '_']).next().unwrap_or("");
    match primary {
        "es" => "Write entirely in Spanish. Preserve Spanish punctuation, accents (á, é, í, ó, ú, ñ, ü), and idioms; do not translate them to English. Keep proper nouns, brand names, and code identifiers exactly as they appear in the transcript.",
        "fr" => "Write entirely in French. Preserve French punctuation, accents (é, è, ê, ë, à, â, î, ï, ô, ù, û, ç), and idioms; do not translate them to English. Keep proper nouns, brand names, and code identifiers exactly as they appear in the transcript.",
        "pt" => "Write entirely in Portuguese. Preserve Portuguese punctuation, accents (á, é, í, ó, ú, ã, õ, â, ê, ô, ç), and idioms; do not translate them to English. Keep proper nouns, brand names, and code identifiers exactly as they appear in the transcript.",
        "de" => "Write entirely in German. Preserve German punctuation, umlauts (ä, ö, ü, ß), and idioms; do not translate them to English. Keep proper nouns, brand names, and code identifiers exactly as they appear in the transcript.",
        "it" => "Write entirely in Italian. Preserve Italian punctuation, accents (à, è, é, ì, ò, ù), and idioms; do not translate them to English. Keep proper nouns, brand names, and code identifiers exactly as they appear in the transcript.",
        "ja" => "Write entirely in Japanese. Use natural Japanese phrasing; do not default to English. Keep proper nouns, brand names, and code identifiers exactly as they appear in the transcript.",
        "ko" => "Write entirely in Korean. Use natural Korean phrasing; do not default to English. Keep proper nouns, brand names, and code identifiers exactly as they appear in the transcript.",
        "zh" => "Write entirely in Chinese. Use natural Chinese phrasing; do not default to English. Keep proper nouns, brand names, and code identifiers exactly as they appear in the transcript.",
        "ru" => "Write entirely in Russian. Use the Cyrillic script; do not transliterate. Keep proper nouns, brand names, and code identifiers exactly as they appear in the transcript.",
        "ar" => "Write entirely in Arabic. Use the Arabic script with appropriate right-to-left punctuation; do not transliterate. Keep proper nouns, brand names, and code identifiers exactly as they appear in the transcript.",
        "hi" => "Write entirely in Hindi (Devanagari). Keep proper nouns, brand names, and code identifiers exactly as they appear in the transcript.",
        "" | "en" => "Write in natural English.",
        _ => "Write entirely in the target language; do not default to English. Preserve the original punctuation, accents, and idioms. Keep proper nouns, brand names, and code identifiers exactly as they appear in the transcript.",
    }
}

// Wire-level response types for the Anthropic-compatible Messages API.
#[derive(Debug, Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_contains_task_and_language() {
        let prompt = AnalyzerService::build_system_prompt(AnalysisPurpose::Summary, "es", None);
        assert!(prompt.contains("Output language: es"));
        assert!(prompt.contains("Task"));
        assert!(prompt.contains("Summary") || prompt.to_lowercase().contains("summary"));
        // Should not wrap output in fences
        assert!(!prompt.contains("```"));
    }

    #[test]
    fn system_prompt_includes_spanish_directives() {
        let prompt = AnalyzerService::build_system_prompt(AnalysisPurpose::Summary, "es", None);
        assert!(
            prompt.contains("Spanish"),
            "prompt should call out Spanish as the target language"
        );
        assert!(
            prompt.contains("á") && prompt.contains("ñ"),
            "prompt should remind the model to preserve Spanish accents"
        );
        assert!(
            prompt.to_lowercase().contains("do not translate")
                || prompt.to_lowercase().contains("not translate"),
            "prompt should forbid silent translation of Spanish content"
        );
    }

    #[test]
    fn system_prompt_handles_spanish_regional_variants() {
        // es-MX, es-AR, es-419 should all get the same Spanish directive
        for lang in ["es", "es-ES", "es-MX", "es-AR", "es-419"] {
            let prompt = AnalyzerService::build_system_prompt(AnalysisPurpose::Summary, lang, None);
            assert!(
                prompt.contains("Spanish"),
                "lang={lang} should mention Spanish"
            );
        }
    }

    #[test]
    fn system_prompt_uses_custom_prompt() {
        let prompt = AnalyzerService::build_system_prompt(
            AnalysisPurpose::Custom,
            "en",
            Some("Extract every product name and price."),
        );
        assert!(prompt.contains("Extract every product name and price."));
    }

    #[test]
    fn user_prompt_wraps_transcript() {
        let prompt = AnalyzerService::build_user_prompt(
            "abc12345678",
            "en",
            AnalysisPurpose::Summary,
            "hello world",
        );
        assert!(prompt.contains("Video ID: abc12345678"));
        assert!(prompt.contains("Transcript language: en"));
        assert!(prompt.contains("<transcript>"));
        assert!(prompt.contains("hello world"));
        assert!(prompt.contains("</transcript>"));
    }

    #[test]
    fn purpose_slug_is_stable() {
        assert_eq!(purpose_slug(AnalysisPurpose::StudyNotes), "study-notes");
        assert_eq!(purpose_slug(AnalysisPurpose::KeyTakeaways), "takeaways");
        assert_eq!(purpose_slug(AnalysisPurpose::BlogPost), "blog");
    }
}
