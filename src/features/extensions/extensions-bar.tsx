import { SETTINGS_URL } from "@app/router";
import { type Extension, type TabId } from "@bindings";
import { holdBar, holdBarForWindow, useBarHold } from "@features/bars/holds";
import { openMenu } from "@features/context-menu/store";
import { MENU_GAP, useMenuDismiss } from "@features/overlays/use-menu-dismiss";
import { useOverlay } from "@features/overlays/use-overlay";
import { commands } from "@ipc/commands";
import { useExtensions, useSettings } from "@ipc/hooks";
import { updateSettings } from "@ipc/store";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { type RefObject, useCallback, useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { ExtensionIcon } from "./extension-icon";
import { movePin, toolbarExtensions } from "./toolbar-extensions";

/** How far a press moves, in pixels, before it is a drag rather than a click. */
const DRAG_THRESHOLD = 5;

interface PinDrag {
  id: string;
  from: number;
  pointerId: number;
  target: HTMLElement;
  x: number;
  y: number;
  started: boolean;
  /** The gap it would land in: before the shown pin at this index. */
  gap: number | null;
  release: (() => void) | null;
}

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
      <PinnedExtensions
        pinned={pinned}
        refs={pinnedRefs}
        canOpen={tab !== undefined}
        onOpen={(extension, from) => void openPopup(extension, from)}
      />

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

/** Moves a pin to `to` among the pins that show, keeping hidden pins in place. */
function savePinOrder(id: string, shown: readonly string[], to: number) {
  void updateSettings((current) => ({
    ...current,
    pinnedExtensions: [...movePin(current.pinnedExtensions, shown, id, to)],
  }));
}

interface PinnedExtensionsProps {
  pinned: readonly Extension[];
  refs: RefObject<Map<string, HTMLButtonElement>>;
  canOpen: boolean;
  onOpen: (extension: Extension, from: HTMLElement) => void;
}

/**
 * The pinned extensions' buttons, in their order, which dragging one along
 * the toolbar changes, as dragging a tab does along the strip. A button's
 * context menu moves it too, so the order does not need a pointer.
 * [Extensions § The toolbar](../../../docs/specs/extensions.md#the-toolbar).
 */
function PinnedExtensions({ pinned, refs, canOpen, onOpen }: PinnedExtensionsProps) {
  const group = useRef<HTMLDivElement>(null);
  const indicator = useRef<HTMLSpanElement>(null);
  const drag = useRef<PinDrag | null>(null);
  const suppressClick = useRef(false);
  const shown = pinned.map((extension) => extension.id);
  const signature = shown.join(",");

  const clearDrag = useCallback(() => {
    const current = drag.current;
    drag.current = null;
    current?.release?.();
    if (current?.target.hasPointerCapture(current.pointerId)) {
      current.target.releasePointerCapture(current.pointerId);
    }
    if (indicator.current) {
      indicator.current.hidden = true;
    }
  }, []);
  // A pin added, removed or moved meanwhile leaves the drag's positions stale.
  useEffect(() => clearDrag, [signature, clearDrag]);
  useEffect(() => {
    const cancel = (event: KeyboardEvent) => {
      if (event.key === "Escape" && drag.current) {
        clearDrag();
      }
    };
    window.addEventListener("keydown", cancel);
    return () => window.removeEventListener("keydown", cancel);
  }, [clearDrag]);

  const updateGap = (x: number, y: number) => {
    const current = drag.current;
    const box = group.current?.getBoundingClientRect();
    if (!current || !box || !indicator.current) {
      return;
    }
    // Released well off the pinned buttons, it lands nowhere.
    const margin = box.height;
    if (y < box.top - margin || y > box.bottom + margin || x < box.left - margin || x > box.right + margin) {
      current.gap = null;
      indicator.current.hidden = true;
      return;
    }
    const buttons = shown.map((id) => refs.current.get(id)).filter((button) => button !== undefined);
    let gap = buttons.length;
    let edge = buttons.at(-1)?.getBoundingClientRect().right ?? box.right;
    for (const [index, button] of buttons.entries()) {
      const rect = button.getBoundingClientRect();
      if (x < rect.left + rect.width / 2) {
        gap = index;
        edge = rect.left;
        break;
      }
    }
    current.gap = gap;
    // Either side of the dragged button is where it already is.
    indicator.current.hidden = gap === current.from || gap === current.from + 1;
    indicator.current.style.left = `${edge - box.left - 1}px`;
  };

  const openPinMenu = (extension: Extension, at: { x: number; y: number }) => {
    const index = shown.indexOf(extension.id);
    openMenu({
      at,
      label: extension.name,
      onClose: holdBar("toolbar"),
      items: [
        {
          kind: "command",
          id: "left",
          label: t("extensions.moveLeft"),
          icon: "ic-arrow-left",
          disabled: index <= 0,
          onSelect: () => savePinOrder(extension.id, shown, index - 1),
        },
        {
          kind: "command",
          id: "right",
          label: t("extensions.moveRight"),
          icon: "ic-arrow-right",
          disabled: index >= shown.length - 1,
          onSelect: () => savePinOrder(extension.id, shown, index + 1),
        },
        { kind: "separator", id: "pin" },
        {
          kind: "command",
          id: "unpin",
          label: t("extensions.unpin"),
          icon: "ic-pin-off",
          onSelect: () =>
            void updateSettings((current) => ({
              ...current,
              pinnedExtensions: current.pinnedExtensions.filter((other) => other !== extension.id),
            })),
        },
        {
          kind: "command",
          id: "manage",
          label: t("extensions.manage"),
          icon: "ic-settings",
          onSelect: () => void commands.openTab(`${SETTINGS_URL}/extensions`, true),
        },
      ],
    });
  };

  if (pinned.length === 0) {
    return null;
  }

  return (
    // oxlint-disable-next-line jsx-a11y/no-static-element-interactions -- Pointer dragging reorders the buttons; their context menu moves them without a pointer.
    <div
      ref={group}
      // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role -- A row of toolbar buttons, not form controls.
      role="group"
      aria-label={t("extensions.pinned")}
      className="relative flex shrink-0 items-center gap-(--haku-gap)"
      onClickCapture={(event) => {
        if (suppressClick.current && event.detail !== 0) {
          event.preventDefault();
          event.stopPropagation();
        }
        suppressClick.current = false;
      }}
      onPointerDown={(event) => {
        if (event.button !== 0 || !event.isPrimary || drag.current) {
          return;
        }
        suppressClick.current = false;
        const target = (event.target as HTMLElement).closest<HTMLElement>("[data-pin]");
        const id = target?.dataset.pin;
        if (!target || id === undefined) {
          return;
        }
        drag.current = {
          id,
          from: shown.indexOf(id),
          pointerId: event.pointerId,
          target,
          x: event.clientX,
          y: event.clientY,
          started: false,
          gap: null,
          release: null,
        };
        // Captured on the button, so a press that stays a click still opens it.
        target.setPointerCapture(event.pointerId);
      }}
      onPointerMove={(event) => {
        const current = drag.current;
        if (!current || current.pointerId !== event.pointerId) {
          return;
        }
        if (!current.started && Math.hypot(event.clientX - current.x, event.clientY - current.y) < DRAG_THRESHOLD) {
          return;
        }
        if (!current.started) {
          current.started = true;
          // A floating toolbar stays out while a pin is dragged along it.
          current.release = holdBar("toolbar");
        }
        suppressClick.current = true;
        event.preventDefault();
        updateGap(event.clientX, event.clientY);
      }}
      onPointerUp={(event) => {
        const current = drag.current;
        if (!current || current.pointerId !== event.pointerId) {
          return;
        }
        if (current.started) {
          updateGap(event.clientX, event.clientY);
        }
        const { gap, started, from, id } = current;
        clearDrag();
        suppressClick.current = started;
        if (started && gap !== null) {
          const to = gap > from ? gap - 1 : gap;
          if (to !== from) {
            savePinOrder(id, shown, to);
          }
        }
      }}
      onPointerCancel={(event) => {
        if (drag.current?.pointerId === event.pointerId) {
          clearDrag();
        }
      }}
      onLostPointerCapture={(event) => {
        if (drag.current?.pointerId === event.pointerId) {
          clearDrag();
        }
      }}
    >
      {pinned.map((extension) => (
        <button
          key={extension.id}
          ref={(button) => {
            if (button) {
              refs.current.set(extension.id, button);
            } else {
              refs.current.delete(extension.id);
            }
          }}
          type="button"
          data-pin={extension.id}
          className="btn icon ghost shrink-0 touch-none"
          aria-label={extension.name}
          title={extension.popup === null ? t("extensions.noPopup") : extension.name}
          // Not `disabled`: a disabled button takes no pointer, so it could
          // neither be dragged nor show its menu.
          aria-disabled={extension.popup === null || !canOpen}
          onClick={(event) => {
            if (extension.popup !== null && canOpen) {
              onOpen(extension, event.currentTarget);
            }
          }}
          onContextMenu={(event) => {
            event.preventDefault();
            openPinMenu(extension, { x: event.clientX, y: event.clientY });
          }}
        >
          <ExtensionIcon extension={extension} />
        </button>
      ))}
      <span
        ref={indicator}
        hidden
        aria-hidden="true"
        className="bg-accent pointer-events-none absolute inset-y-1 w-0.5 rounded-full"
      />
    </div>
  );
}
