// Light client-side validation. The server is the final authority on
// accept/reject (and returns precise error codes), so this just enables
// the submit button for URLs that look like YouTube links.
const VIDEO_ID = /^[A-Za-z0-9_-]{11}$/;
const PLAYLIST_ID = /^[A-Za-z0-9_-]{2,64}$/;

export function isLikelyYouTubeUrl(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed) return false;
  try {
    const url = new URL(trimmed);
    if (url.protocol !== "http:" && url.protocol !== "https:") return false;
    const host = url.hostname.toLowerCase();
    const okHost =
      host === "www.youtube.com" ||
      host === "youtube.com" ||
      host === "youtu.be" ||
      host === "m.youtube.com" ||
      host === "music.youtube.com";
    if (!okHost) return false;
    const path = url.pathname.toLowerCase();
    if (path === "/watch") {
      return VIDEO_ID.test(url.searchParams.get("v") ?? "");
    }
    if (path.startsWith("/shorts/") || path.startsWith("/embed/")) {
      const id = path.split("/")[2] ?? "";
      return VIDEO_ID.test(id);
    }
    if (host === "youtu.be") {
      const id = path.replace(/^\//, "").split("/")[0] ?? "";
      return VIDEO_ID.test(id);
    }
    if (path === "/playlist" || path.startsWith("/playlist/")) {
      return PLAYLIST_ID.test(url.searchParams.get("list") ?? "");
    }
    return false;
  } catch {
    return false;
  }
}
