import { cx } from "@shared/class-names";
import { useState } from "react";

interface DraftInputProps {
  id?: string;
  type: "text" | "number";
  /** Sizing only; the field's look is fixed here. */
  className?: string;
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
 * Borderless and lightly filled, like every control in settings: it sits on a
 * plate that already says where it belongs, and the focus ring still shows.
 *
 * What was actually stored replaces the draft, so a value Rust corrected, such
 * as a blank home page restored to the default, shows as it really is. Escape
 * abandons the edit.
 */
export function DraftInput({ id, type, className, value, min, max, disabled, onCommit, ...rest }: DraftInputProps) {
  const [draft, setDraft] = useState(value);
  // A new stored value replaces the draft. Done while rendering, so the stale
  // draft is never painted first.
  const [stored, setStored] = useState(value);
  if (value !== stored) {
    setStored(value);
    setDraft(value);
  }

  const commit = async () => {
    if (draft === value) {
      return;
    }
    setDraft((await onCommit(draft)) ?? value);
  };

  return (
    <input
      id={id}
      className={cx("ipt soft edgeless", className)}
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
