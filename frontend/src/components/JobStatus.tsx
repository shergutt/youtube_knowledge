import type { JobResponse } from "../types/api";

interface JobStatusProps {
  job: Pick<JobResponse, "status" | "progress" | "error">;
}

const STATUS_LABEL: Record<JobResponse["status"], string> = {
  queued: "Queued",
  running: "Running",
  completed: "Completed",
  failed: "Failed",
  expired: "Expired",
};

export function JobStatusView({ job }: JobStatusProps) {
  return (
    <div className={`job-status status-${job.status}`}>
      <div className="status-header">
        <span className="status-label">{STATUS_LABEL[job.status]}</span>
        {typeof job.progress === "number" && job.status === "running" && (
          <span className="progress">{job.progress}%</span>
        )}
      </div>
      {job.status === "running" && (
        <div className="bar">
          <div
            className="bar-fill"
            style={{ width: `${Math.max(5, job.progress ?? 5)}%` }}
          />
        </div>
      )}
      {job.error && (
        <p className="error">
          <code>{job.error.code}</code>: {job.error.message}
        </p>
      )}
    </div>
  );
}
