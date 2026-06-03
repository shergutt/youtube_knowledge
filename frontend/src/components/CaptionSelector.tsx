import type { CaptionSource } from "../types/api";

interface CaptionSelectorProps {
  source: CaptionSource;
  onSourceChange: (s: CaptionSource) => void;
  language: string;
  onLanguageChange: (l: string) => void;
  availableLanguages: string[];
  hasManual: boolean;
  hasAuto: boolean;
  disabled?: boolean;
}

const SOURCES: { value: CaptionSource; label: string; needs: "manual" | "auto" | "either" }[] = [
  { value: "manual", label: "Manual captions", needs: "manual" },
  { value: "auto", label: "Auto-generated", needs: "auto" },
  { value: "best", label: "Best available", needs: "either" },
];

// Common languages that get a quick-pick chip. These are the codes the
// backend can serve and the most common YouTube caption codes.
const QUICK_LANGUAGES: { code: string; label: string }[] = [
  { code: "en", label: "EN" },
  { code: "es", label: "ES" },
  { code: "pt", label: "PT" },
  { code: "fr", label: "FR" },
  { code: "de", label: "DE" },
  { code: "it", label: "IT" },
  { code: "ja", label: "JA" },
  { code: "ko", label: "KO" },
  { code: "zh-Hans", label: "ZH" },
];

export function CaptionSelector({
  source,
  onSourceChange,
  language,
  onLanguageChange,
  availableLanguages,
  hasManual,
  hasAuto,
  disabled,
}: CaptionSelectorProps) {
  const availableSet = new Set(availableLanguages);
  const quickPicks = QUICK_LANGUAGES.filter(
    (q) => availableSet.size === 0 || availableSet.has(q.code),
  );

  return (
    <div className="caption-selector">
      <fieldset disabled={disabled}>
        <legend>Caption source</legend>
        {SOURCES.map((s) => {
          const unavailable =
            (s.needs === "manual" && !hasManual) ||
            (s.needs === "auto" && !hasAuto);
          return (
            <label key={s.value} className={unavailable ? "muted" : ""}>
              <input
                type="radio"
                name="caption_source"
                value={s.value}
                checked={source === s.value}
                onChange={() => onSourceChange(s.value)}
                disabled={unavailable}
              />
              {s.label}
              {unavailable && <span className="badge">unavailable</span>}
            </label>
          );
        })}
      </fieldset>

      <div className="language">
        <span>Language</span>
        {quickPicks.length > 0 && (
          <div className="chip-row language-chips" role="group" aria-label="Quick language pick">
            {quickPicks.map((q) => {
              const active = language === q.code;
              return (
                <button
                  key={q.code}
                  type="button"
                  className={active ? "chip active" : "chip"}
                  onClick={() => onLanguageChange(q.code)}
                  disabled={disabled}
                  title={q.code}
                  aria-pressed={active}
                >
                  {q.label}
                </button>
              );
            })}
          </div>
        )}
        <select
          value={language}
          onChange={(e) => onLanguageChange(e.target.value)}
          disabled={disabled || availableLanguages.length === 0}
          aria-label="Caption language"
        >
          {availableLanguages.length === 0 ? (
            <option value="">No captions available</option>
          ) : (
            availableLanguages.map((l) => (
              <option key={l} value={l}>
                {l}
              </option>
            ))
          )}
        </select>
      </div>
    </div>
  );
}
