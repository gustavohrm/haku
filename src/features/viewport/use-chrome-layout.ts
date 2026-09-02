import { commands, type Viewport } from "@bindings";
import { overlayStore } from "@features/overlays/registry";
import { rectOf } from "@features/overlays/registry";
import { useEffect, type RefObject } from "react";

/**
 * Reports the interface's geometry to Rust, which positions the content webview
 * and masks the chrome's input area to match.
 *
 * Both halves travel in one call on purpose. If the webview bounds and the input
 * mask were set separately they could disagree for a frame, which shows as a
 * flickering strip along the edge of the page.
 *
 * @param ref - The element page content is shown inside.
 * @param covered - True when the chrome should stay solid: an internal page is
 * open, so nothing should show through and no clicks should reach a webview.
 */
export function useChromeLayout(ref: RefObject<HTMLElement | null>, covered: boolean): void {
  useEffect(() => {
    const element = ref.current;
    if (!element) {
      return;
    }

    let frame = 0;
    let previous = "";

    const push = () => {
      const viewport: Viewport | null = covered ? null : rectOf(element);
      const overlays = [...overlayStore.get()];
      // The page is a native view that CSS cannot clip, so its corners are
      // rounded by rounding the hole. Reading the radius back from the
      // stylesheet keeps CSS the one place that number is written.
      const radius = Number.parseFloat(getComputedStyle(element).borderTopLeftRadius) || 0;

      // Layout events fire far more often than the geometry actually changes,
      // and every push crosses the IPC boundary and rebuilds a native region.
      const signature = JSON.stringify({ viewport, overlays, radius });
      if (signature === previous) {
        return;
      }
      previous = signature;
      void commands.setLayout({ viewport, overlays, radius });
    };

    const schedule = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(push);
    };

    schedule();

    const observer = new ResizeObserver(schedule);
    observer.observe(element);
    window.addEventListener("resize", schedule);
    const stopOverlays = overlayStore.subscribe(schedule);

    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      window.removeEventListener("resize", schedule);
      stopOverlays();
    };
  }, [covered, ref]);
}
