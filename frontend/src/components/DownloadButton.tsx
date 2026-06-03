import { showToast } from "./Toast";

interface DownloadButtonProps {
  href: string;
  filename: string;
  label?: string;
}

export function DownloadButton({ href, filename, label }: DownloadButtonProps) {
  return (
    <a
      className="download-button"
      href={href}
      download={filename}
      onClick={() => showToast(`Downloading ${filename}`, "info", 1800)}
    >
      <span className="icon" aria-hidden="true">↓</span>
      {label ?? `Download ${filename}`}
    </a>
  );
}
