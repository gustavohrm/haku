import { type Layout } from "@bindings";
import { overlayStore } from "@features/overlays/registry";
import { rectOf } from "@features/overlays/registry";
import { commands } from "@ipc/commands";
import { useBrowserState } from "@ipc/hooks";
import { browserStore } from "@ipc/store";
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
  const { focusedViewport, layoutRevision, viewports } = useBrowserState();
  const viewportIds = viewports.map(({ id }) => id).join(",");
  const hasViewport = viewports.some((pane) => pane.id === focusedViewport);
  useEffect(() => {
    const element = ref.current;
    if (!element || !hasViewport) {
      return;
    }

    let frame = 0;
    let previous = "";

    const push = async () => {
      const rect = rectOf(element);
      const overlays = [...overlayStore.get()];
      // The page is a native view that CSS cannot clip, so its corners are
      // rounded by rounding the hole. Reading the radius back from the
      // stylesheet keeps CSS the one place that number is written.
      const radius = Number.parseFloat(getComputedStyle(element).borderTopLeftRadius) || 0;

      // Layout events fire far more often than the geometry actually changes,
      // and every push crosses the IPC boundary and rebuilds a native region.
      const layout: Layout = {
        revision: layoutRevision,
        // Until pane rendering lands, only the focused assignment has a surface.
        viewports: viewportIds
          .split(",")
          .map((id) =>
            id === focusedViewport
              ? { id, rect, covered, radius }
              : { id, rect: { x: 0, y: 0, width: 0, height: 0 }, covered: true, radius: 0 },
          ),
        overlays,
      };
      const signature = JSON.stringify(layout);
      if (signature === previous) {
        return;
      }
      previous = signature;
      const result = await commands.setLayout(layout);
      if (result.status === "ok") {
        browserStore.set(result.data);
      }
    };

    const schedule = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => void push());
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
  }, [covered, ref, focusedViewport, layoutRevision, hasViewport, viewportIds]);
}
