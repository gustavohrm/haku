import { events, type TabId } from "@bindings";
import { useEffect, useRef, type RefObject } from "react";

/**
 * Focuses the address field when a shortcut asks for it, and otherwise leaves
 * it when the active tab changes.
 *
 * A request names its tab, because a shortcut that opens a tab asks before
 * that tab has rendered. The request then waits for the tab to become active,
 * where an ordinary tab switch would end the edit instead.
 */
export function useAddressFocus(tabId: TabId | undefined, inputRef: RefObject<HTMLInputElement | null>) {
  const currentRef = useRef(tabId);
  const pendingRef = useRef<TabId | null>(null);

  useEffect(() => {
    currentRef.current = tabId;
    if (tabId !== undefined && pendingRef.current === tabId) {
      pendingRef.current = null;
      // Leaving first ends an edit begun on the previous tab, so focusing
      // starts a fresh one on this tab's address. Focusing a field that
      // already has focus would change nothing.
      inputRef.current?.blur();
      inputRef.current?.focus();
      return;
    }
    // An edit belongs to the tab it was started in. Switching tabs abandons it,
    // so the field never shows one tab's address while another is active.
    inputRef.current?.blur();
  }, [tabId, inputRef]);

  useEffect(() => {
    let isMounted = true;
    let unlisten: (() => void) | null = null;

    const listen = async () => {
      const stop = await events.addressFocusRequested.listen((event) => {
        if (event.payload === currentRef.current) {
          inputRef.current?.focus();
        } else {
          pendingRef.current = event.payload;
        }
      });
      if (isMounted) {
        unlisten = stop;
      } else {
        stop();
      }
    };
    void listen();

    return () => {
      isMounted = false;
      unlisten?.();
    };
  }, [inputRef]);
}
