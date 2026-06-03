/**
 * Resolve a URL returned by the backend into an absolute URL the browser
 * can fetch or navigate to. The backend returns paths like `/api/downloads/...`
 * (relative). In dev, those are proxied through Vite. In production (Vercel),
 * the SPA rewrite would intercept them, so we prepend VITE_API_BASE.
 */
export function absoluteUrl(path: string | undefined | null): string {
  if (!path) return "";
  if (/^https?:\/\//i.test(path)) return path;
  const base = import.meta.env.VITE_API_BASE ?? "";
  return base + path;
}
