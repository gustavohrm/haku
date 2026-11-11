---
status: IMPLEMENTED
last_updated: 2026-11-11
scope: How Haku's interface renders above page content, and how clicks still reach the page.
---

# Chrome layering

## The requirement

The browser skeleton must sit above every content webview. Without that, the page surface cannot have rounded corners and no menu, dropdown or dialog can overlap page content, because a native webview always composites above the HTML of the window that hosts it.

## What Tauri gives us

Nothing directly. Verified against `tauri 2.10.3` / `wry 0.54.4`:

- `Webview` has **no z-order API**. The surface is `set_bounds`/`size`/`position`, `set_focus`, `hide`/`show`, `reparent`, `navigate`, `reload`, `eval`, `set_zoom`, `set_background_color`.
- On Windows, wry gives each webview its own **container HWND** parented to the window, and calls `SetWindowPos(hwnd, HWND_TOP, …)` when it creates one. The newest webview is therefore always on top, and the window's original webview — Haku's interface — is permanently at the bottom.
- `set_bounds` passes `SWP_NOZORDER`, so repositioning cannot be used to reorder.

The way through is `Webview::with_webview`, which hands out the platform handle. On Windows that is the `ICoreWebView2Controller`, whose `ParentWindow` is exactly the container HWND wry created.

## The mechanism

Two native operations, both in `platform/`:

1. **Raise.** `SetWindowPos(chrome, HWND_TOP, …)` lifts the interface above every content webview. It must be re-applied whenever a content webview is created, because a new one is placed on top.
2. **Mask.** The chrome covers the whole window, so without help it would swallow every click meant for the page. `SetWindowRgn` gives it a region equal to the window minus the union of uncovered viewport rectangles, plus any overlay currently drawn over the page. Where the chrome is not in its region its HWND is not there at all, so clicks, scrolling and hover fall through to the content webview beneath.

### Transparency with the region

The chrome webview is also transparent: its background colour has alpha 0, so wherever its document paints nothing, the page beneath shows through. Transparency does not replace the region, it combines with it. The region still decides who takes input, because transparent pixels do not pass clicks through (see the measurements below). Transparency decides what a pixel the chrome keeps looks like: a pixel it only partly paints blends with the page beneath it. That is what lets the page's corners, overlays' edges and translucent surfaces be antialiased over the page.

Anything drawn over the page must still register itself, or the region clips it away: over the page, the chrome is not there to show it or to take its clicks. That is what `features/overlays` is for.

The chrome's document paints its own colour around the page, not behind everything. Once a pane exists the root and `body` are transparent, and each pane draws the chrome colour outside its own rounded box, from a pseudo-element behind it whose box-shadow spreads across the window (`.haku-pane::before` in `styles/index.css`). The curve between the page and the chrome is therefore the browser's own antialiased one. In a split, each pane's surround is clipped just past its edge toward the divider so it does not paint over the other pane's corners. A covered pane, such as an internal page, also fills its own box, because an outer box-shadow does not paint inside the box that casts it.

#### Decision: transparent chrome, region for input

Until 2026-11-08 the chrome was opaque and the region alone decided what showed, so anything drawn over the page had to be opaque, with hard edges. That was chosen on the belief that two sibling child HWNDs do not alpha-composite reliably, and kept after the measurement below contradicted it, because most effects wanted over the page are blurred and transparency alone cannot blur. That reasoning weighed transparency as a replacement for the region, and did not weigh it as an addition to it: kept together, transparency costs no hit-testing change and gives antialiased corners, fading toasts and translucent popovers, with nothing in the mechanism given up. The gaps listed as untested were then measured where they bore on the change, video and scaling, and did not reproduce.

Rejected:

- **Opaque chrome, region only.** The previous decision. Hard, stepped edges at every page corner and overlay corner, toasts that cannot fade, and no translucency.
- **Transparency instead of the region.** Clicks do not pass through transparent pixels, so the region is needed regardless.
- **Blurred popovers over a capture of the page.** A popover stays up while the page scrolls or plays, so a blur of a still capture visibly goes stale. The address card's frosting is accepted because its backdrop already freezes the page while it is up.

#### What was measured

Measured on 2026-11-03 against `tauri 2.12.5` / `wry 0.57.0` / WebView2 runtime `156.0.4314.58`, Windows 11 build 26300, one GPU and driver, at 100% scaling. Screen pixels were read from the desktop, not from `PrintWindow`. The test window had two Tauri webviews, the chrome raised over the content with `SetWindowPos(HWND_TOP)` as Haku does.

