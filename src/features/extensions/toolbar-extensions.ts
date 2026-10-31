import { type Extension } from "@bindings";

/**
 * What the toolbar shows of the installed extensions: every one switched on,
 * for its menu, and of those the pinned ones, in their order.
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

/**
 * The pins after moving `id` to position `to` among the pins that show.
 *
 * Only shown pins are counted, so a move does what the toolbar shows; a
 * hidden pin keeps its place, next to the shown pin it followed.
 *
 * @param pinned - Every pin, shown or not, in order.
 * @param shown - The ids of the pins that show, in order.
 * @param id - The pin to move; it must show.
 * @param to - Its position among the shown pins once moved.
 * @returns The new order of every pin, or `pinned` when nothing moves, or
 *   when `shown` names a pin no longer in `pinned`.
 */
export function movePin(
  pinned: readonly string[],
  shown: readonly string[],
  id: string,
  to: number,
): readonly string[] {
  const others = shown.filter((other) => other !== id);
  if (!shown.includes(id) || !pinned.includes(id) || others.length === 0 || shown.indexOf(id) === to) {
    return pinned;
  }
  const rest = pinned.filter((other) => other !== id);
  const before = others[Math.max(0, to)];
  const anchor = rest.indexOf(before ?? others.at(-1)!);
  // `shown` is from before the pins last changed, and its pin is gone.
  if (anchor === -1) {
    return pinned;
  }
  const at = before === undefined ? anchor + 1 : anchor;
  return [...rest.slice(0, at), id, ...rest.slice(at)];
}
