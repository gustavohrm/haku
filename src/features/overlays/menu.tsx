import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import { createPortal } from "react-dom";

import {
  firstIndex,
  indexByLetter,
  isChoosable,
  lastIndex,
  MENU_EDGE,
  placeAtPoint,
  placeBeside,
  stepIndex,
  type Box,
  type ChoosableItem,
  type MenuConfirmation,
  type MenuItem,
  type Point,
} from "./menu-model";
import { useMenuDismiss } from "./use-menu-dismiss";
import { useOverlay } from "./use-overlay";

export type { MenuConfirmation, MenuItem } from "./menu-model";

/** An item that runs something, as opposed to opening a submenu. */
type RunnableItem = Extract<ChoosableItem, { onSelect: () => void }>;

interface MenuProps {
  items: readonly MenuItem[];
  /** Where the pointer was, in the window's CSS pixels. */
  at: Point;
  /** What the menu is about, for assistive technology. */
  label: string;
  /**
   * Called once when the menu closes. `fromInside` is true for a choice,
   * Escape or Tab, and false when focus went elsewhere: a click outside, the
   * window losing focus, or a resize.
   */
  onClose: (fromInside: boolean) => void;
}

/**
 * A menu opened at the pointer: the one component every menu Haku draws uses.
 *
 * It takes focus when it opens and moves through its items from the
 * keyboard, as [Context menus § The menu component] describes. Each level is
 * registered as an overlay, so it stays clickable over the page. It is on the
 * opaque overlay surface, with no shadow,
 * which the input mask would cut off at its edge.
 * Focus goes back to where it was when the menu closes from inside it.
 * An item with a `confirm` turns the menu into that question where it was,
 * and runs only once it is answered; Escape or a click away answers no.
 *
 * [Context menus § The menu component]: ../../../docs/specs/context-menus.md#the-menu-component
 */
export function Menu({ items, at, label, onClose }: MenuProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const closedFromInside = useRef(false);
  const close = useCallback(
    (fromInside: boolean) => {
      closedFromInside.current = fromInside;
      onClose(fromInside);
    },
    [onClose],
  );
  useMenuDismiss(true, close, rootRef, null);
  const [confirming, setConfirming] = useState<(RunnableItem & { confirm: MenuConfirmation }) | null>(null);

  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const root = rootRef.current;
    return () => {
      const current = document.activeElement;
      // Only from inside: a click elsewhere, or the window losing focus, has
      // already moved focus where it was meant to go.
      if (closedFromInside.current && (current === null || current === document.body || root?.contains(current))) {
        previous?.focus();
      }
    };
  }, []);

  return createPortal(
    // Contents only: it gives the levels one element to be outside of, and
    // takes no space of its own.
    <div ref={rootRef} className="contents">
      {confirming ? (
        <MenuQuestion
          at={at}
          label={confirming.label}
          confirm={confirming.confirm}
          onAnswer={(yes) => {
            if (yes) {
              confirming.onSelect();
            }
            close(true);
          }}
        />
      ) : (
        <MenuList
          items={items}
          anchor={{ at }}
          label={label}
          level={0}
          focusFirst={false}
          onChosen={() => close(true)}
          onConfirm={setConfirming}
        />
      )}
    </div>,
    document.body,
  );
}

type Anchor = { at: Point } | { beside: Box };

interface MenuListProps {
  items: readonly MenuItem[];
  anchor: Anchor;
  label: string;
  level: number;
  /** Highlight the first item on opening, as opening from the keyboard does. */
  focusFirst: boolean;
  /** An item was chosen; the whole menu closes. */
  onChosen: () => void;
  /** An item that asks first was chosen; the menu turns into its question. */
  onConfirm: (item: RunnableItem & { confirm: MenuConfirmation }) => void;
  /** Closes this level only, back to its parent's item. */
  onBack?: () => void;
}

interface OpenSubmenu {
  index: number;
  /** Opened from the keyboard, so its first item takes focus. */
  focusFirst: boolean;
  /** The row that opened it, which it is placed beside. */
  row: HTMLElement | null;
}

