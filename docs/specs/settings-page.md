---
status: IMPLEMENTED
last_updated: 2026-11-11
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
    Shortcuts
    About                         ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
```

The page has no borders or dividers: groups are soft plates, and spacing separates what is inside them. See [Code guidelines § Styling](../guidelines/code.md#styling).

- **Sidebar**, 13 rem wide, on the left: the search box, sunk into the page as a soft, borderless field, then one entry per section with an icon. The current section is a soft plate, as the active tab is. Below 48 rem of page width the sidebar collapses into a row of section buttons above the content, so a narrow split-view pane still works.
- **Content**, at most 42 rem wide: the section's title, then its groups. A group is one plate, `.card.soft.edgeless`, a step up from the page with no border, with an optional heading above it. Rows inside it are separated by spacing, not lines.
- **Row**: title and description on the left, control on the right, vertically centred. A control too wide for the right, such as the extensions list or a URL field, takes the row's full width under its title instead. A row that has no effect in the current state, as Loaded tabs has none while discarding is Always, is shown disabled with the reason as its description, the way `settings.disabledByDiscard` works today.
- **Controls** are borderless and lightly filled (`.soft.edgeless`): the plate already marks where they belong, and the focus ring still shows. A choice among a few options, as the theme is, is a segmented control: one track holding each option as its icon and its name, the chosen one raised to the page's colour. It is a native radio group underneath, so the arrow keys move between choices. Until 2026-11-11 each option was an icon alone, named by a tooltip. That did not weigh that only the theme's icons are obvious: `panel-left-dashed` against `panel-top-dashed` says nothing until hovered, and a setting read by hovering each choice in turn is slower than one read at a glance.

Pages and overlays set controls 2 rem tall, between the bars' compact density and the package's own 2.5 rem, as [`index.css`](../../src/styles/index.css) specifies. Until 2026-11-11 pages used the package's size, which did not weigh that the interface's text is 13 px: a 40 px control around it reads as a form on a web page, not as part of a desktop application. Secondary text is 12 px, one step under the text it annotates.

## Sections

The current settings, regrouped by what a person comes looking for:

| Section       | Address                      | Rows                                                                                                                                                                                                                                                                                                                                                                                                            |
| ------------- | ---------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Appearance    | `haku://settings/appearance` | Theme; tabs; sidebar side; toolbar placement; address bar ([Address bar](address-bar.md#settings-page)); bars ([Appearance](appearance.md)). Later: density, corners, surfaces, palette, presets, CSS.                                                                                                                                                                                                          |
| Tabs & memory | `haku://settings/tabs`       | Preset; loaded tabs; background memory; freeze; discard. **New:** kept sites, listed with a **Remove** button shown on the row's hover or focus.                                                                                                                                                                                                                                                                |
| Browsing      | `haku://settings/browsing`   | Four titled groups. Search: the list of search engines, which marks the default, and search suggestions ([Address bar](address-bar.md#settings-page)). New tabs: home page. Sites: site permissions ([Permissions](permissions.md)) and zoom levels ([Zoom](zoom.md)), each listed with a **Remove** button shown on the row's hover or focus. History: recording, retention and rules ([History](history.md)). |
| Extensions    | `haku://settings/extensions` | Add extension; each extension with its switch, version and actions; folders that failed; the extensions folder ([Extensions § Settings](extensions.md#settings)).                                                                                                                                                                                                                                               |
| Shortcuts     | `haku://settings/shortcuts`  | One row per action, grouped as Tabs, Windows, Page and Navigation, then Defaults, which holds Reset all ([Shortcuts](#shortcuts)).                                                                                                                                                                                                                                                                              |
| About         | `haku://settings/about`      | Version, the mark and wordmark, links to Memory (`haku://memory`) and History. **New**, read-only.                                                                                                                                                                                                                                                                                                              |

`haku://settings` shows Appearance. **Kept sites** are already stored in `Settings.keptSites` but can only be removed today by visiting each site in an ordinary window and opening its site menu; the list makes them reviewable. Private windows hide the site menu's kept-site control, and `set_settings` refuses a newly kept site from one, so a private visit cannot write a site-derived setting. Removing a kept site from settings works in any window. **About** needs the app version, which the interface reads with Tauri's `getVersion()` from `@tauri-apps/api/app`. The chrome's capability already grants it through `core:default`, so it needs no new command or permission.

## Navigation

Picking a section navigates the tab to its address with the existing `navigate_tab`, so the address bar shows where you are, Back returns to the previous section, and a section can be linked to, as the extensions menu's **Manage extensions** will link to `haku://settings/extensions`. A `#` names a row by its id, and the section opens scrolled to it, as the history page's **History settings** links to `haku://settings/browsing#historyEnabled`; the row is read when the address changes, so editing a row never scrolls back to it. [First release § Internal pages](../first-release.md#internal-pages-are-haku-urls) already routes anything after the host to the page, so the router needs no change.

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

Section icons are Lucide's `palette`, `gauge`, `globe`, `puzzle`, `keyboard` and `info`.

## Shortcuts

Each action is a short row, marked under the pointer: its name on the left, and at its right end its chords, each drawn as one keycap that spells its keys, as in "Ctrl + Shift + T". Before the chords come **Add**, a plus, and, once the action differs from its defaults, **Reset**, so the keys keep one edge down the page whether or not a row shows a button. A chord is its own remove button: under the pointer or keyboard focus it fades behind a cross. **Add** shows on the row's hover or focus, so about 35 rows read as actions and keys rather than as a wall of buttons; **Reset** stays in view, since it also says the action was changed, and so does **Add** while it records. Until 2026-11-12 a chord was one chip of text with a remove button after it and **Add** after the chords; that did not weigh that buttons hidden until hover still take their room, which left the keys floating at a different distance from the edge on every row. **Reset all** was the page's first plate, and is now its last, since it is the rarest thing done here.

A list inside a row, such as the search engines or the site permissions, sits in a well of the page's colour, and says there when it is empty. [Shortcuts § Changing bindings](shortcuts.md#changing-bindings) owns what is stored and which chords are refused.

- **Add** records: the button reads "Press a shortcut…" until a chord is pressed, plain Esc is pressed, or focus leaves it. A refused chord is shown with its reason and recording goes on. A chord another action uses is shown with that action's name, and **Use here** moves it, or **Cancel** drops it. A chord the action already has does nothing.
- Rows are built from one list in `settings/shortcuts.tsx`, an action and its title per entry, so the sidebar and search find them like any other row. TypeScript requires an entry for every `Shortcut` Rust exports, so a new action cannot be left out of settings.
- **Reset all shortcuts** asks in place first, since it has no undo: the button turns into "Reset N changed shortcuts?", counting the actions that show **Reset**, with **Cancel**, which takes focus, and **Reset all**. Esc answers no. Confirmed, it clears `Settings.shortcuts`.
