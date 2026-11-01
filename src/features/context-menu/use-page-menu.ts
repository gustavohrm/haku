import { events, type Tab, type TabId } from "@bindings";
import { paneGeometryStore } from "@features/viewport/use-chrome-layout";
import { commands } from "@ipc/commands";
import { useBrowserState } from "@ipc/hooks";
import { t } from "@shared/i18n";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect, useEffectEvent, useRef } from "react";

import { pageMenu } from "./items";
import { openMenu } from "./store";

/** The page menu on screen, and what it was opened from. */
interface OpenPageMenu {
  tab: TabId;
  pane: string;
  /** The focused tab when it opened, which may not have caught up with the right-click yet. */
  focusedAtOpen: TabId | undefined;
  geometry: string;
  close: () => void;
}

const geometryOf = (pane: string) => JSON.stringify(paneGeometryStore.get().find((each) => each.id === pane));

/**
 * Draws the menu a page in this window asked for, and answers it.
 *
 * The page waits until it is answered: with the engine's item chosen, or with
 * nothing when the choice was one of Haku's own items or the menu closed
 * without one. The menu belongs to the page that asked, so it closes when
 * another tab takes focus, when that page's pane is given another tab, or
 * when that pane moves or resizes.
 * [Context menus § Page menus](../../../docs/specs/context-menus.md#page-menus).
 *
 * @param tab - The focused tab.
 */
export function usePageMenu(tab: Tab | null): void {
  const { tabs, viewports, private: isPrivate } = useBrowserState();
  const tabId = tab?.id;
  const open = useRef<OpenPageMenu | null>(null);
  const tabOf = useEffectEvent((id: number) => tabs.find((candidate) => candidate.id === id) ?? null);
  const privateWindow = useEffectEvent(() => isPrivate);
  const paneOf = useEffectEvent((id: number) => viewports.find((pane) => pane.tab === id)?.id);
  const focusedTab = useEffectEvent(() => tabId);

  // One listener for the window's lifetime: subscribing again on every focus
  // change would leave a gap in which a right-click on the other pane, which
  // moves focus to it, has its request dropped.
  useEffect(() => {
    let isMounted = true;
    let unlisten: (() => void) | null = null;
    const stopGeometry = paneGeometryStore.subscribe(() => {
      const current = open.current;
      if (current && geometryOf(current.pane) !== current.geometry) {
        current.close();
      }
    });

    const listen = async () => {
      const stop = await events.contextMenuRequested(getCurrentWebview()).listen(({ payload: request }) => {
        const id = request.tab;
        const pane = paneOf(id);
        if (pane === undefined) {
          void commands.answerContextMenu(request.request, null, false);
          return;
        }
        const focusedAtOpen = focusedTab();
        // Native focus observation normally got there first; this is a no-op then.
        if (id !== focusedAtOpen) {
          void commands.focusPane(id);
        }
        let choice: number | null = null;
        let ranOwnItem = false;
        const own =
          <Args extends unknown[]>(action: (...args: Args) => void) =>
          (...args: Args) => {
            ranOwnItem = true;
            action(...args);
          };
        const close = openMenu({
          at: { x: request.x, y: request.y },
          label: t("menu.page"),
          items: pageMenu(request, tabOf(id), {
            isPrivate: privateWindow(),
            back: own(() => void commands.goBack(id)),
            forward: own(() => void commands.goForward(id)),
            reload: own(() => void commands.reloadTab(id)),
            print: own(() => void commands.printPage(id)),
            savePage: own(() => void commands.savePage(id)),
            inspect: own(() => void commands.openTabDevtools(id)),
            openInTab: own((url: string) => void commands.openTab(url, false)),
            openInWindow: own((url: string) => void commands.openWindow(url, null)),
            openInPrivateWindow: own((url: string) => void commands.openWindow(url, true)),
            search: own((text: string) => void commands.openTab(text, true)),
            choose: (native) => {
              choice = native;
            },
          }),
          onClose: (fromInside) => {
            if (open.current?.close === close) {
              open.current = null;
            }
            // Escape or Tab puts the keyboard back on the page; one of Haku's
            // items, or a click elsewhere, has already put focus where it goes.
            void commands.answerContextMenu(request.request, choice, fromInside && !ranOwnItem);
          },
        });
        open.current = { tab: id, pane, focusedAtOpen, geometry: geometryOf(pane), close };
      });
      if (isMounted) {
        unlisten = stop;
      } else {
        stop();
      }
    };
    void listen();

    return () => {
      isMounted = false;
      unlisten?.();
      stopGeometry();
      open.current?.close();
    };
  }, []);

  useEffect(() => {
    const current = open.current;
    if (!current) {
      return;
    }
    const isReassigned = !viewports.some((pane) => pane.id === current.pane && pane.tab === current.tab);
    const isRefocused = tabId !== current.tab && tabId !== current.focusedAtOpen;
    if (isReassigned || isRefocused) {
      current.close();
    }
  }, [tabId, viewports]);
}
