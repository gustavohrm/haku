---
status: APPROVED
last_updated: 2026-10-23
scope: The layout, navigation, search and row pattern of `haku://settings`.
---

# Settings page

Approved on 2026-10-23. Nothing here is built yet; `src/features/settings/` is still the single column of cards it replaces.

## Problem

Settings is one centred column of cards, each a heading over stacked form fields with their labels above them ([`settings-page.tsx`](../../src/features/settings/settings-page.tsx)). It has four sections today and [First release § Appearance is data](../first-release.md#appearance-is-data) adds six appearance settings, presets and custom CSS. Stacked further, it becomes a long scroll with no way to jump or search, and every new setting has to invent its own layout. Chrome, Helium and Zen settle on the same answer: a section sidebar, a search box, and rows that put the label and its explanation on the left and the control on the right.

## Layout

```
┌──────────────────────────────────────────────────────────────────┐
│  [ Search settings          ]                                    │
│                                                                  │
│  ◐ Appearance        Appearance                                  │
│  ⚡ Tabs & memory     ┌──────────────────────────────────────────┐│
│  ◎ Browsing          │ Theme                     [Match system ▾]││
│  ⧉ Extensions        │ Light, dark, or follow Windows.          ││
│  ⓘ About             ├──────────────────────────────────────────┤│
│                      │ …                                        ││
│                      └──────────────────────────────────────────┘│
└──────────────────────────────────────────────────────────────────┘
```

- **Sidebar**, 13 rem wide, on the left: the search box, then one entry per section with an icon. The current section is the raised surface (`chrome-raised`), as the active tab is. Below 48 rem of page width the sidebar collapses into a row of section buttons above the content, so a narrow split-view pane still works.
- **Content**, at most 42 rem wide: the section's title, then its groups. A group is one card (`@codenhub/styles` `.card`) with an optional heading above it, holding rows separated by dividers.
- **Row**: title and description on the left, control on the right, vertically centred. A control too wide for the right, such as the extensions list or a URL field, takes the row's full width under its title instead. A row that has no effect in the current state, as Loaded tabs has none while discarding is Always, is shown disabled with the reason as its description, the way `settings.disabledByDiscard` works today.

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

`haku://settings` shows Appearance. **Kept sites** are already stored in `Settings.keptSites` but can only be removed today by visiting each site and opening its site menu; the list makes them reviewable. **About** needs the app version, which the interface reads with Tauri's `getVersion()` from `@tauri-apps/api/app`. The chrome's capability already grants it through `core:default`, so it needs no new command or permission.

## Navigation

Picking a section navigates the tab to its address with the existing `navigate_tab`, so the address field shows where you are, Back returns to the previous section, and a section can be linked to, as the extensions menu's **Manage extensions** will link to `haku://settings/extensions`. [First release § Internal pages](../first-release.md#internal-pages-are-haku-urls) already routes anything after the host to the page, so the router needs no change.

Rejected: keeping the section in component state. It is simpler, but Back would leave settings entirely and nothing could open a section directly.

## Search

The search box filters rows across every section by title, description and a list of extra keywords per row, such as "dark mode" for Theme and "RAM" for background memory. While it has text, the content shows the matching rows grouped under their section names instead of the current section; clearing it returns to the section. It matches the interface's own translated strings, so it works in any locale, and needs nothing from Rust.

Search is not part of the address. A query is a moment, not a place, and putting it in the address would add a history entry per keystroke.

## Code shape

- `settings/rows.tsx`: `SettingsGroup` and `SettingRow` (`title`, `description`, `keywords`, `control`, `wide`, `disabledReason`). Every setting uses them, so a new setting is a row, never a new layout.
- `settings/sections.ts`: the sections as data, `{ id, icon, title, rows }`, which both the sidebar and search read, so they cannot disagree about what exists.
- `InternalPage` gains an optional `sidebar` slot; History and Memory keep the plain frame.
- The existing `OptimizationSection` and `ExtensionsSection` become row lists; their logic, including `DraftInput`'s save-on-commit rule, does not change.

## Open questions

- **Icons per section.** Proposed from Lucide: `palette`, `gauge`, `globe`, `puzzle`, `info`.
- **About in scope?** It is new, not a regrouping of what exists, and can ship in a later change without affecting the rest.
