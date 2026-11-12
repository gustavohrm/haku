import { type ChordRefusal, type KeyBinding, type Settings, type Shortcut } from "@bindings";
import { commands } from "@ipc/commands";
import { useKeymap } from "@ipc/hooks";
import { keymapStore } from "@ipc/store";
import { cx } from "@shared/class-names";
import { t, type TranslationKey } from "@shared/i18n";
import { type KeyboardEvent, useEffect, useRef, useState } from "react";

import { type ControlProps, type SettingGroup, type SettingRowDefinition } from "./types";

/*
  The Shortcuts section. Every action Rust can bind is an entry below, with
  its group and title, and becomes one row. `satisfies` makes a new action in
  the bindings a type error here until it has an entry.
  [Settings page § Shortcuts](../../../docs/specs/settings-page.md#shortcuts).
*/

type Group = "tabs" | "windows" | "page" | "navigation";

const ACTIONS = {
  newTab: { group: "tabs", title: "shortcut.newTab" },
  closeTab: { group: "tabs", title: "shortcut.closeTab" },
  reopenClosedTab: { group: "tabs", title: "shortcut.reopenClosedTab" },
  duplicateTab: { group: "tabs", title: "shortcut.duplicateTab" },
  nextTab: { group: "tabs", title: "shortcut.nextTab" },
  previousTab: { group: "tabs", title: "shortcut.previousTab" },
  tab1: { group: "tabs", title: "shortcut.tab1" },
  tab2: { group: "tabs", title: "shortcut.tab2" },
  tab3: { group: "tabs", title: "shortcut.tab3" },
  tab4: { group: "tabs", title: "shortcut.tab4" },
  tab5: { group: "tabs", title: "shortcut.tab5" },
  tab6: { group: "tabs", title: "shortcut.tab6" },
  tab7: { group: "tabs", title: "shortcut.tab7" },
  tab8: { group: "tabs", title: "shortcut.tab8" },
  lastTab: { group: "tabs", title: "shortcut.lastTab" },
  moveTabLeft: { group: "tabs", title: "shortcut.moveTabLeft" },
  moveTabRight: { group: "tabs", title: "shortcut.moveTabRight" },
  toggleSplit: { group: "tabs", title: "shortcut.toggleSplit" },
  newWindow: { group: "windows", title: "shortcut.newWindow" },
  newPrivateWindow: { group: "windows", title: "shortcut.newPrivateWindow" },
  closeWindow: { group: "windows", title: "shortcut.closeWindow" },
  openBookmarks: { group: "windows", title: "shortcut.openBookmarks" },
  toggleBookmarksBar: { group: "windows", title: "shortcut.toggleBookmarksBar" },
  openHistory: { group: "windows", title: "shortcut.openHistory" },
  reload: { group: "page", title: "shortcut.reload" },
  find: { group: "page", title: "shortcut.find" },
  print: { group: "page", title: "shortcut.print" },
  savePage: { group: "page", title: "shortcut.savePage" },
  resetZoom: { group: "page", title: "shortcut.resetZoom" },
  bookmarkTab: { group: "page", title: "shortcut.bookmarkTab" },
  devTools: { group: "page", title: "shortcut.devTools" },
  focusAddress: { group: "navigation", title: "shortcut.focusAddress" },
  back: { group: "navigation", title: "shortcut.back" },
  forward: { group: "navigation", title: "shortcut.forward" },
  home: { group: "navigation", title: "shortcut.home" },
  memory: { group: "navigation", title: "shortcut.memory" },
} as const satisfies Record<Shortcut, { group: Group; title: TranslationKey }>;

const GROUP_TITLES: Record<Group, TranslationKey> = {
  tabs: "settings.shortcuts.tabs",
  windows: "settings.shortcuts.windows",
  page: "settings.shortcuts.page",
  navigation: "settings.shortcuts.navigation",
};

/** The chords bound to `shortcut` now, as Rust reports them. */
function chordsOf(keymap: readonly KeyBinding[], shortcut: Shortcut): string[] {
  return keymap.find((binding) => binding.shortcut === shortcut)?.chords ?? [];
}

/**
 * The chords `shortcut` has under `settings`: the user's own list when they
 * changed it, since the keymap is read again only after the change is
 * announced, and two changes in quick succession would otherwise build on
 * the older one.
 */
function chordsUnder(settings: Settings, keymap: readonly KeyBinding[], shortcut: Shortcut): string[] {
  return settings.shortcuts[shortcut] ?? chordsOf(keymap, shortcut);
}

