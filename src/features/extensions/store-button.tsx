import { useBarHold } from "@features/bars/holds";
import { useMenuDismiss } from "@features/overlays/use-menu-dismiss";
import { useOverlay } from "@features/overlays/use-overlay";
import { t } from "@shared/i18n";
import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { AddQuestion } from "./add-question";
import { placePanel } from "./extensions-bar";
import { ADDING, addFromStore, answerExtension, useExtensionChange, useExtensionToAdd } from "./manage";

type PanelPlace = ReturnType<typeof placePanel>;

/**
 * **Add to Haku**, shown in the toolbar while the tab is on an extension's
 * page in the Chrome Web Store. Rust downloads the extension and checks the
 * store signed it, then the question of what it may do hangs under the
 * button. Closing the question any other way than Add answers no.
 * [Extensions § From the Chrome Web Store](../../../docs/specs/extensions.md#from-the-chrome-web-store).
 *
 * @param id - The extension the page is about.
 */
export function StoreButton({ id }: { id: string }) {
  const change = useExtensionChange();
  const asked = useExtensionToAdd("store");
  const [place, setPlace] = useState<PanelPlace | null>(null);
  const open = asked !== null && place !== null;
  const buttonRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLElement>(null);
  const panelId = useId();

  useLayoutEffect(() => {
    const button = buttonRef.current?.getBoundingClientRect();
    setPlace(
      asked === null || !button
        ? null
        : placePanel(button, document.documentElement.clientWidth, document.documentElement.clientHeight),
    );
  }, [asked]);

  // The panel hangs over the page, so it must be registered to be clickable.
  useOverlay(panelRef, open);
  useBarHold("toolbar", open);

  const decline = useCallback(() => void answerExtension(false), []);
  useMenuDismiss(open, decline, panelRef, buttonRef);
  // The hook focuses the panel after the question, its child, focused
  // Cancel, so Cancel is focused again once it has.
  useEffect(() => {
    if (open) {
      panelRef.current?.querySelector("button")?.focus();
    }
  }, [open]);

  // Leaving the store page takes the button, and with it the question, away.
  const waiting = useRef(false);
  useEffect(() => {
    waiting.current = asked !== null;
  }, [asked]);
  useEffect(
    () => () => {
      if (waiting.current) {
        void answerExtension(false);
      }
    },
    [],
  );

  const adding = change === ADDING && asked === null;
  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        className="btn soft edgeless shrink-0 gap-2"
        aria-expanded={open}
        aria-controls={open ? panelId : undefined}
        disabled={change !== null}
        onClick={() => void addFromStore(id)}
      >
        <i className="ic-circle-plus ic-sm" aria-hidden="true" />
        {adding ? t("extensions.store.adding") : t("extensions.store.add")}
      </button>

      {/* On the overlay surface, opaque, and without a shadow, which the input
          mask would cut off at its edge, as the extensions menu is. */}
      {open &&
        createPortal(
          <section
            ref={panelRef}
            id={panelId}
            aria-label={t("extensions.store.add")}
            tabIndex={-1}
            className="haku-overlay fixed z-10 flex w-80 flex-col overflow-y-auto overscroll-contain px-3 pt-3 pb-1 outline-none"
            style={place}
          >
            <AddQuestion asked={asked} />
          </section>,
          document.body,
        )}
    </>
  );
}
