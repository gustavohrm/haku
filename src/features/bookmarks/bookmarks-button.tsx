import { events, type TabId } from "@bindings";
import { holdBar } from "@features/bars/holds";
import { openMenu } from "@features/context-menu/store";
import { MENU_GAP } from "@features/overlays/use-menu-dismiss";
import { useBookmarks } from "@ipc/hooks";
import { t } from "@shared/i18n";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect, useRef, useState } from "react";

import { barActions, bookmarksButtonMenu } from "./bar-menus";

/**
 * The column's **Bookmarks** button, where bookmarks go while the toolbar is
 * in the column and `sidebarBookmarks` is `button`: a menu of the bar, then
 * **Other bookmarks** and **Manage bookmarks**. Ctrl+Shift+B, pressed in
 * Rust, opens the same menu through `BookmarksMenuRequested`.
 * [Bookmarks § The bookmarks bar](../../../docs/specs/bookmarks.md#the-bookmarks-bar).
 */
export function BookmarksButton({ tab }: { tab: TabId | null }) {
  const tree = useBookmarks();
  const [open, setOpen] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);

  const show = () => {
    const button = buttonRef.current;
    if (!tree || !button) {
      return;
    }
    const box = button.getBoundingClientRect();
    setOpen(true);
    const release = holdBar("toolbar");
    openMenu({
      // Above the button, at the column's foot, where the menu flips to.
      at: { x: box.left, y: box.top - MENU_GAP },
      label: t("bookmarks.title"),
      items: bookmarksButtonMenu(tree, barActions(tab)),
      onClose: () => {
        release();
        setOpen(false);
      },
    });
  };

  // The shortcut's request opens the menu as it stands when it arrives.
  const showRef = useRef(show);
  useEffect(() => {
    showRef.current = show;
  });
  useEffect(() => {
    let isMounted = true;
    let unlisten: (() => void) | null = null;
    const listen = async () => {
      const stop = await events.bookmarksMenuRequested(getCurrentWebview()).listen(() => showRef.current());
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
    };
  }, []);

  return (
    <button
      ref={buttonRef}
      type="button"
      className="btn icon ghost shrink-0"
      aria-label={t("bookmarks.title")}
      title={t("bookmarks.title")}
      aria-haspopup="menu"
      aria-expanded={open}
      disabled={!tree}
      onClick={show}
    >
      <i className="ic-bookmark ic-sm" aria-hidden="true" />
    </button>
  );
}
