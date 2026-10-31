import { SETTINGS_URL } from "@app/router";
import { type Extension, type TabId } from "@bindings";
import { holdBarForWindow, useBarHold } from "@features/bars/holds";
import { MENU_GAP, useMenuDismiss } from "@features/overlays/use-menu-dismiss";
import { useOverlay } from "@features/overlays/use-overlay";
import { commands } from "@ipc/commands";
import { useExtensions, useSettings } from "@ipc/hooks";
import { updateSettings } from "@ipc/store";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { useCallback, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { toolbarExtensions } from "./toolbar-extensions";

/**
 * The toolbar's extensions: a button for each pinned one, and the extensions
 * button, whose menu lists every extension that is switched on, to open or to
 * pin.
 *
 * An extension's popup opens in a window of its own, below the button it was
 * opened from: its own when pinned, the extensions button otherwise, as in
 * Chrome. Rust places it, so it is given the button's rectangle.
 */
export function ExtensionsBar({ tab }: { tab: TabId | undefined }) {
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
  useBarHold("toolbar", open);

  const close = useCallback(() => setMenuAt(null), []);
  useMenuDismiss(open, close, menuRef, buttonRef);

  const toggle = () => {
    const button = buttonRef.current?.getBoundingClientRect();
    setMenuAt(
      open || !button
        ? null
        : { right: document.documentElement.clientWidth - button.right, top: button.bottom + MENU_GAP },
    );
  };

  const openPopup = async (extension: Extension, from: HTMLElement | null) => {
    setMenuAt(null);
    const anchor = from?.getBoundingClientRect();
    if (!anchor || tab === undefined) {
      return;
    }
    // The popup is a window of its own, and Rust does not report it closing.
    const release = holdBarForWindow("toolbar");
    const result = await commands.openExtensionPopup(extension.id, tab, {
      x: anchor.left,
      y: anchor.top,
      width: anchor.width,
      height: anchor.height,
    });
    // Nothing opened when the click closed the popup that was open.
    if (result.status === "error" || !result.data) {
      release();
    }
  };

  const setPinned = (id: string, isPinned: boolean) => {
    void updateSettings((current) => {
      const others = current.pinnedExtensions.filter((other) => other !== id);
      return { ...current, pinnedExtensions: isPinned ? [...others, id] : others };
    });
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
          disabled={extension.popup === null || tab === undefined}
          onClick={(event) => void openPopup(extension, event.currentTarget)}
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
        <i className="ic-blocks ic-sm" aria-hidden="true" />
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
            tabIndex={-1}
            className="bg-chrome-raised border-text/15 text-text fixed z-10 flex w-64 flex-col rounded-(--radius-surface) border p-1 outline-none"
            style={{ right: menuAt.right, top: menuAt.top }}
          >
            <h2 className="text-text-secondary px-2 pt-1.5 pb-1 text-xs font-medium">{t("extensions.menu")}</h2>

            {enabled.length === 0 && <p className="text-text-secondary px-2 py-1.5">{t("extensions.empty")}</p>}

            {enabled.map((extension) => {
              const isPinned = pinnedIds.includes(extension.id);
              return (
                <div
                  key={extension.id}
                  className="group hover:bg-text/8 flex h-8 items-center rounded-(--radius-control) pr-0.5"
                >
                  <button
                    type="button"
                    className="flex h-full min-w-0 flex-1 items-center gap-2.5 rounded-(--radius-control) px-2 text-left outline-offset-[-2px] disabled:opacity-50"
                    title={extension.popup === null ? t("extensions.noPopup") : extension.name}
                    disabled={extension.popup === null || tab === undefined}
                    onClick={() => void openPopup(extension, pinnedRefs.current.get(extension.id) ?? buttonRef.current)}
                  >
                    <ExtensionIcon extension={extension} />
                    <span className="truncate">{extension.name}</span>
                  </button>
                  {/* Shown while pinned; otherwise only on the row's hover or
                      focus, so the list reads as names, not as pins. */}
                  <button
                    type="button"
                    className={cx(
                      "hover:bg-text/10 grid size-7 shrink-0 place-items-center rounded-(--radius-control) outline-offset-[-2px]",
                      isPinned
                        ? "text-text"
                        : "text-text-secondary opacity-0 group-focus-within:opacity-100 group-hover:opacity-100",
                    )}
                    aria-label={`${t("extensions.pin")} ${extension.name}`}
                    title={isPinned ? t("extensions.unpin") : t("extensions.pin")}
                    aria-pressed={isPinned}
                    disabled={!settings}
                    onClick={() => setPinned(extension.id, !isPinned)}
                  >
                    <i className="ic-pin ic-xs" aria-hidden="true" />
                  </button>
                </div>
              );
            })}

            <hr className="border-text/10 mx-1 my-1" />
            <button
              type="button"
              className="text-text-secondary hover:bg-text/8 hover:text-text flex h-8 items-center gap-2.5 rounded-(--radius-control) px-2 text-left outline-offset-[-2px]"
              onClick={() => {
                setMenuAt(null);
                void commands.openTab(`${SETTINGS_URL}/extensions`, true);
              }}
            >
              <i className="ic-settings ic-sm shrink-0" aria-hidden="true" />
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
    return <i className="ic-blocks ic-sm shrink-0" aria-hidden="true" />;
  }
  return <img src={extension.icon} alt="" className="size-4 shrink-0" draggable={false} />;
}