/**
 * The settings with `chord` added to `shortcut`, and taken from any other
 * action the user gave it. One that has it by default loses it in Rust.
 */
export function withChord(
  settings: Settings,
  keymap: readonly KeyBinding[],
  shortcut: Shortcut,
  chord: string,
): Settings {
  const shortcuts: Settings["shortcuts"] = {};
  for (const [other, chords] of Object.entries(settings.shortcuts) as [Shortcut, string[]][]) {
    shortcuts[other] = chords.filter((bound) => bound !== chord);
  }
  shortcuts[shortcut] = [...chordsUnder(settings, keymap, shortcut), chord];
  return { ...settings, shortcuts };
}

/** The settings with `chord` no longer bound to `shortcut`. */
export function withoutChord(
  settings: Settings,
  keymap: readonly KeyBinding[],
  shortcut: Shortcut,
  chord: string,
): Settings {
  const chords = chordsUnder(settings, keymap, shortcut).filter((bound) => bound !== chord);
  return { ...settings, shortcuts: { ...settings.shortcuts, [shortcut]: chords } };
}

/**
 * The settings with `shortcut` back on its defaults, taking them from any
 * other action the user gave one, since the user's binding would otherwise
 * keep it.
 */
export function withDefaults(settings: Settings, keymap: readonly KeyBinding[], shortcut: Shortcut): Settings {
  const defaults = defaultsOf(keymap, shortcut);
  const shortcuts: Settings["shortcuts"] = {};
  for (const [other, chords] of Object.entries(settings.shortcuts) as [Shortcut, string[]][]) {
    if (other !== shortcut) {
      shortcuts[other] = chords.filter((bound) => !defaults.includes(bound));
    }
  }
  return { ...settings, shortcuts };
}

function defaultsOf(keymap: readonly KeyBinding[], shortcut: Shortcut): string[] {
  return keymap.find((binding) => binding.shortcut === shortcut)?.defaults ?? [];
}

/**
 * Whether `shortcut` differs from its defaults: changed by the user, or
 * having lost a default to another action the user gave it.
 */
function isChanged(settings: Settings, keymap: readonly KeyBinding[], shortcut: Shortcut): boolean {
  if (settings.shortcuts[shortcut] !== undefined) {
    return true;
  }
  const chords = chordsOf(keymap, shortcut);
  const defaults = defaultsOf(keymap, shortcut);
  return chords.length !== defaults.length || chords.some((chord) => !defaults.includes(chord));
}

/** A chord's keys, as "Ctrl+Shift+T" names them; the last may be the plus key itself. */
function keysOf(chord: string): string[] {
  const keys = chord.split("+").filter((key) => key !== "");
  return chord.endsWith("+") ? [...keys, "+"] : keys;
}

/** What the recorder shows besides the action's chords. */
type Recording =
  | { kind: "idle" }
  | { kind: "waiting"; refused: { chord: string; refusal: ChordRefusal } | null }
  | { kind: "conflict"; chord: string; owner: Shortcut };

const REFUSALS: Record<ChordRefusal, TranslationKey> = {
  needsModifier: "settings.shortcuts.needsModifier",
  reserved: "settings.shortcuts.reserved",
};

/**
 * Shown only on the setting row's hover or focus, so about 35 rows read as
 * actions and their keys rather than as a wall of buttons
 * ([Interface guidelines § Lists](../../../docs/guidelines/interface.md#lists)).
 * The row is the `group/setting` that `SettingRow` draws.
 */
const ON_ROW_HOVER = "opacity-0 group-focus-within/setting:opacity-100 group-hover/setting:opacity-100";

/**
 * An action's chords at the row's end, each drawn as one keycap, with Add
 * before them and, once the action differs from its defaults, Reset before
 * that. The actions come first so the keys keep one edge down the page
 * whether or not a row shows any. A chord is its own remove button: under the
 * pointer or focus its keys fade behind a cross. Add shows on the row's hover
 * or focus; Reset stays in view, since it also says the action was changed,
 * and so does Add while it records.
 *
 * Add asks Rust for the next chord pressed, since a bound chord never reaches
 * the interface. A refused chord is shown and recording goes on; a chord
 * another action uses is offered for this one.
 */
