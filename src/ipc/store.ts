/**
 * The interface's view of state Rust owns.
 *
 * There is no client-side tab model. Rust emits the whole browser state on every
 * change and this holds the latest one, so the interface can never drift out of
 * step with the webviews it is describing.
 */

import { events, type BrowserState, type Extensions, type Settings } from "@bindings";
import { getCurrentWebview } from "@tauri-apps/api/webview";

import { commands } from "./commands";
import { unwrap } from "./result";

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

export const EMPTY_STATE: BrowserState = { kind: "normal", tabs: [], active: null, capacity: 1, liveCount: 0 };

export const browserStore = createStore<BrowserState>(EMPTY_STATE);
export const settingsStore = createStore<Settings | null>(null);
/** What the extensions folder installed, or null until it is known. */
export const extensionsStore = createStore<Extensions | null>(null);

/**
 * Subscribes to Rust's state events and loads the first snapshot.
 *
 * Listeners are attached before the snapshot is fetched so that a change landing
 * during startup is not lost between the two.
 *
 * Browser state is this window's own, so it is listened for on this webview
 * only: a listener for any target also receives what Rust sends every other
 * window.
 *
 * @returns A function that detaches every listener.
 */
export async function connect(): Promise<() => void> {
  const [stopState, stopSettings, stopExtensions] = await Promise.all([
    events.stateChanged(getCurrentWebview()).listen((event) => browserStore.set(event.payload)),
    events.settingsChanged.listen((event) => settingsStore.set(event.payload)),
    // The folder is installed after startup, possibly once this has loaded.
    events.extensionsChanged.listen((event) => extensionsStore.set(event.payload)),
  ]);

  const [state, settings, extensions] = await Promise.all([
    unwrap(commands.getState()),
    unwrap(commands.getSettings()),
    unwrap(commands.extensions()),
  ]);
  browserStore.set(state);
  settingsStore.set(settings);
  // Installing can finish between the snapshot being read and arriving, and
  // its event is then the newer of the two.
  if (extensionsStore.get() === null) {
    extensionsStore.set(extensions);
  }

  return () => {
    stopState();
    stopSettings();
    stopExtensions();
  };
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
 * A failed change resolves to null and leaves the store as it was; the failure
 * itself is reported by the command wrapper. Later changes still run.
 *
 * @param change - Returns the whole settings to store, given the current ones.
 * @returns What Rust stored, or null when the change failed or settings are not loaded.
 */
export function updateSettings(change: (current: Settings) => Settings): Promise<Settings | null> {
  const turn = applySettingsChange(settingsQueue, change);
  settingsQueue = turn;
  return turn;
}

async function applySettingsChange(
  previous: Promise<unknown>,
  change: (current: Settings) => Settings,
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
  const result = await commands.setSettings(change(current));
  if (result.status !== "ok") {
    return null;
  }
  settingsStore.set(result.data);
  return result.data;
}
