import { type ThemePreference } from "@app/theme";
import {
  type BarLayout,
  type Extension,
  type Policy,
  type Preset,
  type SidebarSide,
  type TabLayout,
  type ToolbarPlacement,
} from "@bindings";
import { openMenu } from "@features/context-menu/store";
import { ExtensionIcon } from "@features/extensions/extension-icon";
import { ADDING, addExtension, removeExtension, useExtensionChange } from "@features/extensions/manage";
import { reportUnexpected } from "@features/feedback/feedback";
import { MENU_GAP } from "@features/overlays/use-menu-dismiss";
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
  return (
    <IconChoices
      labelId={labelId}
      describedBy={describedBy}
      choices={THEMES}
      value={settings.theme}
      // Only stored here: the shell applies the theme Rust stored, so a
      // failed write cannot leave the page in a theme the settings lack.
      onChoose={(theme) => void update({ theme })}
    />
  );
}

const TAB_LAYOUTS: { value: TabLayout; icon: string; label: TranslationKey }[] = [
  { value: "horizontal", icon: "ic-panel-top", label: "settings.tabLayout.horizontal" },
  { value: "vertical", icon: "ic-panel-left", label: "settings.tabLayout.vertical" },
];

/** Where the tabs go, as the theme's icon buttons; every window follows what is stored. */
export function TabLayoutControl({ labelId, describedBy, settings, update }: ControlProps) {
  return (
    <IconChoices
      labelId={labelId}
      describedBy={describedBy}
      choices={TAB_LAYOUTS}
      value={settings.tabLayout}
      onChoose={(tabLayout) => void update({ tabLayout })}
    />
  );
}

const BAR_LAYOUTS: { value: BarLayout; icon: string; label: TranslationKey }[] = [
  { value: "docked", icon: "ic-app-window", label: "settings.barLayout.docked" },
  { value: "floating", icon: "ic-panel-top-dashed", label: "settings.barLayout.floating" },
];

/** Whether the bars take space or float over the page, as the theme's icon buttons. */
export function BarLayoutControl({ labelId, describedBy, settings, update }: ControlProps) {
  return (
    <IconChoices
      labelId={labelId}
      describedBy={describedBy}
      choices={BAR_LAYOUTS}
      value={settings.barLayout}
      onChoose={(barLayout) => void update({ barLayout })}
    />
  );
}

const SIDEBAR_SIDES: { value: SidebarSide; icon: string; label: TranslationKey }[] = [
  { value: "left", icon: "ic-panel-left", label: "settings.sidebarSide.left" },
  { value: "right", icon: "ic-panel-right", label: "settings.sidebarSide.right" },
];

/** Which side the tab column is on, as the theme's icon buttons. */
export function SidebarSideControl({ labelId, describedBy, disabled, settings, update }: ControlProps) {
  return (
    <IconChoices
      labelId={labelId}
      describedBy={describedBy}
      disabled={disabled}
      choices={SIDEBAR_SIDES}
      value={settings.sidebarSide}
      onChoose={(sidebarSide) => void update({ sidebarSide })}
    />
  );
}

const TOOLBAR_PLACEMENTS: { value: ToolbarPlacement; icon: string; label: TranslationKey }[] = [
  { value: "top", icon: "ic-panel-top", label: "settings.toolbarPlacement.top" },
  { value: "sidebar", icon: "ic-panel-left-dashed", label: "settings.toolbarPlacement.sidebar" },
];

/** Whether the toolbar is a row on top or heads the sidebar, as the theme's icon buttons. */
export function ToolbarPlacementControl({ labelId, describedBy, disabled, settings, update }: ControlProps) {
  return (
    <IconChoices
      labelId={labelId}
      describedBy={describedBy}
      disabled={disabled}
      choices={TOOLBAR_PLACEMENTS}
      value={settings.toolbarPlacement}
      onChoose={(toolbarPlacement) => void update({ toolbarPlacement })}
    />
  );
}

interface IconChoicesProps<T extends string> {
  labelId: string;
  describedBy: string | undefined;
  /** The row has no effect right now; the choice still shows what is stored. */
  disabled?: boolean;
  choices: readonly { value: T; icon: string; label: TranslationKey }[];
  value: string;
  onChoose: (value: T) => void;
}

