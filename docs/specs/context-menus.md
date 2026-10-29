---
status: IMPLEMENTED
last_updated: 2026-10-29
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

- **Items**: command, checkbox, radio, separator, submenu; each with an optional icon, a shortcut hint shown as muted text, and a disabled state.
- **Keyboard**: it opens with focus on the menu; Up and Down move through enabled items and wrap, Home and End jump, Right and Left open and close a submenu, Enter and Space choose, Escape closes one level, and typing a letter jumps to the next item starting with it. Roles are `menu`, `menuitem`, `menuitemcheckbox`, `menuitemradio` and `separator`. Choosing a native radio item, such as a spell-check language, closes the menu and hands the choice to WebView2, which keeps which item of the group is checked; the menu does not group radio items itself.
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

Command `answer_context_menu(request: number, choice: number | null, refocus: boolean)`. An answer to an unknown or already answered request is ignored, not an error, because a menu can close from several causes at once.

Showing the menu moves keyboard focus from the page to the chrome, so the answer says where it goes next. A native item gets the page back its focus, since it acts on the page. `refocus` does the same for a menu closed from inside with nothing chosen, by Escape or Tab, so the keyboard is back where it was before the right-click. A menu closed by a click elsewhere, or by one of Haku's items, passes `false`: focus is already where the person or the item put it.

A request the chrome never answers, say because its window closed, would hold the deferral forever. Rust completes any pending request with no choice when a newer request arrives, and after 60 seconds. An answer that comes after the tab has left the slot that asked runs nothing, and the chrome closes a page's menu when its tab stops being the active one.

### Items

Native items are kept by their `Name`, against an allowlist rather than a blocklist, so an item a future WebView2 adds stays hidden until someone decides on it:

| Kept native items                                                                                                                                                         | Dropped native items, and why                                                                                                                                                                                                                                                                                                                                                             |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| undo, redo, cut, copy, paste, pasteAndMatchStyle, selectAll, spell-check suggestions and settings, emoji, copyImage, copyImageLocation, copyLinkLocation, extension items | back, forward, reload: Haku's own items run them through Haku's commands. inspectElement: replaced by Haku's **Inspect**, below. openLinkInNewWindow and the like: replaced by Haku's items. saveImageAs, saveLinkAs, saveAs, print: they start downloads or dialogs Haku does not handle yet; added back with Downloads and Print. share, webCapture, readAloud and other Edge features. |

The names above are the ones WebView2 reported in the app; **Paste as plain text** is `pasteAndMatchStyle`. Items with no name of their own, such as **Writing direction**, **Import passwords** and **Voice typing**, and every separator, arrive as `other`, so an allowlist by name drops them; separators are kept by kind and tidied by the interface. Debug builds log each request's item names to the terminal with the prefix `[context menu]`.

Haku's items, by what was clicked:

| Target    | Haku's items                                                                           |
| --------- | -------------------------------------------------------------------------------------- |
| Page      | Back, Forward, Reload                                                                  |
| Link      | Open link in new tab, Open link in new window                                          |
| Image     | Open image in new tab, when the engine gives the image's address                       |
| Selection | Search for "…" (the search engine's name is not known, so the label does not name one) |

Native items follow in their own groups, then extension items in a group of their own, as Chrome shows them. Every page menu, whatever was clicked, ends with **Inspect**, which opens the tab's DevTools with `open_tab_devtools`, in place of the engine's inspectElement.

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

## Findings

Settled while building it:

- **Paste in the interface's text fields** uses `navigator.clipboard.readText` from the chrome, with no new dependency. WebView2 asked the user for clipboard access on first use, so `platform::allow_clipboard_read` allows clipboard reads for the chrome webview only; pages keep the engine's prompt. A refusal is reported, not dropped.
- **Open link in new window** and **Reopen closed tab** have their commands, `open_window(url)` and `reopen_closed_tab()`.
- **Where the menu opens.** WebView2's `Location` is in physical pixels relative to the webview; Rust divides it by the window's scale factor and adds the viewport's origin.
- **No `mainFrame` in the target.** WebView2's `IsRequestedForMainFrame` reported `false` for every request tried, top-level pages such as example.com included. Nothing needed it, so the event leaves it out rather than carry a value nobody can rely on.
- **Inspect interface** calls Tauri's `plugin:webview|internal_toggle_devtools`, which `core:default` already grants to the chrome; no capability changes. It is shown only when Vite's `import.meta.env.DEV` is set.
- **Image addresses.** An image in a subframe can arrive with no `sourceUrl`; it then gets the engine's image items but not **Open image in new tab**.
