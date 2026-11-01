---
status: IMPLEMENTED
last_updated: 2026-10-31
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

### Why not transparency

The obvious alternative — a transparent chrome webview on top — depends on two sibling child HWNDs alpha-compositing, which Windows does not do reliably. The region approach needs no compositing at all: the chrome is either present at a pixel or absent.

The cost is that anything drawn over the page must be opaque, and must register itself. That is what `features/overlays` is for.

### Rounded page corners

The page is a native view, so CSS cannot clip it. The hole is cut in the region instead: the chrome keeps painting the corners and the page shows through the curve.

The radius comes from the interface, which reads it back from its own stylesheet with `getComputedStyle`, so `--haku-viewport-radius` stays the one place that number is written. Region edges are hard rather than antialiased; softening them would mean giving up the region approach for one that depends on webview transparency. What the hard edge can do is follow the curve CSS draws: a pixel is in the region when its centre lies inside the rounded box, computed from the box's exact device-pixel bounds, so the edge runs through the middle of the antialiased one on all four sides. `CreateRoundRectRgn` does not do that. Its curve steps unevenly and sits up to two pixels inside the drawn one, it leaves out the last column and row, and given bounds already rounded to whole pixels it shifts the curve by up to half a pixel a different way on each side, which showed as one smooth corner beside a jagged one. The region is built from the shape's rows instead (`RoundedRect::spans`).

Overlays are rounded the same way, from the other side. Each one is reported with the corner radius its own style computes, and added back to the region with the same rounded shape; a square patch would paint chrome over the page at a rounded overlay's corners.

An overlay takes only the pixels it covers wholly, where a hole takes those whose centre it covers (`Edge` in `platform`). The chrome paints an overlay over its own background, not over the page, so a pixel an overlay's antialiased edge only partly covers is blended with that background. Over a light page those pixels showed as a dark, stepped fringe along a toast's corners. Left to the page, they give an edge that is hard but the right colour. A hole needs no such care: the chrome paints outside its curve and blends its corner with its own colour.

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

Anything that floats over the viewport **must** call `useOverlay`, or it will paint correctly and be completely unclickable. This is the single easiest mistake to make in this codebase.

```tsx
const ref = useRef<HTMLDivElement>(null);
useOverlay(ref, isOpen);
```

`useOverlay` measures the element when it opens, as it resizes, and every frame while a transition or animation on it runs, then once more when it ends: a transform is not a resize, so a bar sliding in would otherwise stay registered where it started. Each measurement is one more layout report, coalesced per frame like the rest, so the region trails a moving element by the frames a report is in flight, and the part of it beyond the last measurement is hidden under the page for that long. Following stops after a second, in case an animation never reports its end.

An element that animates away once it closes stays registered, followed the same way, until its animation ends or the second runs out, so it leaves visibly rather than vanishing under the page. One that closes without animating is released at once.

A cover over the whole viewport needs no overlay of its own. `useChromeLayout` measures all assigned pane surfaces together, including each pane's covered state. Internal pages, crashed-page notices and previews keep only their own pane solid. Split pane controls and the 5 CSS-pixel divider sit outside the measured page holes. The whole-interface failure screen covers every assignment. A preview over a reloading page uses the same per-pane cover ([Tab optimization § Previews](tab-optimization.md#previews)).

Toasts and dialogs are the exception that proves the rule: they are created by `@codenhub/toaster`, not by a component, so there is no ref to hand `useOverlay`. `features/feedback` gives the toaster a container of its own and registers each toast and open dialog in it as they appear, resize and animate. A toast is registered where it is painted, with its own corners, not as its stack, which is square and spans the gaps between toasts. Toasts slide without changing the DOM, so they are measured every frame while any is shown, and they do not fade: the region under a toast shows the chrome, not the page, so a fading toast would blend with the chrome. Show notifications through `feedback()` and they are handled; a second toaster instance, or `alert()`, would not be.

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