function IconChoices<T extends string>({
  labelId,
  describedBy,
  disabled = false,
  choices,
  value,
  onChoose,
}: IconChoicesProps<T>) {
  const name = useId();
  return (
    <fieldset
      aria-labelledby={labelId}
      aria-describedby={describedBy}
      disabled={disabled}
      className="m-0 flex gap-0.5 border-0 p-0 disabled:opacity-50 [&:disabled_label]:pointer-events-none"
    >
      {choices.map((choice) => (
        <ThemeOption
          key={choice.value}
          name={name}
          icon={choice.icon}
          label={t(choice.label)}
          checked={value === choice.value}
          onChoose={() => onChoose(choice.value)}
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
export function AddExtensionControl({ labelId, describedBy }: ControlProps) {
  const change = useExtensionChange();
  const adding = change === ADDING;
  return (
    <fieldset className="m-0 flex gap-2 border-0 p-0" aria-labelledby={labelId} aria-describedby={describedBy}>
      <button
        type="button"
        className="btn soft edgeless gap-2"
        disabled={change !== null}
        onClick={() => void addExtension("file")}
      >
        <i className="ic-file-archive ic-sm" aria-hidden="true" />
        {adding ? t("settings.extensions.adding") : t("settings.extensions.addFile")}
      </button>
      <button
        type="button"
        className="btn soft edgeless gap-2"
        disabled={change !== null}
        onClick={() => void addExtension("folder")}
      >
        <i className="ic-folder-plus ic-sm" aria-hidden="true" />
        {t("settings.extensions.addFolder")}
      </button>
    </fieldset>
  );
}

export function ExtensionsControl({ labelId, settings, update }: ControlProps) {
  const extensions = useExtensions();
  const change = useExtensionChange();
  const ids = useId();
  const disabled = settings.disabledExtensions;
  const pinned = settings.pinnedExtensions;

  const setEnabled = (id: string, enabled: boolean) => {
    void update((current) => {
      const others = current.disabledExtensions.filter((other) => other !== id);
      return { ...current, disabledExtensions: enabled ? others : [...others, id] };
    });
  };
  const setPinned = (id: string, isPinned: boolean) => {
    void update((current) => {
      const others = current.pinnedExtensions.filter((other) => other !== id);
      return { ...current, pinnedExtensions: isPinned ? [...others, id] : others };
    });
  };

  const openActions = (extension: Extension, button: HTMLElement) => {
    const isOn = !disabled.includes(extension.id);
    const isPinned = pinned.includes(extension.id);
    const box = button.getBoundingClientRect();
    openMenu({
      at: { x: box.left, y: box.bottom + MENU_GAP },
      label: extension.name,
      items: [
        ...(extension.options === null
          ? []
          : [
              {
                kind: "command" as const,
                id: "options",
                label: t("settings.extensions.options"),
                icon: "ic-settings-2",
                disabled: !isOn,
                disabledReason: isOn ? undefined : t("settings.extensions.options.off"),
                onSelect: () => void commands.openTab(extension.options, true),
              },
            ]),
        {
          kind: "checkbox",
          id: "pin",
          label: t("extensions.pin"),
          icon: "ic-pin",
          checked: isPinned,
          onSelect: () => setPinned(extension.id, !isPinned),
        },
        { kind: "separator", id: "update" },
        {
          kind: "command",
          id: "updateFile",
          label: t("settings.extensions.updateFile"),
          icon: "ic-file-archive",
          disabled: change !== null,
          onSelect: () => void addExtension("file"),
        },
        {
          kind: "command",
          id: "updateFolder",
          label: t("settings.extensions.updateFolder"),
          icon: "ic-folder-plus",
          disabled: change !== null,
          onSelect: () => void addExtension("folder"),
        },
        { kind: "separator", id: "uninstall" },
        {
          kind: "command",
          id: "uninstall",
          label: t("settings.extensions.uninstall"),
          icon: "ic-trash-2",
          disabled: change !== null,
          onSelect: () => void removeExtension(extension.folder, extension),
        },
      ],
    });
  };

  if (extensions === null) {
    return null;
  }
  const isEmpty = extensions.installed.length === 0 && extensions.failed.length === 0 && extensions.error === null;

  return (
    <div className="flex flex-col gap-3">
      {extensions.error !== null && (
        <p className="text-text-secondary text-sm">
          {t("settings.extensions.error")} {describeError(extensions.error)}
        </p>
      )}

      {isEmpty && (
        <div className="text-text-secondary flex items-center gap-3 py-2">
          <i className="ic-puzzle ic-lg shrink-0" aria-hidden="true" />
          <p className="flex flex-col text-sm">
            <span className="text-text">{t("settings.extensions.empty")}</span>
            {t("settings.extensions.empty.help")}
          </p>
        </div>
      )}

      {extensions.installed.length > 0 && (
        <ul className="flex flex-col gap-1" aria-labelledby={labelId}>
          {extensions.installed.map((extension, index) => {
            const isOn = !disabled.includes(extension.id);
            const isRemoving = change === extension.folder;
            const nameId = `${ids}-name-${index}`;
            const details = [
              isRemoving ? t("settings.extensions.removing") : extension.version || t("settings.extensions.noVersion"),
              !isOn && t("settings.extensions.off"),
              isOn && pinned.includes(extension.id) && t("settings.extensions.pinned"),
            ].filter(Boolean);
            return (
              <li key={extension.folder} className="flex min-h-12 items-center gap-3" aria-busy={isRemoving}>
                <span className={cx("grid size-8 shrink-0 place-items-center", !isOn && "opacity-40 grayscale")}>
                  <ExtensionIcon extension={extension} large />
                </span>
                <span className="flex min-w-0 flex-1 flex-col">
                  <span id={nameId} className={cx("truncate", !isOn && "text-text-secondary")}>
                    {extension.name}
                  </span>
                  <span className="text-text-secondary truncate text-sm tabular-nums">{details.join(" · ")}</span>
                </span>
                <input
                  type="checkbox"
                  className="switch"
                  aria-labelledby={nameId}
                  checked={isOn}
                  disabled={isRemoving}
                  onChange={(event) => setEnabled(extension.id, event.target.checked)}
                />
                <button
                  type="button"
                  className="btn icon ghost"
                  aria-label={t("settings.extensions.actions").replace("{name}", extension.name)}
                  aria-haspopup="menu"
                  title={t("settings.extensions.actions").replace("{name}", extension.name)}
                  disabled={isRemoving}
                  onClick={(event) => openActions(extension, event.currentTarget)}
                >
                  <i className="ic-ellipsis ic-sm" aria-hidden="true" />
                </button>
              </li>
            );
          })}
        </ul>
      )}

      {extensions.failed.length > 0 && (
        <section className="flex flex-col gap-1" aria-label={t("settings.extensions.failed")}>
          <h3 className="text-text-secondary text-sm font-medium">{t("settings.extensions.failed")}</h3>
          <ul className="flex flex-col gap-1">
            {extensions.failed.map((failed, index) => (
              <li key={failed.folder} className="flex min-h-12 items-center gap-3" aria-busy={change === failed.folder}>
                <span className="grid size-8 shrink-0 place-items-center">
                  <i className="ic-triangle-alert ic-md text-destructive" aria-hidden="true" />
                </span>
                <span className="flex min-w-0 flex-1 flex-col">
                  <span id={`${ids}-failed-${index}`} className="truncate">
                    {failed.folder}
                  </span>
                  <span className="text-text-secondary text-sm break-words">{describeError(failed.error)}</span>
                </span>
                <button
                  type="button"
                  className="btn soft edgeless"
                  aria-describedby={`${ids}-failed-${index}`}
                  disabled={change !== null}
                  onClick={() => void removeExtension(failed.folder, null)}
                >
                  {t("settings.extensions.remove")}
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

export function ExtensionsFolderControl({ id, describedBy }: ControlProps) {
  return (
    <button
      id={id}
      type="button"
      className="btn soft edgeless gap-2"
      aria-describedby={describedBy}
      onClick={() => void commands.openExtensionsFolder()}
    >
      <i className="ic-folder-open ic-sm" aria-hidden="true" />
      {t("settings.extensions.folder")}
    </button>
  );
}

export function VersionControl({ labelId }: ControlProps) {
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    let current = true;
    const read = async () => {
      try {
        const value = await getVersion();
        if (current) {
          setVersion(value);
        }
      } catch (error) {
        reportUnexpected(error);
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
