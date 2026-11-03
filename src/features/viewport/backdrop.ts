import type { TabId } from "@bindings";
import { commands } from "@ipc/commands";
import { useBrowserState } from "@ipc/hooks";
import { createStore } from "@ipc/store";
import { useEffect, useEffectEvent, useSyncExternalStore } from "react";

/**
 * Each shown live page's capture while a backdrop is up, or `null` while none
 * is. A pane draws its capture under the chrome, so the backdrop, which covers
 * the page, can blur what the page showed.
 */
export const backdropStore = createStore<ReadonlyMap<TabId, string> | null>(null);

/** The capture a pane draws for its tab while a backdrop is up. */
export function useBackdropCapture(tab: TabId): string | undefined {
  return useSyncExternalStore(backdropStore.subscribe, backdropStore.get)?.get(tab);
}

/**
 * Captures the window's shown pages while `open`, for a backdrop over them.
 *
 * The chrome cannot draw over a page, only in its place, so a backdrop needs
 * every shown live page as an image first. It goes up only once all of them
 * have one: a page left out would show as a plain surface, not blurred.
 * [Address bar § Layouts](../../../docs/specs/address-bar.md#layouts).
 *
 * @returns Whether the backdrop is up.
 */
export function usePageBackdrop(open: boolean): boolean {
  const { viewports, tabs, fullscreen } = useBrowserState();
  const captures = useSyncExternalStore(backdropStore.subscribe, backdropStore.get);

  // A page the chrome draws itself, or covers while it reloads, is under the
  // backdrop already.
  const livePages = useEffectEvent((): TabId[] =>
    viewports.flatMap((pane) => {
      const tab = tabs.find((candidate) => candidate.id === pane.tab);
      const isShown = fullscreen === null || pane.tab === fullscreen;
      return tab && isShown && tab.presence.status === "live" && !tab.restoring ? [tab.id] : [];
    }),
  );

  useEffect(() => {
    if (!open) {
      return;
    }
    let current = true;
    const take = async () => {
      const pages = livePages();
      const results = await Promise.all(pages.map((tab) => commands.capturePage(tab)));
      const taken = new Map<TabId, string>();
      for (const [index, result] of results.entries()) {
        if (result.status !== "ok" || result.data === null) {
          return;
        }
        taken.set(pages[index]!, result.data);
      }
      if (current) {
        backdropStore.set(taken);
      }
    };
    void take();
    return () => {
      current = false;
      backdropStore.set(null);
    };
  }, [open]);

  return open && captures !== null;
}
