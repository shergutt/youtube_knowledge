import { useEffect, useState } from "react";
import { showToast } from "./Toast";

interface TranscriptPreviewProps {
  downloadUrl: string;
  outputFilename: string;
}

export function TranscriptPreview({
  downloadUrl,
  outputFilename,
}: TranscriptPreviewProps) {
  const [text, setText] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  useEffect(() => {
    let cancelled = false;
    setText(null);
    setError(null);
    setLoading(true);
    fetch(downloadUrl)
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.text();
      })
      .then((t) => {
        if (!cancelled) setText(t);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [downloadUrl]);

  const lines = text ? text.split(/\r?\n/) : [];
  const totalChars = text?.length ?? 0;
  const wordCount = text ? (text.trim().match(/\S+/g)?.length ?? 0) : 0;
  const filtered = query
    ? lines.filter((l) => l.toLowerCase().includes(query.toLowerCase()))
    : lines;

  async function copyAll() {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      showToast("Transcript copied to clipboard", "success");
    } catch {
      showToast("Copy failed — your browser may block it", "error");
    }
  }

  return (
    <div className="transcript-preview">
      <div className="preview-header">
        <h3>Transcript preview</h3>
        <div className="preview-actions">
          <input
            type="search"
            className="preview-search"
            placeholder="Filter lines…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            disabled={loading || !!error}
          />
          <button
            className="ghost"
            onClick={copyAll}
            disabled={loading || !!error}
          >
            Copy
          </button>
        </div>
      </div>
      <p className="preview-stats">
        <span>{outputFilename}</span>
        <span>{wordCount.toLocaleString()} words</span>
        <span>{totalChars.toLocaleString()} chars</span>
        {query && <span>{filtered.length} / {lines.length} lines</span>}
      </p>
      {loading && (
        <div className="preview-skeleton">
          {Array.from({ length: 8 }).map((_, i) => (
            <div className="skeleton-line" key={i} style={{ width: `${60 + ((i * 13) % 35)}%` }} />
          ))}
        </div>
      )}
      {error && <p className="preview-error">Could not load preview: {error}</p>}
      {!loading && !error && (
        <pre className="preview-text">
          {filtered.length === 0 ? (
            <em>(no matching lines)</em>
          ) : (
            filtered.join("\n")
          )}
        </pre>
      )}
    </div>
  );
}
