---
status: APPROVED
last_updated: 2026-10-23
scope: Right-click menus over pages and over the interface, and the shared menu component they are drawn with.
---

# Context menus

Approved on 2026-10-23. It adds an IPC event and a command, so it is a contract decision under [Agent instructions § Judgment](../../AGENTS.md#judgment). [First release](../first-release.md#features) already lists the feature as "WebView2's context-menu event, drawn by the chrome with Haku's own items"; this is how.

## Problem

Nothing in Haku handles a right click today.

- **Over a page**, WebView2 draws its own menu, in Edge's style rather than Haku's, with Edge features such as Share and Read aloud that Haku does not support, and **Save as** and **Print**, which start downloads and dialogs Haku does not handle yet.
- **Over the interface**, the chrome webview draws the same default menu: Back, Reload, Save as, Print and **Inspect**. Back and Reload act on the interface document itself, and Reload drops the interface's state. Inspect opens DevTools on the one webview that holds IPC, in release builds too, because Tauri's `devtools` feature is on for `open_devtools` ([`tauri/AGENTS.md`](../../tauri/AGENTS.md)). It needs local access to use, so the realistic harm is a confused user, not an attacker, but it is not a browser feature.

## The menu component

Every menu Haku draws uses one component, `features/overlays/menu.tsx`. The site menu and the extensions menu are panels, not menus, and keep their own markup, but share its positioning and dismissal.

- **Items**: command, checkbox, radio, separator, submenu; each with an optional icon, a shortcut hint shown with `.kbd`, and a disabled state.
- **Keyboard**: it opens with focus on the menu; Up and Down move through enabled items and wrap, Home and End jump, Right and Left open and close a submenu, Enter and Space choose, Escape closes one level, and typing a letter jumps to the next item starting with it. Roles are `menu`, `menuitem`, `menuitemcheckbox`, `menuitemradio` and `separator`. Native radio items, such as a spell-check language choice, keep their group: choosing one checks it and unchecks the others in its run between separators.
- **Dismissal**: `useMenuDismiss`, as the site menu closes now.
- **Placement**: opens at the pointer, down and to the right; flips left or up when it would leave the window, and then clamps to the window. A submenu opens beside its item and flips the same way.
- **Over the page**: registered with `useOverlay`, opaque, no shadow, rounded corners cut from its own style, as [Chrome layering](chrome-layering.md#registering-an-overlay) requires. One consequence: the menu cannot extend past Haku's window, where a native menu can. In a small window a long menu is clamped and scrolls.

## Page menus

### Approach

WebView2's `ContextMenuRequested` event hands over what was clicked and the menu it would show. Haku keeps the menu's working parts and draws it itself:

1. `platform/` handles `ContextMenuRequested` on every content webview, marks it handled, and takes a deferral.
2. Rust sends the clicked target and the default items it allows to the chrome of the tab's window.
3. The chrome draws the menu: Haku's own items, then the allowed native items.
4. The chrome answers with the native item chosen, or with nothing when the choice was one of Haku's items or the menu was dismissed. Rust sets `SelectedCommandId` for a native item and completes the deferral.

Haku's own items are run by the chrome through existing commands, such as `open_tab` for **Open link in new tab**. Their labels are translated by the interface, where translations live; native items arrive already labelled in the engine's language.

Rejected alternatives:

- **Haku's items only, with no native items.** Cut, copy, paste, spell-check suggestions, **Copy image** and the items extensions add would each need reimplementing, and spell-check suggestions cannot be reached any other way. Keeping native items gets them for free and correct.
- **Rust builds the whole menu, Haku's items included.** It keeps the menu's content in one place, as the keymap is, but needs translations in Rust, which has none.
- **Keep WebView2's own menu and only remove items.** It is the cheapest, but it still looks foreign, which is the request.

### Contract

Event `contextMenuRequested`, sent with `emit_to` to the tab's window only; listeners use `events.contextMenuRequested(getCurrentWebview()).listen`, per the trap in [`src/AGENTS.md`](../../src/AGENTS.md):

```ts
type ContextMenuRequested = {
  request: number; // answers pair with requests
  tab: TabId;
  x: number; // in the chrome's CSS pixels, already offset by the viewport
  y: number;
  target: {
    kind: "page" | "image" | "selection" | "audio" | "video";
    pageUrl: string;
    linkUrl: string | null;
    linkText: string | null;
    sourceUrl: string | null; // the image or media
    selection: string | null;
    editable: boolean;
    mainFrame: boolean;
  };
  items: NativeMenuItem[]; // allowed native items only, in the engine's order
};

type NativeMenuItem = {
  id: number; // WebView2's CommandId
  name: string; // stable, e.g. "copy", "paste", "spellCheck"
  label: string;
  kind: "command" | "checkbox" | "radio" | "separator" | "submenu";
  enabled: boolean;
  checked: boolean;
  shortcut: string | null;
  children: NativeMenuItem[];
};
```

Command `answer_context_menu(request: number, choice: number | null)`. An answer to an unknown or already answered request is ignored, not an error, because a menu can close from several causes at once.

A request the chrome never answers, say because its window closed, would hold the deferral forever. Rust completes any pending request with no choice when a newer request arrives, when the tab closes or changes slot, and after 60 seconds.

### Items

Native items are kept by their `Name`, against an allowlist rather than a blocklist, so an item a future WebView2 adds stays hidden until someone decides on it:

| Kept native items                                                                                                                                                       | Dropped native items, and why                                                                                                                                                                                                                                                                                                                                                             |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| undo, redo, cut, copy, paste, pasteAsPlainText, selectAll, spell-check suggestions and settings, emoji, copyImage, copyImageLocation, copyLinkLocation, extension items | back, forward, reload: Haku's own items run them through Haku's commands. inspectElement: replaced by Haku's **Inspect**, below. openLinkInNewWindow and the like: replaced by Haku's items. saveImageAs, saveLinkAs, saveAs, print: they start downloads or dialogs Haku does not handle yet; added back with Downloads and Print. share, webCapture, readAloud and other Edge features. |

Haku's items, by what was clicked:

| Target    | Haku's items                                                                                     |
| --------- | ------------------------------------------------------------------------------------------------ |
| Page      | Back, Forward, Reload; then **Inspect**, which opens the tab's DevTools with `open_tab_devtools` |
| Link      | Open link in new tab, Open link in new window                                                    |
| Image     | Open image in new tab                                                                            |
| Selection | Search for "…" (the search engine's name is not known, so the label does not name one)           |

Native items follow in their own groups. Extension items stay in their own group at the bottom, as Chrome shows them.

## Interface menus

The chrome's own default menu is turned off from the interface: a `contextmenu` listener on the document calls `preventDefault`, which suppresses WebView2's menu, and draws Haku's menu where there is one. No Rust is needed.

| Where             | Menu                                                                                                              |
| ----------------- | ----------------------------------------------------------------------------------------------------------------- |
| A tab             | Reload, Duplicate, Keep loaded (checkbox, `set_tab_fixed`), then Close, Close other tabs, Close tabs to the right |
| The tab strip     | New tab, Reopen closed tab                                                                                        |
| A text field      | Cut, Copy, Paste, Select all                                                                                      |
| Anywhere else     | Nothing                                                                                                           |
| Debug builds only | **Inspect interface** appended to every interface menu, so developing Haku keeps its DevTools                     |

Duplicate opens the tab's current URL with `open_tab`, without its back history; a true duplicate needs a new command and is left for later. Close other tabs and Close tabs to the right call `close_tab` per tab.

## Open questions

- **Paste in the interface's text fields.** `navigator.clipboard.readText` in the chrome webview may raise WebView2's clipboard permission prompt. Whether Tauri answers it, or Haku needs `@tauri-apps/plugin-clipboard-manager`, which is a new dependency and capability, is checked when this is built.
- **Extension items.** Whether WebView2 includes `chrome.contextMenus` items from extensions in `MenuItems` is to be checked with Bitwarden. If it does not, extension items are out of scope until it does.
- **Open link in new window.** `open_tab` opens a tab in the calling window only, and no command opens a window on a URL, so the item needs a new command, such as `open_window(url)`, which opens a browser window whose one tab loads the URL, as Shift+clicking a link does ([Windows and popups](new-windows.md)). Until it exists, the link menu has only Open link in new tab.
- **Reopen closed tab.** [Shortcuts](shortcuts.md) handles Ctrl+Shift+T in Rust, and no command exposes it to the interface, so the menu item needs a new command, `reopen_closed_tab`. Without it, the tab strip menu has only New tab.
