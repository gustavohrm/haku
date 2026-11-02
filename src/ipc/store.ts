/**
 * The interface's view of state Rust owns.
 *
 * There is no client-side tab model. Rust emits the whole browser state on every
 * change and this holds the latest one, so the interface can never drift out of
 * step with the webviews it is describing.
 */

import {
  events,
  type BrowserState,
  type Download,
  type Extensions,
  type KeyBinding,
  type Preset,
  type Settings,
} from "@bindings";
import { getCurrentWebview } from "@tauri-apps/api/webview";

import { commands, reportFailure } from "./commands";
import { unwrap, type CommandResult } from "./result";

type Listener = () => void;

export interface ExternalStore<T> {
  get(): T;
  set(next: T): void;
  subscribe(listener: Listener): () => void;
}

export function createStore<T>(initial: T): ExternalStore<T> {
  let snapshot = initial;
  const listeners = new Set<Listener>();

  return {
    get: () => snapshot,
    set(next: T) {
      if (Object.is(next, snapshot)) {
        return;
      }
      snapshot = next;
      for (const listener of listeners) {
        listener();
      }
    },
    subscribe(listener: Listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}

export const EMPTY_STATE: BrowserState = {
  kind: "normal",
  private: false,
  tabs: [],
  active: null,
  viewports: [],
  focusedViewport: "",
  splits: [],
  layoutRevision: 0,
  fullscreen: null,
  capacity: 1,
  liveCount: 0,
};

export const browserStore = createStore<BrowserState>(EMPTY_STATE);
export const settingsStore = createStore<Settings | null>(null);
/** What the extensions folder installed, or null until it is known. */
export const extensionsStore = createStore<Extensions | null>(null);
/** The downloads this window lists, newest first. */
export const downloadsStore = createStore<Download[]>([]);

/**
 * The list with one download's progress taken in. Every other entry is kept as
 * it was, so only that one re-renders.
 * [Downloads § Contract](../../docs/specs/downloads.md#contract).
 *
 * @param list - The downloads listed now.
 * @param download - The download as Rust has it now.
 * @returns The list itself when it does not hold that download: the next
 *   whole list brings it.
 */
export function withProgress(list: Download[], download: Download): Download[] {
  return list.some((entry) => entry.id === download.id)
    ? list.map((entry) => (entry.id === download.id ? download : entry))
    : list;
}
/** Every shortcut with its chords, in Rust's table order; empty until it loads. */
export const keymapStore = createStore<KeyBinding[]>([]);

/** How many keymap reads were started, so a slower, older one is not kept. */
let keymapReads = 0;

/**
 * Reads the keymap Rust has in effect. The keymap follows the settings, so
 * this runs again whenever they change.
 */
async function loadKeymap(): Promise<void> {
  const read = ++keymapReads;
  const keymap = await unwrap(commands.keymap());
  if (read === keymapReads) {
    keymapStore.set(keymap);
  }
}

/**
 * Subscribes to Rust's state events and loads the first snapshot.
 *
 * Listeners are attached before the snapshot is fetched so that a change landing
 * during startup is not lost between the two. What loads is kept even when
 * another part fails, and the first failure is then thrown.
 *
 * Browser state and failures are this window's own, so they are listened for
 * on this webview only: a listener for any target also receives what Rust
 * sends every other window. Rust holds a failure from before any window could
 * listen until the state is asked for, which is why that comes second.
 *
 * @returns A function that detaches every listener.
 */
export async function connect(): Promise<() => void> {
  let hasStateEvent = false;
  let hasDownloadsEvent = false;
  // Progress for downloads not listed yet, until the startup list arrives:
  // reported before that list resolves, it would otherwise be lost to it.
  let early: Map<number, Download> | null = new Map();
  const [stopState, stopFailures, stopSettings, stopExtensions, stopDownloads, stopProgress] = await Promise.all([
    events.stateChanged(getCurrentWebview()).listen((event) => {
      hasStateEvent = true;
      browserStore.set(event.payload);
    }),
    events.failureReported(getCurrentWebview()).listen((event) => reportFailure(event.payload)),
    events.settingsChanged.listen((event) => {
      settingsStore.set(event.payload);
      // A failure is reported by the command wrapper; the hints keep the
      // keymap read last.
      loadKeymap().catch(() => {});
    }),
    // The folder is installed after startup, possibly once this has loaded.
    events.extensionsChanged.listen((event) => extensionsStore.set(event.payload)),
    // Each window lists its own profile's downloads only.
    events.downloadsChanged(getCurrentWebview()).listen((event) => {
      hasDownloadsEvent = true;
      // Sent after any progress held, so it already carries it.
      early = null;
      downloadsStore.set(event.payload);
    }),
    // Progress alone arrives on its own, for one listed download.
    events.downloadProgressed(getCurrentWebview()).listen((event) => {
      early?.set(event.payload.id, event.payload);
      downloadsStore.set(withProgress(downloadsStore.get(), event.payload));
    }),
  ]);

  // Each part is kept as it arrives, so one that fails to load does not take
  // the others with it: the tab strip still shows when the settings could not
  // be read.
  const loaded = await Promise.allSettled([
    unwrap(commands.getState()).then((state) => {
      // An event delivered during startup takes precedence over the snapshot.
      if (!hasStateEvent) {
        browserStore.set(state);
      }
    }),
    unwrap(commands.getSettings()).then((settings) => settingsStore.set(settings)),
    loadKeymap(),
    unwrap(commands.extensions()).then((extensions) => {
      // Installing can finish between the snapshot being read and arriving,
      // and its event is then the newer of the two.
      if (extensionsStore.get() === null) {
        extensionsStore.set(extensions);
      }
    }),
    unwrap(commands.downloads()).then((downloads) => {
      if (!hasDownloadsEvent) {
        // Progress is held only while a download runs, when its bytes only
        // grow (a restart sends the whole list), so the larger figure is the
        // newer one.
        const held = [...(early?.values() ?? [])];
        downloadsStore.set(
          held.reduce(
            (list, progress) =>
              list.some((entry) => entry.id === progress.id && entry.received <= progress.received)
                ? withProgress(list, progress)
                : list,
            downloads,
          ),
        );
      }
      early = null;
    }),
  ]);
  const failed = loaded.find((part) => part.status === "rejected");
  if (failed) {
    throw failed.reason;
  }

  return () => {
    stopState();
    stopFailures();
    stopSettings();
    stopExtensions();
    stopDownloads();
    stopProgress();
  };
}

/**
 * Reads the settings again, for a page that could not show them because
 * loading them failed. Settings a change in another window delivered while
 * this was being read are newer, and are kept.
 *
 * @throws {HakuFailure} When they still cannot be read; the command wrapper
 *   has already reported it.
 */
export async function reloadSettings(): Promise<void> {
  const settings = await unwrap(commands.getSettings());
  if (settingsStore.get() === null) {
    settingsStore.set(settings);
  }
}

/** The last change queued, which the next one waits for. */
let settingsQueue: Promise<unknown> = Promise.resolve();

/**
 * Changes the settings from their latest value, one change at a time.
 *
 * Each change is built from what the store holds when its turn comes, not from
 * a copy taken when it was requested. Two changes made back to back therefore
 * both survive; built from the same snapshot, the second would write the list
 * the first had just altered back to its old value.
 *
 * A failed change resolves to null; the failure itself is reported by the
 * command wrapper. The store changes only if Rust stored the settings before
 * failing to apply them, through the settings event every window hears. Later
 * changes still run.
 *
 * @param change - Returns the whole settings to store, given the current ones.
 * @returns What Rust stored, or null when the change failed or settings are not loaded.
 */
export function updateSettings(change: (current: Settings) => Settings): Promise<Settings | null> {
  return queueSettings((current) => commands.setSettings(change(current)));
}

/**
 * Applies a preset's optimization values, in turn with every other change, so
 * a change made while the preset is applied builds on the preset's result.
 *
 * @param preset - The preset to apply.
 * @returns What Rust stored, or null when it failed or settings are not loaded.
 */
export function applySettingsPreset(preset: Preset): Promise<Settings | null> {
  return queueSettings(() => commands.applyPreset(preset));
}

function queueSettings(send: (current: Settings) => Promise<CommandResult<Settings>>): Promise<Settings | null> {
  const turn = sendInTurn(settingsQueue, send);
  settingsQueue = turn;
  return turn;
}

async function sendInTurn(
  previous: Promise<unknown>,
  send: (current: Settings) => Promise<CommandResult<Settings>>,
): Promise<Settings | null> {
  try {
    await previous;
  } catch {
    // A change that threw is its own caller's problem; it must not stop this one.
  }
  const current = settingsStore.get();
  if (current === null) {
    return null;
  }
  const result = await send(current);
  if (result.status !== "ok") {
    return null;
  }
  settingsStore.set(result.data);
  return result.data;
}
