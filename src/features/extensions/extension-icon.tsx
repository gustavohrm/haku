import { type Extension } from "@bindings";
import { cx } from "@shared/class-names";

/**
 * An extension's own icon, or a generic one when its manifest has none.
 *
 * @param large - Settings lists it at twice the toolbar's size.
 */
export function ExtensionIcon({ extension, large = false }: { extension: Extension; large?: boolean }) {
  if (extension.icon === null) {
    return <i className={cx("ic-blocks shrink-0", large ? "ic-lg" : "ic-sm")} aria-hidden="true" />;
  }
  return <img src={extension.icon} alt="" className={cx("shrink-0", large ? "size-8" : "size-4")} draggable={false} />;
}
