import { type Shortcut } from "@bindings";
import { keymapStore } from "@ipc/store";

/**
 * The chord a menu item shows for the shortcut doing the same: the first one
 * bound to it in the keymap Rust has in effect, or null while none is.
 */
export function shortcutHint(shortcut: Shortcut): string | null {
  return keymapStore.get().find((binding) => binding.shortcut === shortcut)?.chords[0] ?? null;
}
