import { NavLink } from "react-router-dom";
import { ThemeToggle } from "./ThemeToggle";

export function NavBar() {
  return (
    <header className="app-header">
      <div className="brand">
        <span className="logo" aria-hidden="true">▶</span>
        <h1>YT Transcripts</h1>
      </div>
      <nav className="primary-nav" aria-label="Primary">
        <NavLink
          to="/"
          end
          className={({ isActive }) =>
            isActive ? "nav-link active" : "nav-link"
          }
        >
          Single video
        </NavLink>
        <NavLink
          to="/playlist"
          className={({ isActive }) =>
            isActive ? "nav-link active" : "nav-link"
          }
        >
          Playlist
        </NavLink>
      </nav>
      <div className="header-tools">
        <ThemeToggle />
      </div>
    </header>
  );
}