function ShortcutControl({ shortcut, labelId, describedBy, settings, update }: ControlProps & { shortcut: Shortcut }) {
  const keymap = useKeymap();
  const chords = chordsOf(keymap, shortcut);
  const [recording, setRecording] = useState<Recording>({ kind: "idle" });
  // Each recording's turn, so the answer to one replaced or cancelled is not
  // taken as the current one's.
  const turn = useRef(0);
  // Whether Rust holds a recording for this row, which leaving must cancel.
  // A finished one must not be: another row may be recording by then.
  const pending = useRef(false);
  const messageId = `shortcut-message-${shortcut}`;

  useEffect(
    () => () => {
      if (pending.current) {
        pending.current = false;
        turn.current++;
        void commands.cancelShortcutRecording();
      }
    },
    [],
  );

  const record = async () => {
    const mine = ++turn.current;
    let refused: { chord: string; refusal: ChordRefusal } | null = null;
    for (;;) {
      setRecording({ kind: "waiting", refused });
      pending.current = true;
      // oxlint-disable-next-line no-await-in-loop -- Each chord is asked for once the last was refused.
      const result = await commands.recordShortcut();
      if (mine !== turn.current) {
        return;
      }
      pending.current = false;
      const recorded = result.status === "ok" ? result.data : null;
      if (recorded === null) {
        setRecording({ kind: "idle" });
        return;
      }
      if (recorded.refusal !== null) {
        refused = { chord: recorded.chord, refusal: recorded.refusal };
        continue;
      }
      // The keymap as it is now: another window may have changed it meanwhile.
      const latest = keymapStore.get();
      const owner = latest.find((binding) => binding.shortcut !== shortcut && binding.chords.includes(recorded.chord));
      if (owner) {
        setRecording({ kind: "conflict", chord: recorded.chord, owner: owner.shortcut });
        return;
      }
      setRecording({ kind: "idle" });
      if (!chordsOf(latest, shortcut).includes(recorded.chord)) {
        void update((current) => withChord(current, keymapStore.get(), shortcut, recorded.chord));
      }
      return;
    }
  };

  const stopRecording = () => {
    if (recording.kind === "waiting") {
      pending.current = false;
      turn.current++;
      setRecording({ kind: "idle" });
      void commands.cancelShortcutRecording();
    }
  };

  const message =
    recording.kind === "conflict"
      ? t("settings.shortcuts.usedBy")
          .replace("{chord}", () => recording.chord)
          .replace("{action}", () => t(ACTIONS[recording.owner].title))
      : recording.kind === "waiting" && recording.refused
        ? t(REFUSALS[recording.refused.refusal]).replace("{chord}", () => recording.refused?.chord ?? "")
        : null;

  return (
    <div className="flex flex-col items-end gap-1.5">
      <div className="flex flex-wrap items-center justify-end gap-1">
        {isChanged(settings, keymap, shortcut) && (
          <button
            type="button"
            className="btn icon ghost sm text-text-secondary"
            aria-label={t("settings.shortcuts.resetLabel")}
            title={t("settings.shortcuts.resetLabel")}
            onClick={() => void update((current) => withDefaults(current, keymapStore.get(), shortcut))}
          >
            <i className="ic-rotate-ccw ic-sm" aria-hidden="true" />
          </button>
        )}
        <button
          type="button"
          className={cx(
            "btn sm",
            recording.kind === "waiting" ? "soft edgeless" : cx("icon ghost text-text-secondary", ON_ROW_HOVER),
          )}
          aria-label={
            recording.kind === "waiting" ? t("settings.shortcuts.recording") : t("settings.shortcuts.addLabel")
          }
          title={recording.kind === "waiting" ? undefined : t("settings.shortcuts.addLabel")}
          aria-describedby={message ? messageId : undefined}
          aria-pressed={recording.kind === "waiting"}
          onClick={() => recording.kind !== "waiting" && void record()}
          onBlur={stopRecording}
        >
          {recording.kind === "waiting" ? (
            t("settings.shortcuts.recording")
          ) : (
            <i className="ic-plus ic-sm" aria-hidden="true" />
          )}
        </button>
        <ul
          className="ms-1 flex flex-wrap items-center justify-end gap-2.5"
          aria-labelledby={labelId}
          aria-describedby={describedBy}
        >
          {chords.length === 0 && <li className="text-text-secondary text-sm">{t("settings.shortcuts.none")}</li>}
          {chords.map((chord) => (
            <li key={chord} className="flex">
              <button
                type="button"
                className="group/chord relative flex cursor-pointer rounded-(--radius-small) outline-offset-2"
                aria-label={t("settings.shortcuts.remove").replace("{chord}", () => chord)}
                title={t("settings.shortcuts.remove").replace("{chord}", () => chord)}
                onClick={() => void update((current) => withoutChord(current, keymapStore.get(), shortcut, chord))}
              >
                <kbd className="haku-key font-sans transition-opacity group-hover/chord:opacity-25 group-focus-visible/chord:opacity-25">
                  {keysOf(chord).join(" + ")}
                </kbd>
                <span
                  className="absolute inset-0 grid place-items-center opacity-0 transition-opacity group-hover/chord:opacity-100 group-focus-visible/chord:opacity-100"
                  aria-hidden="true"
                >
                  <i className="ic-x ic-sm" />
                </span>
              </button>
            </li>
          ))}
        </ul>
      </div>
      {message && (
        <div className="flex flex-wrap items-center justify-end gap-2 text-sm">
          <span id={messageId} className="text-text-secondary" aria-live="polite">
            {message}
          </span>
          {recording.kind === "conflict" && (
            <>
              <button
                type="button"
                className="btn primary sm"
                onClick={() => {
                  const { chord } = recording;
                  setRecording({ kind: "idle" });
                  void update((current) => withChord(current, keymapStore.get(), shortcut, chord));
                }}
              >
                {t("settings.shortcuts.useHere")}
              </button>
              <button type="button" className="btn ghost sm" onClick={() => setRecording({ kind: "idle" })}>
                {t("settings.shortcuts.cancel")}
              </button>
            </>
          )}
        </div>
      )}
    </div>
  );
}

