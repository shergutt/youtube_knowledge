import { type FormEvent, useState } from "react";
import { isLikelyYouTubeUrl } from "../utils/validators";

interface UrlInputProps {
  value: string;
  onChange: (v: string) => void;
  onSubmit: () => void;
  onUseDemo: () => void;
  loading: boolean;
}

const DEMO_URL = "https://www.youtube.com/watch?v=jNQXAC9IVRw";

export function UrlInput({
  value,
  onChange,
  onSubmit,
  onUseDemo,
  loading,
}: UrlInputProps) {
  const [touched, setTouched] = useState(false);
  const showError = touched && value.trim().length > 0 && !isLikelyYouTubeUrl(value);

  function handleSubmit(e: FormEvent) {
    e.preventDefault();
    if (!isLikelyYouTubeUrl(value)) {
      setTouched(true);
      return;
    }
    onSubmit();
  }

  return (
    <form className="url-input" onSubmit={handleSubmit}>
      <label htmlFor="url">YouTube video URL</label>
      <div className="row">
        <input
          id="url"
          type="url"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder="https://www.youtube.com/watch?v=…"
          autoComplete="off"
          spellCheck={false}
          disabled={loading}
        />
        <button type="submit" disabled={loading || !isLikelyYouTubeUrl(value)}>
          {loading ? "Checking…" : "Check captions"}
        </button>
      </div>
      <div className="row sub">
        <p className="hint">
          Supported: youtube.com/watch, youtu.be, youtube.com/shorts
        </p>
        <button
          type="button"
          className="ghost"
          onClick={onUseDemo}
          disabled={loading}
          title="Load the first YouTube video ever uploaded"
        >
          Try sample
        </button>
      </div>
      {showError && (
        <p className="hint error">
          Please paste a valid YouTube video URL (watch, youtu.be, or shorts).
        </p>
      )}
    </form>
  );
}

export const DEMO_VIDEO_URL = DEMO_URL;
