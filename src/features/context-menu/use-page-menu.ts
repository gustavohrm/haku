import { events, type Tab } from "@bindings";
import { commands } from "@ipc/commands";
import { useBrowserState } from "@ipc/hooks";
import { t } from "@shared/i18n";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect, useEffectEvent } from "react";

import { pageMenu } from "./items";
import { openMenu } from "./store";

/**
 * Draws the menu a page in this window asked for, and answers it.
 *
 * The page waits until it is answered: with the engine's item chosen, or with
 * nothing when the choice was one of Haku's own items or the menu closed
 * without one. Switching tabs closes it, since it belongs to the page that
 * was showing. [Context menus § Page menus](../../../docs/specs/context-menus.md#page-menus).
 *
 * @param tab - The active tab.
 */
export function usePageMenu(tab: Tab | null): void {
  const { tabs } = useBrowserState();
  const tabId = tab?.id;
  const tabOf = useEffectEvent((id: number) => tabs.find((candidate) => candidate.id === id) ?? null);
  const activeTab = useEffectEvent(() => tabId);

  useEffect(() => {
    let isMounted = true;
    let unlisten: (() => void) | null = null;
    let closeOpen: (() => void) | null = null;

    const listen = async () => {
      const stop = await events.contextMenuRequested(getCurrentWebview()).listen(({ payload: request }) => {
        if (request.tab !== activeTab()) {
          void commands.answerContextMenu(request.request, null, false);
          return;
        }
        let choice: number | null = null;
        let ranOwnItem = false;
        const own =
          <Args extends unknown[]>(action: (...args: Args) => void) =>
          (...args: Args) => {
            ranOwnItem = true;
            action(...args);
          };
        const id = request.tab;
        const close = openMenu({
          at: { x: request.x, y: request.y },
          label: t("menu.page"),
          items: pageMenu(request, tabOf(id), {
            back: own(() => void commands.goBack(id)),
            forward: own(() => void commands.goForward(id)),
            reload: own(() => void commands.reloadTab(id)),
            inspect: own(() => void commands.openTabDevtools(id)),
            openInTab: own((url: string) => void commands.openTab(url, false)),
            openInWindow: own((url: string) => void commands.openWindow(url)),
            search: own((text: string) => void commands.openTab(text, true)),
            choose: (native) => {
              choice = native;
            },
          }),
          onClose: (fromInside) => {
            if (closeOpen === close) {
              closeOpen = null;
            }
            // Escape or Tab puts the keyboard back on the page; one of Haku's
            // items, or a click elsewhere, has already put focus where it goes.
            void commands.answerContextMenu(request.request, choice, fromInside && !ranOwnItem);
          },
        });
        closeOpen = close;
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
      closeOpen?.();
    };
    // Resubscribed per tab, so leaving the tab closes its page's menu.
  }, [tabId]);
}
