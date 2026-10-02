import { type Tab } from "@bindings";
import { useOverlay } from "@features/overlays/use-overlay";
import { commands } from "@ipc/commands";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { addressIcon } from "./address";

/** Space between the icon and the menu below it, in pixels. */
const MENU_GAP = 6;

interface SiteMenuProps {
  tab: Tab;
  url: string;
}

/**
 * The address field's leading icon, and the menu it opens about the site.
 *
 * Keeping a tab loaded lives here rather than on the tab itself: it overrides
 * the optimization settings, so it should take a deliberate step, with the
 * warning beside it.
 */
export function SiteMenu({ tab, url }: SiteMenuProps) {
  // Where the menu opens, or null while closed.
  const [anchor, setAnchor] = useState<{ x: number; y: number } | null>(null);
  const open = anchor !== null;
  const buttonRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLElement>(null);
  const menuId = useId();

  // The menu hangs over the page, so it must be registered to be clickable.
  useOverlay(menuRef, open);

  useEffect(() => {
    if (!open) {
      return;
    }
    const close = () => setAnchor(null);
    const closeOutside = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!menuRef.current?.contains(target) && !buttonRef.current?.contains(target)) {
        close();
      }
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        close();
      }
    };
    // A click on the page never reaches the chrome; it only takes focus away.
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    document.addEventListener("pointerdown", closeOutside);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
      document.removeEventListener("pointerdown", closeOutside);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [open]);

  const toggle = () => {
    const rect = buttonRef.current?.getBoundingClientRect();
    setAnchor(open || !rect ? null : { x: rect.left, y: rect.bottom + MENU_GAP });
  };

  const secure = url.startsWith("https://");

  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        className="btn icon ghost dense -mx-1 shrink-0 [--ui-radius:var(--radius-small)]"
        aria-label={t("site.menu")}
        title={t("site.menu")}
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        onClick={toggle}
      >
        <i className={cx(addressIcon(url, false), "ic-sm")} aria-hidden="true" />
      </button>

      {/*
        Portalled out of the address field: inside its <label>, any click in
        the menu would focus the field. Opaque and square, because the input
        mask cuts a plain rectangle and a shadow or a rounded corner would show
        the page through it.
      */}
      {anchor &&
        createPortal(
          <section
            ref={menuRef}
            id={menuId}
            aria-label={t("site.menu")}
            className="bg-chrome-raised border-text/15 text-text fixed z-10 flex w-80 flex-col gap-3 border p-3"
            style={{ left: anchor.x, top: anchor.y }}
          >
            <div className="flex min-w-0 flex-col">
              <span className="truncate font-medium">{hostOf(url)}</span>
              <span className="text-text-secondary text-sm">{secure ? t("site.secure") : t("site.insecure")}</span>
            </div>

            <label className="flex items-center justify-between gap-3">
              <span>{t("site.keepLoaded")}</span>
              <input
                type="checkbox"
                className="switch"
                checked={tab.fixed}
                onChange={(event) => void commands.setTabFixed(tab.id, event.target.checked)}
              />
            </label>

            <div className="alert warning soft text-sm" role="note">
              <i className="ic-triangle-alert alert-icon" aria-hidden="true" />
              <span>{t("site.keepLoaded.warning")}</span>
            </div>
          </section>,
          document.body,
        )}
    </>
  );
}

function hostOf(url: string): string {
  try {
    return new URL(url).host || url;
  } catch {
    return url;
  }
}
