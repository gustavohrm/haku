import { reportUnexpected } from "@features/feedback/feedback";
import { Menu } from "@features/overlays/menu";
import { t } from "@shared/i18n";
import { useEffect } from "react";

import { fieldMenu } from "./items";
import { closeMenu, openMenu, useOpenMenu } from "./store";

/** Input types that hold text a person types and selects. */
const TEXT_INPUTS = new Set(["text", "search", "url", "email", "tel", "password", "number"]);

type TextField = HTMLInputElement | HTMLTextAreaElement;

function textFieldOf(target: EventTarget | null): TextField | null {
  if (target instanceof HTMLTextAreaElement) {
    return target;
  }
  if (target instanceof HTMLInputElement && TEXT_INPUTS.has(target.type)) {
    return target;
  }
  return null;
}

/**
 * Draws the window's context menu, and turns off the engine's own menu over
 * the interface.
 *
 * A right-click a component has not answered with a menu of its own opens the
 * text field menu over a text field, and nothing anywhere else.
 * [Context menus § Interface menus](../../../docs/specs/context-menus.md#interface-menus).
 */
export function ContextMenuHost() {
  const open = useOpenMenu();

  useEffect(() => {
    // Bubbling, so it runs after React's handlers, which mark the event as
    // answered by preventing its default.
    const onContextMenu = (event: MouseEvent) => {
      if (event.defaultPrevented) {
        return;
      }
      event.preventDefault();
      const field = textFieldOf(event.target);
      if (field) {
        openFieldMenu(field, { x: event.clientX, y: event.clientY });
      }
    };
    document.addEventListener("contextmenu", onContextMenu);
    return () => document.removeEventListener("contextmenu", onContextMenu);
  }, []);

  if (open === null) {
    return null;
  }
  return (
    <Menu
      key={open.key}
      items={open.items}
      at={open.at}
      label={open.label}
      onClose={(fromInside) => closeMenu(open.key, fromInside)}
    />
  );
}

function openFieldMenu(field: TextField, at: { x: number; y: number }) {
  // A number field reports no selection at all; then Cut and Copy stay
  // offered rather than never being.
  const selected = field.selectionStart === null || field.selectionStart !== field.selectionEnd;
  const editable = !field.readOnly && !field.disabled;
  // The menu takes focus, so each action gives it back to the field first,
  // where the selection is still kept.
  const inField = (action: () => void) => () => {
    field.focus();
    action();
  };
  openMenu({
    at,
    label: fieldName(field),
    items: fieldMenu(selected, editable, {
      cut: inField(() => document.execCommand("cut")),
      copy: inField(() => document.execCommand("copy")),
      paste: inField(() => void pasteInto(field)),
      selectAll: inField(() => field.select()),
    }),
  });
}

/** What a field is called, for the menu's accessible name. */
function fieldName(field: TextField): string {
  const named = field.getAttribute("aria-label") ?? field.labels?.[0]?.textContent ?? field.placeholder;
  return named.trim() || t("menu.field");
}

async function pasteInto(field: TextField) {
  let text: string;
  try {
    text = await navigator.clipboard.readText();
  } catch (error) {
    // A refusal is reported, not dropped, so a paste never fails unseen.
    reportUnexpected(error);
    return;
  }
  // The read can wait on the user; by then the field may be gone, or they
  // may be typing somewhere else.
  if (!field.isConnected || document.activeElement !== field) {
    return;
  }
  // Inserted as typing would, so the field's own input handling sees it.
  document.execCommand("insertText", false, text);
}