| Question                                                    | Result                                                                                                                                                                                                                                                 | Where it stops                                                  |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------- |
| Does a transparent chrome webview show the page beneath it? | Yes, with Tauri's `transparent(true)` or a background colour of alpha 0. A transparent pixel read exactly the page's colour, and a `rgba(255,0,0,0.5)` panel blended exactly as expected over it. The page repainting under the chrome showed at once. | Nothing on this stack                                           |
| Can the webview background be partly transparent?           | No. `SetDefaultBackgroundColor` accepts only alpha 0 or 255 (`E_INVALIDARG` otherwise), and wry clamps any other alpha to 255. CSS `rgba` on elements gives any alpha, so this costs nothing.                                                          | WebView2, then wry                                              |
| Can `backdrop-filter` blur the page?                        | No. It blurs only what the chrome's own document painted.                                                                                                                                                                                              | Each webview is a separate document rendered in its own surface |
| Can CSS blur the page any other way?                        | No. `filter`, and SVG filters such as `feGaussianBlur`, take the element's own rendering as their input. SVG's `BackgroundImage` input is not implemented in Chromium, and would still see only the chrome's document.                                 | Same boundary                                                   |
| Do clicks pass through the transparent chrome?              | No. Clicks went to the chrome until a region hole was cut.                                                                                                                                                                                             | Native HWND hit testing, which the region already handles       |
| What breaks the transparency?                               | `WS_CLIPSIBLINGS` on the content webview's container: the chrome then shows the window's background instead of the page. wry does not set it. Making the chrome a layered window does not help: a global alpha fades opaque elements too.              | Win32 clipping                                                  |

Measured on 2026-11-08 in Haku itself, same machine (AMD Radeon RX 5500 XT), before adopting transparency:

- **Video under a translucent overlay.** A `rgba(255,0,0,0.5)` menu over playing VP9 and H.264 video blended exactly with each frame, and changed with the frames; no hardware overlay plane bypassed the chrome.
- **150% scaling.** The same blend was exact, and the page's antialiased corners and straight edges showed no fringe, including at fractional edges.

Still untested: other GPU vendors and hybrid graphics; live window resizing; older WebView2 runtimes.

#### What blur would take

Blurring the live page needs a native effect under the interface, because no document can see another's pixels. Haku blurs a still image of the page instead: the [address bar's card](address-bar.md) draws its backdrop over captures taken with `capture_page`, so a playing video looks paused while it is up, and the card is frosted with `backdrop-filter` over that backdrop, which is in the chrome's own document. That suits a surface that is up briefly and covers the window, not one that stays over a page that keeps changing. Two ways to blur the live page were tried outside Haku, on the same machine:

