import { client } from "./client";
import type {
  CreatePlaylistRequest,
  CreatePlaylistResponse,
  PlaylistProbeRequest,
  PlaylistProbeResponse,
  PlaylistResponse,
} from "../types/api";

export const playlistsApi = {
  probe: (req: PlaylistProbeRequest) =>
    client.post<PlaylistProbeResponse>("/api/playlists/probe", req),
  create: (req: CreatePlaylistRequest) =>
    client.post<CreatePlaylistResponse>("/api/playlists", req),
  status: (id: string) =>
    client.get<PlaylistResponse>(`/api/playlists/${id}`),
  zipUrl: (id: string) => `/api/playlists/${id}/download`,
};

export async function pollPlaylist(
  id: string,
  intervalMs: number,
  signal: AbortSignal,
  onTick: (s: PlaylistResponse) => void,
): Promise<PlaylistResponse> {
  while (!signal.aborted) {
    const status = await playlistsApi.status(id);
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
