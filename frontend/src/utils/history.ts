export interface HistoryAnalysis {
  analysis_id: string;
  purpose: string;
  output_language: string;
  output_filename?: string;
  created_at: string;
  expires_at?: string;
}

export interface HistoryEntry {
  job_id: string;
  video_id: string;
  title: string;
  language: string;
  output_format: string;
  output_filename?: string;
  download_url?: string;
  created_at: string;
  expires_at?: string;
  analyses: HistoryAnalysis[];
}

const KEY = "yt-transcript-history-v1";
const MAX_ENTRIES = 12;

function read(): HistoryEntry[] {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isEntry);
  } catch {
    return [];
  }
}

function write(entries: HistoryEntry[]) {
  try {
    localStorage.setItem(KEY, JSON.stringify(entries));
  } catch {
    // Storage might be full or disabled; non-fatal
  }
}

function isEntry(x: unknown): x is HistoryEntry {
  if (!x || typeof x !== "object") return false;
  const o = x as Record<string, unknown>;
  return (
    typeof o.job_id === "string" &&
    typeof o.video_id === "string" &&
    typeof o.title === "string" &&
    typeof o.language === "string" &&
    typeof o.output_format === "string"
  );
}

export function getHistory(): HistoryEntry[] {
  return read();
}

export function recordTranscript(entry: Omit<HistoryEntry, "analyses" | "created_at">) {
  const all = read();
  const existing = all.findIndex((e) => e.job_id === entry.job_id);
  const merged: HistoryEntry = {
    ...entry,
    created_at: new Date().toISOString(),
    analyses: existing >= 0 ? all[existing].analyses : [],
  };
  if (existing >= 0) {
    all[existing] = merged;
  } else {
    all.unshift(merged);
  }
  write(all.slice(0, MAX_ENTRIES));
}

export function recordAnalysis(args: {
  job_id: string;
  analysis: HistoryAnalysis;
}) {
  const all = read();
  const idx = all.findIndex((e) => e.job_id === args.job_id);
  if (idx < 0) return;
  const entry = all[idx];
  const others = entry.analyses.filter(
    (a) => a.analysis_id !== args.analysis.analysis_id,
  );
  entry.analyses = [args.analysis, ...others].slice(0, 8);
  all[idx] = entry;
  write(all);
}

export function clearHistory() {
  write([]);
}

export function removeEntry(job_id: string) {
  write(read().filter((e) => e.job_id !== job_id));
}
