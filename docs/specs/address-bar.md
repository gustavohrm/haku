---
status: APPROVED
last_updated: 2026-11-02
scope: The address bar's two layouts, inline and floating; its suggestions from history, bookmarks, open tabs and the search engine; search engines, their presets, custom engines and keywords.
---

# Address bar

Approved by the maintainer on 2026-11-02, with every recommendation under [Approved decisions](#approved-decisions). It adds persisted settings, replaces one (`searchUrl`), adds IPC commands and a dependency, and reverses part of [Appearance § Sidebar](appearance.md#sidebar), so it is a contract decision under [Agent instructions § Judgment](../../AGENTS.md#judgment).

## Problem

The address field is a plain input. Three things are missing:

- **Suggestions.** Nothing is offered while typing, so reaching a page visited a hundred times means typing its address in full. [History](history.md) has recorded what ranking needs (`typedCount`, `visitCount`, `lastVisitedAt`) since it landed, and left suggestions to this change.
- **A layout for the sidebar.** With the toolbar in the column, the field grows over the page while edited ([Appearance § Sidebar](appearance.md#sidebar)). That gives a long address room, but no place for a list of suggestions with titles.
- **Choosing a search engine.** The only setting is a free-text `searchUrl`. Someone who wants Google has to know to type `https://www.google.com/search?q=%s`, and typing `google.com` there yields a home page, not a search.

## Direction

Confirmed by the maintainer on 2026-11-02:

1. The floating layout is Zen's: focusing the address opens a wide card centred horizontally over the window, in its top third, with the input on top and the suggestions inside the same card below it. Escape or a click outside closes it. The page behind is not dimmed; the chrome cannot draw translucency ([Chrome layering § Why not transparency](chrome-layering.md#why-not-transparency)).
2. With the toolbar in the sidebar, the floating layout is enforced. With the toolbar on top it is optional.
3. Suggestions come from history, bookmarks and open tabs, and, as an option, from the search engine.
4. Search engines ship as presets, custom engines can be added, and each engine has a keyword that searches it once.
5. DuckDuckGo stays the default.

## Approved decisions

Approved on 2026-11-02 as recommended.

| #   | Decision                           | Approved                                                                                                                                                                                                                                                       |
| --- | ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A   | The layout setting                 | `addressBarLayout: inline \| floating`, default `inline`, so an existing settings file opens unchanged. Ignored, and its row disabled, while the toolbar is in the sidebar.                                                                                    |
| B   | Where presets live                 | In code, not in settings. A user can change a preset's keyword or hide it, not its URL; a later version can then fix a preset's URL for everyone. Custom engines are stored in full.                                                                           |
| C   | Keyword syntax                     | Presets get `@` keywords (`@g`, `@d`, …); a custom engine's keyword is whatever the user types. A bare `g cats` stays a search for "g cats". `!` is left for [bangs](../first-release.md#features), which stay a separate, later feature.                      |
| D   | `searchUrl` migration              | Read once at load: a stored URL equal to a preset's selects that preset; any other becomes a custom engine named after its host and is selected. `searchUrl` is then no longer written.                                                                        |
| E   | Engine suggestions                 | Off by default (`searchSuggestions: false`), never in a private window, never for input that resolves to an address, and fetched by Rust without cookies.                                                                                                      |
| F   | HTTP client for engine suggestions | `ureq` with `rustls`: small, blocking (run off the main thread), no cookie store. The chrome cannot fetch them itself: its CSP allows only `'self'` and `ipc:`, and a page-side fetch would carry the engine's cookies.                                        |
| G   | Private windows                    | History and bookmark suggestions are shown (reading history records nothing), open tabs from private windows only, no engine suggestions. Private tabs are never offered in an ordinary window.                                                                |
| H   | Inline completion                  | Typing the start of a host the user has typed before completes it in the field, with the completed part selected, so `you` + Enter goes to `youtube.com`. Only hosts with `typedCount > 0`, never a path, so a link once clicked does not become a completion. |
| I   | Open tabs in other windows         | Offered; choosing one focuses that window and selects the tab. `select_tab` works only in the caller's window, so this needs a new command, `switch_to_tab`.                                                                                                   |

## Behaviour

### Layouts

**Inline** is today's field in the top toolbar, with the suggestions in a list directly below it at the field's width, registered as an overlay ([Chrome layering § Registering an overlay](chrome-layering.md#registering-an-overlay)).

**Floating** keeps the field where it is at rest, showing the address as now. Focusing it, by click, Ctrl+L, Alt+D, F6 or Ctrl+T, opens the card:

- Centred horizontally on the window, its top edge at 20% of the window's height, `min(48rem, 100vw − 2 × 3rem)` wide. It covers the page and, where they overlap, the bars; centring on the page instead would put it off-centre in the window whenever a column is docked.
- An opaque card on `chrome-raised`, edged 1 px, radius as a floating bar's card, no shadow, as [Appearance § Bars](appearance.md#bars) draws one.
- The input on top, then the suggestions. The site menu, zoom and star stay on the field at rest, not in the card: they act on the page, and the card is for going somewhere else.
- Opens and closes at once, with no motion: it is a field taking focus, not a bar arriving.
- Behind it, the window is blurred 4 px and darkened 10%, so the card has the focus. The chrome cannot draw over a page, only in its place, so each shown live page is captured (`capture_page`) and the backdrop goes up over the captures, covering the window and registered as an overlay. It comes in over the enter duration rather than at once, so the blur does not snap, and at once with reduced motion. It goes up only once every shown live page has a capture, within 250 ms each: a page left out would show as a plain surface, and a backdrop arriving later would land while the user is typing. Without it the card shows over the live page. Pressing it closes the card, and the page behind does not take the press. While it is up, the page is a still image: a playing video looks paused until the card closes.
- It holds no floating bar, unlike a menu opened from one ([Appearance § Bars](appearance.md#bars)): a menu hangs from its bar, the card from none, and the address the bar shows is not what is being typed. A floating bar hides once the pointer and focus have left it, as it otherwise does.

The card closes on Escape, on a click outside it, on submitting and on the window losing focus. Closing discards what was typed, as blurring the field does today.

**In the sidebar** the floating layout replaces the field that grows over the page. Reversed: that field was chosen to give a long address room in a narrow column. It did not weigh suggestions, which need room for a title and an address per row; a list hanging from the column's edge would sit against one side of the window and be as narrow as the column.

### Suggestions

The list is one column of rows, at most 10, refreshed on every keystroke. Each row has an icon, a title and, beneath or beside it, the address or what choosing it does.

1. **What Enter does**, always first and selected: "Search DuckDuckGo for …", "Go to …", or the completed host ([H](#approved-decisions)). With a keyword, it names the keyword's engine.
2. **Open tabs**, up to 3, as "Switch to tab".
3. **History and bookmarks**, together, deduplicated by URL, up to 6. Bookmarks carry the star.
4. **Engine suggestions**, up to 4, when on.

Typing `@` alone lists the engines and their keywords.

**Matching.** The input is split on whitespace; a page matches when every word appears, case-insensitively, in its URL or title. A word matching the start of the host or of a title word ranks above one matching inside.

**Ranking** history pages, after the match boost: typed visits count more than other visits, and recent visits more than old ones, as Chrome and Firefox rank. A bookmark ranks as a page with one typed visit, so a bookmarked page never visited still appears. The weights are tuned in code and pinned by tests, not fixed here.

**Keyboard.** Up and Down move the selection and put its address, or its text for a search, in the input; Escape first restores what was typed, then closes. Enter opens the selection. Tab moves focus out, as today.

**Transitions** ([History § Transitions](history.md#transitions)): choosing a history or bookmark row, or the completion, is `typed`, as typing it out would be; an engine suggestion is `search`; switching to a tab is no navigation.

### Search engines

**Presets**, each with a search URL, a suggestion URL where one is public, and a keyword:

| Engine     | Keyword | Engine suggestions               |
| ---------- | ------- | -------------------------------- |
| DuckDuckGo | `@d`    | Yes                              |
| Google     | `@g`    | Yes                              |
| Bing       | `@b`    | Yes                              |
| Brave      | `@br`   | Yes                              |
| Startpage  | `@s`    | Yes                              |
| Ecosia     | `@e`    | Yes                              |
| Kagi       | `@k`    | No, it needs a signed-in session |

Each URL is checked by hand during implementation; an engine whose suggestion URL does not answer without cookies ships without one.

**Custom engines** have a name, a search URL (`http` or `https`, with `%s` where the query goes, or the query appended as today) and an optional keyword. Their suggestion URL is optional too.

A keyword holds no whitespace and is unique among engines, ignoring case. `@g cats` searches Google for "cats" once. A keyword alone, with nothing after it, is an ordinary search for that word.

The default engine is used for searches typed without a keyword, for the context menu's search, which then names it ("Search Google for …", replacing the unnamed label in [Context menus](context-menus.md)), and later the launcher.

### Settings page

- **Appearance → Address bar**, after Toolbar: icon buttons for inline and floating. Disabled while the toolbar is in the sidebar, still showing what is stored, as Sidebar and Toolbar are while the tabs are horizontal; its description says the sidebar always floats it.
- **Browsing → Search engine** replaces the search URL field: a select of the visible engines.
- **Browsing → Search engines**: one row per engine with its name, editable keyword, and hide (presets) or remove (custom), then **Add search engine** opening a form of name, URL and keyword. The default cannot be hidden or removed.
- **Browsing → Search suggestions**: the switch for [E](#approved-decisions), with a description saying what is sent and to whom.

## Contract

### Settings

| Field                 | Type                                         | Default        |
| --------------------- | -------------------------------------------- | -------------- |
| `addressBarLayout`    | `inline`, `floating`                         | `inline`       |
| `searchEngine`        | engine id                                    | `"duckduckgo"` |
| `customSearchEngines` | `{ id, name, url, suggestUrl?, keyword? }[]` | `[]`           |
| `presetKeywords`      | map from preset id to keyword                | `{}`           |
| `hiddenSearchEngines` | preset ids                                   | `[]`           |
| `searchSuggestions`   | boolean                                      | `false`        |

`searchUrl` is read for [migration](#approved-decisions) only. `addressBarLayout` is read leniently, as the appearance enums are. An unknown `searchEngine` reads as DuckDuckGo. `sanitized` drops custom engines with an invalid URL and keywords that clash, keeping the first.

### Commands

| Command              | Does                                                                                                                                                                    |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `suggest`            | Takes the input; returns the first row and the open-tab, history and bookmark rows, ranked. Local only, so it answers fast.                                             |
| `search_suggestions` | Takes the input; returns the engine's completions, or none when off, private, address-like or failed. Times out at 1 s.                                                 |
| `switch_to_tab`      | Focuses the window holding a tab and selects it.                                                                                                                        |
| `search_engines`     | Returns the presets with the user's keywords applied, so the interface does not duplicate the list.                                                                     |
| `capture_page`       | Takes a tab in the caller's window; returns what its page shows as a JPEG `data:` URL, or none without a live page, on failure or past 250 ms. For the card's backdrop. |

The interface asks `search_suggestions` 150 ms after typing stops and drops answers to input it no longer shows. `resolve_input` gains the engines, so `navigate_tab` resolves keywords exactly as the first row describes.

## Slices

Each lands on `feat/address-bar` as its own commits, verified alone:

1. **Engines**: settings, migration, presets, keywords in `resolve_input`, the settings rows, the context-menu label.
2. **Local suggestions**: `suggest`, `switch_to_tab`, the list under the inline field, keyboard, completion.
3. **Floating layout**: the setting, the card, the sidebar enforcing it, the grown field removed.
4. **Engine suggestions**: the dependency, the setting, `search_suggestions`.

## Rejected alternatives

- **No backdrop**, because the chrome cannot draw over the page translucently. Reversed: that weighed drawing over the live page. It did not weigh drawing the page's capture in its place, which `CapturePreview` already takes as a tab is left, so the chrome blurs an image of the page rather than the page.
- **Fetching engine suggestions from the interface.** The CSP forbids it, and loosening it would let the chrome reach any site; a request from the chrome would also carry the engine's cookies, tying each keystroke to an account.
- **Presets stored in settings in full.** A preset's URL could never be corrected for existing users.
- **Bare keywords for presets**, such as `g`. They would capture real searches that start with a letter or short word.
- **Keeping the grown field in the sidebar and hanging the list under it.** See [Layouts](#layouts).
- **Suggestions by querying history with `query_history`.** It pages visits newest first through the full filter; ranking needs pages by counts, per keystroke.

## Testing

Per [Tests](tests.md): Rust unit tests for migration, keyword resolution, sanitizing, matching and ranking order; interface tests for the list's keyboard handling, stale answers being dropped, the floating card opening and closing, its backdrop going up only once every shown page is captured, and the layout rule in `shellLayout`. Engine suggestion URLs are checked by hand once, recorded in the pull request.
