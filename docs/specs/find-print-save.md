---
status: APPROVED
last_updated: 2026-10-31
scope: Finding text in a page, printing a page and saving a page, and the bar, shortcuts and menu items that start them.
---

# Find, print and save

Approved on 2026-10-31. It adds IPC commands and an event, so it is a contract decision under [Agent instructions § Judgment](../../AGENTS.md#judgment). [First release](../first-release.md#features) plans find as "WebView2's find API, with a bar drawn by the chrome", and print and save as WebView2's dialogs driven from Rust.

## Problem

Ctrl+F, Ctrl+P and Ctrl+S are not in the keymap, so [Shortcuts](shortcuts.md#how-keys-are-read) leaves them to the engine, and each one opens WebView2's own interface:

- **Ctrl+F** opens Edge's find bar inside the page, in Edge's style rather than Haku's.
- **Ctrl+P** opens Edge's print preview.
- **Ctrl+S** opens the engine's Save As dialog. That dialog is the Windows file dialog, which Haku keeps for every file it saves.

The page menu has none of the three: [Context menus](context-menus.md#items) withholds the engine's **Save as** and **Print**. Nothing else in Haku starts them.

The maintainer decided on 2026-10-31 that Haku replaces the engine's interface but keeps the operating system's dialogs. So find gets a bar of Haku's own, print opens the Windows print dialog, and save as keeps the Windows file dialog.

## Behaviour

### Find in page

**Opening.** Ctrl+F opens the find bar for the focused tab, wherever focus is in its window. The field takes focus, and if the bar was already open its text is selected. The bar is a small floating card at the top-right corner of the focused pane, over the page, like the browsers users know. It does not push the page down, which would resize the page and make it reflow on every open and close.

**The bar** holds:

- a text field;
- the match count, "3 of 12" or "No results";
- previous and next buttons;
- a close button.

Typing searches as you type, from the top of the page, and the engine highlights every match and scrolls to the current one. Enter goes to the next match and Shift+Enter to the previous one, both wrapping. Esc closes the bar and gives the page back its focus.

**Closing.** The bar closes, and the page's highlights clear, when:

- the user closes it with Esc or its button;
- the focused tab changes, as another tab is selected or the focused pane moves to the other half of a split;
- the page navigates or reloads;
- the page goes away.

The bar belongs to the window and follows the focused tab only while it stays focused. It does not reopen with its old text when the tab is focused again.

**Where it does not apply.** Internal pages, such as `haku://settings`, are drawn by the chrome rather than by a webview, so the engine cannot search them. Ctrl+F does nothing on them, and so does it on a crashed page.

### Print

Ctrl+P and **Print…** in the page menu open the Windows print dialog for the focused tab's page. WebView2's own print preview is never shown. The page prints as the engine lays it out for print, honouring the page's print styles. If the engine reports a failure, an error toast says so. Cancelling the dialog is not a failure. Print does nothing on internal and crashed pages.

### Save as

Ctrl+S and **Save as…** in the page menu open the engine's Save As dialog for the focused tab's page. It is the Windows file dialog and offers the engine's formats:

- webpage, complete (the page and a `_files` folder);
- HTML only;
- a single file (MHTML).

The saved page does not appear in Downloads. The engine writes it without the `DownloadStarting` event that Haku follows downloads by ([Downloads § Context menu](downloads.md#context-menu)). If the engine reports a failure, an error toast says so. Cancelling is not a failure, and success needs no toast, since the dialog just closed on the user's own choice. Save as does nothing on internal and crashed pages.

### The page menu

**Print…** and **Save as…** join Haku's own items in the page menu shown over a page with nothing selected or clicked, after **Reload**. The engine's own `print` and `saveAs` items stay withheld: the engine's Print opens its print preview, and Haku's item calls the same API as Ctrl+S. Over a link, an image, a selection or a field, the menu is unchanged.

## Mechanism

**Keys.** The keymap binds Ctrl+F (`Find`), Ctrl+P (`Print`) and Ctrl+S (`SavePage`). Like every binding, they are marked handled before the page or the engine sees them ([Shortcuts](shortcuts.md#how-keys-are-read)), which is what keeps Edge's find bar and print preview away. A page that handles Ctrl+S itself, such as an online editor, no longer receives it; Chrome makes the same choice.

**Find.** WebView2's `ICoreWebView2Find` runs the search, started with `FindOptions` that set `SuppressDefaultFindDialog` and highlight every match. Its `MatchCountChanged` and `ActiveMatchIndexChanged` events report the count and the current match. Find state lives in the webview: Rust keeps no find state in `Browser`, and the bar's text lives in the interface.

**Print.** `ShowPrintUI(COREWEBVIEW2_PRINT_DIALOG_KIND_SYSTEM)` on the tab's webview.

**Save as.** `ShowSaveAsUI` on the tab's webview, with the engine's default dialog.

**Each command resolves the tab to its live slot**, as every page command does. A tab without one (internal, crashed, or relieved) is a no-op, not an error: the bar and the menu items are only offered for a live page in the first place.

### Contract

Event `findRequested`, sent with `emit_to` to the tab's window only, like `addressFocusRequested`:

```ts
type FindRequested = TabId; // the focused tab to open the bar for
```

Event `findResult`, sent to the tab's window as the engine reports:

```ts
type FindResult = {
  tab: TabId;
  matches: number; // 0 when nothing matches
  active: number | null; // 1-based index of the current match
};
```

Commands:

- `find_in_page(tab: TabId, query: string, direction: "first" | "next" | "previous")`. `"first"` starts a new search for `query`. The other two move within the search `query` started, and start it first if the page has none.
- `stop_find(tab: TabId)`: clears the highlights. Stopping a page with no search is not an error.
- `print_page(tab: TabId)`.
- `save_page(tab: TabId)`.

`query` is limited to 1,000 characters. That is far past any phrase a person types, and it keeps a page-sized paste from becoming a page-sized IPC message. A longer query is truncated by the interface, which owns the field's `maxLength`, and refused by Rust as an invalid argument.

The bar is an overlay registered with `useOverlay` ([Chrome layering § Registering an overlay](chrome-layering.md#registering-an-overlay)).

## Decisions

- **Haku's find bar, the system's dialogs.** Decided by the maintainer on 2026-10-31: replace the engine's interface, keep the operating system's. Edge's find bar and print preview are engine interface. The Windows print and file dialogs are the system's.
- **The Windows print dialog, not a preview.** Decided by the maintainer on 2026-10-31. Building a print preview is a feature of its own, and the engine's preview is what this replaces.
- **A floating card, not a docked strip.** A strip above the page would resize the webview on every open and close, making the page reflow and lose its scroll anchor. A card over the page's corner covers a little of it, as in every major browser.
- **One bar per window, closing on a tab change.** Chrome keeps a bar per tab. Haku's pool can give a background tab's webview away, and the find session goes with it, so a per-tab bar would need its search restarted on return. Closing on a change is simpler and loses nothing a keypress cannot restore.
- **Shortcuts and the page menu as the only entry points.** Haku has no app menu. A ⋮ menu holding these and other actions is a feature of its own, with its own questions about what it holds.

## Not built

- Case-sensitive and whole-word matching. The engine supports both; the bar has no options yet.
- F3, Shift+F3 and Ctrl+G for the next and previous match without the bar focused.
- Finding in internal pages.
- Remembering the last search, per tab or per window.
- A print preview, and printing to PDF from Haku's own interface. The Windows dialog offers Microsoft Print to PDF.
- Showing a saved page in Downloads. Not possible ([Downloads](downloads.md)).
- An app menu.
