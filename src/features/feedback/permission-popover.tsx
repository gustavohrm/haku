import type { PermissionDecision, PermissionRequest, Tab } from "@bindings";
import { MENU_EDGE, type Box, type Point } from "@features/overlays/menu-model";
import { useOverlay } from "@features/overlays/use-overlay";
import { commands } from "@ipc/commands";
import { t } from "@shared/i18n";
import { useId, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import { createPortal } from "react-dom";

import { permissionTitle, placePrompt, type PromptAnchor } from "./permission-prompt";

/** Marks the site menu's button, which the prompt is anchored to. */
export const SITE_MENU_ANCHOR = "data-site-menu";

interface PermissionPopoverProps {
  /** The active tab. */
  tab: Tab | null;
  /** Whether the window is private, whose decisions are not kept in settings. */
  isPrivate: boolean;
}

/**
 * Shows the permission request the active tab is waiting on, in a popover at
 * the site menu, and answers it. [Permissions](../../../docs/specs/permissions.md).
 *
 * It does not block the window: the tabs, the page and the rest of the
 * interface stay usable while it waits, and it takes no focus, so a key meant
 * for the page cannot answer it. A click elsewhere leaves it open; Escape
 * inside it, or its close button, dismisses the request without storing it.
 *
 * Only the active tab's request is shown, and only once its page dialog, if
 * any, is answered. If Rust withdraws the request first, because the page went
 * away, it goes without an answer.
 */
export function PermissionPopover({ tab, isPrivate }: PermissionPopoverProps) {
  const request = tab?.dialog ? null : (tab?.permission ?? null);
  // An answered request stays on the tab until Rust takes it off; hidden
  // meanwhile, so it cannot be answered twice.
  const [answered, setAnswered] = useState<number | null>(null);
  const shown = tab !== null && request !== null && request.id !== answered ? request : null;

  if (tab === null || shown === null) {
    return null;
  }
  const answer = async (decision: PermissionDecision) => {
    setAnswered(shown.id);
    const result = await commands.answerPermission(tab.id, shown.id, decision);
    // An answer that never reached Rust leaves the request waiting: shown again, to answer again.
    if (result.status !== "ok") {
      setAnswered((current) => (current === shown.id ? null : current));
    }
  };
  // Keyed by the request, so a new one is measured and placed afresh.
  return <Prompt key={shown.id} request={shown} isPrivate={isPrivate} onAnswer={(decision) => void answer(decision)} />;
}

interface PromptProps {
  request: PermissionRequest;
  isPrivate: boolean;
  onAnswer: (decision: PermissionDecision) => void;
}

function Prompt({ request, isPrivate, onAnswer }: PromptProps) {
  const ref = useRef<HTMLElement>(null);
  const [position, setPosition] = useState<Point | null>(null);
  const titleId = useId();
  const messageId = useId();

  useOverlay(ref, position !== null);

  // Placed before it is shown, and again as the window or the bars move.
  useLayoutEffect(() => {
    const place = () => {
      const prompt = ref.current;
      if (prompt) {
        const size = { width: prompt.offsetWidth, height: prompt.offsetHeight };
        setPosition(placePrompt(siteMenuAnchor(), size, { width: window.innerWidth, height: window.innerHeight }));
      }
    };
    place();
    // The bars move without the window resizing: a floating bar slides in
    // once the prompt holds it, and the sidebar is resized.
    window.addEventListener("resize", place);
    document.addEventListener("transitionend", place, true);
    const observer = new ResizeObserver(place);
    const sidebar = document.querySelector(".haku-sidebar");
    if (sidebar) {
      observer.observe(sidebar);
    }
    return () => {
      window.removeEventListener("resize", place);
      document.removeEventListener("transitionend", place, true);
      observer.disconnect();
    };
  }, []);

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onAnswer("dismiss");
    }
  };

  return createPortal(
    // Not modal and not focused on opening: the window stays usable, and a key
    // pressed for the page is not taken as an answer.
    // oxlint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- Escape dismisses, as the spec says, once focus is in it.
    <section
      ref={ref}
      role="alertdialog"
      aria-modal="false"
      aria-labelledby={titleId}
      aria-describedby={messageId}
      onKeyDown={onKeyDown}
      // Narrower and scrolling in a window smaller than it, such as a small popup, so its buttons stay in reach.
      className="haku-overlay fixed z-10 flex max-h-[calc(100vh-8px)] w-80 max-w-[calc(100vw-8px)] flex-col gap-2 overflow-y-auto p-2"
      style={position === null ? { left: 0, top: 0, visibility: "hidden" } : { left: position.x, top: position.y }}
    >
      <div className="flex items-start gap-2">
        <h2 id={titleId} className="min-w-0 flex-1 px-1 pt-1 font-medium break-words">
          {permissionTitle(request)}
        </h2>
        <button
          type="button"
          className="btn icon ghost sm shrink-0"
          aria-label={t("permission.dismiss")}
          title={t("permission.dismiss")}
          onClick={() => onAnswer("dismiss")}
        >
          <i className="ic-x ic-sm" aria-hidden="true" />
        </button>
      </div>
      <p id={messageId} className="text-text-secondary px-1 text-sm">
        {t(isPrivate ? "permission.messagePrivate" : "permission.message")}
      </p>
      <div className="flex justify-end gap-2">
        <button type="button" className="btn ghost" onClick={() => onAnswer("block")}>
          {t("permission.block")}
        </button>
        <button type="button" className="btn" onClick={() => onAnswer("allow")}>
          {t("permission.allow")}
        </button>
      </div>
    </section>,
    document.body,
  );
}

/** The site menu's button, measured, or null when none is on screen. */
function siteMenuAnchor(): PromptAnchor | null {
  const button = document.querySelector<HTMLElement>(`[${SITE_MENU_ANCHOR}]`);
  const field = button?.closest("label");
  if (!button || !field) {
    return null;
  }
  const box = (element: Element): Box => {
    const { left, top, right, bottom } = element.getBoundingClientRect();
    return { left, top, right, bottom };
  };
  const fieldBox = box(field);
  // Hidden, as in fullscreen, or slid off the window with a floating bar.
  if (
    fieldBox.right <= fieldBox.left ||
    fieldBox.bottom <= MENU_EDGE ||
    fieldBox.top >= window.innerHeight ||
    fieldBox.right <= 0 ||
    fieldBox.left >= window.innerWidth
  ) {
    return null;
  }
  const sidebar = button.closest(".haku-sidebar");
  return {
    field: fieldBox,
    glyph: box(button.querySelector("i") ?? button),
    sidebar: sidebar ? box(sidebar) : null,
  };
}
