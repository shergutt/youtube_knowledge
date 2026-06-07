import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CaptionSelector } from "../components/CaptionSelector";
import { DownloadButton } from "../components/DownloadButton";
import { ErrorMessage } from "../components/ErrorMessage";
import { FormatSelector } from "../components/FormatSelector";
import { JobStatusView } from "../components/JobStatus";
import { showToast } from "../components/Toast";
import { UrlInput } from "../components/UrlInput";
import { ApiError } from "../api/client";
import { pollPlaylist, playlistsApi } from "../api/playlists";
import {
  recordAnalysis,
  recordTranscript,
  type HistoryEntry,
} from "../utils/history";
import { absoluteUrl } from "../utils/url";
import type {
  AnalysisPurpose,
  CaptionSource,
  HealthResponse,
  OutputFormat,
  PlaylistProbeResponse,
  PlaylistResponse,
} from "../types/api";

type AppError = { message: string; code?: string };

const PURPOSES: { value: AnalysisPurpose; label: string }[] = [
  { value: "summary", label: "Summary" },
  { value: "study_notes", label: "Study notes" },
  { value: "key_takeaways", label: "Key takeaways" },
  { value: "action_items", label: "Action items" },
  { value: "blog_post", label: "Blog post" },
  { value: "tutorial", label: "Tutorial" },
  { value: "custom", label: "Custom" },
];

const PURPOSE_LABEL: Record<string, string> = PURPOSES.reduce(
  (acc, p) => {
    acc[p.value] = p.label;
    return acc;
  },
  {} as Record<string, string>,
);

function abbreviatePurpose(p: AnalysisPurpose): string {
  switch (p) {
    case "summary":
      return "S";
    case "study_notes":
      return "N";
    case "key_takeaways":
      return "K";
    case "action_items":
      return "A";
    case "blog_post":
      return "B";
    case "tutorial":
      return "T";
    case "custom":
      return "C";
    default:
      return "M";
  }
}