function shortcutRow(shortcut: Shortcut, title: TranslationKey): SettingRowDefinition {
  function Control(props: ControlProps) {
    return <ShortcutControl {...props} shortcut={shortcut} />;
  }
  return { id: `shortcut-${shortcut}`, title, dense: true, labelsControl: false, Control };
}

/**
 * Reset all, which asks in place first, saying how many actions it puts back:
 * it has no undo, and the changes it drops may have taken a while to make.
 * The count is of the actions showing Reset.
 */
function ResetAllControl({ id, describedBy, settings, update }: ControlProps) {
  const keymap = useKeymap();
  const [asking, setAsking] = useState(false);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const resetRef = useRef<HTMLButtonElement>(null);
  // Focus follows the question, and goes back to Reset all once it is answered.
  const answered = useRef(false);

  useEffect(() => {
    if (asking) {
      cancelRef.current?.focus();
    } else if (answered.current) {
      answered.current = false;
      resetRef.current?.focus();
    }
  }, [asking]);

  const answer = (reset: boolean) => {
    answered.current = true;
    setAsking(false);
    if (reset) {
      void update({ shortcuts: {} });
    }
  };

  const onEscape = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      answer(false);
    }
  };

  if (asking) {
    const count = (Object.keys(ACTIONS) as Shortcut[]).filter((shortcut) =>
      isChanged(settings, keymap, shortcut),
    ).length;
    return (
      <div role="alert" className="flex flex-wrap items-center justify-end gap-2 text-sm">
        <p>
          {count === 1
            ? t("settings.shortcuts.resetAll.confirm.one")
            : t("settings.shortcuts.resetAll.confirm.many").replace("{count}", count.toLocaleString())}
        </p>
        <button ref={cancelRef} type="button" className="btn ghost" onClick={() => answer(false)} onKeyDown={onEscape}>
          {t("settings.shortcuts.cancel")}
        </button>
        <button type="button" className="btn destructive" onClick={() => answer(true)} onKeyDown={onEscape}>
          {t("settings.shortcuts.resetAll.button")}
        </button>
      </div>
    );
  }
  return (
    <button
      ref={resetRef}
      id={id}
      type="button"
      className="btn soft edgeless gap-2"
      aria-describedby={describedBy}
      disabled={Object.keys(settings.shortcuts).length === 0}
      onClick={() => setAsking(true)}
    >
      <i className="ic-rotate-ccw ic-sm" aria-hidden="true" />
      {t("settings.shortcuts.resetAll.button")}
    </button>
  );
}

/**
 * The Shortcuts section's groups: one row per action, by group, then Reset
 * all, which is last because it is the rarest thing done here.
 */
export const SHORTCUT_GROUPS: SettingGroup[] = [
  ...(Object.keys(GROUP_TITLES) as Group[]).map((group) => ({
    title: GROUP_TITLES[group],
    rows: (Object.entries(ACTIONS) as [Shortcut, (typeof ACTIONS)[Shortcut]][])
      .filter(([, action]) => action.group === group)
      .map(([shortcut, action]) => shortcutRow(shortcut, action.title)),
  })),
  {
    title: "settings.shortcuts.defaults",
    rows: [
      {
        id: "resetShortcuts",
        title: "settings.shortcuts.resetAll",
        description: "settings.shortcuts.resetAll.help",
        keywords: "settings.shortcuts.resetAll.keywords",
        Control: ResetAllControl,
      },
    ],
  },
];
