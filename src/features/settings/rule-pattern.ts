import { type UrlPattern } from "@bindings";

/**
 * Which kind of pattern `typed` most likely is, so a rule can be added
 * without choosing one first: `/…/` is a regular expression, without its
 * slashes; anything with `*` a wildcard; an address with a path, query or
 * fragment a page; anything else a site. The guess is shown, and can be
 * changed, before the rule is saved.
 * [History § Rules](../../../docs/specs/history.md#rules).
 */
export function guessPattern(typed: string): UrlPattern {
  const value = typed.trim();
  if (value.length > 2 && value.startsWith("/") && value.endsWith("/")) {
    return { kind: "regex", value: value.slice(1, -1) };
  }
  if (value.includes("*")) {
    return { kind: "wildcard", value };
  }
  if (value.includes("://")) {
    try {
      const url = new URL(value);
      if (url.pathname !== "/" || url.search !== "" || url.hash !== "") {
        return { kind: "page", value };
      }
    } catch {
      // Not an address after all: Rust keeps the host of whatever it can.
    }
  }
  return { kind: "site", value };
}

/** `items` with the one at `from` moved to `to`, or unchanged when either is outside it. */
export function moveItem<T>(items: readonly T[], from: number, to: number): T[] {
  const next = [...items];
  if (from < 0 || from >= next.length || to < 0 || to >= next.length) {
    return next;
  }
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item as T);
  return next;
}