- **Composition hosting.** WebView2 renders into a DirectComposition visual instead of a child HWND (`CreateCoreWebView2CompositionController`). Two webviews in one visual tree blended correctly, and a native blur could sit between them; that blur was not tried. The host then owns input: mouse, pointer, cursor and focus are forwarded by hand. wry creates only windowed controllers (`src/webview2/mod.rs`, around line 398), so this needs wry first. [wry#1762](https://github.com/tauri-apps/wry/pull/1762) adds composition hosting as an opt-in, aimed at native content under a single webview rather than stacked webviews; it was open and unreviewed on 2026-11-03.
- **A transparent owned popup window with the system backdrop** (`DWMSBT_TRANSIENTWINDOW`). The page beneath was really blurred. But clicking the popup deactivated the main window, which `WS_EX_NOACTIVATE` did not prevent, and while inactive the backdrop falls back to flat gray. The popup did not follow the main window without being moved on each move and resize, and the main window did not clip it.

Until one of these is available, nothing over the live page is blurred.

#### Flat surfaces over the page

Menus, popovers, panels and toasts over the live page are opaque: they use the `chrome-overlay` surface, which is the raised surface with no transparency. A surface that is translucent is an exception and says why where it is drawn, as the address card's frosting over its still backdrop does.

Decided by the maintainer on 2026-11-11, reversing the translucent default chosen with transparency on 2026-11-08. That choice weighed translucency as what placed a surface over the page. It did not weigh legibility: without a blur, a busy or high-contrast page shows through as sharp shapes behind the text, and menus and the bookmark editor were hard to read over such pages. The border and rounded corners already place a surface over the page.

Rejected:

- **Translucent by default, at 88% opacity.** The previous default, and less legible over busy pages.
- **Translucent only over calm pages.** Haku cannot see the page's pixels, so it cannot tell which pages are calm.

Transparency itself stays: it is what antialiases the page's corners and the overlays' edges, and what lets a toast fade.

### Rounded page corners

The page is a native view, so CSS cannot clip it. The hole is cut in the region instead, and the chrome paints the corners over the page: the hole leaves out only the pixels wholly inside the curve, so the chrome keeps every pixel the curve passes through, and its surround paints them antialiased, blending with the page beneath because the chrome is transparent.

The radius comes from the interface, which reads it back from its own stylesheet with `getComputedStyle`, so `--haku-viewport-radius` stays the one place that number is written. Region edges are hard, so the region must follow the curve CSS draws, computed from the box's device-pixel bounds, on all four sides. `CreateRoundRectRgn` does not do that. Its curve steps unevenly and sits up to two pixels inside the drawn one, it leaves out the last column and row, and given bounds already rounded to whole pixels it shifts the curve by up to half a pixel a different way on each side, which showed as one smooth corner beside a jagged one. The region is built from the shape's rows instead (`RoundedRect::spans`), only for rows inside the window.

A hole's bounds are the page's own: Tauri places the page by rounding its origin and its size apart, so its far edge can lie a pixel from the exact one, and a hole cut at the exact edge would leave that pixel showing neither page nor chrome. An overlay has no native view to agree with, so its bounds stay exact.

Overlays are rounded the same way, from the other side. Each one is reported with the corner radius its own style computes, and added back to the region with the same rounded shape; a square patch would paint chrome over the page at a rounded overlay's corners.

Both take every pixel the curve passes through for the chrome (`Edge` in `platform`): a hole leaves out only the pixels wholly inside it (`Edge::Whole`), and an overlay adds every pixel it covers at all (`Edge::Touched`). The chrome then owns each partly covered pixel and paints it antialiased over the page. Left to the page instead, those pixels give an edge that is hard; before the chrome was transparent, that was the better choice for an overlay, because the chrome blended its edge with its own background and showed a dark, stepped fringe over a light page.

## The contract with the interface

Each assigned viewport has a stable window-local id owned by Rust. [Split view slice 4b](split-view.md#implementation-slices) shows one viewport for an unpaired tab or two for a selected pair in a normal window; popups retain one. Each pane has a content element in `features/viewport`. `useChromeLayout` measures all panes and registered overlays and sends the entire report in a **single** async `set_layout({ layout })` call:

```ts
type Layout = {
  revision: number; // BrowserState.layoutRevision being rendered
  viewports: { id: string; rect: Viewport; covered: boolean; radius: number }[];
  overlays: Overlay[];
};
```

Coordinates and radii are in chrome CSS pixels. Every assignment is reported, including internal/crashed pages and preview covers (`covered: true`), with real bounds instead of a null rectangle. `BrowserState` exposes `viewports: { id, tab }[]`, `focusedViewport`, `splits: { tabs: [TabId, TabId], ratio }[]` (including background pairs) and `layoutRevision`; `active` is derived from the focused assignment. The revision advances only when the set of panes or committed split ratio changes, including creation of the first pane. Ordinary single-tab replacement keeps the revision because its geometry is still valid; switching pairs replaces pane identities even at equal ratios, while focus-only and background-pair edits preserve the revision; [Split view, D1](split-view.md#decisions) records why.

A report whose revision does not match current state is ignored and returns current state. Before any geometry changes, Rust rejects duplicate/unknown ids, missing assignments, non-finite geometry, negative sizes/radii and intersecting positive-area pane rectangles through the existing error result (`Unsupported`). Negative origins are allowed. Invalid reports apply nothing. Zero-sized panes cut no hole and hide their native surface while retaining assignment/protection and last measured size. As before, subpixel dimensions below one CSS pixel are treated as collapsed. Covered panes cut no hole but keep real bounds and native geometry. A newly assigned slot stays hidden until usable geometry for its current revision arrives.

Placement resolves each visible slot to its current `(window, viewport)`; hidden background slots cut no holes. Every accepted layout updates bounds for all slots in its window: assigned slots use their pane's cached rectangle, and background slots use the focused pane's, retaining the last measured bounds for collapsed panes. The mask subtracts each uncovered rounded pane rectangle, then adds registered rounded overlays back. Cached bounds are keyed by the same window/pane pair.

All panes and overlays travel in one report. Reports are coalesced per animation frame, with one IPC call in flight and only the latest pending layout retained. Divider dragging writes temporary geometry through refs; its ratio is committed once on release.

Both halves travel together deliberately. Setting the webview bounds and the input mask separately would let them disagree for a frame, which shows as a flickering strip along the edge of the page.

Validation, bounds, the input mask and reconciliation share the serialized change path in `state.rs`, so a layout cannot race a tab switch. This does not guarantee atomic painting between native child windows.

`set_layout` also reconciles the browser. A restored session has tabs but no webviews, because until the interface reports its geometry there is nowhere on screen to put one; this is what loads the page the window opens on.

Background slots use the focused pane's last measured rectangle, including when the surviving pane no longer has the initial id. Both layout updates and effect application resolve bounds this way. Visible sibling slots retain their own pane bounds.

### Registering an overlay

Anything that floats over the viewport **must** call `useOverlay`, or the region clips it away and the page shows where it should be. This is the single easiest mistake to make in this codebase.

```tsx
const ref = useRef<HTMLDivElement>(null);
useOverlay(ref, isOpen);
```

`useOverlay` measures the element when it opens, as it resizes, and every frame while a transition or animation on it runs, then once more when it ends: a transform is not a resize, so a bar sliding in would otherwise stay registered where it started. Each measurement is one more layout report, coalesced per frame like the rest, so the region trails a moving element by the frames a report is in flight, and the part of it beyond the last measurement is hidden under the page for that long. Following stops after a second, in case an animation never reports its end.

An element that animates away once it closes stays registered, followed the same way, until its animation ends or the second runs out, so it leaves visibly rather than vanishing under the page. One that closes without animating is released at once.

A cover over the whole viewport needs no overlay of its own. `useChromeLayout` measures all assigned pane surfaces together, including each pane's covered state. Internal pages, crashed-page notices and previews keep only their own pane solid. Split pane controls and the 5 CSS-pixel divider sit outside the measured page holes. The whole-interface failure screen covers every assignment. A preview over a reloading page uses the same per-pane cover ([Tab optimization § Previews](tab-optimization.md#previews)).

Toasts and dialogs are the exception that proves the rule: they are created by `@codenhub/toaster`, not by a component, so there is no ref to hand `useOverlay`. `features/feedback` gives the toaster a container of its own and registers each toast and open dialog in it as they appear, resize and animate. A toast is registered where it is painted, with its own corners, not as its stack, which is square and spans the gaps between toasts. Toasts slide without changing the DOM, so they are measured every frame while any is shown. A toast fades as it comes and goes: the chrome is transparent, so it blends with the page beneath while it does. Shown, its surface is opaque, as [every surface over the page is](#flat-surfaces-over-the-page). Show notifications through `feedback()` and they are handled; a second toaster instance, or `alert()`, would not be.

## Platforms

Windows is implemented. macOS and Linux are `unimplemented` stubs behind the same interface, failing with `HakuError::Unsupported` rather than silently doing nothing, so a missing platform surfaces as a visible error instead of a subtly broken window.

- **macOS**: reorder with `addSubview:positioned:relativeTo:` on the `WKWebView`; mask with a `CAShapeLayer`.
- **Linux**: restack the GTK child; shape with `gdk_window_shape_combine_region`.

Both reach their handles through the same `Webview::with_webview` hook.

## Evidence

The mechanism was proven on Windows before the architecture was committed to, using the OS's own hit test (`WindowFromPoint`) and a direct region query (`GetWindowRgn` + `PtInRegion`):

| State                    | z-order            | viewport centre          | chrome bar             |
| ------------------------ | ------------------ | ------------------------ | ---------------------- |
| Before raising           | `CONTENT > CHROME` | —                        | —                      |
| Raised, no mask          | `CHROME > CONTENT` | chrome takes the click   | chrome takes the click |
| Raised + mask            | `CHROME > CONTENT` | falls through to content | chrome takes the click |
| Mask cleared             | `CHROME > CONTENT` | chrome takes the click   | chrome takes the click |
| After the page navigates | `CHROME > CONTENT` | falls through to content | chrome takes the click |

The last row matters most: a content webview does not re-raise itself when it navigates, so the raise is stable and does not need re-applying on every page load.
