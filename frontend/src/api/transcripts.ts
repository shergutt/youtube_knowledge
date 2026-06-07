import { client } from "./client";
import type {
  AnalysisJobResponse,
  CreateAnalysisRequest,
  CreateAnalysisResponse,
  CreateTranscriptRequest,
  CreateTranscriptResponse,
  JobResponse,
  ProbeRequest,
  ProbeResponse,
} from "../types/api";

export const transcriptsApi = {
  probe: (req: ProbeRequest) =>
    client.post<ProbeResponse>("/api/probe", req),
  create: (req: CreateTranscriptRequest) =>
    client.post<CreateTranscriptResponse>("/api/transcripts", req),
  status: (jobId: string) => client.get<JobResponse>(`/api/jobs/${jobId}`),
  downloadUrl: (jobId: string) => `/api/downloads/${jobId}`,
  analyze: (req: CreateAnalysisRequest) =>
    client.post<CreateAnalysisResponse>("/api/analyze", req),
  analysisStatus: (id: string) =>
    client.get<AnalysisJobResponse>(`/api/analyses/${id}`),
  analysisDownloadUrl: (id: string) => `/api/analyses/${id}/download`,
};

export async function pollJob(
  jobId: string,
  intervalMs: number,
  signal: AbortSignal,
  onTick: (s: JobResponse) => void,
): Promise<JobResponse> {
  while (!signal.aborted) {
    const status = await transcriptsApi.status(jobId);
    onTick(status);
    if (
      status.status === "completed" ||
      status.status === "failed" ||
      status.status === "expired"
    ) {
      return status;
    }
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(resolve, intervalMs);
      signal.addEventListener(
        "abort",
        () => {
          clearTimeout(timer);
          reject(new DOMException("aborted", "AbortError"));
        },
        { once: true },
      );
    });
  }
  throw new DOMException("aborted", "AbortError");
}

export async function pollAnalysis(
  id: string,
  intervalMs: number,
  signal: AbortSignal,
  onTick: (s: AnalysisJobResponse) => void,
): Promise<AnalysisJobResponse> {
  while (!signal.aborted) {
    const status = await transcriptsApi.analysisStatus(id);
    onTick(status);
    if (
      status.status === "completed" ||
      status.status === "failed" ||
      status.status === "expired"
    ) {
      return status;
    }
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(resolve, intervalMs);
      signal.addEventListener(
        "abort",
        () => {
          clearTimeout(timer);
          reject(new DOMException("aborted", "AbortError"));
        },
        { once: true },
      );
    });
  }
  throw new DOMException("aborted", "AbortError");
}

const TERMINAL = new Set(["completed", "failed", "expired"]);

/**
 * Poll many analyses concurrently. Returns a map of id -> final state once
 * every analysis has reached a terminal status. The onTick callback fires
 * after every individual status fetch and receives the full latest snapshot
 * map, which the caller can use to drive per-row progress UI.
 */
export async function pollAnalyses(
  ids: string[],
  intervalMs: number,
  signal: AbortSignal,
  onTick: (snapshot: Record<string, AnalysisJobResponse>) => void,
): Promise<Record<string, AnalysisJobResponse>> {
  if (ids.length === 0) return {};
  const latest: Record<string, AnalysisJobResponse> = {};
  const pending = new Set(ids);
  const tick = () => onTick({ ...latest });
  while (pending.size > 0 && !signal.aborted) {
    const responses = await Promise.all(
      Array.from(pending).map(async (id) => {
        try {
          const s = await transcriptsApi.analysisStatus(id);
          return [id, s] as const;
        } catch (e) {
          if ((e as Error).name === "AbortError") throw e;
          return null;
        }
      }),
    );
    for (const r of responses) {
      if (!r) continue;
      const [id, s] = r;
      latest[id] = s;
      if (TERMINAL.has(s.status)) {
        pending.delete(id);
      }
    }
    tick();
    if (pending.size > 0) {
      await new Promise<void>((resolve, reject) => {
        const timer = setTimeout(resolve, intervalMs);
        signal.addEventListener(
          "abort",
          () => {
            clearTimeout(timer);
            reject(new DOMException("aborted", "AbortError"));
          },
          { once: true },
        );
      });
    }
  }
  if (signal.aborted) {
    throw new DOMException("aborted", "AbortError");
  }
  tick();
  return latest;
}
