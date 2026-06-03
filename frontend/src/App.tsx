import { useEffect, useState } from "react";
import { Route, Routes } from "react-router-dom";
import "./App.css";
import { NavBar } from "./components/NavBar";
import { ToastHost } from "./components/Toast";
import { SinglePage } from "./pages/SinglePage";
import { PlaylistPage } from "./pages/PlaylistPage";
import type { HealthResponse } from "./types/api";

export default function App() {
  const [health, setHealth] = useState<HealthResponse | null>(null);

  useEffect(() => {
    fetch((import.meta.env.VITE_API_BASE ?? "") + "/health")
      .then((r) => (r.ok ? r.json() : null))
      .then((d: HealthResponse | null) => d && setHealth(d))
      .catch(() => undefined);
  }, []);

  return (
    <div className="app">
      <NavBar />
      <p className="subtitle">
        Paste a YouTube URL, pick a caption track, and download the transcript —
        or let the AI summarize it.
      </p>
      <Routes>
        <Route path="/" element={<SinglePage health={health} />} />
        <Route path="/playlist" element={<PlaylistPage health={health} />} />
      </Routes>
      <footer>
        <small>
          Powered by yt-dlp and MiniMax-M3. Unofficial, not affiliated with
          YouTube.
        </small>
      </footer>
      <ToastHost />
    </div>
  );
}
