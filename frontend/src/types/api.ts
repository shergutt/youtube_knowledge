export type CaptionSource = "manual" | "auto" | "best";
export type OutputFormat = "vtt" | "srt" | "txt" | "json";

export interface ProbeRequest {
  url: string;
}

export interface CaptionTrack {
  language: string;
  name: string;
  formats: string[];
}

export interface ProbeResponse {
  video_id: string;
  title: string;
  duration_seconds?: number;
  thumbnail_url?: string;
  manual_captions: CaptionTrack[];
  automatic_captions: CaptionTrack[];
}

export interface CreateTranscriptRequest {
  url: string;
  language: string;
  caption_source: CaptionSource;
  output_format: OutputFormat;
  title?: string;
}

export interface CreateTranscriptResponse {
  job_id: string;
  status: "queued";
}

export type JobStatus =
  | "queued"
  | "running"
  | "completed"
  | "failed"
  | "expired";

export interface JobResponse {
  job_id: string;
  video_id: string;
  title: string;
  status: JobStatus;
  progress?: number;
  output_filename?: string;
  download_url?: string;
  error?: { code: string; message: string };
  expires_at?: string;
}

export type AnalysisPurpose =
  | "summary"
  | "study_notes"
  | "key_takeaways"
  | "action_items"
  | "blog_post"
  | "tutorial"
  | "custom";

export type AnalysisStatus = JobStatus;

export interface AnalysisSpec {
  purpose: AnalysisPurpose;
  custom_prompt?: string;
  output_language: string;
}

export interface CreateAnalysisRequest {
  job_id: string;
  /** One or more analysis goals. Each spec is queued and run in parallel
   *  against the same transcript. */
  specs: AnalysisSpec[];
  /** Optional default output language applied to specs whose own
   *  `output_language` is empty. */
  output_language?: string;
  /** Optional default custom prompt applied to specs whose purpose is
   *  `custom` and whose own `custom_prompt` is empty. */
  custom_prompt?: string;
}

export interface CreateAnalysisResponse {
  analysis_ids: string[];
  status: AnalysisStatus;
}

export interface AnalysisJobResponse {
  analysis_id: string;
  source_job_id: string;
  video_id: string;
  title: string;
  purpose: AnalysisPurpose;
  output_language: string;
  status: AnalysisStatus;
  progress: number;
  output_filename?: string;
  download_url?: string;
  error?: { code: string; message: string };
  model: string;
  input_tokens?: number;
  output_tokens?: number;
  expires_at?: string;
}

export interface HealthResponse {
  status: "ok";
  analyzer: boolean;
  analyzer_model?: string;
}

export interface ApiErrorBody {
  code: string;
  message: string;
}

export interface ApiErrorEnvelope {
  error: ApiErrorBody;
}

// ---------- Playlist ----------

export interface PlaylistProbeRequest {
  url: string;
}

export interface PlaylistProbeVideo {
  video_id: string;
  title: string;
  duration_seconds?: number;
}

export interface PlaylistProbeResponse {
  playlist_id: string;
  title: string;
  video_count: number;
  videos: PlaylistProbeVideo[];
}

export interface CreatePlaylistRequest {
  url: string;
  language: string;
  caption_source: CaptionSource;
  output_format: OutputFormat;
  analysis?: AnalysisSpec[];
}

export interface CreatePlaylistResponse {
  playlist_id: string;
  status: "queued" | "running" | "completed" | "failed" | "expired";
}

export type PlaylistChildStage =
  | "pending"
  | "running"
  | "completed"
  | "failed"
  | "skipped";

export interface ChildAnalysis {
  analysis_id: string;
  purpose: AnalysisPurpose;
  output_language: string;
  filename?: string;
  download_url?: string;
  status: string;
  error?: { code: string; message: string };
}

export interface PlaylistChild {
  video_id: string;
  title: string;
  stage: PlaylistChildStage;
  transcript_job_id?: string;
  transcript_filename?: string;
  transcript_download_url?: string;
  /** One entry per analysis spec that ran against this child's transcript. */
  analyses: ChildAnalysis[];
  error?: { code: string; message: string };
}

export interface PlaylistResponse {
  playlist_id: string;
  source_url: string;
  playlist_title: string;
  language: string;
  caption_source: CaptionSource;
  output_format: OutputFormat;
  analysis?: AnalysisSpec[];
  status: "queued" | "running" | "completed" | "failed" | "expired";
  total: number;
  completed: number;
  failed: number;
  children: PlaylistChild[];
  error?: { code: string; message: string };
  zip_url?: string;
  created_at: string;
  updated_at: string;
  expires_at?: string;
}
