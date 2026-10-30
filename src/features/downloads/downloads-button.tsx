import { DOWNLOADS_URL } from "@app/router";
import { MENU_GAP, useMenuDismiss } from "@features/overlays/use-menu-dismiss";
import { useOverlay } from "@features/overlays/use-overlay";
import { commands } from "@ipc/commands";
import { useDownloads } from "@ipc/hooks";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { useCallback, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { DownloadItem } from "./download-item";

/**
 * The toolbar's downloads button, and the list it opens.
 *
 * Shown only once a download has started since Haku launched: earlier ones
 * are on `haku://downloads`, and a button that is always there is one more
 * thing in the toolbar nobody asked for.
 */
export function DownloadsButton() {
  const recent = useDownloads().filter((download) => download.recent);
  // Where the list opens, from the window's right edge, or null while closed.
  const [anchor, setAnchor] = useState<{ right: number; top: number } | null>(null);
  const open = anchor !== null;
  const buttonRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLElement>(null);
  const listId = useId();

  // The list hangs over the page, so it must be registered to be clickable.
  useOverlay(listRef, open);
  const close = useCallback(() => setAnchor(null), []);
  useMenuDismiss(open, close, listRef, buttonRef);

  if (recent.length === 0) {
    return null;
  }
  const running = recent.some((download) => download.state === "inProgress");

  const toggle = () => {
    const button = buttonRef.current?.getBoundingClientRect();
    setAnchor(
      open || !button
        ? null
        : { right: document.documentElement.clientWidth - button.right, top: button.bottom + MENU_GAP },
    );
  };

  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        className="btn icon ghost shrink-0"
        aria-label={t("downloads.title")}
        title={t("downloads.title")}
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        onClick={toggle}
      >
        <i className={cx("ic-download ic-sm", running && "text-accent")} aria-hidden="true" />
      </button>
      {/* Opaque and without a shadow, because the input mask cannot show
          either over the page. */}
      {anchor &&
        createPortal(
          <section
            ref={listRef}
            id={listId}
            aria-label={t("downloads.title")}
            tabIndex={-1}
            className="bg-chrome-raised border-text/15 text-text fixed z-10 flex w-96 flex-col rounded-(--radius-surface) border py-1 outline-none"
            style={{ right: anchor.right, top: anchor.top }}
          >
            <ul className="flex max-h-96 flex-col overflow-auto">
              {recent.map((download) => (
                <DownloadItem key={download.id} download={download} />
              ))}
            </ul>
            <button
              type="button"
              className="btn ghost mx-1 justify-start"
              onClick={() => {
                close();
                void commands.openTab(DOWNLOADS_URL, true);
              }}
            >
              {t("downloads.all")}
            </button>
          </section>,
          document.body,
        )}
    </>
  );
}
