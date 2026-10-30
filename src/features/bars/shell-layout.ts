import type { BarLayout, Settings, TabLayout, WindowKind } from "@bindings";

/** The appearance attributes the shell's root carries. */
export interface ShellLayout {
  tabs: TabLayout;
  bars: BarLayout;
}

/**
 * Maps the layout settings onto the shell, for a window of `kind`.
 *
 * Until settings load, the defaults Rust would give. A popup has no tabs, and
 * its bar stays docked so its read-only address is always on screen
 * ([Appearance § Popups](../../../docs/specs/appearance.md#popups)).
 */
export function shellLayout(settings: Pick<Settings, "tabLayout" | "barLayout"> | null, kind: WindowKind): ShellLayout {
  if (kind === "popup") {
    return { tabs: "horizontal", bars: "docked" };
  }
  return { tabs: settings?.tabLayout ?? "horizontal", bars: settings?.barLayout ?? "docked" };
}