/** One level of a menu: the top one, or a submenu beside its item. */
function MenuList({ items, anchor, label, level, focusFirst, onChosen, onConfirm, onBack }: MenuListProps) {
  const listRef = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const [position, setPosition] = useState<Point | null>(null);
  const [active, setActive] = useState(() => (focusFirst ? firstIndex(items) : -1));
  const [submenu, setSubmenu] = useState<OpenSubmenu | null>(null);
  const baseId = useId();
  const showsMarks = items.some((item) => item.kind !== "separator" && (item.icon || item.kind !== "command"));

  useOverlay(listRef, position !== null);

  // Measured before it is shown, so it never flashes where it does not fit.
  useLayoutEffect(() => {
    const list = listRef.current;
    if (!list) {
      return;
    }
    const size = { width: list.offsetWidth, height: list.offsetHeight };
    const bounds = { width: window.innerWidth, height: window.innerHeight };
    const inset = list.clientTop + (Number.parseFloat(getComputedStyle(list).paddingTop) || 0);
    setPosition(
      "at" in anchor ? placeAtPoint(anchor.at, size, bounds) : placeBeside(anchor.beside, size, bounds, inset),
    );
  }, [anchor]);

  useEffect(() => {
    if (position === null) {
      return;
    }
    const target = active >= 0 ? itemRefs.current[active] : level === 0 ? listRef.current : null;
    target?.focus({ preventScroll: active < 0 });
  }, [active, position, level]);

  const closeSubmenu = useCallback(() => {
    setSubmenu((open) => {
      if (open) {
        itemRefs.current[open.index]?.focus();
      }
      return null;
    });
  }, []);

  const choose = (index: number, fromKeyboard: boolean) => {
    const item = items[index];
    if (!isChoosable(item)) {
      return;
    }
    if (item.kind === "submenu") {
      setSubmenu({ index, focusFirst: fromKeyboard, row: itemRefs.current[index] ?? null });
      return;
    }
    if (item.confirm) {
      onConfirm({ ...item, confirm: item.confirm });
      return;
    }
    item.onSelect();
    onChosen();
  };

  const onKeyDown = (event: KeyboardEvent) => {
    const handled = handleKey(event.key);
    if (handled) {
      event.preventDefault();
      event.stopPropagation();
    }
  };

  const handleKey = (key: string): boolean => {
    switch (key) {
      case "ArrowDown":
        setActive(stepIndex(items, active, 1));
        return true;
      case "ArrowUp":
        setActive(stepIndex(items, active < 0 ? 0 : active, -1));
        return true;
      case "Home":
        setActive(firstIndex(items));
        return true;
      case "End":
        setActive(lastIndex(items));
        return true;
      case "ArrowRight":
        if (items[active]?.kind === "submenu" && isChoosable(items[active])) {
          setSubmenu({ index: active, focusFirst: true, row: itemRefs.current[active] ?? null });
        }
        return true;
      case "ArrowLeft":
        onBack?.();
        return true;
      case "Escape":
        // The top level is closed by the dismissal, which handles Escape for
        // every menu.
        if (onBack) {
          onBack();
          return true;
        }
        return false;
      case "Enter":
      case " ":
        if (active >= 0) {
          choose(active, true);
        }
        return true;
      case "Tab":
        onChosen();
        return true;
      default:
        if (key.length === 1 && key !== " ") {
          setActive(indexByLetter(items, active, key));
          return true;
        }
        return false;
    }
  };

  const openItem = submenu === null ? undefined : items[submenu.index];

  return (
    <>
      <div
        ref={listRef}
        role="menu"
        aria-label={label}
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className="haku-overlay fixed z-20 flex max-w-72 min-w-40 flex-col overflow-y-auto p-1 outline-none"
        style={{
          maxHeight: `calc(100vh - ${2 * MENU_EDGE}px)`,
          ...(position === null ? { left: 0, top: 0, visibility: "hidden" } : { left: position.x, top: position.y }),
        }}
      >
        {items.map((item, index) =>
          item.kind === "separator" ? (
            <hr key={item.id} className="bg-text/10 mx-2 my-1 h-px shrink-0 border-0" />
          ) : (
            <MenuRow
              key={item.id}
              ref={(element) => {
                itemRefs.current[index] = element;
              }}
              id={`${baseId}-${index}`}
              item={item}
              highlighted={index === active}
              expanded={submenu?.index === index}
              showsMarks={showsMarks}
              onHover={() => {
                setActive(isChoosable(item) ? index : -1);
                if (item.kind === "submenu" && isChoosable(item)) {
                  setSubmenu({ index, focusFirst: false, row: itemRefs.current[index] ?? null });
                } else {
                  setSubmenu(null);
                }
              }}
              onChoose={() => choose(index, false)}
            />
          ),
        )}
      </div>
      {openItem?.kind === "submenu" && submenu && (
        <Submenu
          // Opened again from the keyboard, it starts over on its first item.
          key={`${submenu.index}-${submenu.focusFirst}`}
          item={openItem}
          row={submenu.row}
          level={level + 1}
          focusFirst={submenu.focusFirst}
          onChosen={onChosen}
          onConfirm={onConfirm}
          onBack={closeSubmenu}
        />
      )}
    </>
  );
}

interface SubmenuProps {
  item: Extract<MenuItem, { kind: "submenu" }>;
  row: HTMLElement | null;
  level: number;
  focusFirst: boolean;
  onChosen: () => void;
  onConfirm: MenuListProps["onConfirm"];
  onBack: () => void;
}

