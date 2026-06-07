import { useEffect, useMemo, useRef, useState } from "react";
import type { AnalysisPurpose } from "../types/api";
import { ApiError } from "../api/client";
import { pollAnalyses, transcriptsApi } from "../api/transcripts";
import { showToast } from "./Toast";

interface AnalyzerPanelProps {
  jobId: string;
  videoId: string;
  title: string;
  transcriptLanguage: string;
  analyzerEnabled: boolean;
  onError: (msg: { message: string; code?: string }) => void;
  /** Called with every freshly-completed analysis. Multiple calls can fire
   *  in quick succession when several analyses finish in the same poll tick. */
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

const PURPOSE_LABEL: Record<AnalysisPurpose, string> = PURPOSES.reduce(
  (acc, p) => {
    acc[p.value] = p.label;
    return acc;
  },
  {} as Record<AnalysisPurpose, string>,
);
// Re-export so the parent (SinglePage) can read the same map if it wants.
export { PURPOSE_LABEL };

export function AnalyzerPanel({
  jobId,
  transcriptLanguage,
  analyzerEnabled,
  onError,
  onComplete,
}: AnalyzerPanelProps) {
  const [selected, setSelected] = useState<Set<AnalysisPurpose>>(
    () => new Set<AnalysisPurpose>(["summary"]),
  );
  const [customPrompt, setCustomPrompt] = useState("");
  const [outputLanguage, setOutputLanguage] = useState("en");
  const [generating, setGenerating] = useState(false);
  const [progress, setProgress] = useState<
    Record<AnalysisPurpose, { status: string; pct: number }>
  >({} as Record<AnalysisPurpose, { status: string; pct: number }>);
  const abortRef = useRef<AbortController | null>(null);

  useEffect(() => {
    return () => {
      abortRef.current?.abort();
    };
  }, []);

  useEffect(() => {
    setProgress(
      {} as Record<AnalysisPurpose, { status: string; pct: number }>,
    );
    setGenerating(false);
    abortRef.current?.abort();
  }, [jobId]);

  if (!analyzerEnabled) {
    return (
      <div className="analyzer-panel disabled">
        <h3>AI analysis</h3>
        <p className="hint">
          The analyzer is not configured on the server. Set the
          <code> MINIMAX_API_KEY </code>environment variable and restart the
          backend to enable it.
        </p>
      </div>
    );
  }

  function togglePurpose(p: AnalysisPurpose) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(p)) next.delete(p);
      else next.add(p);
      return next;
    });
  }

  async function handleGenerate() {
    onError({ message: "" });
    const purposes = PURPOSES.filter((p) => selected.has(p.value)).map(
      (p) => p.value,
    );
    if (purposes.length === 0) return;
    const specs = purposes.map((purpose) => ({
      purpose,
      output_language: outputLanguage,
      custom_prompt: purpose === "custom" ? customPrompt : undefined,
    }));
    const initial: Record<AnalysisPurpose, { status: string; pct: number }> =
      {} as Record<AnalysisPurpose, { status: string; pct: number }>;
    for (const p of purposes) initial[p] = { status: "queued", pct: 5 };
    setProgress(initial);
    setGenerating(true);
    abortRef.current?.abort();
    const ctrl = new AbortController();
    abortRef.current = ctrl;
    try {
      const created = await transcriptsApi.analyze({
        job_id: jobId,
        specs,
        output_language: outputLanguage,
        custom_prompt: customPrompt,
      });
      const ids = created.analysis_ids;
      const final = await pollAnalyses(
        ids,
        1500,
        ctrl.signal,
        (snapshot) => {
          setProgress((prev) => {
            const next = { ...prev };
            for (let i = 0; i < ids.length; i++) {
              const s = snapshot[ids[i]];
              if (s) {
                next[purposes[i]] = {
                  status: s.status,
                  pct: s.progress ?? (s.status === "running" ? 25 : 100),
                };
              }
            }
            return next;
          });
        },
      );
      for (const id of ids) {
        const r = final[id];
        if (r) onComplete(r);
      }
    } catch (e) {
      if ((e as Error).name === "AbortError") return;
      onError(toAppError(e));
      showToast("Analysis failed", "error");
    } finally {
      setGenerating(false);
    }
  }

  const needsCustomPrompt = selected.has("custom");
  const canGenerate =
    !generating &&
    selected.size > 0 &&
    (!needsCustomPrompt || customPrompt.trim().length > 0);

  const selectedPurposes = useMemo(
    () => PURPOSES.filter((p) => selected.has(p.value)),
    [selected],
  );

  const progressValues = Object.values(progress);
  const progressDone = progressValues.filter(
    (p) => p.status === "completed" || p.status === "failed",
  ).length;
  const progressRunning = progressValues.filter(
    (p) => p.status === "running" || p.status === "queued",
  ).length;
  const hasProgress = progressValues.length > 0;

  return (
    <div className="analyzer-panel">
      <h3>AI analysis (MiniMax-M3)</h3>
      <p className="hint">
        Pick one or more goals — each runs in parallel against the same
        transcript. Output language defaults to <code>en</code> but you can
        ask for any language.
      </p>

      <div className="analysis-purpose">
        <fieldset disabled={generating}>
          <legend>Goals (multi-select)</legend>
          {PURPOSES.map((p) => {
            const isSelected = selected.has(p.value);
            return (
              <label
                key={p.value}
                className={isSelected ? "selected" : undefined}
              >
                <input
                  type="checkbox"
                  checked={isSelected}
                  onChange={() => togglePurpose(p.value)}
                />
                <span className="label">{p.label}</span>
                <span className="hint">{p.description}</span>
              </label>
            );
          })}
        </fieldset>
        {needsCustomPrompt && (
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
          {generating
            ? `Analyzing ${progressDone}/${selectedPurposes.length}…`
            : selectedPurposes.length > 1
              ? `Generate ${selectedPurposes.length} analyses in parallel`
              : "Generate analysis"}
        </button>
        {generating && hasProgress && (
          <div className="analysis-progress-list">
            {selectedPurposes.map((p) => {
              const row = progress[p.value];
              return (
                <span key={p.value} className="muted small inline-progress">
                  {p.label}
                  {row ? `: ${row.status} · ${row.pct}%` : ""}
                </span>
              );
            })}
            {progressRunning > 0 && (
              <p className="muted small">
                {progressRunning} of {progressValues.length} still running
              </p>
            )}
          </div>
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
