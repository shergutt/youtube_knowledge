import { useEffect, useState } from "react";

type Mode = "auto" | "light" | "dark";

const KEY = "yt-transcript-theme";

function apply(mode: Mode) {
  const root = document.documentElement;
  if (mode === "auto") {
    root.removeAttribute("data-theme");
  } else {
    root.setAttribute("data-theme", mode);
  }
}

function read(): Mode {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "light" || v === "dark" || v === "auto") return v;
  } catch {
    // ignore
  }
  return "auto";
}

export function ThemeToggle() {
  const [mode, setMode] = useState<Mode>(() => read());

  useEffect(() => {
    apply(mode);
    try {
      localStorage.setItem(KEY, mode);
    } catch {
      // ignore
    }
  }, [mode]);

  function next() {
    setMode((m) => (m === "auto" ? "light" : m === "light" ? "dark" : "auto"));
  }

  const label =
    mode === "auto" ? "Auto theme" : mode === "light" ? "Light theme" : "Dark theme";

  return (
    <button
      className="theme-toggle"
      onClick={next}
      title="Cycle theme: auto / light / dark"
      aria-label={label}
    >
      {mode === "light" ? "☀" : mode === "dark" ? "☾" : "◐"}
    </button>
  );
}
