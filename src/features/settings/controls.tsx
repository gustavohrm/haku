import { type ThemePreference } from "@app/theme";
import { type Policy, type Preset } from "@bindings";
import { commands } from "@ipc/commands";
import { useExtensions } from "@ipc/hooks";
import { describeError } from "@ipc/result";
import { applySettingsPreset } from "@ipc/store";
import { cx } from "@shared/class-names";
import { t, type TranslationKey } from "@shared/i18n";
import { getVersion } from "@tauri-apps/api/app";
import { useEffect, useId, useState } from "react";

import { DraftInput } from "./draft-input";
import { type ControlProps } from "./types";

/*
  The control of each setting row. A row lays out the title and description;
  a control only renders the input and writes its value through `update`.
*/

const MAX_CAPACITY = 12;

/** Bounds of the background memory budget, in megabytes. */
const MIN_KEPT_MEMORY_MB = 0;
const MAX_KEPT_MEMORY_MB = 16384;

/**
 * Selects in a row share one width, so their right edges and arrows line up.
 * Borderless, as every control in settings is: the plate groups them already.
 */
const SELECT_CLASS = "select soft edgeless w-44";

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

const THEMES: { value: ThemePreference; icon: string; label: TranslationKey }[] = [
  { value: "system", icon: "ic-monitor", label: "settings.theme.system" },
  { value: "light", icon: "ic-sun", label: "settings.theme.light" },
  { value: "dark", icon: "ic-moon", label: "settings.theme.dark" },
];

/**
 * The theme as a group of icon buttons, one per choice, each named by a tooltip.
 *
 * The buttons are native radios under their icons, so the group is one stop in
 * the tab order and the arrow keys move between choices, as any radio group
 * does. The group itself draws nothing; the choice is a soft plate in the
 * text colour, which reads on either theme where a surface token would not.
 */
export function ThemeControl({ labelId, describedBy, settings, update }: ControlProps) {
  const name = useId();
  return (
    <fieldset aria-labelledby={labelId} aria-describedby={describedBy} className="m-0 flex gap-0.5 border-0 p-0">
      {THEMES.map((theme) => (
        <ThemeOption
          key={theme.value}
          name={name}
          icon={theme.icon}
          label={t(theme.label)}
          checked={settings.theme === theme.value}
          // Only stored here: the shell applies the theme Rust stored, so a
          // failed write cannot leave the page in a theme the settings lack.
          onChoose={() => void update({ theme: theme.value })}
        />
      ))}
    </fieldset>
  );
}

interface ThemeOptionProps {
  name: string;
  icon: string;
  label: string;
  checked: boolean;
  onChoose: () => void;
}

function ThemeOption({ name, icon, label, checked, onChoose }: ThemeOptionProps) {
  const bubbleId = useId();
  return (
    <span className="tooltip flex">
      <label
        className={cx(
          "has-focus-visible:outline-text/30 flex size-8 cursor-pointer items-center justify-center rounded-(--radius-small) transition-colors has-focus-visible:outline-2",
          checked ? "bg-text/10 text-text" : "text-text-secondary hover:bg-text/6 hover:text-text",
        )}
      >
        <input
          type="radio"
          name={name}
          className="sr-only"
          aria-label={label}
          aria-describedby={bubbleId}
          checked={checked}
          onChange={onChoose}
        />
        <i className={cx(icon, "ic-md")} aria-hidden="true" />
      </label>
      <span id={bubbleId} role="tooltip" className="tooltip-bubble edgeless">
        {label}
      </span>
    </span>
  );
}

/**
 * Which preset is in effect is asked of Rust rather than worked out here, so
 * the preset values have one definition.
 */
