import { useEffect, useMemo, useState } from "react";
import { renderMarkdown } from "../utils/markdown";
import { showToast } from "./Toast";

interface MarkdownPreviewProps {
  downloadUrl: string;
  outputFilename: string;
  purposeLabel: string;
}

export function MarkdownPreview({
  downloadUrl,
  outputFilename,
  purposeLabel,
}: MarkdownPreviewProps) {
  const [text, setText] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<"rendered" | "source">("rendered");

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

  const html = useMemo(() => (text ? renderMarkdown(text) : ""), [text]);

  async function copy() {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      showToast("Markdown copied to clipboard", "success");
    } catch {
      showToast("Copy failed — your browser may block it", "error");
    }
  }

  return (
    <div className="markdown-preview">
      <div className="preview-header">
        <h3>
          {purposeLabel} <span className="muted">· {outputFilename}</span>
        </h3>
        <div className="preview-actions">
          <div className="tab-group" role="tablist">
            <button
              role="tab"
              aria-selected={tab === "rendered"}
              className={tab === "rendered" ? "tab active" : "tab"}
              onClick={() => setTab("rendered")}
            >
              Rendered
            </button>
            <button
              role="tab"
              aria-selected={tab === "source"}
              className={tab === "source" ? "tab active" : "tab"}
              onClick={() => setTab("source")}
            >
              Source
            </button>
          </div>
          <button className="ghost" onClick={copy} disabled={loading || !!error}>
            Copy
          </button>
        </div>
      </div>
      {loading && (
        <div className="preview-skeleton">
          <div className="skeleton-line" style={{ width: "60%" }} />
          <div className="skeleton-line" style={{ width: "90%" }} />
          <div className="skeleton-line" style={{ width: "75%" }} />
          <div className="skeleton-line" style={{ width: "85%" }} />
          <div className="skeleton-line" style={{ width: "50%" }} />
        </div>
      )}
      {error && <p className="preview-error">Could not load preview: {error}</p>}
      {!loading && !error && tab === "rendered" && (
        <div className="markdown-body" dangerouslySetInnerHTML={{ __html: html }} />
      )}
      {!loading && !error && tab === "source" && (
        <pre className="preview-text">{text}</pre>
      )}
    </div>
  );
}
