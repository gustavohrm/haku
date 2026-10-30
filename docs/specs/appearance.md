---
status: IMPLEMENTED
last_updated: 2026-10-30
scope: The layout settings of "Appearance is data" — where the tabs go and whether the bars take space — and how the shell applies them.
---

# Appearance

[First release § Appearance is data](../first-release.md#appearance-is-data) makes appearance a set of settings that the shell maps to attributes and tokens. This spec covers its two layout axes, **tabs** and **bars**. Density, corners, surfaces, palette, presets and custom CSS are not built.

## Settings

Both are fields of the Rust `Settings`, stored in `settings.json` with the rest and read by the interface from the settings store.

| Field       | Values                   | Default      |
| ----------- | ------------------------ | ------------ |
| `tabLayout` | `horizontal`, `vertical` | `horizontal` |
| `barLayout` | `docked`, `floating`     | `docked`     |

The defaults are what Haku drew before the settings existed, so an existing settings file opens unchanged.

Each is an enum in Rust (`TabLayout`, `BarLayout`) and a string union in the bindings. A stored value Haku does not know, such as a hand-edited `"Vertical"` or one written by a later version, reads as the default and leaves every other setting as stored. `read_json` sets a whole file aside when it fails to deserialize, so a strict read would trade every setting for one unknown value. The leniency is written as the enums' own `Deserialize`, because a `deserialize_with` field attribute makes the binding generator split `Settings` into separate read and written types.

Changing either goes through `set_settings` like any setting, and `SettingsChanged` reaches every window, which re-renders in the new layout at once. No command or event was added.

Rejected:

- **Free strings, as `theme` is.** The bindings would accept any string, and every reader would need its own check. An enum gives the interface a closed union and Rust one place to fall back.
- **One combined layout setting** (`horizontal-docked`, …). The axes are independent, and [First release](../first-release.md#appearance-is-data) lists them as separate settings so presets can bundle them.
- **Rejecting an unknown value at load.** That is what a strict enum did, and it costs the user every other setting for a value that only changes the look.

## The shell

The shell's root carries `data-tabs` and `data-bars`, beside `data-density`. A pure function, `shellLayout` in `features/bars`, maps the settings and window kind to them; while settings are still loading it gives the defaults. `src/styles/index.css` places the bars and the page from those attributes with a CSS grid. The page is measured where it lands, as [Chrome layering](chrome-layering.md#the-contract-with-the-interface) requires; nothing computes its rectangle from the settings, so a layout change reaches Rust as one more resize of the viewport.

## Tabs

**Horizontal** is the strip across the top, beside the window controls, above the toolbar.

**Vertical** puts the tabs in a 15 rem column down the left side of the window, beside the page. The toolbar becomes the top row, and the window controls move to its right end, so the window keeps one bar across the top rather than two. The column lists the tabs top to bottom at full width, each as tall as a control, with the new-tab button after the last one, and scrolls when they do not fit. Its free space below them is a drag region for the window, as the strip's free width is.

Everything the strip does, the column does, because it is the same component on the other axis:

- Dragging a tab or a pair reorders it, with the drop line between units, never inside a pair. The drop position is read from the units' measured geometry along the column, and a pointer outside the column's width offers no drop.
- A split pair is one joined unit, stacked: the left member above the right, with one outer shape. Membership and position are announced as in the strip.
- Fixed tabs, the private icon, the waiting-dialog badge, the discarded, frozen, crashed and unloaded markers, middle-click to close, and the tab and strip menus are unchanged.
- Each tab is a button in tab order, and the keyboard opens its menu as in the strip.

Dragging does not scroll the column. A tab can be dropped on any position the column shows; reaching a position scrolled out of view takes two drags.

Rejected: a column holding the toolbar too, as Zen draws it. The address field would be 15 rem wide, too narrow for most addresses, and the toolbar's extension buttons would wrap.

## Bars

**Docked** bars take space and the page fills what they leave, as before.

**Floating** bars sit over the page, which takes the whole window but for a 4 CSS-pixel margin on every side. That margin is chrome, inside the chrome's region, so it sees the pointer arrive at the window's edge, as [First release § What the chrome can draw over a page](../first-release.md#what-the-chrome-can-draw-over-a-page) requires. The margin was already there between the page and the window's edge, so the page's own rectangle loses nothing to it.

A floating bar is shown while any of these holds, and hidden otherwise:

- **Pointer.** It entered the bar's edge of the window, or is over the shown bar. Leaving starts a 300 ms delay before the bar hides, so a pointer crossing back does not flicker it.
- **Focus.** Anything in the bar has focus: the address field focused by Ctrl+L or Ctrl+T, or the keyboard tabbing into it. A hidden bar stays in the tab order for that reason; it is clipped, not removed, so focusing it is what shows it.
- **Something opened from it.** A tab or strip menu, the site menu or the extensions menu holds its bar until it closes. An extension's toolbar popup is a window of its own that Rust does not report closing, so it holds the bar until the chrome has lost focus to it and then sees the pointer or focus again, or the user clicks in the chrome.
- **Something in it needs attention.** A pending dialog on the focused tab shows the toolbar, which names the page asking. A pending dialog on any other tab shows the tabs, where its **Dialog waiting** badge is ([Split view § Page-anchored surfaces](split-view.md#page-anchored-surfaces)).

A shown bar is opaque, on the `chrome` surface, square, with no shadow, and drawn with a 1 px edge on its page side: over an unknown page a border is what separates it ([Code guidelines § Styling](../guidelines/code.md#styling)). It registers itself with `useOverlay` while shown and is otherwise clipped away, so it neither paints nor takes input over the page, or over an internal page the chrome draws in the page's place. It appears and leaves without animation: a moving overlay is measured once it settles, and is hidden under the page until then ([Chrome layering § Registering an overlay](chrome-layering.md#registering-an-overlay)), and a fade would be translucent.

With horizontal tabs, the tab row and the toolbar float together from the top edge. With vertical tabs, the column floats too, as one more bar: the toolbar from the top edge, the column from the left edge below the toolbar's height, each shown on its own. The same setting governs every bar, since a docked column beside floating toolbar would keep most of the space floating was chosen to give the page.

Moving the window takes a shown bar: the drag regions are on the bars, and the edge margin stays a hover target only.

Rejected:

- **Sliding or fading the bar in.** Both are ruled out above; an instant change is what the input mask can show faithfully.
- **Showing a floating bar on every keyboard shortcut**, such as Ctrl+Tab. A shortcut that changes the page shows its result in the page; the bar shows when focus or a dialog puts something in it.
- **A docked column with floating toolbar,** or the reverse, as separate settings. [First release](../first-release.md#appearance-is-data) has one Bars axis; splitting it is a later decision if a preset needs it.

### Popups

A popup's bar is always docked, whatever `barLayout` says, and `tabLayout` does not apply because a popup has no tabs. Its read-only address is what keeps a page from opening a window that passes for another site's sign-in ([Windows and popups](new-windows.md#windows)); a bar that hides would hide that.

### Split view

Unchanged. Floating bars sit over both panes; the divider and the focused-pane outline stay in the chrome between and around the panes, below a shown bar.

### Private windows

The **Private** marker is in the toolbar, so it floats with it and is hidden while the toolbar is. A hidden marker makes a private window look like an ordinary one, never the reverse, so no one is led to believe a window is private when it is not. The private new-tab page still says what the window keeps.

## Settings page

Appearance shows **Tabs** and **Bars** after Theme, each a group of icon buttons as Theme is ([Settings page § Layout](settings-page.md#layout)): `panel-top` and `panel-left` for the tabs, `app-window` and `panel-top-dashed` for the bars.

## Verification

Rust tests cover the defaults, the stored spelling, settings files from before the fields, and unknown values falling back without setting the file aside. Vitest covers the shell attributes, the vertical strip's drag targets and joined pairs, the reveal rules and holds, and the settings rows.

What only the running app shows, checked by hand: the edge margin taking the pointer over a native page, a shown bar taking clicks over the page and hiding when the pointer leaves for it, the page keeping its size as bars show and hide, and the layout following a change made in another window.
