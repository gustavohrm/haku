import { type Tab } from "@bindings";
import { commands } from "@ipc/commands";
import { useEffect, useState } from "react";

/** Longest the cover hides a loading page, in milliseconds. */
export const COVER_TIMEOUT = 4000;

interface Cover {
  /** The tab the cover is for. */
  tab: number;
  /** The tab's capture as a `data:` URL, or `null` while there is none. */
  src: string | null;
  /** The cover has been up for {@link COVER_TIMEOUT} and comes off. */
  expired: boolean;
}

/**
 * Whether to cover the page while the active tab reloads, and with what.
 *
 * A tab coming back from discarded loads into a slot that may still show
 * another tab's page, or a blank one, until its own document arrives. Rust
 * marks it `restoring` until then; the cover shows the tab as it was left, or
 * the plain surface when there is no capture, so neither is ever seen.
 *
 * @returns The cover's image, `""` for the plain surface, or `null` when no
 *   cover is shown.
 */
export function usePreviewCover(tab: Tab | null): string | null {
  const restoring = tab?.restoring ? tab.id : null;
  const [cover, setCover] = useState<Cover | null>(null);

  useEffect(() => {
    if (restoring === null) {
      setCover(null);
      return;
    }
    let current = true;
    setCover({ tab: restoring, src: null, expired: false });

    const load = async () => {
      const result = await commands.tabPreview(restoring);
      if (current && result.status === "ok" && result.data !== null) {
        const src = result.data;
        setCover((shown) => (shown?.tab === restoring ? { ...shown, src } : shown));
      }
    };
    void load();

    // A page that never reports its document loading must not stay hidden.
    const timer = window.setTimeout(() => {
      setCover((shown) => (shown?.tab === restoring ? { ...shown, expired: true } : shown));
    }, COVER_TIMEOUT);

    return () => {
      current = false;
      window.clearTimeout(timer);
    };
  }, [restoring]);

  if (restoring === null || cover?.tab !== restoring || cover.expired) {
    return null;
  }
  return cover.src ?? "";
}

/**
 * The cover itself: the capture, scaled to the viewport's width and anchored
 * at its top-left as the page was, over the plain surface.
 *
 * It is opaque and fills the viewport. While it shows, the chrome is kept solid
 * over the page, the same way as for an internal page: the chrome can only be
 * seen where its input region includes it, so the cover also takes the clicks
 * until the page has loaded.
 */
export function PreviewCover({ src }: { src: string }) {
  return (
    <div className="bg-surface absolute inset-0" aria-hidden="true">
      {src && <img src={src} alt="" className="size-full object-cover object-left-top" draggable={false} />}
    </div>
  );
}