/** A submenu, placed beside the row that opened it. */
function Submenu({ item, row, level, focusFirst, onChosen, onConfirm, onBack }: SubmenuProps) {
  // Read once: a submenu stays where it opened.
  const [anchor] = useState<Anchor | null>(() => {
    const rect = row?.getBoundingClientRect();
    return rect ? { beside: { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom } } : null;
  });
  if (anchor === null) {
    return null;
  }
  return (
    <MenuList
      items={item.items}
      anchor={anchor}
      label={item.label}
      level={level}
      focusFirst={focusFirst}
      onChosen={onChosen}
      onConfirm={onConfirm}
      onBack={onBack}
    />
  );
}

interface MenuQuestionProps {
  at: Point;
  /** The item that asked, which names the question. */
  label: string;
  confirm: MenuConfirmation;
  onAnswer: (yes: boolean) => void;
}

/**
 * The question an item asks before it runs, drawn where the menu was. Focus
 * starts on the safe answer when going ahead destroys something.
 */
function MenuQuestion({ at, label, confirm, onAnswer }: MenuQuestionProps) {
  const ref = useRef<HTMLDivElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const actionRef = useRef<HTMLButtonElement>(null);
  const [position, setPosition] = useState<Point | null>(null);
  const messageId = useId();

  useOverlay(ref, position !== null);

  useLayoutEffect(() => {
    const box = ref.current;
    if (box) {
      const size = { width: box.offsetWidth, height: box.offsetHeight };
      setPosition(placeAtPoint(at, size, { width: window.innerWidth, height: window.innerHeight }));
    }
  }, [at]);

  useEffect(() => {
    if (position !== null) {
      (confirm.destructive ? cancelRef : actionRef).current?.focus();
    }
  }, [position, confirm.destructive]);

  return (
    <div
      ref={ref}
      role="alertdialog"
      aria-label={label}
      aria-describedby={messageId}
      className="haku-overlay fixed z-20 flex w-72 flex-col gap-2 p-2"
      style={position === null ? { left: 0, top: 0, visibility: "hidden" } : { left: position.x, top: position.y }}
    >
      <p id={messageId} className="px-1 pt-1">
        {confirm.message}
      </p>
      <div className="flex justify-end gap-2">
        <button ref={cancelRef} type="button" className="btn ghost sm" onClick={() => onAnswer(false)}>
          {t("dialog.cancel")}
        </button>
        <button
          ref={actionRef}
          type="button"
          className={cx("btn sm", confirm.destructive && "destructive")}
          onClick={() => onAnswer(true)}
        >
          {confirm.action}
        </button>
      </div>
    </div>
  );
}

interface MenuRowProps {
  ref: (element: HTMLButtonElement | null) => void;
  id: string;
  item: ChoosableItem;
  highlighted: boolean;
  expanded: boolean;
  /** Some row in this menu has an icon or a check, so every row keeps room for one. */
  showsMarks: boolean;
  onHover: () => void;
  onChoose: () => void;
}

const ROLES = {
  command: "menuitem",
  submenu: "menuitem",
  checkbox: "menuitemcheckbox",
  radio: "menuitemradio",
} as const;

function MenuRow({ ref, id, item, highlighted, expanded, showsMarks, onHover, onChoose }: MenuRowProps) {
  const checked = item.kind === "checkbox" || item.kind === "radio" ? item.checked : undefined;
  const mark = checked ? "ic-check" : item.icon;

  return (
    <button
      ref={ref}
      id={id}
      type="button"
      role={ROLES[item.kind]}
      tabIndex={-1}
      aria-checked={checked}
      aria-disabled={item.disabled || undefined}
      aria-describedby={item.disabledReason ? `${id}-reason` : undefined}
      aria-haspopup={item.kind === "submenu" ? "menu" : undefined}
      aria-expanded={item.kind === "submenu" ? expanded : undefined}
      className={cx(
        // The recipe's row without its hover: a menu highlights the one active
        // row, which the pointer and the keyboard both move.
        "flex min-h-6 shrink-0 items-center gap-2 rounded-(--radius-small) px-2 text-left outline-none",
        (highlighted || expanded) && "bg-text/8",
        item.disabled && "text-text-secondary opacity-60",
      )}
      onPointerEnter={onHover}
      onClick={onChoose}
    >
      {showsMarks && (
        <span className="flex w-4 shrink-0 justify-center" aria-hidden="true">
          {mark && <i className={cx(mark, "ic-xs")} />}
        </span>
      )}
      <span className={cx("min-w-0 flex-1", item.disabledReason && "py-1")}>
        <span className="block truncate">{item.label}</span>
        {item.disabledReason && (
          <span id={`${id}-reason`} className="block text-xs">
            {item.disabledReason}
          </span>
        )}
      </span>
      {item.shortcut && <span className="text-text-secondary ml-4 shrink-0 text-xs">{item.shortcut}</span>}
      {item.kind === "submenu" && (
        <i className="ic-chevron-right ic-xs text-text-secondary ml-2 shrink-0" aria-hidden="true" />
      )}
    </button>
  );
}
