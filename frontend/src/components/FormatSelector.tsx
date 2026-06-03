import type { OutputFormat } from "../types/api";

interface FormatSelectorProps {
  value: OutputFormat;
  onChange: (v: OutputFormat) => void;
  disabled?: boolean;
}

const FORMATS: { value: OutputFormat; label: string; hint: string }[] = [
  { value: "vtt", label: "VTT", hint: "WebVTT, with timing" },
  { value: "srt", label: "SRT", hint: "SubRip, with timing" },
  { value: "txt", label: "TXT", hint: "Plain text, no timing" },
  { value: "json", label: "JSON", hint: "Timestamped segments" },
];

export function FormatSelector({ value, onChange, disabled }: FormatSelectorProps) {
  return (
    <div className="format-selector">
      <fieldset disabled={disabled}>
        <legend>Output format</legend>
        {FORMATS.map((f) => (
          <label key={f.value}>
            <input
              type="radio"
              name="output_format"
              value={f.value}
              checked={value === f.value}
              onChange={() => onChange(f.value)}
            />
            <span className="label">{f.label}</span>
            <span className="hint">{f.hint}</span>
          </label>
        ))}
      </fieldset>
    </div>
  );
}
