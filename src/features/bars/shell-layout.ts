import type { BarLayout, Settings, SidebarSide, TabLayout, ToolbarPlacement, WindowKind } from "@bindings";

/** The tab column's width, in CSS pixels, before settings load: what Rust defaults to. */
export const DEFAULT_SIDEBAR_WIDTH = 240;

/** The appearance attributes the shell's root carries. */
export interface ShellLayout {
  tabs: TabLayout;
  bars: BarLayout;
  /** Which side the tab column is on. Meaningful only with vertical tabs. */
  side: SidebarSide;
  /** Where the toolbar is. Always `top` unless the tabs are in a column. */
  toolbar: ToolbarPlacement;
  /** The tab column's width, in CSS pixels. */
  sidebarWidth: number;
}

type LayoutSettings = Pick<Settings, "tabLayout" | "barLayout" | "sidebarSide" | "toolbarPlacement" | "sidebarWidth">;

/**
 * Maps the layout settings onto the shell, for a window of `kind`.
 *
 * Until settings load, the defaults Rust would give. A popup has no tabs, and
 * its bar stays docked so its read-only address is always on screen
 * ([Appearance § Popups](../../../docs/specs/appearance.md#popups)). A private
 * window's bars stay docked too, so its Private marker never hides
 * ([Appearance § Private windows](../../../docs/specs/appearance.md#private-windows)).
 * The toolbar goes in the sidebar only when there is one.
 */
export function shellLayout(settings: LayoutSettings | null, kind: WindowKind, isPrivate: boolean): ShellLayout {
  const sidebarWidth = settings?.sidebarWidth ?? DEFAULT_SIDEBAR_WIDTH;
  if (kind === "popup") {
    return { tabs: "horizontal", bars: "docked", side: "left", toolbar: "top", sidebarWidth };
  }
  const tabs = settings?.tabLayout ?? "horizontal";
  return {
    tabs,
    bars: isPrivate ? "docked" : (settings?.barLayout ?? "docked"),
    side: settings?.sidebarSide ?? "left",
    toolbar: tabs === "vertical" ? (settings?.toolbarPlacement ?? "top") : "top",
    sidebarWidth,
  };
}
