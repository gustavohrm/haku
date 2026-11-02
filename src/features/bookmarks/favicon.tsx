import { cx } from "@shared/class-names";
import { useState } from "react";

import { faviconOf } from "./tree";

/**
 * A link's icon, derived as a tab's is, with the generic one in its place
 * when it fails to load or the window is private. Keyed by its URL by the
 * caller, so a changed address tries again.
 */
export function Favicon({ url, small = false }: { url: string | null; small?: boolean }) {
  const [broken, setBroken] = useState(false);
  const src = url === null ? null : faviconOf(url);
  if (!src || broken) {
    return <i className={cx("ic-globe text-text-secondary shrink-0", small ? "ic-sm" : "ic-md")} aria-hidden="true" />;
  }
  return (
    <img
      src={src}
      alt=""
      draggable={false}
      className={cx("shrink-0", small ? "size-3.5" : "size-4")}
      onError={() => setBroken(true)}
    />
  );
}
