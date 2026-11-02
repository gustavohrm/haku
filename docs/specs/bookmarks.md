---
status: APPROVED
last_updated: 2026-11-02
scope: Saving pages as bookmarks, organising them in folders, the bookmarks bar, the bookmarks page, and importing and exporting them.
---

# Bookmarks

Approved by the maintainer on 2026-11-02. It adds IPC commands and an event, a database, persisted settings and keymap actions, so it is a contract decision under [Agent instructions § Judgment](../../AGENTS.md#judgment). It implements the Bookmarks row of [First release § Features](../first-release.md#features), which decides that bookmarks are stored in SQLite. The row's other half, shortcuts on the new-tab page, is left out: the maintainer plans to make the new-tab page fully customizable, and shortcuts built on today's page would be rebuilt then (decided 2026-11-02).

Chosen by the maintainer among alternatives on 2026-11-02: Chrome's fixed roots, a bookmarks bar toggled by Ctrl+Shift+B, where bookmarks go when the toolbar is in the tab column, icons derived as tabs' are, the import limits, importing from a Netscape HTML file only, no new-tab shortcuts, and delivery as this spec followed by one pull request per [slice](#slices).

## Problem

Haku has no way to keep a page to return to. History records every visit but answers "what was that page?", not "the pages I chose to keep". People moving from Chrome, Edge or Helium also bring a bookmark tree with them, sometimes hundreds of links deep in folders, and expect it to arrive intact and to leave intact.

The reference case is a Chrome export of 82 links in 34 folders, nested four levels below the bar (`fisk/SGF/Novo/Interno`), all under the **Bookmarks bar** folder Chrome marks `PERSONAL_TOOLBAR_FOLDER`, with 68 of the links carrying their favicon as an embedded `data:` PNG of about 1 kB.

## Behaviour

### The tree

Bookmarks form one tree shared by every window, with two fixed roots, as in Chrome:

- **Bookmarks bar** — what the [bar](#the-bookmarks-bar) shows.
- **Other bookmarks** — everything else.

The roots cannot be renamed, moved or removed. Under them, folders nest without a fixed depth limit and hold links and folders in an order the user sets. A link has a title and a URL; a folder has a title. The same URL may be bookmarked more than once, in different folders or the same one, as Chrome allows: a link is a thing the user placed, not a set of URLs.

Chrome's third root, **Mobile bookmarks**, is not kept: Haku has no sync, so it would always be empty. An imported file's mobile folder becomes an ordinary folder in **Other bookmarks**.

### Bookmarking a page

The address field shows a star for the focused tab's page, filled when its URL is bookmarked. Clicking it, or **Bookmark this tab** (Ctrl+D):

- On a page that is not bookmarked, adds a link with the page's title and URL to the folder used last, the bar at first, and opens the bookmark editor anchored to the star. The folder used last is the one a link was last added to or moved into, from anywhere: the star, the editor's folder menu, or `haku://bookmarks`. Moving a folder does not change it.
- On a bookmarked page, opens the editor on its most recently added bookmark.

The editor is a popover with the title, the folder (a menu of the tree), **Remove**, **More…** (opens the link's folder in `haku://bookmarks`, in a new tab), and **Done**. Changes apply as they are made: a folder chosen moves the link at once, and the title is kept when the editor closes, however it closes, so a title is not written on every keystroke. Esc or clicking away closes it; **Remove** removes the link and closes it. Internal pages (`haku://`) and pages with no URL show no star and ignore Ctrl+D, since there is nothing durable to return to. A popup window has no address field, so no star, and ignores Ctrl+D too, as it ignores the split and duplicate shortcuts ([Shortcuts](shortcuts.md)).

### The bookmarks bar

A row under the toolbar showing the bar root's contents: links as an icon and title, folders as a folder icon and title. Items that do not fit go into a **»** menu at the row's end.

- A link opens in the focused tab; middle-click or Ctrl+click opens it in a new background tab, Shift+click in a new window.
- A folder opens a menu of its contents, with subfolders as submenus and, at the end, **Open all (N)**, which opens its links in new tabs. N counts the folder's own links, not those in its subfolders, and the tabs open in the background in the folder's order. An empty folder's menu says so. A link chosen from a menu opens in the focused tab.
- A right-click opens the item's menu, the one a bookmark has on `haku://bookmarks`: **Open in new tab**, **Open in new window**, **Open in private window** (ordinary windows only), **Edit…**, **Move to**, **Delete**; and, on the row itself, **Add page…**, **Add folder…**, **Show bookmarks bar**, **Manage bookmarks**. **Show bookmarks bar** is a submenu of the setting's three choices, so the row's menu sets any of them. **Edit…**, **Add page…** and **Add folder…** open the bookmarks page's form in a popover where the menu was; **Add page…** adds to the end of the bar and starts empty.

**Showing it.** A setting, `bookmarksBar`, with three values, settable from `haku://settings/appearance` and from the row's menu:

| Value    | Shown                                                       |
| -------- | ----------------------------------------------------------- |
| `always` | In every tab                                                |
| `newTab` | Only while the focused tab is on the new-tab page (default) |
| `never`  | Never                                                       |

**Toggle bookmarks bar** (Ctrl+Shift+B) switches between `always` and `never`, from `newTab` to `always`. `newTab` is the default because it is Chrome's: the bar appears where the user is choosing where to go, and costs no height on every page otherwise. While bookmarks are the column's **Bookmarks** button there is no bar to toggle, so in an ordinary window it opens that button's menu instead, and changes no setting (chosen by the maintainer on 2026-11-02).

With floating bars the bookmarks bar floats with the toolbar, as part of the same card, so it takes no space from pages either ([Appearance § Bars](appearance.md#bars)).

**With the toolbar in the tab column** (`toolbarPlacement: sidebar`) there is no top row to sit under, so the user chooses where bookmarks go, with a second setting, `sidebarBookmarks` (decided 2026-11-02):

| Value    | Where                                                                                                                                                                                                                             |
| -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `row`    | The bar as above, as a row of its own across the top of the page area, beside the column. `bookmarksBar` governs it as it governs the bar under the toolbar. With floating bars it floats from the top edge, as a bar of its own. |
| `button` | A **Bookmarks** button in the column's foot row, beside the extensions and settings. It opens a menu of the bar root: its links, its folders as submenus, then **Other bookmarks** as a submenu and **Manage bookmarks**.         |

`sidebarBookmarks` is shown in Appearance beside **Toolbar**, and disabled, still showing what is stored, while the toolbar is on top, as **Sidebar** and **Toolbar** are while the tabs are horizontal ([Appearance § Settings page](appearance.md#settings-page)). `button` is the default: the toolbar is in the column to give the page the window's full height, and a row above the page would take some back on every new-tab page. Under `button` the button is always shown, `bookmarksBar` has no effect and is disabled in Appearance, and Ctrl+Shift+B opens the button's menu, since there is no row to show or hide.

### The bookmarks page

`haku://bookmarks` (**Manage bookmarks**, Ctrl+Shift+O), an [internal page](../architecture.md#internal-pages):

- The folder tree on the left, the selected folder's contents on the right, and a search field that searches every bookmark's title and URL, showing matches with their folder.
- Create a folder, add a link, rename, edit, delete, and move, by drag and drop in either pane or by **Move to…** in an item's menu.
- **Import…** and **Export…** in the page's header ([Import and export](#import-and-export)).
- `haku://bookmarks?folder=<id>` opens a folder, so **More…** and the bar's folder menus can link into it. An id that names no folder opens **Bookmarks bar**.
- A link opens as it does on the bar: in this tab, or with middle-click or Ctrl+click in a new background tab, and with Shift+click in a new window. Its menu, on right-click or its actions button, holds **Open in new tab**, **Open in new window**, **Open in private window** (ordinary windows only), **Edit…**, **Move to** and **Delete**.
- A name and an address are edited in a form in place of the row; an address typed without a scheme, such as `example.com`, is taken as `https://`.

Dragging is pointer-driven, as the tab strip's is, not HTML drag and drop: enabling HTML drops in the chrome lets an external drop navigate it ([`tauri/AGENTS.md`](../../tauri/AGENTS.md)). Dragging a link out of a page onto the bar is therefore not supported.

**Deleting.** Deleting a link or an empty folder happens at once. Deleting a folder that holds anything asks first, naming how many bookmarks it holds, because there is no undo.

### Icons

A link shows the icon a tab on its URL would: `https://<host>/favicon.ico`, loaded by the interface (`favicon_for` in `browser.rs`), with the generic page icon when that fails or the URL has no host. Nothing is stored for an icon (decided 2026-11-02). In a private window links show the generic icon, as its tabs do, because the interface fetching an icon would send the ordinary profile's cookies to the site ([First release § Observed, not reported](../first-release.md#observed-not-reported)).

The cost: a site that serves no icon at `/favicon.ico` shows the generic one, including 68 imported links whose file carried their icon. When the favicon cache that section approves is built, tabs and bookmarks both move to it.

### Private windows

Bookmarks are shared with private windows, which can see, open, add, edit and delete them, as in Chrome. A bookmark is something the user chose to keep, not a record of browsing, so adding one from a private window is the user writing it deliberately, unlike a visit or a site setting ([Settings page § Sections](settings-page.md#sections)). Opening a bookmark from a private window opens it in that private window.

### Import and export

**Import…** opens the system's file dialog, filtered to `*.html` and `*.htm`, and reads a Netscape bookmark file, the format every major browser exports:

- The file's contents go into a new folder in **Other bookmarks** named "Imported", with the date, so an import never mixes into what is already there and can be removed as a unit. The folder Chrome, Edge and Helium mark `PERSONAL_TOOLBAR_FOLDER` is the exception: its contents go to the end of **Bookmarks bar**, because that is where the user kept them, and an imported bar left in a subfolder would leave the new bar empty.
- Nothing is de-duplicated: importing the same file twice gives two copies, since two bookmarks of a URL are allowed and a merge would have to guess which copy's folder is right.
- Titles, URLs, nesting, order and `ADD_DATE` are kept. A missing or invalid date becomes the import's time. `ICON` is dropped ([Icons](#icons)).
- A file larger than 32 MB is refused before it is read. Folders nested deeper than 64 levels are flattened: whatever is below the 64th level goes into its folder at that level, in order. Both keep a broken or hostile file from exhausting memory or the parser's stack; the reference file is 74 kB for 82 links, about 0.9 kB a link with its icon, so 32 MB holds tens of thousands of links (decided 2026-11-02).
- Only `http:`, `https:` and `file:` links are imported. `javascript:` bookmarklets, `data:` URLs, `place:` queries (Firefox) and anything else are skipped, because a link in a bookmark is followed by a click and Haku's pages are not for running script from a file. Separators (Firefox's `<HR>`) are skipped.
- The whole import is one database transaction: a file that fails to parse or to store adds nothing. `BookmarksChanged` is sent once, after the transaction commits; an import that rolls back sends none, so no window sees part of one.
- When it ends, a notice says how many links and folders were imported and how many were skipped. A file that is not a bookmark file says so and imports nothing.

**Export…** opens the system's save dialog on `bookmarks.html` and writes every bookmark in the same format, as Chrome writes it: **Bookmarks bar** as a folder marked `PERSONAL_TOOLBAR_FOLDER`, **Other bookmarks** as a folder after it, `ADD_DATE` and `LAST_MODIFIED` in seconds, titles and URLs HTML-escaped, and no `ICON`. Exporting from Haku and importing into Chrome gives the same tree.

The chrome never names a path. Both dialogs are opened by Rust, which reads or writes only the file the user chose there, as `open_download` acts only on a download Haku listed ([Downloads § Opening files](downloads.md#opening-files)).

## Contract

Bookmarks are owned by Rust: the types and the URL rule in `model/bookmark.rs`, the tree and every change to it in `storage/bookmarks_db.rs`, and the commands in `ipc/commands.rs`, where every command is defined. The interface renders them and asks for changes.

- **Type.** `BookmarkNode` is `{ id, title, addedAt, modifiedAt, kind }`, where `kind` is `{ link: { url } }` or `{ folder: { children: BookmarkNode[] } }`. `Bookmarks` is `{ bar: BookmarkNode, other: BookmarkNode }`, both folders. Ids are numbers, unique and stable for the bookmark's life; times are milliseconds since the epoch, as in history.
- **Event.** `BookmarksChanged(Bookmarks)` is sent to every window, ordinary and private, whenever anything in the tree changes, with the whole tree. `bookmarks()` returns the same for a window that has just started.
- **Event.** `BookmarkEditRequested { tab, id }` is sent to the chrome of the tab's window when Ctrl+D bookmarks it, after the `BookmarksChanged` that adds the link when one is added, so the editor finds the bookmark in the tree it has. A chrome whose tab has closed by then ignores it; the bookmark stays.
- **Event.** `BookmarksMenuRequested` is sent to the chrome of the window Ctrl+Shift+B was pressed in, an ordinary one, while bookmarks are the column's button, after the chrome takes the keyboard; the interface opens the button's menu. Added on 2026-11-02, with the maintainer's approval.
- **Commands**, all chrome-only:
  - `add_bookmark(parent, index, title, url)` and `add_bookmark_folder(parent, index, title)` return the new id. `index` is `null` for the end.
  - `update_bookmark(id, title, url)`; `url` is ignored for a folder.
  - `move_bookmark(id, parent, index)`; moving a folder into itself or its descendants is refused.
  - `remove_bookmark(id)` removes a link, or a folder with everything in it.
  - `bookmark_tab(tab)` adds the tab's page to the folder used last, or returns its most recently added bookmark of the page's URL, and returns the id with whether it was added. The editor opens on that id. Ctrl+D runs the same in Rust and sends `BookmarkEditRequested`, since a key pressed in a page never reaches the interface ([Shortcuts](shortcuts.md#how-keys-are-read)).
  - `import_bookmarks()` and `export_bookmarks()` open the file dialog for the calling window. They return `null` when the dialog is cancelled; `import_bookmarks` otherwise returns `{ links, folders, skipped }`.
- **Refusals.** A command naming an id that does not exist, a root where a root may not go, or a link URL of a scheme Haku would not import is an `Unsupported` error ([Errors](errors.md)), shown as a toast through `@ipc/commands`.
- **Keymap.** Three new actions, with Chrome's defaults: `bookmarkTab` (Ctrl+D), `toggleBookmarksBar` (Ctrl+Shift+B), `openBookmarks` (Ctrl+Shift+O), in the Page and Windows groups of [Shortcuts](shortcuts.md#the-keymap). None is refused by its rules.
- **Settings.** `bookmarksBar` (`always`, `newTab`, `never`; default `newTab`) and `sidebarBookmarks` (`row`, `button`; default `button`), both read leniently as the appearance enums are ([Appearance § Settings](appearance.md#settings)). The folder used last is kept in the bookmarks database, not in settings, because it names a bookmark id and is meaningless without that database. When that folder has since been deleted, the next bookmark goes to **Bookmarks bar**.

The whole tree in every event was chosen because bookmarks change only when the user does something, never on a timer, so the per-progress cost that ended whole lists for downloads (issue #75) does not apply. The cost is recorded under [Measurements](#measurements).

## Storage

`bookmarks.sqlite`, beside `history.sqlite`, with its own migrations, opened, set aside when corrupt and kept in memory when it cannot be opened, as history is. The user is then told once, through [`FailureReported`](errors.md#failures-no-command-is-waiting-on), that bookmarks could not be opened and changes made in this run will not be kept.

A file of its own rather than a table in `history.sqlite`, which [Downloads](downloads.md#rejected-alternatives) chose for downloads because they are, like history, records of what was fetched that grow alike and are kept alike. Bookmarks are none of those: they are the user's own work, small, kept until the user deletes them, and must survive clearing history and a history database set aside as corrupt. A separate file also lets a user back it up or copy it alone.

## Interface

- `src/features/bookmarks/`: the bar, the editor popover, `haku://bookmarks`, and the store over `BookmarksChanged`.
- The star sits at the end of the address field, beside the zoom level when one shows ([Zoom](zoom.md#behaviour)).
- Overlays — the editor, folder menus, the » menu — register with `useOverlay` and use the menu component ([Chrome layering](chrome-layering.md#registering-an-overlay), [Context menus](context-menus.md#the-menu-component)).
- A bar item re-renders only when its own node changed, so editing one bookmark does not redraw the bar.

## Slices

One pull request each, in order, each leaving Haku working:

1. **Storage, IPC and `haku://bookmarks`**: the database, the model, every command but import and export, the event, and the page.
2. **Bookmarking a page**: the star, the editor, `bookmark_tab`, `BookmarkEditRequested`, Ctrl+D and Ctrl+Shift+O.
3. **The bar**: the row under the toolbar and above the page, the column's **Bookmarks** button, folder menus, context menus, both settings, Ctrl+Shift+B.
4. **Import and export**: the file dialogs in `platform/`, the parser and the writer.

The First release row stays as it is, and this spec APPROVED, until slice 4 lands; that change marks the row **Implemented** and moves this spec to IMPLEMENTED.

## Measurements

`BookmarksChanged`, measured in slice 1 (2026-11-02) on Windows 11, an AMD 4700S, from an in-memory database: the size of the serialized tree, and the time Rust spends reading it and serializing it for one change, averaged over 20 changes. The reference file is its 82 links and 34 folders loaded into **Bookmarks bar**; the generated tree is 5 000 links in 50 folders, with titles of about 60 characters and URLs of about 70.

| Tree           | Payload  | Read, debug | Serialize, debug | Read, release | Serialize, release |
| -------------- | -------- | ----------- | ---------------- | ------------- | ------------------ |
| Reference file | 15.8 kB  | 0.36 ms     | 0.85 ms          | 0.10 ms       | 0.03 ms            |
| 5 000 links    | 1 097 kB | 10.9 ms     | 54.0 ms          | 3.4 ms        | 1.9 ms             |

The change itself, an add and a remove, took under 0.5 ms in both builds. Not measured: delivering the payload to each window and parsing it there, which grows with the payload and the number of windows open. At 5 000 links a change sends about 1.1 MB to every window; that is acceptable for a change the user makes by hand, and is the figure to revisit if bookmarks gain anything that changes them often.

The bar, measured in slice 3 (2026-11-02) on the same machine, in Vitest 4.1 under jsdom on Node 24, averaged over 20 runs: mounting the bar, drawing it again for a change that renames one item on it (the whole tree arriving anew, as every change does), and building every menu it opens: the column's **Bookmarks** menu and each of the bar's folders. jsdom does no layout or painting, so these are React's and the DOM's work alone, and the mount includes warming up. The reference file's bar holds 6 folders, with its 82 links and 33 folders inside them; the generated tree is 5 000 links in 50 folders on the bar.

| Tree           | Mount   | One item changed | Every menu |
| -------------- | ------- | ---------------- | ---------- |
| Reference file | 13.7 ms | 2.0 ms           | 1.7 ms     |
| 5 000 links    | 30.4 ms | 3.3 ms           | 2.6 ms     |

The bar draws only its root's own items: a folder's menu is built when it opens, so a change inside a folder draws nothing on the bar, and a change to one item draws that item alone. A menu is built whole, its submenus with it, so opening the column's **Bookmarks** menu builds both roots' trees; the last column measures that. Not measured: layout and painting in WebView2, and the cost of measuring which items fit, which reads every item's position once per change or resize.

## Rejected alternatives

- **One free-form tree with a folder flagged as the bar.** More flexible, but every import from Chrome, Edge or Helium, which all use fixed roots, would need translating, and export back would too. Settled 2026-11-02.
- **Importing from installed browsers' profiles.** Reading Chrome's, Edge's and Helium's `Bookmarks` JSON from disk saves the user an export, but needs per-browser profile discovery and a second parser, and every one of those browsers exports the HTML file. Settled 2026-11-02; can be added later on top of the same import path.
- **Merging an import into existing folders.** Two bookmarks of one URL are allowed, so a merge would have to decide which copy's folder wins; a dated folder of its own is reversible by deleting it.
- **A table in `history.sqlite`.** See [Storage](#storage).
- **Storing imported icons.** It keeps the reference file's 68 icons, but stores about 1 kB a link and sends it in every `BookmarksChanged`, for icons the favicon cache will replace. Decided 2026-11-02.
- **New-tab shortcuts now.** See the opening paragraph.
- **Undo for deletion.** It needs either the removed subtree held by the interface and sent back, which lets the interface write arbitrary trees, or removed rows kept in the database. A confirmation before deleting a non-empty folder covers the costly case.

## Testing

- `model/bookmark.rs`: which URLs may be bookmarked, and the shape the interface receives.
- `storage/bookmarks_db.rs`: the migration, adding, moving, refusing a move into a descendant, removing a folder with its contents, the roots' refusals, positions kept dense and in order across moves and removals, a corrupt file set aside, the in-memory fallback; later, the folder used last and an import's transaction rolling back.
- The parser and writer: the reference file's structure (82 links, 34 folders, the bar's contents arriving in **Bookmarks bar**), Firefox's and Edge's exports, skipped schemes counted, the size refusal, flattening below 64 levels, escaping, a round trip of export then import giving the same tree, for a tree no deeper than the import's 64-level limit.
- The interface with Vitest: the star's state and the editor, the bar's overflow menu and folder menus, `haku://bookmarks` search and editing, the delete confirmation.
- In the app: importing the reference file, using the bar and the page, exporting and importing the export into Chrome.
