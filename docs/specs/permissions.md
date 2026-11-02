---
status: IMPLEMENTED
last_updated: 2026-11-01
scope: What happens when a page asks for a permission, such as the camera or location, and how decisions are kept.
---

# Permissions

Approved on 2026-11-01. It adds a field to `Tab`, a command and a persisted setting, so it is a contract decision under [Agent instructions § Judgment](../../AGENTS.md#judgment). It implements the Permissions row of [First release § Features](../first-release.md#features).

## Problem

A page asks for a permission with an API such as `getUserMedia` or `Notification.requestPermission`. Haku does not answer, so WebView2 draws its own prompt: native, styled unlike the interface, and storing its answer where Haku cannot list or remove it.

## Behaviour

**Asking.** The interface asks with a dialog titled with the requesting origin, as [page dialogs](page-observation.md#page-dialogs) are titled with their host: "_https://meet.example_ wants to use your camera", with **Allow** and **Block**. Only the focused tab's request is shown. A background tab's request waits, and its strip item shows the same **Dialog waiting** badge as a page dialog. A page dialog on the same tab is shown first.

**Answering.**

| Answer                   | This request | Kept                                                                                                                      |
| ------------------------ | ------------ | ------------------------------------------------------------------------------------------------------------------------- |
| Allow                    | Granted      | Stored for the origin and kind                                                                                            |
| Block                    | Denied       | Stored for the origin and kind                                                                                            |
| Escape, or closing it    | Denied       | Not stored. Denied without asking until Haku quits, or in a private window until the private session ends                 |
| The page goes away first | Denied       | Nothing. Closing the tab, navigating or reloading it from Haku, the page leaving the request's origin, or losing its slot |

A request whose origin and kind already have a decision is answered at once, without a dialog. Two requests for the same origin and kind that wait together are answered by the first answer, in whichever tabs of the window's profile they wait.

**Private windows.** A stored Block applies, ahead of any answer given in the private session; a stored Allow does not. Decisions made in a private window are kept in memory for the private session only and forgotten when it ends, as private visits never write a site-derived setting ([Settings page § Sections](settings-page.md#sections)).

**Reviewing.** `haku://settings/browsing` lists stored decisions under **Site permissions**, by origin, and removes them one at a time. The next request from that origin asks again.

## Kinds

| Kind                | WebView2 kind                    | Asked as                         |
| ------------------- | -------------------------------- | -------------------------------- |
| `camera`            | `CAMERA`                         | use your camera                  |
| `microphone`        | `MICROPHONE`                     | use your microphone              |
| `location`          | `GEOLOCATION`                    | know your location               |
| `notifications`     | `NOTIFICATIONS`                  | show notifications               |
| `midiSysex`         | `MIDI_SYSTEM_EXCLUSIVE_MESSAGES` | control your MIDI devices        |
| `sensors`           | `OTHER_SENSORS`                  | use your device's motion sensors |
| `windowManagement`  | `WINDOW_MANAGEMENT`              | manage windows on your screens   |
| `localFonts`        | `LOCAL_FONTS`                    | use the fonts on your computer   |
| `fileReadWrite`     | `FILE_READ_WRITE`                | edit files you open with it      |
| `clipboardRead`     | `CLIPBOARD_READ`                 | see text and images you copy     |
| `multipleDownloads` | `MULTIPLE_AUTOMATIC_DOWNLOADS`   | download multiple files          |

Autoplay is engine policy and never asks the user, so it stays the engine's. A kind the bindings do not name is left to the engine (see [Known limits](#known-limits)).

## Mechanism

**The signal.** `platform/` handles WebView2's `PermissionRequested` on every content webview. For a kind in the table it sets `SavesInProfile` to false, so the engine stores nothing itself, takes a deferral, holds the event keyed by a `PermissionRequestId`, and reports a page signal. The page is not paused.

**The state.** `Browser::request_permission` attaches the request to the tab owning the slot, or answers it at once from a decision it already has: stored, private, or dismissed. A request from a slot no tab owns, or from an origin its tab has already left, is denied. Each tab shows one request, `Tab::permission`; further ones queue behind it in Rust. Every path that releases a tab's page dialog releases its permission requests too, and a tab waiting on one is not discarded under memory pressure, as one paused on a dialog is not. An answer withdraws every request for the same origin and kind waiting in tabs of the same profile, shown or queued, so a second tab cannot ask again and overwrite it. A request queued later is answered from the decision when its turn comes, rather than shown. A page that navigates itself releases nothing on the way, so once a commit leaves the origin a request came from, that request is denied (`withdraw_stale_permissions`).

**The contract.**

```ts
// Tab
permission: PermissionRequest | null; // { id, kind, origin }

// Settings
sitePermissions: SitePermission[]; // { origin, kind, state: "allow" | "block" }

// Command
answerPermission(tab: TabId, request: PermissionRequestId, decision: "allow" | "block" | "dismiss");
```

An answer to a request that is no longer shown is ignored, as for dialogs. An Allow or Block from an ordinary window is stored through the same path as every other setting, and announced with `settingsChanged`. A setting that adds a decision is refused from a private window, as a kept site is. Removing one is a `setSettings` without it.

**Origins.** A decision is keyed by the origin WebView2 reports for the request, `scheme://host[:port]` as `URL.origin` gives it, so `http` and `https` are separate. A frame embedded from another origin, with the permission delegated to it by `allow=`, is decided as the page that embeds it, as in Chromium: it shares the page's decision and never asks under its own name. Checked on 2026-11-01: a `127.0.0.1` frame inside a `localhost` page was granted location from `localhost`'s Allow, with no prompt. Haku writes stored origins in the same form, lowercased and without a default port, so `HTTPS://A.test:443` is kept as `https://a.test`, and one that is not an `http` or `https` origin is dropped; a stored entry that cannot be read is dropped rather than failing the whole settings file.

**The interface.** `features/feedback` shows the request through `feedback().dialog.confirm`. The toaster resolves Cancel and Escape to the same `false`, so the hook tells them apart by the dialog's `cancel` event, which only Escape raises. The hook gives its dialog a class naming the request, and counts a `cancel` only on that dialog, so Escape on a dialog the toaster shows first is not read as this request's answer. `isWaiting` puts a request on the same footing as a page dialog for the strip badge and for floating bars.

## Decisions

Chosen by the maintainer on 2026-11-01.

- **Haku stores decisions, not the engine.** The engine's store cannot be listed in settings, tested without the engine, or kept apart for private windows. `SavesInProfile` is turned off so there is one store.
- **Origin, not host.** Permissions are origin-scoped in every browser; a grant on `https` must not reach `http`. Kept sites stay keyed by host, on purpose.
- **Allow and Block, both remembered; Escape remembered for the process only.** "Allow once" would add a third button and a lifetime to track. Remembering a dismissal until quit stops a page from asking again in a loop without storing an answer the user never gave.
- **A field of its own, not a `DialogKind`.** A permission request does not pause the page, its answer is stored, and it can be withdrawn by a navigation; folding it into `PageDialog` would blur those. It shares the dialog's presentation and badge instead.
- **Multiple automatic downloads ask Haku.** This reverses [Downloads](downloads.md), which left that prompt to the engine.

## Known limits

- A permission kind added by a later WebView2 runtime, unknown to the bindings, is left to the engine and shows its native prompt until it is added here.
- A decision the engine stored before this change, from its own prompt, may still answer requests without Haku being asked.
- A request survives a navigation or reload the page makes itself within its origin, and is answered for that origin.
- A request from a page that is not `http` or `https`, such as a `file:` page, has no origin to key a decision by, and is left to the engine.
