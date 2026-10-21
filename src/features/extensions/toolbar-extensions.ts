import { type Extension } from "@bindings";

/**
 * What the toolbar shows of the installed extensions: every one switched on,
 * for its menu, and of those the pinned ones, in the order they were pinned.
 *
 * A pin is kept for an extension switched off or no longer installed, so it
 * comes back with it, but shows nothing meanwhile.
 */
export function toolbarExtensions(
  installed: readonly Extension[],
  disabled: readonly string[],
  pinned: readonly string[],
): { enabled: Extension[]; pinned: Extension[] } {
  const enabled = installed.filter((extension) => !disabled.includes(extension.id));
  return {
    enabled,
    pinned: pinned.flatMap((id) => enabled.filter((extension) => extension.id === id)),
  };
}
