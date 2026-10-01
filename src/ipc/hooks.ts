import type { BrowserState, Settings, Tab } from "@bindings";
import { useCallback, useSyncExternalStore } from "react";

import { browserStore, settingsStore, type ExternalStore } from "./store";

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

export function useActiveTab(): Tab | null {
  const state = useBrowserState();
  return state.tabs.find((tab) => tab.id === state.active) ?? null;
}
