---
status: IMPLEMENTED
last_updated: 2026-10-31
scope: The layout settings of "Appearance is data" — where the tabs, the sidebar and the toolbar go, and whether the bars take space — and how the shell applies them.
---

# Appearance

[First release § Appearance is data](../first-release.md#appearance-is-data) makes appearance a set of settings that the shell maps to attributes and tokens. This spec covers its layout axes: **tabs** and **bars**, and for a tab column its **side**, the **toolbar**'s placement and its **width**. Density, corners, surfaces, palette, presets and custom CSS are not built.

## Settings

All are fields of the Rust `Settings`, stored in `settings.json` with the rest and read by the interface from the settings store.

| Field              | Values                      | Default      |
| ------------------ | --------------------------- | ------------ |
| `tabLayout`        | `horizontal`, `vertical`    | `horizontal` |
| `barLayout`        | `docked`, `floating`        | `docked`     |
| `sidebarSide`      | `left`, `right`             | `left`       |
| `toolbarPlacement` | `top`, `sidebar`            | `top`        |
| `sidebarWidth`     | CSS pixels, 224 through 480 | 240          |

The defaults are what Haku drew before the settings existed, so an existing settings file opens unchanged. The last three shape the tab column, and have no effect while the tabs are horizontal.

Each of the first four is an enum in Rust (`TabLayout`, `BarLayout`, `SidebarSide`, `ToolbarPlacement`) and a string union in the bindings. The width is a number that `sanitized` clamps to its bounds, as the webview capacity is. A stored value Haku does not know, such as a hand-edited `"Vertical"` or one written by a later version, reads as the default and leaves every other setting as stored. The next save writes that default over it, so the newer value is lost once anything in settings changes. `read_json` sets a whole file aside when it fails to deserialize, so a strict read would trade every setting for one unknown value. The leniency is written as the enums' own `Deserialize`, because a `deserialize_with` field attribute makes the binding generator split `Settings` into separate read and written types.

Changing either goes through `set_settings` like any setting, and `SettingsChanged` reaches every window, which re-renders in the new layout at once. No command or event was added.

Rejected:

- **Free strings, as `theme` is.** The bindings would accept any string, and every reader would need its own check. An enum gives the interface a closed union and Rust one place to fall back.
- **One combined layout setting** (`horizontal-docked`, …), or a `tabLayout` of `horizontal`, `left` and `right`. The axes are independent, and [First release](../first-release.md#appearance-is-data) lists them as separate settings so presets can bundle them; folding the side into `tabLayout` would also change the values of a setting already stored.
- **Rejecting an unknown value at load.** That is what a strict enum did, and it costs the user every other setting for a value that only changes the look.

## The shell

The shell's root carries `data-tabs`, `data-bars`, `data-side` and `data-toolbar`, beside `data-density`, and the column's width as the `--haku-sidebar-width` custom property. A pure function, `shellLayout` in `features/bars`, maps the settings, the window kind and whether the window is private to them; while settings are still loading it gives the defaults, and it gives `toolbar: top` whenever the tabs are horizontal. `src/styles/index.css` places the bars and the page from those attributes with a CSS grid, so mirroring the column is a stylesheet concern. The page is measured where it lands, as [Chrome layering](chrome-layering.md#the-contract-with-the-interface) requires; nothing computes its rectangle from the settings, so a layout change reaches Rust as one more resize of the viewport.

## Tabs

**Horizontal** is the strip across the top, beside the window controls, above the toolbar.

**Vertical** puts the tabs in a column down one side of the window, beside the page, as [Sidebar](#sidebar) describes. With the toolbar on top, the toolbar becomes the top row, and the window controls move to its right end, so the window keeps one bar across the top rather than two. The column lists the tabs top to bottom at full width, each as tall as a control, with the new-tab button after the last one, and scrolls when they do not fit. Its free space below them is a drag region for the window, as the strip's free width is.

Everything the strip does, the column does, because it is the same component on the other axis:

- Dragging a tab or a pair reorders it, with the drop line between units, never inside a pair. The drop position is read from the units' measured geometry along the column, and a pointer outside the column's width offers no drop.
- A split pair is one joined unit, stacked: the left member above the right, with one outer shape. Membership and position are announced as in the strip.
- Fixed tabs, the favicons a private window leaves out, the waiting-dialog badge, the discarded, frozen, crashed and unloaded markers, middle-click to close, and the tab and strip menus are unchanged.
- Each tab is a button in tab order, and the keyboard opens its menu as in the strip.

Dragging does not scroll the column. A tab can be dropped on any position the column shows; reaching a position scrolled out of view takes two drags.

## Spacing

The chrome is compact: it is used all day, and every pixel of height it spends is taken from the page. One 3 px gap is every space in it: between the window's edge and what is inside it, the page and a bar, a bar's edge and its contents, the groups inside a bar, and the items of a group, such as the tabs and the buttons of a row. Controls are 28 px tall in the bars and 32 px in the column, so with horizontal tabs the page starts 64 px below the window's top edge, less than a compact Chromium-based browser's two rows take.

Controls in a bar have a 6 px radius; containers, the page and a floating card, take that plus the gap, so a control nests concentric in a card's corner. A window control is the exception to the gap: it is flush with the window's corner, as the platform draws it. The column's resizing edge is 6 px wide, wider than the gap it sits on, so it stays a target a pointer can find.

In the strip, the new-tab button is one gap from the last tab, as every tab is from the next; in the column it is the list's last row, at the column's full width.

Rejected:

- **A larger gap**, 6 or 8 px, with or without a smaller step between items. Each cost 10 to 20 px of the page's height for chrome that groups as clearly by its controls' own shapes.
- **A fixed height for a bar's top row.** The space left above and below its controls came out unlike the gap everywhere else, and a hovered button met the column's edge.
- **A reveal strip wider than the gap** for floating bars. It would widen the page's margin on every side; at a maximized window's screen edge the pointer stops in the strip whatever its width.

## Sidebar

The tab column is the sidebar. Three settings shape it, and none applies while the tabs are horizontal: the settings page shows their rows disabled then.

**Side.** `sidebarSide` puts the column on the left or the right. Right mirrors left: the column, its resizing edge, the strip where a floating column is revealed, and the direction it slides in from all move to the right edge. The page keeps its gap on the side away from the column.

**Toolbar.** With `toolbarPlacement: top` the toolbar is the window's top row, as above. With `sidebar` it heads the column, and the page runs the window's full height:

- The first row holds back, forward and reload. Its free space is the window's drag region, as is the column's below the tabs. There are no window controls: with no top bar, the window has no row to hold them without spending the height this placement exists to give the page. Double-clicking a drag region maximizes or restores the window, dragging it moves and snaps it, Win+Down minimizes and Alt+F4 or the taskbar closes. Every layout with a top bar keeps the controls at its end.
- The address field is under it at the column's full width. At rest it shows the address as the top toolbar does. While it is edited it grows over the page, toward it, to `max(100%, min(40rem, 60vw))`, so a long address has room however narrow the column is; grown, it registers as an overlay ([Chrome layering](chrome-layering.md#registering-an-overlay)).
- The tabs follow, then a foot row holding what the toolbar's right end holds: the Private marker or the extensions, downloads, developer tools and settings.

The column is then one bar holding both tabs and toolbar, so a hold on either, or a dialog on any tab, shows it while it floats.

**Width.** `sidebarWidth` sets the column's width, docked or floating. Its inner edge is a handle: dragging it resizes the column, writing the width to the shell's style each frame so the page follows without a render or a settings write per frame, and releasing stores it once. A double click restores 240. The handle is a focusable separator; the arrow keys move it 16 pixels, Home and End to its bounds. A line shows on it once the pointer has rested on it 150 ms. The bounds are 224, below which a tab's title is cut to a few characters, and 480, past which the column takes more of a common window than the page needs to give.

Reversed: the toolbar in the column was rejected because a 15 rem address field is too narrow for most addresses and the extension buttons would wrap. That weighed a fixed width and the address field at rest. It did not weigh a field that grows over the page while edited, a column the user can widen, or moving the extensions to a row of their own, which together answer both reasons.

Rejected:

- **The window controls at the head of the column**, beside navigation or in a row of their own. Beside navigation they crowd a narrow column's first row; in a row of their own they spend height on three buttons whose jobs the platform already does, which is what someone choosing a layout without a top bar is getting away from.
- **A width stored per window.** Windows share settings; a width that differs between them would be a session setting, which nothing else in appearance is.

## Bars

**Docked** bars take space and the page fills what they leave, as before.

**Floating** bars sit over the page, which takes the whole window but for the frame's gap on every side. That margin is chrome, inside the chrome's region, so it sees the pointer arrive at the window's edge, as [First release § What the chrome can draw over a page](../first-release.md#what-the-chrome-can-draw-over-a-page) requires. The margin was already there between the page and the window's edge, so the page's own rectangle loses nothing to it.

A floating bar is shown while any of these holds, and hidden otherwise:

- **Pointer.** It has rested 150 ms on the bar's edge of the window, or is over the shown bar. A pointer that leaves the edge sooner shows nothing, so one passing over the edge on its way to a page's own controls there does not pull a bar over them. A pointer reaching a bar already shown for another reason takes it at once. Leaving starts a 300 ms delay before the bar hides, so a pointer crossing back does not flicker it.
- **Focus.** Anything in the bar has focus: the address field focused by Ctrl+L or Ctrl+T, or the keyboard tabbing into it. A hidden bar stays in the tab order for that reason; it is clipped, not removed, so focusing it is what shows it.
- **Something opened from it.** A tab or strip menu, the site menu or the extensions menu holds its bar until it closes. An extension's toolbar popup is a window of its own that Rust does not report closing, so it holds the bar until the chrome has lost focus to it and then sees the pointer or focus again, or the user clicks in the chrome. A click that closes the open popup opens nothing, so it holds nothing.
- **Something in it needs attention.** A pending dialog on the focused tab shows the toolbar, which names the page asking. A pending dialog on any other tab shows the tabs, where its **Dialog waiting** badge is ([Split view § Page-anchored surfaces](split-view.md#page-anchored-surfaces)).

A shown bar is a card: opaque, on the `chrome` surface, inset from the window's edges by the frame's gap, rounded with the radius the page shares, edged all round with 1 px, and with no shadow, which the page would cover. Over an unknown page a border is what separates it ([Code guidelines § Styling](../guidelines/code.md#styling)). Its window controls take its corner's radius, so their hover does not square it off.

Hidden, the card waits just past its edge of the window, where the shell clips it; it is moved, not removed, so it stays in the tab order. Shown, it slides in from that edge in 180 ms, easing out, and leaves the same way in 120 ms, easing in. It registers itself with `useOverlay` while shown and while it leaves, and `useOverlay` measures it every frame while it moves ([Chrome layering § Registering an overlay](chrome-layering.md#registering-an-overlay)), so the region the page is cut from follows it. Under reduced motion it appears and leaves at once. It never fades: anything drawn over the page is opaque.

With horizontal tabs, the tab row and the toolbar float together from the top edge. With vertical tabs, the column floats too, as one more bar: the toolbar from the top edge, the column from its side's edge, each shown on its own. The column runs the window's full height, as it does with the toolbar in it, so it keeps one shape whichever bar shows; where the two cards overlap, the one shown last is on top, since it is the one the pointer or the keyboard just reached for. With the toolbar in the column, the column is the only bar, full height, from its side's edge. The same setting governs every bar, since a docked column beside floating toolbar would keep most of the space floating was chosen to give the page.

Moving the window takes a shown bar: the drag regions are on the bars, and the edge margin stays a hover target only.

Reversed: bars appeared and left without animation, because a moving overlay was measured once it settled and was hidden under the page until then. That weighed measuring at the end of an animation only. It did not weigh measuring every frame while the bar moves, which the per-frame coalescing of layout reports already allows: the region trails the card by at most the frames a report is in flight. Whether that trail shows on Windows is checked by hand (see [Verification](#verification)); if it does, the fallback is the card appearing at once with only its contents moving.

Rejected:

- **Fading the bar in.** A translucent bar over the page cannot be drawn ([Chrome layering § Why not transparency](chrome-layering.md#why-not-transparency)).
- **Showing a bar as soon as the pointer reaches the edge.** Pages put their own controls at the window's edges too, and a bar pulled over them by a pointer on its way there hides what the pointer was going for.
- **Showing a floating bar on every keyboard shortcut**, such as Ctrl+Tab. A shortcut that changes the page shows its result in the page; the bar shows when focus or a dialog puts something in it.
- **A docked column with floating toolbar,** or the reverse, as separate settings. [First release](../first-release.md#appearance-is-data) has one Bars axis; splitting it is a later decision if a preset needs it.

### Popups

A popup's bar is always docked, whatever `barLayout` says, and `tabLayout` does not apply because a popup has no tabs. Its read-only address is what keeps a page from opening a window that passes for another site's sign-in ([Windows and popups](new-windows.md#windows)); a bar that hides would hide that.

### Split view

Unchanged. Floating bars sit over both panes; the divider and the focused-pane outline stay in the chrome between and around the panes, below a shown bar.

### Private windows

A private window's bars are always docked, whatever `barLayout` says, as a popup's are. It still follows `tabLayout`, so vertical tabs work in it. The **Private** marker is in the toolbar, and it must never hide: a floating toolbar would hide it with the bar, and a user who forgets a window is private expects what they do there to be kept, or a sign-in there to last. `shellLayout` applies the rule from the window's `private` flag in its browser state, so it needs no command or event.

Rejected: letting the marker float with the toolbar. A hidden marker makes a private window look like an ordinary one, and the private new-tab page that says what the window keeps is gone once the user navigates.

## Settings page

Appearance shows **Tabs**, **Sidebar**, **Toolbar** and **Bars** after Theme, each a group of icon buttons as Theme is ([Settings page § Layout](settings-page.md#layout)): `panel-top` and `panel-left` for the tabs, `panel-left` and `panel-right` for the sidebar's side, `panel-top` and `panel-left-dashed` for the toolbar, `app-window` and `panel-top-dashed` for the bars. Sidebar and Toolbar are disabled, still showing what is stored, while the tabs are horizontal. The width has no row: it is set where it shows, by dragging the column's edge.

## Verification

Rust tests cover the defaults, the stored spelling, settings files from before the fields, unknown values falling back without setting the file aside, and the width's clamp. Vitest covers the shell attributes, the vertical strip's drag targets and joined pairs, the reveal rules, its delay and holds, the bar shown last stacking on top, an overlay kept registered while it animates away, the resizing edge, and the settings rows.

What only the running app shows, not yet checked by hand: the edge margin taking the pointer over a native page, a shown bar taking clicks over the page and hiding when the pointer leaves for it, the region following a sliding card without a visible trail, the page keeping its size as bars show and hide, the grown address field taking clicks over the page, and the layout following a change made in another window.
