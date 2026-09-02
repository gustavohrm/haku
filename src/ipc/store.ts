/**
 * The interface's view of state Rust owns.
 *
 * There is no client-side tab model. Rust emits the whole browser state on every
 * change and this holds the latest one, so the interface can never drift out of
 * step with the webviews it is describing.
 */

import { commands, events, type BrowserState, type Settings } from "@bindings";

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

export const EMPTY_STATE: BrowserState = { tabs: [], active: null, capacity: 1, liveCount: 0 };

export const browserStore = createStore<BrowserState>(EMPTY_STATE);
export const settingsStore = createStore<Settings | null>(null);

/**
 * Subscribes to Rust's state events and loads the first snapshot.
 *
 * Listeners are attached before the snapshot is fetched so that a change landing
 * during startup is not lost between the two.
 *
 * @returns A function that detaches both listeners.
 */
export async function connect(): Promise<() => void> {
  const [stopState, stopSettings] = await Promise.all([
    events.stateChanged.listen((event) => browserStore.set(event.payload)),
    events.settingsChanged.listen((event) => settingsStore.set(event.payload)),
  ]);

  const [state, settings] = await Promise.all([unwrap(commands.getState()), unwrap(commands.getSettings())]);
  browserStore.set(state);
  settingsStore.set(settings);

  return () => {
    stopState();
    stopSettings();
  };
}
