import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AnalyzerPanel } from "../components/AnalyzerPanel";
import { CaptionSelector } from "../components/CaptionSelector";
import { DownloadButton } from "../components/DownloadButton";
import { ErrorMessage } from "../components/ErrorMessage";
import { FormatSelector } from "../components/FormatSelector";
import { HistoryPanel, reviveJob } from "../components/HistoryPanel";
import { JobStatusView } from "../components/JobStatus";
import { MarkdownPreview } from "../components/MarkdownPreview";
import { showToast } from "../components/Toast";
import { TranscriptPreview } from "../components/TranscriptPreview";
import { DEMO_VIDEO_URL, UrlInput } from "../components/UrlInput";
import { ApiError } from "../api/client";
import { pollJob, transcriptsApi } from "../api/transcripts";
import {
  recordAnalysis,
  recordTranscript,
  type HistoryAnalysis,
  type HistoryEntry,
} from "../utils/history";
import { themedBase } from "../utils/slug";
import { absoluteUrl } from "../utils/url";
import type {
  AnalysisJobResponse,
  CaptionSource,
  HealthResponse,
  JobResponse,
  OutputFormat,
  ProbeResponse,
} from "../types/api";

type AppError = { message: string; code?: string };

const PURPOSE_LABEL: Record<string, string> = {
  summary: "Summary",
  study_notes: "Study notes",
  key_takeaways: "Key takeaways",
  action_items: "Action items",
  blog_post: "Blog post",
  tutorial: "Tutorial",
  custom: "Custom analysis",
};

