import type { TabId } from "@bindings";
import { feedback } from "@features/feedback/feedback";
import { t } from "@shared/i18n";
import { useEffect } from "react";

/** How long the notice stays, long enough to read before the video starts. */
const NOTICE_MS = 4_000;

/**
 * Names the site whose page went fullscreen, and how to leave.
 *
 * A fullscreen page owns every pixel of the screen and could draw something
 * that looks like Haku or Windows; naming it on entry is how browsers answer
 * that. [Fullscreen](../../../docs/specs/fullscreen.md#decisions).
 *
 * @param tab - The fullscreen tab, or `null` while none is.
 * @param url - That tab's address.
 */
export function useFullscreenNotice(tab: TabId | null, url: string): void {
  useEffect(() => {
    if (tab === null) {
      return;
    }
    const handle = feedback().info({
      title: titleFor(url),
      description: t("fullscreen.exit"),
      duration: NOTICE_MS,
    });
    return () => handle.dismiss();
    // Shown on entering, and again when fullscreen passes to the other pane of
    // a split, which may be another site; not as the page changes its address.
    // oxlint-disable-next-line react-hooks/exhaustive-deps
  }, [tab]);
}

function titleFor(url: string): string {
  let host = "";
  try {
    host = new URL(url).host;
  } catch {
    // An unparseable address still gets a notice; it just cannot name a site.
  }
  return host ? t("fullscreen.site").replace("{host}", host) : t("fullscreen.page");
}
