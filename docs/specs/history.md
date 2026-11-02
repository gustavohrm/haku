---
status: APPROVED
last_updated: 2026-11-02
scope: What browsing history records, the rules and switches that stop it, how it is searched, filtered and deleted, and its storage.
---

# History

Approved by the maintainer on 2026-11-02, with every recommendation of the design discussion, listed under [Approved direction](#approved-direction). It adds IPC commands, a parameter to two commands, a field to `BrowserState`, persisted settings and a history schema migration, so it is a contract decision under [Agent instructions § Judgment](../../AGENTS.md#judgment). It implements the History row of [First release § Features](../first-release.md#features), and extends it with regular expressions, rule actions other than "never", pausing and retention.

## Problem

Haku has recorded visits since the first release, but `haku://history` shows only the newest 200 with a single Clear button. Most browsers stop at a searchable access log. People want three things that log does not give them:

- **Finding something again** by more than its title: when it was visited, on which site, how it was reached.
- **Control over what is kept.** Turning recording off and on easily, and saying once that a site, or any URL matching a pattern, is never recorded.
- **Deleting precisely.** Removing exactly what a search shows, not everything or one row at a time.

History also feeds the address bar's suggestions, which come later. The data they need — how a page was reached, how often it was typed — cannot be recovered afterwards, so it is recorded from this change on.

## Approved direction

Approved on 2026-11-02:

1. Address-bar suggestions are out of scope; the data they need is recorded now.
2. Each visit records how it was reached (its [transition](#transitions)).
3. Recording has a permanent switch and a pause that lasts until Haku restarts.
4. That recording is off is shown on the history page and in the site menu, not in the toolbar.
5. Rules can say never record, record the address without the title, or always record as an exception. There is no per-rule age limit.
6. History is kept forever by default; a setting deletes it after 30 days, 90 days or 1 year.
7. The list collapses repeat visits to one page within a day into one row with a count, which expands.
8. Regular-expression rules use the `regex` crate, a new dependency.
9. Finished includes Ctrl+H, **Show history for this site** in the site menu, and a sites view. Export, import from other browsers, and forgetting a site everywhere are deferred.
10. Page contents are not indexed; only titles and addresses are searched.
11. This spec comes first, then the code on `feat/history`, in the [slices](#slices) below.

## Behaviour

### What is recorded

A visit is what [Page observation § What counts as a visit](page-observation.md#what-counts-as-a-visit) already defines. That stays unchanged. A visit is recorded when all of these hold:

1. The tab is not private. Private reports never reach the store, as today.
2. Its URL is `http`, `https` or `file`. Internal pages never reached the store; `data:`, `blob:`, `about:` and `chrome-extension:` pages no longer do either. A `data:` URL can be megabytes, and an extension's own page is not somewhere a person browses to.
3. Recording is on and not paused.
4. No rule says otherwise ([Rules](#rules)).

A non-visit report still retitles the latest visit to its URL, under the same conditions, so turning recording off also stops titles from changing.

Each visit stores its URL, its host, its title, its time and its transition. The favicon is not stored: it is derived from the URL, as the tab strip and bookmarks already do.

### Transitions

How the visit was reached:

| Transition    | When                                                                                                                       |
| ------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `typed`       | The address field, when the input resolved to an address.                                                                  |
| `search`      | The address field, when the input resolved to a search; or a context menu's search for the selected text.                  |
| `bookmark`    | A bookmark, from the bar, the star's menu, the sidebar or `haku://bookmarks`.                                              |
| `link`        | A link or pushed route in the page, a link opened in a new tab or window, or a page's popup.                               |
| `backForward` | Back or forward, from a button, a shortcut, a mouse button or the page.                                                    |
| `other`       | Anything else: the home page, reopening a closed tab, a link on an internal page, and every visit from before this change. |

`typedCount` on a page counts its `typed` visits, the signal the address bar will rank by, as Chrome and Firefox do.

### Recording on, off and paused

- **The switch**, `Settings.historyEnabled`, on by default. Turning it off stops recording until it is turned back on, across restarts. Existing history stays.
- **Pause**, until Haku restarts or recording is resumed. It is not a setting: `BrowserState.historyPaused`, so every window shows it at once and nothing persists it.

Both are on the history page, at the top: the switch, and **Pause until restart** beside it. The switch is also a row in **Settings → Browsing → History**. While recording is off or paused, the history page says so in a banner with the way to resume, and the site menu says **History is off** or **History is paused**.

### Rules

`Settings.historyRules` is an ordered list. Each rule is a [pattern](#patterns) and an action:

| Action        | Effect on a matching visit                                                                                  |
| ------------- | ----------------------------------------------------------------------------------------------------------- |
| `never`       | Not recorded, and no retitle reaches an earlier visit to it.                                                |
| `addressOnly` | Recorded with an empty title, and never retitled. The list shows its address. For mail, chat and documents. |
| `always`      | Recorded, whatever later rules say. The exception to a broader rule.                                        |

The **first matching rule decides**; a visit no rule matches is recorded. A new rule goes to the top, from the site menu, a history row or settings, so the newest decision wins. Rules are reordered with move up and move down in settings.

Rules apply to visits recorded from then on. Adding a `never` rule shows how many existing visits match it, and offers to delete them in the same step: **Never record and delete 42 visits**, or **Only from now on**. Nothing is deleted without that choice. Turning recording off or pausing it overrides `always`: the switch is the user's broadest statement.

Rules are managed in **Settings → Browsing → History**: add (pattern kind, pattern, action), reorder, remove. An invalid pattern is refused when it is saved, with the reason under the field.

From a private window, the site menu's rule action is hidden and `set_settings` refuses a rule that was not there before, as it refuses a newly kept site ([Settings page § Sections](settings-page.md#sections)): writing the host to disk would record the private visit the rule was meant to hide. Removing rules works anywhere.

### Patterns

One pattern type, `UrlPattern`, serves rules, filters and deletion, and one Rust matcher evaluates it everywhere, inside SQLite as a registered function, so what a rule stops, what a filter shows and what a delete removes cannot disagree.

| Kind       | Matches                                                                                                                         | Example                           |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------- | --------------------------------- |
| `page`     | That exact URL.                                                                                                                 | `https://example.com/a?b=1`       |
| `site`     | A host and every subdomain of it. Entered as a host or any URL, of which only the host is kept; compared lowercase.             | `example.com`, `mail.example.com` |
| `wildcard` | The whole URL, where `*` stands for any run of characters, possibly none. Case-insensitive. No other character is special.      | `*://*.bank.com/*`, `*token=*`    |
| `regex`    | Anywhere in the URL, as the `regex` crate's `is_match`; anchor with `^` and `$`. Case-sensitive unless the pattern says `(?i)`. | `^https://[^/]+/admin(/\|$)`      |

Limits, so a pattern cannot make every visit slow: at most 2,000 characters, and a regular expression whose compiled form exceeds the crate's default size limit is refused. The `regex` crate matches in time linear in the URL, so no pattern can hang recording.

### Retention

`Settings.historyRetention`: `forever` (default), `days30`, `days90` or `year`. Visits older than that are deleted at startup and then hourly by the recording thread. Shortening it deletes what falls outside at once, after a confirmation that says how many visits go.

### The history page

`haku://history`, drawn by the chrome as today.

- **Header:** the recording switch and pause, and **Clear browsing history…**. Below it, the search field with the filters, then a view toggle, **Visits** or **Sites**, beside the delete actions. While recording is off or paused, a notice above the filters says so and offers to turn it on or resume.
- **Filters**, beside the search field:
  - **Text**, matched against title and address.
  - **When:** today, yesterday, last 7 days, last 30 days, or a custom range.
  - **Sites**, to include or exclude.
  - **Pattern**, a wildcard or regular expression matched against the address.
  - **Reached by**, one or more transitions.

  Active filters show as removable chips. **Delete all matching…** acts on exactly the current filter, after a confirmation that says how many visits it removes.

- **Visits view:** grouped under day headings, newest first; repeat visits to a page within a day are one row with a count, which expands to each time. More loads as the list scrolls.
- **Sites view:** one row per host with its visit count and last visit, under the same filter. A row opens the visits view filtered to it.
- **Rows:** activate opens the page in this tab; middle click or Ctrl+click opens a new tab. Checkboxes select rows; with a selection, the header offers **Delete selected**. A row's menu offers Open in new tab, Copy link, **More from this site**, Delete, **Delete all visits to this page**, **Delete everything from this site…** and **Never record this site…**.
- **Clear browsing history…** offers the last hour, last day, last 7 days, last 4 weeks or all time. It deletes visits only; downloads stay, as [Downloads](downloads.md#storage) already says.

The filter lives in the page, not in the address: a query is a moment, not a place, and the address would gain a back entry per keystroke, as [Settings page § Search](settings-page.md#search) decided. An address the page is opened with is read once: `haku://history?q=<text>` and `haku://history?site=<host>`.

### Elsewhere

- **Ctrl+H** opens `haku://history` in a new tab: a new shortcut action, `openHistory`, in the same group as `openBookmarks`.
- **The site menu**, for a web page in an ordinary window, gains **Show history for this site**, which opens `haku://history?site=<host>`, and **Never record this site…**, which adds a `site` rule at the top and makes the same offer to delete.
- **About**, in settings, already links to history and is unchanged.

## Contract

### Types

```ts
type UrlPatternKind = "page" | "site" | "wildcard" | "regex";
type UrlPattern = { kind: UrlPatternKind; value: string };

type HistoryRuleAction = "never" | "addressOnly" | "always";
type HistoryRule = { pattern: UrlPattern; action: HistoryRuleAction };

type Transition = "typed" | "search" | "bookmark" | "link" | "backForward" | "other";
type NavigationSource = "typed" | "bookmark" | "link" | "other"; // `search` is decided by Rust

type HistoryFilter = {
  text: string | null;
  from: number | null; // ms since the epoch, inclusive
  to: number | null; // exclusive
  include: UrlPattern[]; // any of them; empty means everything
  exclude: UrlPattern[];
  transitions: Transition[]; // empty means every one
};

type HistoryVisit = { id: number; url: string; host: string; title: string; visitedAt: number; transition: Transition };
type HistoryCursor = { visitedAt: number; id: number };
type HistoryPage = { visits: HistoryVisit[]; next: HistoryCursor | null };
type HistorySite = { host: string; visits: number; lastVisitedAt: number };
type HistoryRetention = "forever" | "days30" | "days90" | "year";
```

### Commands

| Command                               | Returns         | Does                                                                        |
| ------------------------------------- | --------------- | --------------------------------------------------------------------------- |
| `query_history(filter, after, limit)` | `HistoryPage`   | Visits matching, newest first, after the cursor. `limit` is at most 500.    |
| `count_history(filter)`               | `number`        | How many visits match. Used by every confirmation.                          |
| `history_sites(filter, limit)`        | `HistorySite[]` | Hosts of matching visits, most visited first.                               |
| `delete_history(filter)`              | `number`        | Deletes matching visits; how many went. An empty filter deletes everything. |
| `delete_history_visits(ids)`          | `()`            | Deletes those visits.                                                       |
| `set_history_paused(paused)`          | `BrowserState`  | Pauses or resumes recording.                                                |

All of them refuse a content webview, as every command does. An invalid pattern, in a filter or in a rule given to `set_settings`, is refused with a new `HakuError::InvalidPattern(reason)`, whose reason the interface shows under the field. A variant of its own, rather than reusing `InvalidUrl` or `Unsupported`, because it is the one error the interface shows inline rather than as a toast, and neither existing name describes it.

Changed: `open_tab(url, activate, source)` and `navigate_tab(id, input, source)` gain `source: NavigationSource`. Removed: `recent_history`, replaced by `query_history`, and `clear_history`, replaced by `delete_history`.

`BrowserState` gains `historyPaused: boolean`. `Settings` gains `historyEnabled: boolean` (default `true`), `historyRetention: HistoryRetention` (default `forever`) and `historyRules: HistoryRule[]` (default empty). A stored rule that no longer parses, such as a hand-edited regular expression, is dropped on load rather than failing the settings file, as stored site permissions are.

This changes the persisted history format and removes two commands, so the commits that do it are marked breaking.

## Storage

`history.sqlite`, as today. Its third migration replaces `visits(id, url, title, visited_at)` with:

```sql
pages  (id, url UNIQUE, host, title, visit_count, typed_count, last_visited_at)
visits (id, page_id → pages, visited_at, transition)
pages_fts  -- FTS5, trigram tokenizer, over pages.title and pages.url
```

with indexes on `visits(visited_at DESC, id DESC)`, `visits(page_id)` and `pages(host)`. Every existing visit is carried over with transition `other`; each page takes the title of its latest visit.

- **Why pages and visits.** The address bar will rank every keystroke by visit and typed counts. Counting raw visits per keystroke grows with history; a page row keeps them at hand. Chrome (`urls`, `visits`) and Firefox (`moz_places`, `moz_historyvisits`) split it the same way. The counts and `last_visited_at` are kept in step on insert and delete, in the same transaction, and a page left with no visits is deleted.
- **Why trigram FTS5.** People search addresses by fragments (`ithub`, `/issues/`), which word tokenizers miss. Trigram indexes substrings; text shorter than three characters falls back to `LIKE`. The bundled SQLite is built with FTS5.
- **The migration needs the URL parser** to fill `host`, which SQL cannot do reliably, so `sqlite::migrate` gains steps that run a Rust function as well as SQL. Bookmarks' migrations are SQL and are unaffected.
- **Downgrading.** A Haku from before this change cannot read the migrated file and keeps history in memory for that run, as it does for any file it cannot open ([Errors](errors.md#failures-that-must-not-propagate)). It does not damage it.

The recording rules and the switch are read in `record_visit`, on the recording thread, against the compiled rules `AppState` keeps beside the settings and replaces when the settings change.

## Slices

On `feat/history`, each a pull request that leaves Haku working:

1. **Storage and recording.** Migration, pages and transitions, the matcher, rules, the switch and pause, retention, the commands, the settings rows. The current page moves to `query_history` and `delete_history` unchanged in look.
2. **The page.** Filters, the visits and sites views, grouping and paging, selection, the row menu, Clear browsing history.
3. **Elsewhere.** Ctrl+H, the site menu's items.

## Rejected alternatives

- **One `visits` table, as today.** Counting per keystroke for suggestions would scan every visit to a page; see [Storage](#storage).
- **Rules by precedence of action** (`always` beats `never` beats `addressOnly`) instead of order. It needs no reordering, but cannot say "always record `example.com`, except `example.com/private`": the broad `always` would win. Order handles both directions and is how firewalls and ad blockers do it.
- **Pause as a setting.** It would survive a restart, which the pause promises not to.
- **The filter in the address.** See [The history page](#the-history-page).
- **SQL `GLOB` and `LIKE` for wildcards.** `GLOB` is case-sensitive and `LIKE` treats `_` as special; either would disagree with the matcher the rules use.
- **`fancy-regex`.** Backreferences and lookaround cost backtracking, and a pattern could hang every visit. The `regex` crate cannot.
- **Grouping by day in Rust.** A day is local and changes with daylight saving; the interface groups the rows it has, and merges a group a page boundary split.
- **A per-rule age limit.** Retention is one setting; per-rule ages multiply cases for little gain (approved direction 5).
- **Indexing page contents.** A large privacy and storage decision of its own (approved direction 10).

## Known limits

- A `site`, `wildcard` or `regex` filter runs the matcher on every visit, so it cannot use an index. Measured on 2026-11-02 (release build, in-memory database, 200,000 visits over 2,000 hosts): about 0.1 s for a page of 100 and 0.05–0.07 s to count. A text filter through FTS took about 4 ms, and an unfiltered page 0.2 ms.
- A rule cannot stop a page loading or the engine's own records, such as cache and cookies. It governs Haku's history only.
- Visits before this change are all `other`, so typed counts start at zero.

## Testing

- `storage/history_db.rs`: the migration from revision 2 with visits and downloads; counts kept in step through insert and delete; every filter field and their combination; paging by cursor without gaps or repeats; delete by filter equals what the same filter queries; FTS and its short-text fallback; retention.
- The matcher, as a module of its own: each kind, case rules, subdomains, invalid and oversized patterns.
- `record_visit`: the switch, pause, each rule action, first match wins, private reports, refused schemes.
- `browser.rs`: each transition reaches the report.
- Settings: lenient loading of a broken rule; a private window refused a new rule.
- Interface: filters to `HistoryFilter`, grouping and collapsing across pages, confirmations naming counts, the banner, the site menu items.
