/**
 * Joins class names, skipping the ones a condition switched off.
 *
 * @param names - Class names, or falsy values for conditions that did not apply.
 * @returns A single class attribute value.
 */
export function cx(...names: (string | false | null | undefined)[]): string {
  return names.filter(Boolean).join(" ");
}
