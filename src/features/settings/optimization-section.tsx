import { type Policy, type Preset, type Settings } from "@bindings";
import { commands } from "@ipc/commands";
import { t, type TranslationKey } from "@shared/i18n";
import { useEffect, useId, useState, type ReactNode } from "react";

import { DraftInput } from "./draft-input";

const MAX_CAPACITY = 12;

/** Bounds of the background memory budget, in megabytes. */
const MIN_KEPT_MEMORY_MB = 0;
const MAX_KEPT_MEMORY_MB = 16384;

const PRESETS: Record<Preset, TranslationKey> = {
  saveMemory: "settings.preset.saveMemory",
  balanced: "settings.preset.balanced",
  performance: "settings.preset.performance",
};

const POLICIES: Record<Policy, TranslationKey> = {
  never: "settings.policy.never",
  smart: "settings.policy.smart",
  always: "settings.policy.always",
};

function isPreset(value: string): value is Preset {
  return value in PRESETS;
}

function isPolicy(value: string): value is Policy {
  return value in POLICIES;
}

interface OptimizationSectionProps {
  settings: Settings;
  update: (patch: Partial<Settings>) => Promise<Settings | null>;
}

/**
 * How far Haku goes to keep memory down.
 *
 * Which preset is in effect is asked of Rust rather than worked out here, so
 * the preset values have one definition.
 */
export function OptimizationSection({ settings, update }: OptimizationSectionProps) {
  const [preset, setPreset] = useState<Preset | null>(null);

  useEffect(() => {
    let current = true;
    const read = async () => {
      const result = await commands.currentPreset();
      if (current && result.status === "ok") {
        setPreset(result.data);
      }
    };
    void read();
    return () => {
      current = false;
    };
  }, [settings]);

  // Discarding every background tab leaves nothing to freeze, nothing for
  // extra slots to hold and nothing to spend a budget on.
  const discardsEverything = settings.discardTabs === "always";

  return (
    <section className="card stack">
      <h2 className="text-title-sm">{t("settings.optimization")}</h2>

      <label className="field">
        <span className="label">{t("settings.preset")}</span>
        <select
          className="select"
          value={preset ?? "custom"}
          onChange={(event) => {
            const next = event.target.value;
            if (isPreset(next)) {
              void commands.applyPreset(next);
            }
          }}
        >
          {Object.entries(PRESETS).map(([value, label]) => (
            <option key={value} value={value}>
              {t(label)}
            </option>
          ))}
          <option value="custom" disabled>
            {t("settings.preset.custom")}
          </option>
        </select>
        <span className="hint">{t("settings.preset.help")}</span>
      </label>

      <DisabledFor reason={discardsEverything ? t("settings.disabledByDiscard") : null}>
        {(describedBy) => (
          <label className="field">
            <span className="label">{t("settings.capacity")}</span>
            <DraftInput
              type="number"
              min={1}
              max={MAX_CAPACITY}
              disabled={discardsEverything}
              aria-describedby={describedBy}
              value={String(settings.webviewCapacity)}
              onCommit={async (next) => {
                const value = Number.parseInt(next, 10);
                if (!Number.isFinite(value)) {
                  return null;
                }
                const stored = await update({ webviewCapacity: Math.min(Math.max(value, 1), MAX_CAPACITY) });
                return stored && String(stored.webviewCapacity);
              }}
            />
            <span className="hint">{t("settings.capacity.help")}</span>
          </label>
        )}
      </DisabledFor>

      <DisabledFor reason={discardsEverything ? t("settings.disabledByDiscard") : null}>
        {(describedBy) => (
          <label className="field">
            <span className="label">{t("settings.keptMemory")}</span>
            <DraftInput
              type="number"
              min={MIN_KEPT_MEMORY_MB}
              max={MAX_KEPT_MEMORY_MB}
              disabled={discardsEverything}
              aria-describedby={describedBy}
              value={String(settings.keptMemoryMb)}
              onCommit={async (next) => {
                const value = Number.parseInt(next, 10);
                if (!Number.isFinite(value)) {
                  return null;
                }
                const keptMemoryMb = Math.min(Math.max(value, MIN_KEPT_MEMORY_MB), MAX_KEPT_MEMORY_MB);
                const stored = await update({ keptMemoryMb });
                return stored && String(stored.keptMemoryMb);
              }}
            />
            <span className="hint">{t("settings.keptMemory.help")}</span>
          </label>
        )}
      </DisabledFor>

      <DisabledFor reason={discardsEverything ? t("settings.disabledByDiscard") : null}>
        {(describedBy) => (
          <PolicySelect
            label={t("settings.freeze")}
            help={t("settings.freeze.help")}
            value={settings.freezeTabs}
            disabled={discardsEverything}
            describedBy={describedBy}
            onChange={(freezeTabs) => void update({ freezeTabs })}
          />
        )}
      </DisabledFor>

      <PolicySelect
        label={t("settings.discard")}
        help={t("settings.discard.help")}
        value={settings.discardTabs}
        onChange={(discardTabs) => void update({ discardTabs })}
      />
    </section>
  );
}

interface PolicySelectProps {
  label: string;
  help: string;
  value: Policy;
  disabled?: boolean | undefined;
  describedBy?: string | undefined;
  onChange: (value: Policy) => void;
}

function PolicySelect({ label, help, value, disabled, describedBy, onChange }: PolicySelectProps) {
  return (
    <label className="field">
      <span className="label">{label}</span>
      <select
        className="select"
        value={value}
        disabled={disabled}
        aria-describedby={describedBy}
        onChange={(event) => {
          const next = event.target.value;
          if (isPolicy(next)) {
            onChange(next);
          }
        }}
      >
        {Object.entries(POLICIES).map(([policy, key]) => (
          <option key={policy} value={policy}>
            {t(key)}
          </option>
        ))}
      </select>
      <span className="hint">{help}</span>
    </label>
  );
}

interface DisabledForProps {
  /** Why the wrapped control is disabled, or `null` while it is not. */
  reason: string | null;
  /** Renders the control, given the id of the explanation to point at. */
  children: (describedBy: string | undefined) => ReactNode;
}

/**
 * Explains why a control is disabled, on hover.
 *
 * The tooltip hangs off a wrapper because a disabled control receives no
 * pointer events of its own.
 */
function DisabledFor({ reason, children }: DisabledForProps) {
  const id = useId();
  if (!reason) {
    return children(undefined);
  }
  return (
    <span className="tooltip flex w-full" data-tooltip-position="top">
      <span className="w-full">{children(id)}</span>
      <span className="tooltip-bubble" role="tooltip" id={id}>
        {reason}
      </span>
    </span>
  );
}
