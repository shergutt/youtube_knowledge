import { useEffect, useState } from "react";
import type { HistoryEntry } from "../utils/history";
import {
  clearHistory,
  getHistory,
  recordAnalysis,
  recordTranscript,
  removeEntry,
} from "../utils/history";
import { transcriptsApi } from "../api/transcripts";
import type { AnalysisJobResponse, JobResponse } from "../types/api";
import { absoluteUrl } from "../utils/url";

interface HistoryPanelProps {
  /**
   * Called when the user clicks a past job. The parent should switch the UI
   * into that job's state (re-fetch status, show previews, etc.).
   */
  onSelect: (entry: HistoryEntry) => void;
  /**
   * Re-render the history list when the parent records a new job or analysis.
   * Bumping this counter from the parent is the easiest way to keep in sync.
   */
  refreshKey: number;
}

function isExpired(iso?: string): boolean {
  if (!iso) return false;
  return new Date(iso).getTime() < Date.now();
}

function fmtDate(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleString();
  } catch {
    return iso;
  }
}

export function HistoryPanel({ onSelect, refreshKey }: HistoryPanelProps) {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [expanded, setExpanded] = useState<string | null>(null);

  useEffect(() => {
    setEntries(getHistory());
  }, [refreshKey]);

  const live = entries.filter((e) => !isExpired(e.expires_at));
  const expired = entries.length - live.length;
  if (live.length === 0) {
    return (
      <div className="history-panel empty">
        <h3>Recent jobs</h3>
        <p className="muted">No history yet. Generate a transcript to see it here.</p>
      </div>
    );
  }

  return (
    <div className="history-panel">
      <div className="history-header">
        <h3>Recent jobs</h3>
        <button
          className="ghost small"
          onClick={() => {
            if (confirm("Clear all history?")) {
              clearHistory();
              setEntries([]);
            }
          }}
        >
          Clear
        </button>
      </div>
      {expired > 0 && (
        <p className="muted small">{expired} expired entries hidden</p>
      )}
      <ul className="history-list">
        {live.map((e) => {
          const isOpen = expanded === e.job_id;
          return (
            <li key={e.job_id} className="history-item">
              <div className="history-row">
                <button
                  className="history-pick"
                  onClick={() => onSelect(e)}
                  title={`${e.title}\n${fmtDate(e.created_at)}`}
                >
                  <span className="history-title">{e.title}</span>
                  <span className="history-meta">
                    {e.language} · {e.output_format.toUpperCase()} · {fmtDate(e.created_at)}
                  </span>
                </button>
                <button
                  className="ghost small"
                  onClick={() => setExpanded(isOpen ? null : e.job_id)}
                  aria-label={isOpen ? "Hide details" : "Show details"}
                >
                  {isOpen ? "▾" : "▸"}
                </button>
                <button
                  className="ghost small"
                  onClick={() => {
                    removeEntry(e.job_id);
                    setEntries(getHistory());
                  }}
                  aria-label="Remove from history"
                  title="Remove from history"
                >
                  ×
                </button>
              </div>
              {isOpen && (
                <div className="history-details">
                  {e.output_filename && (
                    <a
                      className="ghost"
                      href={absoluteUrl(e.download_url) ||
                        absoluteUrl(`/api/downloads/${e.job_id}`)}
                      download={e.output_filename}
                    >
                      Download {e.output_filename}
                    </a>
                  )}
                  {e.analyses.length > 0 && (
                    <div className="history-analyses">
                      <p className="muted small">Analyses</p>
                      {e.analyses.map((a) => (
                        <a
                          key={a.analysis_id}
                          className="ghost"
                          href={absoluteUrl(`/api/analyses/${a.analysis_id}/download`)}
                          download={a.output_filename}
                        >
                          {a.purpose} ({a.output_language})
                          {a.output_filename ? ` · ${a.output_filename}` : ""}
                        </a>
                      ))}
                    </div>
                  )}
                </div>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

/**
 * Re-validate a job in history by re-fetching its status from the server.
 * If the job is still alive, refresh its record in history; if it has
 * expired, drop it. This is used when the user picks a past job to view.
 */
export async function reviveJob(entry: HistoryEntry): Promise<{
  job: JobResponse | null;
  analyses: AnalysisJobResponse[];
}> {
  try {
    const job = await transcriptsApi.status(entry.job_id);
    recordTranscript({
      job_id: job.job_id,
      video_id: job.video_id,
      title: job.title,
      language: entry.language,
      output_format: entry.output_format,
      output_filename: job.output_filename,
      download_url: job.download_url,
      expires_at: job.expires_at,
    });
    const analyses: AnalysisJobResponse[] = [];
    for (const a of entry.analyses) {
      try {
        const fresh = await transcriptsApi.analysisStatus(a.analysis_id);
        analyses.push(fresh);
        recordAnalysis({
          job_id: entry.job_id,
          analysis: {
            analysis_id: fresh.analysis_id,
            purpose: fresh.purpose,
            output_language: fresh.output_language,
            output_filename: fresh.output_filename,
            created_at: a.created_at,
            expires_at: fresh.expires_at,
          },
        });
      } catch {
        // Analysis gone — skip
      }
    }
    return { job, analyses };
  } catch {
    removeEntry(entry.job_id);
    return { job: null, analyses: [] };
  }
}
