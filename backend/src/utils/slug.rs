/// Convert an arbitrary string into a filename-safe ASCII slug.
///
/// - Lowercases ASCII letters.
/// - Replaces any non-alphanumeric run with a single `-`.
/// - Trims leading/trailing `-`.
/// - Caps total length to `max_len` (in chars).
/// - Falls back to `"untitled"` when the result is empty.
pub fn slugify(input: &str, max_len: usize) -> String {
    let mut out = String::with_capacity(input.len().min(max_len + 8));
    let mut last_dash = true;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            if out.len() >= max_len {
                break;
            }
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            if out.len() >= max_len {
                break;
            }
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_end_matches('-');
    if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Build a base name for a generated file: `{title-slug}-{short-id}`.
///
/// The `short_id` is the first 6 characters of the video id, guaranteeing
/// uniqueness across videos that share the same title.
pub fn themed_base(title: &str, video_id: &str) -> String {
    let slug = slugify(title, 60);
    let short_id: String = video_id.chars().take(6).collect();
    format!("{slug}-{short_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_basic() {
        assert_eq!(
            slugify("Microservices Patterns", 60),
            "microservices-patterns"
        );
    }

    #[test]
    fn slug_lowercases() {
        assert_eq!(slugify("Rust Axum Tutorial", 60), "rust-axum-tutorial");
    }

    #[test]
    fn slug_collapses_runs() {
        assert_eq!(slugify("Hello,   World!!!", 60), "hello-world");
    }

    #[test]
    fn slug_trims_dashes() {
        assert_eq!(slugify("---hello---", 60), "hello");
    }

    #[test]
    fn slug_caps_length() {
        let s = "a".repeat(100);
        let out = slugify(&s, 20);
        assert_eq!(out.len(), 20);
    }

    #[test]
    fn slug_falls_back() {
        assert_eq!(slugify("", 60), "untitled");
        assert_eq!(slugify("!!!", 60), "untitled");
        assert_eq!(slugify("---", 60), "untitled");
    }

    #[test]
    fn slug_keeps_digits() {
        assert_eq!(
            slugify("Top 10 Rust Tips 2024", 60),
            "top-10-rust-tips-2024"
        );
    }

    #[test]
    fn themed_base_includes_short_id() {
        let base = themed_base("Microservices Patterns in 2024", "dQw4w9WgXcQ");
        assert!(base.starts_with("microservices-patterns-in-2024-"));
        assert!(base.ends_with("-dQw4w9"));
    }

    #[test]
    fn themed_base_handles_empty_title() {
        let base = themed_base("", "abcdefghijk");
        assert_eq!(base, "untitled-abcdef");
    }
}