export function PlaylistPage({ health }: { health: HealthResponse | null }) {
  const [url, setUrl] = useState("");
  const [probe, setProbe] = useState<PlaylistProbeResponse | null>(null);
  const [probeLoading, setProbeLoading] = useState(false);
  const [error, setError] = useState<AppError | null>(null);

  const [language, setLanguage] = useState("en");
  const [source, setSource] = useState<CaptionSource>("auto");
  const [format, setFormat] = useState<OutputFormat>("txt");
  const [enableAnalysis, setEnableAnalysis] = useState(true);
  const [purposes, setPurposes] = useState<Set<AnalysisPurpose>>(
    () => new Set<AnalysisPurpose>(["summary"]),
  );
  const [outputLanguage, setOutputLanguage] = useState("en");
  const [customPrompt, setCustomPrompt] = useState("");

  const [playlist, setPlaylist] = useState<PlaylistResponse | null>(null);
  const [generating, setGenerating] = useState(false);
  const pollAbort = useRef<AbortController | null>(null);

  useEffect(() => {
    return () => {
      pollAbort.current?.abort();
    };
  }, []);

  // Reset when a new probe is loaded
  useEffect(() => {
    setPlaylist(null);
    pollAbort.current?.abort();
  }, [probe?.playlist_id]);

  const handleProbe = useCallback(async () => {
    setError(null);
    setProbe(null);
    setPlaylist(null);
    setProbeLoading(true);
    try {
      const result = await playlistsApi.probe({ url });
      setProbe(result);
      showToast(`Loaded "${result.title}" (${result.video_count} videos)`, "success");
    } catch (e) {
      setError(toAppError(e));
    } finally {
      setProbeLoading(false);
    }
  }, [url]);

  const togglePurpose = useCallback((p: AnalysisPurpose) => {
    setPurposes((prev) => {
      const next = new Set(prev);
      if (next.has(p)) next.delete(p);
      else next.add(p);
      return next;
    });
  }, []);

  const handleProcess = useCallback(async () => {
    if (!probe) return;
    setError(null);
    setPlaylist(null);
    setGenerating(true);
    pollAbort.current?.abort();
    const ctrl = new AbortController();
    pollAbort.current = ctrl;
    const orderedPurposes = PURPOSES.filter((p) => purposes.has(p.value)).map(
      (p) => p.value,
    );
    const analysisSpecs = orderedPurposes.map((p) => ({
      purpose: p,
      custom_prompt: p === "custom" ? customPrompt : undefined,
      output_language: outputLanguage,
    }));
    try {
      const created = await playlistsApi.create({
        url,
        language,
        caption_source: source,
        output_format: format,
        analysis: enableAnalysis && analysisSpecs.length > 0 ? analysisSpecs : undefined,
      });
      const final = await pollPlaylist(
        created.playlist_id,
        3000,
        ctrl.signal,
        (s) => setPlaylist(s),
      );
      setPlaylist(final);
      // Record each child to history
      for (const c of final.children) {
        if (c.stage === "completed" && c.transcript_job_id && c.transcript_filename) {
          const he: Omit<HistoryEntry, "analyses" | "created_at"> = {
            job_id: c.transcript_job_id,
            video_id: c.video_id,
            title: c.title,
            language,
            output_format: format,
            output_filename: c.transcript_filename,
            download_url: c.transcript_download_url ?? `/api/downloads/${c.transcript_job_id}`,
            expires_at: final.expires_at,
          };
          recordTranscript(he);
          for (const a of c.analyses) {
            if (a.status === "completed" && a.filename) {
              recordAnalysis({
                job_id: c.transcript_job_id,
                analysis: {
                  analysis_id: a.analysis_id,
                  purpose: a.purpose,
                  output_language: a.output_language,
                  output_filename: a.filename,
                  created_at: new Date().toISOString(),
                  expires_at: final.expires_at,
                },
              });
            }
          }
        }
      }
      showToast(
        `Playlist complete: ${final.completed} / ${final.total} done`,
        final.failed > 0 ? "info" : "success",
      );
    } catch (e) {
      if ((e as Error).name === "AbortError") return;
      setError(toAppError(e));
    } finally {
      setGenerating(false);
    }
  }, [
    probe,
    url,
    language,
    source,
    format,
    enableAnalysis,
    purposes,
    outputLanguage,
    customPrompt,
  ]);

  const canProcess = useMemo(() => {
    if (!probe || generating) return false;
    if (language.length === 0) return false;
    if (enableAnalysis) {
      if (purposes.size === 0) return false;
      if (purposes.has("custom") && customPrompt.trim().length === 0) return false;
    }
    return true;
  }, [probe, generating, language, enableAnalysis, purposes, customPrompt]);

  return (
    <main>
      <section className="card">
        <UrlInput
          value={url}
          onChange={setUrl}
          onSubmit={handleProbe}
          onUseDemo={() => {
            // A small public channel uploads playlist for the demo button
            setUrl(
              "https://www.youtube.com/playlist?list=UU_x5XG1OV2P6uZZ5FSM9Ttw",
            );
            setTimeout(handleProbe, 0);
          }}
          loading={probeLoading}
        />
        <ErrorMessage error={error} />
      </section>

      {probe && (
        <section className="card">
          <div className="playlist-info">
            <h2 title={probe.title}>{probe.title}</h2>
            <p className="meta">
              <span className="meta-chip">ID: {probe.playlist_id}</span>
              <span className="meta-chip">{probe.video_count} videos</span>
            </p>
          </div>
        </section>
      )}

      {probe && (
        <section className="card">
          <CaptionSelector
            source={source}
            onSourceChange={setSource}
            language={language}
            onLanguageChange={setLanguage}
            availableLanguages={["en"]}
            hasManual={true}
            hasAuto={true}
            disabled={generating}
          />
          <FormatSelector
            value={format}
            onChange={setFormat}
            disabled={generating}
          />

          <div className="analysis-options" style={{ marginTop: "12px" }}>
            <label>
              <input
                type="checkbox"
                checked={enableAnalysis}
                onChange={(e) => setEnableAnalysis(e.target.checked)}
                disabled={generating}
              />
              <span>Run AI analysis on every video</span>
            </label>
            {enableAnalysis && (
              <>
                <fieldset
                  className="analysis-purpose"
                  disabled={
                    generating || (health?.analyzer ?? false) === false
                  }
                >
                  <legend>Goals (multi-select)</legend>
                  {PURPOSES.map((p) => {
                    const isSelected = purposes.has(p.value);
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
                      </label>
                    );
                  })}
                </fieldset>
                {purposes.size > 1 && (
                  <p className="hint">
                    {purposes.size} analyses will run in parallel on each
                    video.
                  </p>
                )}
                <label>
                  <span>Output language</span>
                  <input
                    type="text"
                    value={outputLanguage}
                    onChange={(e) =>
                      setOutputLanguage(e.target.value.toLowerCase())
                    }
                    placeholder="en, es, zh, fr, de, ja…"
                    disabled={generating}
                    maxLength={16}
                  />
                </label>
                {purposes.has("custom") && (
                  <label>
                    <span>Custom prompt</span>
                    <textarea
                      value={customPrompt}
                      onChange={(e) => setCustomPrompt(e.target.value)}
                      placeholder="What do you want the AI to do with each transcript?"
                      rows={3}
                      maxLength={2000}
                      disabled={generating}
                    />
                  </label>
                )}
                {(health?.analyzer ?? false) === false && (
                  <p className="hint error">
                    AI analysis is not configured on the server (set
                    <code> MINIMAX_API_KEY </code>). Uncheck above to process
                    transcripts only.
                  </p>
                )}
              </>
            )}
          </div>

          <div className="actions">
            <button
              className="primary"
              onClick={handleProcess}
              disabled={!canProcess}
            >
              {generating
                ? "Processing…"
                : `Process ${probe.video_count} videos`}
            </button>
          </div>
        </section>
      )}

      {playlist && (
        <section className="card">
          <div className="playlist-progress">
            <h3>Progress</h3>
            <JobStatusView
              job={{
                status: playlist.status,
                progress:
                  playlist.total > 0
                    ? Math.round(
                        ((playlist.completed + playlist.failed) /
                          playlist.total) *
                          100,
                      )
                    : 0,
              }}
            />
            <div className="playlist-summary">
              <span className="meta-chip">
                {playlist.completed} done
              </span>
              <span
                className="meta-chip"
                style={
                  playlist.failed > 0
                    ? {
                        color: "var(--error)",
                        borderColor: "var(--error)",
                      }
                    : undefined
                }
              >
                {playlist.failed} failed
              </span>
              <span className="meta-chip">
                {playlist.total - playlist.completed - playlist.failed} pending
              </span>
            </div>
          </div>

          {playlist.status === "completed" && playlist.zip_url && (
            <DownloadButton
              href={absoluteUrl(playlist.zip_url)}
              filename={`${playlist.playlist_title.replace(/[^a-zA-Z0-9]+/g, "-").toLowerCase()}-${playlist.playlist_id.slice(0, 8)}.zip`}
              label={`Download all (${playlist.completed} files as zip)`}
            />
          )}

          <ul className="playlist-children">
            {playlist.children.map((c, i) => (
              <li
                key={c.video_id + i}
                className={`playlist-child stage-${c.stage}`}
              >
                <span className="child-index">{String(i + 1).padStart(2, "0")}</span>
                <div className="child-info">
                  <span className="child-title">{c.title}</span>
                  <span className="child-meta">
                    {c.video_id} · {stageLabel(c.stage)}
                  </span>
                  {c.error && (
                    <span className="child-error">
                      <code>{c.error.code}</code> {c.error.message}
                    </span>
                  )}
                </div>
                <div className="child-actions">
                  {c.transcript_download_url && c.transcript_filename && (
                    <a
                      className="ghost small"
                      href={absoluteUrl(c.transcript_download_url)}
                      download={c.transcript_filename}
                      title={c.transcript_filename}
                    >
                      T
                    </a>
                  )}
                  {c.analyses.map((a) => {
                    const label = `${PURPOSE_LABEL[a.purpose] ?? a.purpose}`;
                    if (a.status === "completed" && a.download_url) {
                      return (
                        <a
                          key={a.analysis_id}
                          className="ghost small"
                          href={absoluteUrl(a.download_url)}
                          download={a.filename ?? a.analysis_id}
                          title={`${label} (${a.output_language})${a.filename ? ` · ${a.filename}` : ""}`}
                        >
                          {abbreviatePurpose(a.purpose)}
                        </a>
                      );
                    }
                    return (
                      <span
                        key={a.analysis_id}
                        className="ghost small"
                        title={`${label} (${a.output_language}) — ${a.status}${a.error ? `: ${a.error.message}` : ""}`}
                        aria-disabled="true"
                      >
                        {abbreviatePurpose(a.purpose)}·
                      </span>
                    );
                  })}
                </div>
              </li>
            ))}
          </ul>
          {playlist.error && (
            <div className="error-message" role="alert">
              <strong>Failed</strong>
              <p>{playlist.error.message}</p>
              <code>{playlist.error.code}</code>
            </div>
          )}
        </section>
      )}
    </main>
  );
}

function stageLabel(s: string): string {
  switch (s) {
    case "pending":
      return "Pending";
    case "running":
      return "Running…";
    case "completed":
      return "Completed";
    case "failed":
      return "Failed";
    case "skipped":
      return "Skipped";
    default:
      return s;
  }
}

function toAppError(e: unknown): AppError {
  if (e instanceof ApiError) {
    return { message: e.message, code: e.code };
  }
  if (e instanceof Error) {
    return { message: e.message };
  }
  return { message: "Unknown error" };
}