export function SinglePage({ health }: { health: HealthResponse | null }) {
  const [url, setUrl] = useState("");
  const [probe, setProbe] = useState<ProbeResponse | null>(null);
  const [probeLoading, setProbeLoading] = useState(false);
  const [error, setError] = useState<AppError | null>(null);

  const [source, setSource] = useState<CaptionSource>("best");
  const [language, setLanguage] = useState<string>("");
  const [format, setFormat] = useState<OutputFormat>("txt");

  const [job, setJob] = useState<JobResponse | null>(null);
  const [generating, setGenerating] = useState(false);
  const [analyses, setAnalyses] = useState<AnalysisJobResponse[]>([]);
  const [activeAnalysisId, setActiveAnalysisId] = useState<string | null>(null);
  const pollAbort = useRef<AbortController | null>(null);
  const [historyKey, setHistoryKey] = useState(0);

  useEffect(() => {
    return () => {
      pollAbort.current?.abort();
    };
  }, []);

  const availableLanguages = useMemo<string[]>(() => {
    if (!probe) return [];
    const set = new Set<string>();
    for (const t of probe.manual_captions) set.add(t.language);
    for (const t of probe.automatic_captions) set.add(t.language);
    return Array.from(set).sort();
  }, [probe]);

  useEffect(() => {
    if (!language && availableLanguages.includes("en")) {
      setLanguage("en");
    } else if (language && !availableLanguages.includes(language)) {
      setLanguage(availableLanguages[0] ?? "");
    }
  }, [availableLanguages, language]);

  const handleCheck = useCallback(async () => {
    setError(null);
    setProbe(null);
    setJob(null);
    setAnalyses([]);
    setActiveAnalysisId(null);
    setProbeLoading(true);
    try {
      const result = await transcriptsApi.probe({ url });
      setProbe(result);
      showToast(`Loaded "${result.title}"`, "success");
    } catch (e) {
      setError(toAppError(e));
    } finally {
      setProbeLoading(false);
    }
  }, [url]);

  const handleGenerate = useCallback(async () => {
    if (!probe) return;
    setError(null);
    setJob(null);
    setAnalyses([]);
    setActiveAnalysisId(null);
    setGenerating(true);
    pollAbort.current?.abort();
    const ctrl = new AbortController();
    pollAbort.current = ctrl;
    try {
      const created = await transcriptsApi.create({
        url,
        language,
        caption_source: source,
        output_format: format,
        title: probe.title,
      });
      const final = await pollJob(created.job_id, 1500, ctrl.signal, (s) =>
        setJob(s),
      );
      setJob(final);
      recordTranscript({
        job_id: final.job_id,
        video_id: final.video_id,
        title: final.title,
        language,
        output_format: format,
        output_filename: final.output_filename,
        download_url: final.download_url,
        expires_at: final.expires_at,
      });
      setHistoryKey((k) => k + 1);
      showToast("Transcript ready", "success");
    } catch (e) {
      if ((e as Error).name === "AbortError") return;
      setError(toAppError(e));
    } finally {
      setGenerating(false);
    }
  }, [probe, url, language, source, format]);

  const handleAnalysisComplete = useCallback(
    (resp: AnalysisJobResponse) => {
      setAnalyses((prev) => {
        const next = prev.filter((a) => a.analysis_id !== resp.analysis_id);
        return [resp, ...next];
      });
      setActiveAnalysisId(resp.analysis_id);
      const ha: HistoryAnalysis = {
        analysis_id: resp.analysis_id,
        purpose: resp.purpose,
        output_language: resp.output_language,
        output_filename: resp.output_filename,
        created_at: new Date().toISOString(),
        expires_at: resp.expires_at,
      };
      if (job) {
        recordAnalysis({ job_id: job.job_id, analysis: ha });
        setHistoryKey((k) => k + 1);
      }
      showToast("Analysis ready", "success");
    },
    [job],
  );

  async function handleSelectHistory(entry: HistoryEntry) {
    setError(null);
    setGenerating(true);
    try {
      const { job, analyses } = await reviveJob(entry);
      if (!job) {
        showToast("That job has expired and was removed", "error");
        setHistoryKey((k) => k + 1);
        return;
      }
      setUrl(`https://www.youtube.com/watch?v=${entry.video_id}`);
      setLanguage(entry.language);
      setFormat(entry.output_format as OutputFormat);
      setJob(job);
      setAnalyses(analyses);
      setActiveAnalysisId(analyses[0]?.analysis_id ?? null);
      setProbe({
        video_id: entry.video_id,
        title: entry.title,
        manual_captions: [],
        automatic_captions: [],
      });
      setHistoryKey((k) => k + 1);
      showToast(`Loaded "${entry.title}"`, "success");
    } finally {
      setGenerating(false);
    }
  }

  const hasManual = (probe?.manual_captions.length ?? 0) > 0;
  const hasAuto = (probe?.automatic_captions.length ?? 0) > 0;
  const canGenerate =
    !!probe && !generating && language && (hasManual || hasAuto);

  const downloadName = useMemo(() => {
    if (!probe || !job || job.status !== "completed") return "";
    if (job.output_filename) return job.output_filename;
    const base = themedBase(probe.title || probe.video_id, probe.video_id);
    return `${base}.${language}.${format}`;
  }, [probe, job, language, format]);

  const activeAnalysis = analyses.find((a) => a.analysis_id === activeAnalysisId);

  return (
    <main>
      <section className="card">
        <UrlInput
          value={url}
          onChange={setUrl}
          onSubmit={handleCheck}
          onUseDemo={() => {
            setUrl(DEMO_VIDEO_URL);
            setTimeout(handleCheck, 0);
          }}
          loading={probeLoading}
        />
        <ErrorMessage error={error} />
      </section>

      {probe && (
        <section className="card">
          <div className="video-info">
            {probe.thumbnail_url && (
              <a
                href={`https://www.youtube.com/watch?v=${probe.video_id}`}
                target="_blank"
                rel="noreferrer"
                className="thumb-link"
              >
                <img
                  src={probe.thumbnail_url}
                  alt=""
                  className="thumb"
                  referrerPolicy="no-referrer"
                  loading="lazy"
                />
              </a>
            )}
            <div className="video-meta">
              <h2 title={probe.title}>{probe.title}</h2>
              <p className="meta">
                <span className="meta-chip">ID: {probe.video_id}</span>
                {probe.duration_seconds != null && (
                  <span className="meta-chip">
                    {formatDuration(probe.duration_seconds)}
                  </span>
                )}
                <span className="meta-chip">
                  {probe.manual_captions.length} manual
                </span>
                <span className="meta-chip">
                  {probe.automatic_captions.length} auto
                </span>
              </p>
            </div>
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
            availableLanguages={availableLanguages}
            hasManual={hasManual}
            hasAuto={hasAuto}
            disabled={generating}
          />
          <FormatSelector
            value={format}
            onChange={setFormat}
            disabled={generating}
          />
          <div className="actions">
            <button
              className="primary"
              onClick={handleGenerate}
              disabled={!canGenerate}
            >
              {generating ? "Generating…" : "Generate transcript"}
            </button>
          </div>
        </section>
      )}

      {job && (
        <section className="card">
          <JobStatusView job={job} />
          {job.status === "completed" && job.download_url && downloadName && (
            <>
              <DownloadButton
                href={absoluteUrl(job.download_url)}
                filename={downloadName}
              />
              <TranscriptPreview
                downloadUrl={absoluteUrl(job.download_url)}
                outputFilename={downloadName}
              />
            </>
          )}
          {job.status === "failed" && job.error && (
            <div className="error-message" role="alert">
              <strong>Failed</strong>
              <p>{job.error.message}</p>
              <code>{job.error.code}</code>
            </div>
          )}
        </section>
      )}

      {job && job.status === "completed" && (
        <section className="card">
          <AnalyzerPanel
            jobId={job.job_id}
            videoId={probe?.video_id ?? job.video_id}
            title={probe?.title ?? job.title}
            transcriptLanguage={language || "en"}
            analyzerEnabled={health?.analyzer ?? false}
            onError={setError}
            onComplete={handleAnalysisComplete}
          />
        </section>
      )}

      {analyses.length > 0 && (
        <section className="card">
          <div className="analysis-tabs">
            <h3>Analyses</h3>
            <div className="chip-row" role="tablist">
              {analyses.map((a) => (
                <button
                  key={a.analysis_id}
                  role="tab"
                  aria-selected={activeAnalysisId === a.analysis_id}
                  className={
                    activeAnalysisId === a.analysis_id ? "chip active" : "chip"
                  }
                  onClick={() => setActiveAnalysisId(a.analysis_id)}
                >
                  {PURPOSE_LABEL[a.purpose] ?? a.purpose}{" "}
                  <span className="chip-meta">{a.output_language}</span>
                </button>
              ))}
            </div>
          </div>
          {activeAnalysis &&
            activeAnalysis.status === "completed" &&
            activeAnalysis.download_url && (
              <>
                <DownloadButton
                  href={absoluteUrl(activeAnalysis.download_url)}
                  filename={
                    activeAnalysis.output_filename ??
                    `${probe?.video_id ?? "analysis"}.${activeAnalysis.output_language}.md`
                  }
                  label={`Download ${activeAnalysis.output_filename ?? "analysis"}`}
                />
                <MarkdownPreview
                  downloadUrl={absoluteUrl(activeAnalysis.download_url)}
                  outputFilename={activeAnalysis.output_filename ?? "analysis.md"}
                  purposeLabel={
                    PURPOSE_LABEL[activeAnalysis.purpose] ?? activeAnalysis.purpose
                  }
                />
                {(activeAnalysis.input_tokens != null ||
                  activeAnalysis.output_tokens != null) && (
                  <p className="tokens muted">
                    {activeAnalysis.model}
                    {activeAnalysis.input_tokens != null &&
                      ` · input: ${activeAnalysis.input_tokens} tokens`}
                    {activeAnalysis.output_tokens != null &&
                      ` · output: ${activeAnalysis.output_tokens} tokens`}
                  </p>
                )}
              </>
            )}
          {activeAnalysis &&
            activeAnalysis.status === "failed" &&
            activeAnalysis.error && (
              <div className="error-message" role="alert">
                <strong>Failed</strong>
                <p>{activeAnalysis.error.message}</p>
                <code>{activeAnalysis.error.code}</code>
              </div>
            )}
        </section>
      )}

      <section className="card history-card">
        <HistoryPanel refreshKey={historyKey} onSelect={handleSelectHistory} />
      </section>
    </main>
  );
}

function formatDuration(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  if (h > 0) {
    return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }
  return `${m}:${String(s).padStart(2, "0")}`;
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
