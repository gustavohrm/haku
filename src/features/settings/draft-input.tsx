import { useEffect, useState } from "react";

interface DraftInputProps {
  type: "text" | "number";
  value: string;
  min?: number;
  max?: number;
  disabled?: boolean | undefined;
  "aria-describedby"?: string | undefined;
  /** Saves the value, resolving to what was stored, or `null` if nothing was. */
  onCommit: (value: string) => Promise<string | null>;
}

/**
 * A field edited locally and saved when it is left or Enter is pressed.
 *
 * What was actually stored replaces the draft, so a value Rust corrected, such
 * as a blank home page restored to the default, shows as it really is. Escape
 * abandons the edit.
 */
export function DraftInput({ type, value, min, max, disabled, onCommit, ...rest }: DraftInputProps) {
  const [draft, setDraft] = useState(value);

  useEffect(() => setDraft(value), [value]);

  const commit = async () => {
    if (draft === value) {
      return;
    }
    setDraft((await onCommit(draft)) ?? value);
  };

  return (
    <input
      className="ipt"
      type={type}
      min={min}
      max={max}
      disabled={disabled}
      aria-describedby={rest["aria-describedby"]}
      value={draft}
      spellCheck={false}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={() => void commit()}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          void commit();
        } else if (event.key === "Escape") {
          setDraft(value);
        }
      }}
    />
  );
}
