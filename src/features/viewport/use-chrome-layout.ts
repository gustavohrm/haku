import { type Layout } from "@bindings";
import { overlayStore } from "@features/overlays/registry";
import { rectOf } from "@features/overlays/registry";
import { commands } from "@ipc/commands";
import { useBrowserState } from "@ipc/hooks";
import { browserStore, createStore } from "@ipc/store";
import { useEffect, type RefObject } from "react";

// Chrome runs in one JS context per window. Keep the queue across effect restarts.
let pendingLayout: Layout | undefined;
let isSending = false;

/** Last measured pane geometry, for dismissing page-anchored surfaces before moving native bounds. */
export const paneGeometryStore = createStore<Layout["viewports"]>([]);

async function sendLayout(layout: Layout): Promise<void> {
  pendingLayout = layout;
  if (isSending) {
    return;
  }
  isSending = true;
  try {
    while (pendingLayout) {
      const next = pendingLayout;
      pendingLayout = undefined;
      // oxlint-disable-next-line no-await-in-loop -- Layout IPC must stay serialized so older geometry cannot win.
      const result = await commands.setLayout(next);
      // A pending report was measured from newer state than this response.
      if (result.status === "ok" && !pendingLayout && result.data.layoutRevision >= browserStore.get().layoutRevision) {
        browserStore.set(result.data);
      }
    }
  } finally {
    isSending = false;
  }
}

/** Reports every pane and overlay together; one IPC is in flight and the latest pending report wins.
 * `covered` is used by the whole-interface failure screen, where no native hole may remain.
 */
export function useChromeLayout(ref: RefObject<HTMLElement | null>, covered?: boolean): void {
  const { layoutRevision, viewports } = useBrowserState();
  const viewportIds = viewports.map(({ id }) => id).join(",");
  useEffect(() => {
    const element = ref.current;
    if (!element || !viewportIds) {
      return;
    }
    let frame = 0;
    let previous = "";
    const push = () => {
      const surfaces = [...element.querySelectorAll<HTMLElement>("[data-viewport]")];
      const layout: Layout = {
        revision: layoutRevision,
        viewports: viewportIds.split(",").map((id, index, ids) => {
          const surface = surfaces.find((candidate) => candidate.dataset.viewport === id);
          // The failure screen covers all assignments. Divide its measured bounds
          // so the rectangles remain disjoint even when the pane tree has failed.
          const rect = rectOf(surface ?? element);
          if (!surface && ids.length > 1) {
            rect.width /= ids.length;
            rect.x += index * rect.width;
          }
          return {
            id,
            rect,
            // A pane with no rendered surface has nothing to show through it.
            covered: covered ?? (surface ? surface.dataset.covered === "true" : true),
            radius: Number.parseFloat(getComputedStyle(surface ?? element).borderTopLeftRadius) || 0,
          };
        }),
        overlays: [...overlayStore.get()],
      };
      const signature = JSON.stringify(layout);
      if (signature === previous) {
        return;
      }
      previous = signature;
      paneGeometryStore.set(layout.viewports);
      void sendLayout(layout);
    };
    const schedule = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(push);
    };
    const observer = new ResizeObserver(schedule);
    const observe = () => {
      observer.disconnect();
      observer.observe(element);
      element.querySelectorAll("[data-viewport]").forEach((surface) => observer.observe(surface));
      schedule();
    };
    const mutations = new MutationObserver(observe);
    mutations.observe(element, {
      subtree: true,
      childList: true,
      attributes: true,
      attributeFilter: ["data-covered", "data-viewport", "style"],
    });
    observe();
    window.addEventListener("resize", schedule);
    const stopOverlays = overlayStore.subscribe(schedule);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      mutations.disconnect();
      window.removeEventListener("resize", schedule);
      stopOverlays();
    };
  }, [covered, ref, layoutRevision, viewportIds]);
}
