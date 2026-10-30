---
status: IMPLEMENTED
last_updated: 2026-10-30
scope: The layout, navigation, search and row pattern of `haku://settings`.
---

# Settings page

## Why this layout

Settings was one centred column of cards, each a heading over stacked form fields with their labels above them. It had four sections, and [First release § Appearance is data](../first-release.md#appearance-is-data) adds six appearance settings, presets and custom CSS. Stacked further, it becomes a long scroll with no way to jump or search, and every new setting has to invent its own layout. Chrome, Helium and Zen settle on the same answer: a section sidebar, a search box, and rows that put the label and its explanation on the left and the control on the right.

## Layout

```
  Settings
  ░ Search settings ░░░░░          Appearance

  ▓ Appearance ▓▓▓▓▓▓▓▓▓          ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
    Tabs & memory                 ░ Theme                   [▣][☼][☾]   ░
    Browsing                      ░ Light, dark, or the same as your     ░
    Extensions                    ░ system.                              ░
    About                         ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
```

The page has no borders or dividers: groups are soft plates, and spacing separates what is inside them. See [Code guidelines § Styling](../guidelines/code.md#styling).

- **Sidebar**, 13 rem wide, on the left: the search box, sunk into the page as a soft, borderless field, then one entry per section with an icon. The current section is a soft plate, as the active tab is. Below 48 rem of page width the sidebar collapses into a row of section buttons above the content, so a narrow split-view pane still works.
- **Content**, at most 42 rem wide: the section's title, then its groups. A group is one plate, `.card.soft.edgeless`, a step up from the page with no border, with an optional heading above it. Rows inside it are separated by spacing, not lines.
- **Row**: title and description on the left, control on the right, vertically centred. A control too wide for the right, such as the extensions list or a URL field, takes the row's full width under its title instead. A row that has no effect in the current state, as Loaded tabs has none while discarding is Always, is shown disabled with the reason as its description, the way `settings.disabledByDiscard` works today.
- **Controls** are borderless and lightly filled (`.soft.edgeless`): the plate already marks where they belong, and the focus ring still shows. A choice among a few options that each have an obvious icon, as the theme does, is a group of icon buttons, each named by a tooltip; it is a native radio group underneath, so the arrow keys move between choices.

The page keeps using the package's own sizing, not the compact bar density, as [`index.css`](../../src/styles/index.css) already specifies for internal pages.

## Sections

The current settings, regrouped by what a person comes looking for:

| Section       | Address                      | Rows                                                                                                       |
| ------------- | ---------------------------- | ---------------------------------------------------------------------------------------------------------- |
| Appearance    | `haku://settings/appearance` | Theme. Later: tabs, bars, density, corners, surfaces, palette, presets, custom CSS.                        |
| Tabs & memory | `haku://settings/tabs`       | Preset; loaded tabs; background memory; freeze; discard. **New:** kept sites, listed with a remove button. |
| Browsing      | `haku://settings/browsing`   | Search engine; home page.                                                                                  |
| Extensions    | `haku://settings/extensions` | The current list and folder button, unchanged in behaviour.                                                |
| About         | `haku://settings/about`      | Version, the mark and wordmark, links to Memory (`haku://memory`) and History. **New**, read-only.         |

`haku://settings` shows Appearance. **Kept sites** are already stored in `Settings.keptSites` but can only be removed today by visiting each site in an ordinary window and opening its site menu; the list makes them reviewable. Private windows hide the site menu's kept-site control so a private visit cannot write a site-derived setting. **About** needs the app version, which the interface reads with Tauri's `getVersion()` from `@tauri-apps/api/app`. The chrome's capability already grants it through `core:default`, so it needs no new command or permission.

## Navigation

Picking a section navigates the tab to its address with the existing `navigate_tab`, so the address field shows where you are, Back returns to the previous section, and a section can be linked to, as the extensions menu's **Manage extensions** will link to `haku://settings/extensions`. [First release § Internal pages](../first-release.md#internal-pages-are-haku-urls) already routes anything after the host to the page, so the router needs no change.

Rejected: keeping the section in component state. It is simpler, but Back would leave settings entirely and nothing could open a section directly.

## Search

The search box filters rows across every section by title, description and a list of extra keywords per row, such as "dark mode" for Theme and "RAM" for background memory. While it has text, the content shows the matching rows grouped under their section names instead of the current section; clearing it returns to the section. It matches the interface's own translated strings, so it works in any locale, and needs nothing from Rust.

Search is not part of the address. A query is a moment, not a place, and putting it in the address would add a history entry per keystroke.

## Code shape

- `settings/types.ts`: a row is data, `{ id, title, description, keywords, disabledReason, wide, labelsControl, Control }`, grouped into cards and sections. Titles, descriptions and keywords are translation keys, so search reads the same strings the page shows.
- `settings/sections.ts`: every section and row, which both the sidebar and search read, so they cannot disagree about what exists; with `sectionOf` and `searchSettings`. A new setting is a row here, never a new layout.
- `settings/rows.tsx`: `SettingsGroup` and `SettingRow`, which lay out a row and wire its title and description to the control for assistive technology.
- `settings/controls.tsx`: each row's control. The logic of the old sections moved here unchanged, including `DraftInput`'s save-on-commit rule.
- `InternalPage` has an optional `sidebar` slot; History and Memory keep the plain frame. Every internal page is rendered with its `url` and `tabId` rather than reading the active tab, since split view will show more than one.

A disabled row used to explain itself in a tooltip on hover. Its reason now replaces its description, so it is visible without hovering and read with the control.

Section icons are Lucide's `palette`, `gauge`, `globe`, `puzzle` and `info`.
