import { type Extension } from "@bindings";
import { useOverlay } from "@features/overlays/use-overlay";
import { commands } from "@ipc/commands";
import { useExtensions, useSettings } from "@ipc/hooks";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { toolbarExtensions } from "./toolbar-extensions";

const SETTINGS_URL = "haku://settings";

/** Space between the extensions button and the menu below it, in pixels. */
const MENU_GAP = 6;

/**
 * The toolbar's extensions: a button for each pinned one, and the extensions
 * button, whose menu lists every extension that is switched on, to open or to
 * pin.
 *
 * An extension's popup opens in a window of its own, below the button it was
 * opened from: its own when pinned, the extensions button otherwise, as in
 * Chrome. Rust places it, so it is given the button's rectangle.
 */
export function ExtensionsBar() {
  const extensions = useExtensions();
  const settings = useSettings();
  const [menuAt, setMenuAt] = useState<{ right: number; top: number } | null>(null);
  const open = menuAt !== null;
  const buttonRef = useRef<HTMLButtonElement>(null);
  const pinnedRefs = useRef(new Map<string, HTMLButtonElement>());
  const menuRef = useRef<HTMLElement>(null);
  const menuId = useId();

  const pinnedIds = settings?.pinnedExtensions ?? [];
  const { enabled, pinned } = toolbarExtensions(
    extensions?.installed ?? [],
    settings?.disabledExtensions ?? [],
    pinnedIds,
  );

  // The menu hangs over the page, so it must be registered to be clickable.
  useOverlay(menuRef, open);

  useEffect(() => {
    if (!open) {
      return;
    }
    const close = () => setMenuAt(null);
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
    const button = buttonRef.current?.getBoundingClientRect();
    setMenuAt(
      open || !button
        ? null
        : { right: document.documentElement.clientWidth - button.right, top: button.bottom + MENU_GAP },
    );
  };

  const openPopup = (extension: Extension, from: HTMLElement | null) => {
    setMenuAt(null);
    const anchor = from?.getBoundingClientRect();
    if (!anchor) {
      return;
    }
    void commands.openExtensionPopup(extension.id, {
      x: anchor.left,
      y: anchor.top,
      width: anchor.width,
      height: anchor.height,
    });
  };

  const setPinned = (id: string, isPinned: boolean) => {
    if (!settings) {
      return;
    }
    const others = settings.pinnedExtensions.filter((other) => other !== id);
    void commands.setSettings({ ...settings, pinnedExtensions: isPinned ? [...others, id] : others });
  };

  return (
    <>
      {pinned.map((extension) => (
        <button
          key={extension.id}
          ref={(button) => {
            if (button) {
              pinnedRefs.current.set(extension.id, button);
            } else {
              pinnedRefs.current.delete(extension.id);
            }
          }}
          type="button"
          className="btn icon ghost shrink-0"
          aria-label={extension.name}
          title={extension.name}
          disabled={extension.popup === null}
          onClick={(event) => openPopup(extension, event.currentTarget)}
        >
          <ExtensionIcon extension={extension} />
        </button>
      ))}

      <button
        ref={buttonRef}
        type="button"
        className="btn icon ghost shrink-0"
        aria-label={t("extensions.menu")}
        title={t("extensions.menu")}
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        onClick={toggle}
      >
        <i className="ic-puzzle ic-sm" aria-hidden="true" />
      </button>

      {/* Opaque and without a shadow, because the input mask cannot show
          either over the page. Its rounded corners are cut into the mask from
          its own style. */}
      {menuAt &&
        createPortal(
          <section
            ref={menuRef}
            id={menuId}
            aria-label={t("extensions.menu")}
            className="bg-chrome-raised border-text/15 text-text fixed z-10 flex w-72 flex-col gap-0.5 rounded-(--radius-surface) border p-1.5"
            style={{ right: menuAt.right, top: menuAt.top }}
          >
            {enabled.length === 0 && <p className="text-text-secondary px-1.5 py-1 text-sm">{t("extensions.empty")}</p>}

            {enabled.map((extension) => {
              const isPinned = pinnedIds.includes(extension.id);
              return (
                <div key={extension.id} className="flex items-center gap-0.5">
                  <button
                    type="button"
                    className="btn ghost min-w-0 flex-1 justify-start gap-2"
                    title={extension.popup === null ? t("extensions.noPopup") : extension.name}
                    disabled={extension.popup === null}
                    onClick={() => openPopup(extension, pinnedRefs.current.get(extension.id) ?? buttonRef.current)}
                  >
                    <ExtensionIcon extension={extension} />
                    <span className="truncate">{extension.name}</span>
                  </button>
                  <button
                    type="button"
                    className="btn icon ghost shrink-0"
                    aria-label={`${t("extensions.pin")} ${extension.name}`}
                    title={isPinned ? t("extensions.unpin") : t("extensions.pin")}
                    aria-pressed={isPinned}
                    disabled={!settings}
                    onClick={() => setPinned(extension.id, !isPinned)}
                  >
                    <i className={cx(isPinned ? "ic-pin-off" : "ic-pin", "ic-sm")} aria-hidden="true" />
                  </button>
                </div>
              );
            })}

            <hr className="border-text/15 my-1" />
            <button
              type="button"
              className="btn ghost justify-start"
              onClick={() => {
                setMenuAt(null);
                void commands.openTab(SETTINGS_URL, true);
              }}
            >
              {t("extensions.manage")}
            </button>
          </section>,
          document.body,
        )}
    </>
  );
}

/** An extension's own icon, or a generic one when its manifest has none. */
function ExtensionIcon({ extension }: { extension: Extension }) {
  if (extension.icon === null) {
    return <i className="ic-puzzle ic-sm shrink-0" aria-hidden="true" />;
  }
  return <img src={extension.icon} alt="" className="size-4 shrink-0" draggable={false} />;
}
