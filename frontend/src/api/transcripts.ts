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
