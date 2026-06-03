import { useEffect, useRef, useState } from "react";
import type { AnalysisPurpose } from "../types/api";
import { ApiError } from "../api/client";
import { pollAnalysis, transcriptsApi } from "../api/transcripts";
import { showToast } from "./Toast";

interface AnalyzerPanelProps {
  jobId: string;
  videoId: string;
  title: string;
  transcriptLanguage: string;
  analyzerEnabled: boolean;
  onError: (msg: { message: string; code?: string }) => void;
  onComplete: (resp: import("../types/api").AnalysisJobResponse) => void;
}

const PURPOSES: { value: AnalysisPurpose; label: string; description: string }[] = [
  {
    value: "summary",
    label: "Summary",
    description: "Overview + key points + notable quotes + conclusion.",
  },
  {
    value: "study_notes",
    label: "Study notes",
    description: "Topics, definitions, Q&A, flashcards, further study.",
  },
  {
    value: "key_takeaways",
    label: "Key takeaways",
    description: "Insights grouped by theme, plus counterpoints.",
  },
  {
    value: "action_items",
    label: "Action items",
    description: "Decisions, tasks with owners and deadlines, follow-ups.",
  },
  {
    value: "blog_post",
    label: "Blog post",
    description: "Rewritten as an engaging article with intro and conclusion.",
  },
  {
    value: "tutorial",
    label: "Tutorial",
    description: "Step-by-step with prerequisites, examples, troubleshooting.",
  },
  {
    value: "custom",
    label: "Custom goal",
    description: "Provide your own instruction (1-2 sentences work best).",
  },
];

// Quick-pick output languages. The label is the chip text, the value is the
// language code sent to the backend (BCP-47 short codes are accepted).
const QUICK_OUTPUT_LANGUAGES: { code: string; label: string; full: string }[] = [
  { code: "en", label: "EN", full: "English" },
  { code: "es", label: "ES", full: "Spanish" },
  { code: "pt", label: "PT", full: "Portuguese" },
  { code: "fr", label: "FR", full: "French" },
  { code: "de", label: "DE", full: "German" },
  { code: "it", label: "IT", full: "Italian" },
  { code: "ja", label: "JA", full: "Japanese" },
];

export function AnalyzerPanel({
  jobId,
  transcriptLanguage,
  analyzerEnabled,
  onError,
  onComplete,
}: AnalyzerPanelProps) {
  const [purpose, setPurpose] = useState<AnalysisPurpose>("summary");
  const [customPrompt, setCustomPrompt] = useState("");
  const [outputLanguage, setOutputLanguage] = useState("en");
  const [generating, setGenerating] = useState(false);
  const [progress, setProgress] = useState<{ status: string; pct: number } | null>(
    null,
  );
  const abortRef = useRef<AbortController | null>(null);

  useEffect(() => {
    return () => {
      abortRef.current?.abort();
    };
  }, []);

  useEffect(() => {
    setProgress(null);
    setGenerating(false);
    abortRef.current?.abort();
  }, [jobId]);

  if (!analyzerEnabled) {
    return (
      <div className="analyzer-panel disabled">
        <h3>AI analysis</h3>
        <p className="hint">
          The analyzer is not configured on the server. Set the
          <code> MINIMAX_API_KEY </code> environment variable and restart the
          backend to enable it.
        </p>
      </div>
    );
  }

  async function handleGenerate() {
    onError({ message: "" });
    setProgress({ status: "queued", pct: 5 });
    setGenerating(true);
    abortRef.current?.abort();
    const ctrl = new AbortController();
    abortRef.current = ctrl;
    try {
      const created = await transcriptsApi.analyze({
        job_id: jobId,
        purpose,
        custom_prompt: purpose === "custom" ? customPrompt : undefined,
        output_language: outputLanguage,
      });
      const final = await pollAnalysis(
        created.analysis_id,
        1500,
        ctrl.signal,
        (s) =>
          setProgress({
            status: s.status,
            pct: s.progress ?? (s.status === "running" ? 25 : 100),
          }),
      );
      onComplete(final);
    } catch (e) {
      if ((e as Error).name === "AbortError") return;
      onError(toAppError(e));
      showToast("Analysis failed", "error");
    } finally {
      setGenerating(false);
    }
  }

  const canGenerate =
    !generating &&
    (purpose !== "custom" || customPrompt.trim().length > 0);

  return (
    <div className="analyzer-panel">
      <h3>AI analysis (MiniMax-M3)</h3>
      <p className="hint">
        Turn the transcript into a structured markdown document. Output
        language defaults to <code>en</code> but you can ask for any language.
      </p>

      <div className="analysis-purpose">
        <fieldset disabled={generating}>
          <legend>Goal</legend>
          {PURPOSES.map((p) => (
            <label key={p.value}>
              <input
                type="radio"
                name="analysis_purpose"
                value={p.value}
                checked={purpose === p.value}
                onChange={() => setPurpose(p.value)}
              />
              <span className="label">{p.label}</span>
              <span className="hint">{p.description}</span>
            </label>
          ))}
        </fieldset>
        {purpose === "custom" && (
          <textarea
            value={customPrompt}
            onChange={(e) => setCustomPrompt(e.target.value)}
            placeholder="What do you want the AI to do with this transcript?"
            rows={3}
            disabled={generating}
            maxLength={2000}
          />
        )}
      </div>

      <div className="analysis-options">
        <label>
          <span>Output language</span>
          <input
            type="text"
            value={outputLanguage}
            onChange={(e) => setOutputLanguage(e.target.value.toLowerCase())}
            placeholder="en, es, zh, fr, de, ja…"
            disabled={generating}
            maxLength={16}
          />
        </label>
        <div
          className="chip-row language-chips"
          role="group"
          aria-label="Quick output language"
        >
          {QUICK_OUTPUT_LANGUAGES.map((q) => {
            const active = outputLanguage === q.code;
            return (
              <button
                key={q.code}
                type="button"
                className={active ? "chip active" : "chip"}
                onClick={() => setOutputLanguage(q.code)}
                disabled={generating}
                title={q.full}
                aria-pressed={active}
              >
                {q.label}
              </button>
            );
          })}
        </div>
        <p className="hint">
          Transcript language: <code>{transcriptLanguage}</code>. The output
          document will be written in <code>{outputLanguage || "en"}</code>
          {isSpanishOutput(outputLanguage)
            ? " — Spanish accents (á, é, í, ó, ú, ñ) and idioms will be preserved."
            : "."}
        </p>
      </div>

      <div className="actions">
        <button
          className="primary"
          onClick={handleGenerate}
          disabled={!canGenerate}
        >
          {generating ? "Analyzing…" : "Generate analysis"}
        </button>
        {generating && progress && (
          <p className="muted small inline-progress">
            {progress.status} · {progress.pct}%
          </p>
        )}
      </div>
    </div>
  );
}

function toAppError(e: unknown): { message: string; code?: string } {
  if (e instanceof ApiError) {
    return { message: e.message, code: e.code };
  }
  if (e instanceof Error) {
    return { message: e.message };
  }
  return { message: "Unknown error" };
}

function isSpanishOutput(code: string): boolean {
  const trimmed = code.trim().toLowerCase();
  if (!trimmed) return false;
  const primary = trimmed.split(/[-_]/)[0] ?? "";
  return primary === "es";
}
