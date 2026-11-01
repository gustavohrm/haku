import type { BrowserState, Download, Extensions, KeyBinding, Settings, Tab } from "@bindings";
import { useCallback, useSyncExternalStore } from "react";

import { browserStore, downloadsStore, extensionsStore, keymapStore, settingsStore, type ExternalStore } from "./store";

function useStore<T>(store: ExternalStore<T>): T {
  return useSyncExternalStore(
    useCallback((listener) => store.subscribe(listener), [store]),
    store.get,
    store.get,
  );
}

export function useBrowserState(): BrowserState {
  return useStore(browserStore);
}

export function useSettings(): Settings | null {
  return useStore(settingsStore);
}

export function useExtensions(): Extensions | null {
  return useStore(extensionsStore);
}

export function useDownloads(): Download[] {
  return useStore(downloadsStore);
}

/** Every shortcut with its chords now and its defaults. */
export function useKeymap(): KeyBinding[] {
  return useStore(keymapStore);
}

export function useActiveTab(): Tab | null {
  const state = useBrowserState();
  const focused = state.viewports.find((pane) => pane.id === state.focusedViewport);
  return state.tabs.find((tab) => tab.id === focused?.tab) ?? null;
}
