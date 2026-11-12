import type {
  AddressBarLayout,
  BarLayout,
  Settings,
  SidebarSide,
  TabLayout,
  ToolbarPlacement,
  WindowKind,
} from "@bindings";

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
  /** How the address field is edited. Always `floating` with the toolbar in the column. */
  address: AddressBarLayout;
}

type LayoutSettings = Pick<
  Settings,
  "tabLayout" | "barLayout" | "sidebarSide" | "toolbarPlacement" | "sidebarWidth" | "addressBarLayout"
>;

/**
 * Maps the layout settings onto the shell, for a window of `kind`.
 *
 * Until settings load, the defaults Rust would give. A popup has no tabs, and
 * its bar stays docked so its read-only address is always on screen
 * ([Appearance § Popups](../../../docs/specs/appearance.md#popups)). A private
 * window's bars stay docked too, so its Private marker never hides
 * ([Appearance § Private windows](../../../docs/specs/appearance.md#private-windows)).
 * The toolbar goes in the sidebar only when there is one, and the address
 * field there always floats: a list hanging from a narrow column has no room
 * for a title and an address per row
 * ([Address bar § Layouts](../../../docs/specs/address-bar.md#layouts)).
 */
export function shellLayout(settings: LayoutSettings | null, kind: WindowKind, isPrivate: boolean): ShellLayout {
  const sidebarWidth = settings?.sidebarWidth ?? DEFAULT_SIDEBAR_WIDTH;
  if (kind === "popup") {
    return { tabs: "horizontal", bars: "docked", side: "left", toolbar: "top", sidebarWidth, address: "inline" };
  }
  const tabs = settings?.tabLayout ?? "horizontal";
  const toolbar = tabs === "vertical" ? (settings?.toolbarPlacement ?? "top") : "top";
  return {
    tabs,
    bars: isPrivate ? "docked" : (settings?.barLayout ?? "docked"),
    side: settings?.sidebarSide ?? "left",
    toolbar,
    sidebarWidth,
    address: toolbar === "sidebar" ? "floating" : (settings?.addressBarLayout ?? "inline"),
  };
}

/**
 * Where bookmarks are shown: `bar` under the toolbar, `row` as a row of its own
 * across the top of the page area, `button` in the column's foot row, or
 * nowhere. [Bookmarks § The bookmarks bar](../../../docs/specs/bookmarks.md#the-bookmarks-bar).
 *
 * Nowhere until settings load, and never in a popup, which shows one page.
 *
 * @param onNewTab - Whether the focused tab is on the new-tab page, for `newTab`.
 * @param isBarEmpty - Whether the bar has nothing in it, or bookmarks have not
 *   loaded. On the new-tab page an empty bar offers nothing and is left off;
 *   asked for always, it shows empty, as the place to add to.
 */
export function bookmarksPlace(
  settings: Pick<Settings, "bookmarksBar" | "sidebarBookmarks"> | null,
  layout: Pick<ShellLayout, "toolbar">,
  kind: WindowKind,
  onNewTab: boolean,
  isBarEmpty: boolean,
): "bar" | "row" | "button" | null {
  if (settings === null || kind === "popup") {
    return null;
  }
  const inColumn = layout.toolbar === "sidebar";
  if (inColumn && settings.sidebarBookmarks === "button") {
    return "button";
  }
  const isShown = settings.bookmarksBar === "always" || (settings.bookmarksBar === "newTab" && onNewTab && !isBarEmpty);
  if (!isShown) {
    return null;
  }
  return inColumn ? "row" : "bar";
}