export function PresetControl({ id, describedBy, settings }: ControlProps) {
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

  return (
    <select
      id={id}
      className={SELECT_CLASS}
      aria-describedby={describedBy}
      value={preset ?? "custom"}
      onChange={(event) => {
        const next = event.target.value;
        if (isPreset(next)) {
          void applySettingsPreset(next);
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
  );
}

export function CapacityControl({ id, describedBy, disabled, settings, update }: ControlProps) {
  return (
    <DraftInput
      id={id}
      type="number"
      className="w-24"
      min={1}
      max={MAX_CAPACITY}
      disabled={disabled}
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
  );
}

export function KeptMemoryControl({ id, describedBy, disabled, settings, update }: ControlProps) {
  return (
    <DraftInput
      id={id}
      type="number"
      className="w-24"
      min={MIN_KEPT_MEMORY_MB}
      max={MAX_KEPT_MEMORY_MB}
      disabled={disabled}
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
  );
}

export function FreezeControl({ settings, update, ...props }: ControlProps) {
  return <PolicySelect {...props} value={settings.freezeTabs} onChange={(freezeTabs) => void update({ freezeTabs })} />;
}

export function DiscardControl({ settings, update, ...props }: ControlProps) {
  return (
    <PolicySelect {...props} value={settings.discardTabs} onChange={(discardTabs) => void update({ discardTabs })} />
  );
}

interface PolicySelectProps {
  id: string;
  describedBy: string | undefined;
  disabled: boolean;
  value: Policy;
  onChange: (value: Policy) => void;
}

function PolicySelect({ id, describedBy, disabled, value, onChange }: PolicySelectProps) {
  return (
    <select
      id={id}
      className={SELECT_CLASS}
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
  );
}

/**
 * Sites kept loaded from the site menu, which is otherwise the only place to
 * undo one: by visiting each site again.
 */
export function KeptSitesControl({ labelId, describedBy, settings, update }: ControlProps) {
  const sites = settings.keptSites;
  if (sites.length === 0) {
    return <p className="text-text-secondary text-sm">{t("settings.keptSites.empty")}</p>;
  }
  return (
    <ul className="flex flex-col" aria-labelledby={labelId} aria-describedby={describedBy}>
      {sites.map((site) => (
        <li key={site} className="flex items-center gap-2">
          {/* The button is described by the site, so it reads as "Remove, example.com". */}
          <span id={`kept-site-${site}`} className="min-w-0 flex-1 truncate">
            {site}
          </span>
          <button
            type="button"
            className="btn icon ghost"
            aria-label={t("settings.keptSites.remove")}
            aria-describedby={`kept-site-${site}`}
            title={t("settings.keptSites.remove")}
            onClick={() =>
              void update((current) => ({ ...current, keptSites: current.keptSites.filter((kept) => kept !== site) }))
            }
          >
            <i className="ic-x ic-sm" aria-hidden="true" />
          </button>
        </li>
      ))}
    </ul>
  );
}

export function SearchUrlControl({ id, describedBy, settings, update }: ControlProps) {
  return (
    <DraftInput
      id={id}
      type="text"
      className="w-full"
      aria-describedby={describedBy}
      value={settings.searchUrl}
      onCommit={async (next) => (await update({ searchUrl: next }))?.searchUrl ?? null}
    />
  );
}

export function HomeUrlControl({ id, describedBy, settings, update }: ControlProps) {
  return (
    <DraftInput
      id={id}
      type="text"
      className="w-full"
      aria-describedby={describedBy}
      value={settings.homeUrl}
      onCommit={async (next) => (await update({ homeUrl: next }))?.homeUrl ?? null}
    />
  );
}

/**
 * Extensions installed from the extensions folder, each with a switch.
 *
 * Switching one off is stored as an exception in the settings, which Rust
 * applies to every page at once. Popups are opened from the toolbar.
 */
export function ExtensionsControl({ labelId, settings, update }: ControlProps) {
  const extensions = useExtensions();
  const disabled = settings.disabledExtensions;

  const setEnabled = (id: string, enabled: boolean) => {
    void update((current) => {
      const others = current.disabledExtensions.filter((other) => other !== id);
      return { ...current, disabledExtensions: enabled ? others : [...others, id] };
    });
  };

  return (
    <fieldset className="m-0 flex min-w-0 flex-col gap-2 border-0 p-0" aria-labelledby={labelId}>
      {extensions !== null && extensions.error !== null && (
        <p className="text-text-secondary text-sm">
          {t("settings.extensions.error")} {describeError(extensions.error)}
        </p>
      )}

      {extensions?.installed.length === 0 && extensions.error === null && (
        <p className="text-text-secondary text-sm">{t("settings.extensions.empty")}</p>
      )}

      {extensions?.installed.map((extension) => (
        <div key={extension.id} className="flex items-center gap-1.5">
          <label htmlFor={`extension-${extension.id}`} className="min-w-0 flex-1 truncate">
            {extension.name}
          </label>
          <input
            id={`extension-${extension.id}`}
            type="checkbox"
            className="switch"
            checked={!disabled.includes(extension.id)}
            onChange={(event) => setEnabled(extension.id, event.target.checked)}
          />
        </div>
      ))}

      {extensions !== null && extensions.failed.length > 0 && (
        <p className="text-text-secondary text-sm">
          {t("settings.extensions.failed")} {extensions.failed.join(", ")}
        </p>
      )}
    </fieldset>
  );
}

export function ExtensionsFolderControl({ id, describedBy }: ControlProps) {
  return (
    <button
      id={id}
      type="button"
      className="btn"
      aria-describedby={describedBy}
      onClick={() => void commands.openExtensionsFolder()}
    >
      {t("settings.extensions.folder")}
    </button>
  );
}

export function VersionControl({ labelId }: ControlProps) {
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    let current = true;
    const read = async () => {
      const value = await getVersion();
      if (current) {
        setVersion(value);
      }
    };
    void read();
    return () => {
      current = false;
    };
  }, []);
  return (
    <output className="text-text-secondary tabular-nums" aria-labelledby={labelId}>
      {version}
    </output>
  );
}

/** A button opening an internal page, for the About section's links. */
function OpenPage({ id, describedBy, url }: { id: string; describedBy: string | undefined; url: string }) {
  return (
    <button
      id={id}
      type="button"
      className="btn"
      aria-describedby={describedBy}
      onClick={() => void commands.openTab(url, true)}
    >
      {t("settings.open")}
    </button>
  );
}

export function MemoryPageControl({ id, describedBy }: ControlProps) {
  return <OpenPage id={id} describedBy={describedBy} url="haku://memory" />;
}

export function HistoryPageControl({ id, describedBy }: ControlProps) {
  return <OpenPage id={id} describedBy={describedBy} url="haku://history" />;
}
