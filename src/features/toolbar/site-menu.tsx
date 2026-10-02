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
  const switchId = useId();

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

  // The menu is anchored at the lock glyph rather than its button, whose hover
  // box is wider than the glyph, and pulls itself back by its own inset so its
  // text starts directly under the lock.
  const toggle = () => {
    const button = buttonRef.current?.getBoundingClientRect();
    const glyph = buttonRef.current?.querySelector("i")?.getBoundingClientRect();
    setAnchor(open || !button || !glyph ? null : { x: glyph.left, y: button.bottom + MENU_GAP });
  };

  const secure = url.startsWith("https://");

  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        className="btn icon ghost p-xs -mx-1 shrink-0 [--ui-radius:var(--radius-small)]"
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
        the menu would focus the field. Opaque and without a shadow, because the
        input mask cannot show either over the page. Its rounded corners are
        cut into the mask from its own style.
      */}
      {anchor &&
        createPortal(
          <section
            ref={menuRef}
            id={menuId}
            aria-label={t("site.menu")}
            className="bg-chrome-raised border-text/15 text-text fixed z-10 -ml-[calc(--spacing(3)+1px)] flex w-80 flex-col gap-3 rounded-(--radius-surface) border p-3"
            style={{ left: anchor.x, top: anchor.y }}
          >
            <div className="flex min-w-0 flex-col">
              <span className="truncate font-medium">{hostOf(url)}</span>
              <span className="text-text-secondary text-sm">{secure ? t("site.secure") : t("site.insecure")}</span>
            </div>

            {/* The warning sits outside the label: a label takes its first
                labelable descendant as its control, which would be the
                warning's button rather than the switch. */}
            <div className="flex items-center gap-1.5">
              <label htmlFor={switchId}>{t("site.keepLoaded")}</label>
              <Warning message={t("site.keepLoaded.warning")} />
              <input
                id={switchId}
                type="checkbox"
                className="switch ml-auto"
                checked={tab.fixed}
                onChange={(event) => void commands.setTabFixed(tab.id, event.target.checked)}
              />
            </div>
          </section>,
          document.body,
        )}
    </>
  );
}

/**
 * A warning icon that explains itself on hover or focus.
 *
 * The bubble can reach past the menu, and anything outside a registered
 * overlay is drawn under the page's hole in the input mask, so it would be
 * invisible there. Whether it is open is therefore tracked here rather than
 * left to CSS, and the bubble is registered while it shows; `useOverlay`
 * measures it again once it has finished scaling up.
 */
function Warning({ message }: { message: string }) {
  const [open, setOpen] = useState(false);
  const bubbleRef = useRef<HTMLSpanElement>(null);
  const bubbleId = useId();

  useOverlay(bubbleRef, open);

  return (
    <span
      className="tooltip flex"
      data-state={open ? "open" : "closed"}
      onPointerEnter={() => setOpen(true)}
      onPointerLeave={() => setOpen(false)}
      onFocus={() => setOpen(true)}
      onBlur={() => setOpen(false)}
    >
      <button
        type="button"
        className="text-warning flex outline-offset-2"
        aria-label={t("site.warning")}
        aria-describedby={bubbleId}
      >
        <i className="ic-triangle-alert ic-xs" aria-hidden="true" />
      </button>
      <span ref={bubbleRef} id={bubbleId} role="tooltip" className="tooltip-bubble warning soft edged">
        {message}
      </span>
    </span>
  );
}

function hostOf(url: string): string {
  try {
    return new URL(url).host || url;
  } catch {
    return url;
  }
}
