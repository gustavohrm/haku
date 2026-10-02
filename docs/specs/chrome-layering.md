# Chrome layering

**Status:** IMPLEMENTED
**Last updated:** 2026-10-02
**Scope:** How Haku's interface renders above page content, and how clicks still reach the page.

## The requirement

The browser skeleton must sit above every content webview. Without that, the page surface cannot have rounded
corners and no menu, dropdown or dialog can overlap page content, because a native webview always composites
above the HTML of the window that hosts it.

## What Tauri gives us

Nothing directly. Verified against `tauri 2.10.3` / `wry 0.54.4`:

- `Webview` has **no z-order API**. The surface is `set_bounds`/`size`/`position`, `set_focus`, `hide`/`show`,
  `reparent`, `navigate`, `reload`, `eval`, `set_zoom`, `set_background_color`.
- On Windows, wry gives each webview its own **container HWND** parented to the window, and calls
  `SetWindowPos(hwnd, HWND_TOP, …)` when it creates one. The newest webview is therefore always on top, and the
  window's original webview — Haku's interface — is permanently at the bottom.
- `set_bounds` passes `SWP_NOZORDER`, so repositioning cannot be used to reorder.

The way through is `Webview::with_webview`, which hands out the platform handle. On Windows that is the
`ICoreWebView2Controller`, whose `ParentWindow` is exactly the container HWND wry created.

## The mechanism

Two native operations, both in `platform/`:

1. **Raise.** `SetWindowPos(chrome, HWND_TOP, …)` lifts the interface above every content webview. It must be
   re-applied whenever a content webview is created, because a new one is placed on top.
2. **Mask.** The chrome covers the whole window, so without help it would swallow every click meant for the
   page. `SetWindowRgn` gives it a region equal to the window minus the viewport, plus any overlay currently
   drawn over the page. Where the chrome is not in its region its HWND is not there at all, so clicks,
   scrolling and hover fall through to the content webview beneath.

### Why not transparency

The obvious alternative — a transparent chrome webview on top — depends on two sibling child HWNDs
alpha-compositing, which Windows does not do reliably. The region approach needs no compositing at all: the
chrome is either present at a pixel or absent.

The cost is that anything drawn over the page must be opaque, and must register itself. That is what
`features/overlays` is for.

### Rounded page corners

The page is a native view, so CSS cannot clip it. The hole is cut with `CreateRoundRectRgn` instead: the
chrome keeps painting the corners and the page shows through the curve.

The radius comes from the interface, which reads it back from its own stylesheet with `getComputedStyle`, so
`--haku-viewport-radius` stays the one place that number is written. Region edges are hard rather than
antialiased; at the radius in use this is not noticeable, and softening it would mean giving up the region
approach for one that depends on webview transparency.

Overlays are rounded the same way, from the other side. Each one is reported with the corner radius its own
style computes, and added back to the region with `CreateRoundRectRgn`; a square patch would paint chrome over
the page at a rounded overlay's corners.

## The contract with the interface

One element — `features/viewport` — marks where page content goes. `useChromeLayout` observes its rectangle,
collects any registered overlays, each a rectangle and a corner radius, and pushes both to Rust in a **single**
`set_layout` call.

Both halves travel together deliberately. Setting the webview bounds and the input mask separately would let
them disagree for a frame, which shows as a flickering strip along the edge of the page.

`set_layout` also reconciles the browser. A restored session has tabs but no webviews, because until the
interface reports its geometry there is nowhere on screen to put one; this is what loads the page the window
opens on.

### Registering an overlay

Anything that floats over the viewport **must** call `useOverlay`, or it will paint correctly and be
completely unclickable. This is the single easiest mistake to make in this codebase.

```tsx
const ref = useRef<HTMLDivElement>(null);
useOverlay(ref, isOpen);
```

`useOverlay` measures the element when it opens, as it resizes, and again when a transition or animation on it
ends: a transform is not a resize, so a tooltip scaling up would otherwise stay registered at its starting size.
While it is still animating, the part of it beyond what was measured is hidden under the page.

Toasts and dialogs are the exception that proves the rule: they are created by `@codenhub/toaster`, not by a
component, so there is no ref to hand `useOverlay`. `features/feedback` gives the toaster a container of its
own and registers each toast stack and open dialog in it as they appear, resize and animate. Show
notifications through `feedback()` and they are handled; a second toaster instance, or `alert()`, would not be.

## Platforms

Windows is implemented. macOS and Linux are `unimplemented` stubs behind the same interface, failing with
`HakuError::Unsupported` rather than silently doing nothing, so a missing platform surfaces as a visible error
instead of a subtly broken window.

- **macOS**: reorder with `addSubview:positioned:relativeTo:` on the `WKWebView`; mask with a `CAShapeLayer`.
- **Linux**: restack the GTK child; shape with `gdk_window_shape_combine_region`.

Both reach their handles through the same `Webview::with_webview` hook.

## Evidence

The mechanism was proven on Windows before the architecture was committed to, using the OS's own hit test
(`WindowFromPoint`) and a direct region query (`GetWindowRgn` + `PtInRegion`):

| State                    | z-order            | viewport centre          | chrome bar             |
| ------------------------ | ------------------ | ------------------------ | ---------------------- |
| Before raising           | `CONTENT > CHROME` | —                        | —                      |
| Raised, no mask          | `CHROME > CONTENT` | chrome takes the click   | chrome takes the click |
| Raised + mask            | `CHROME > CONTENT` | falls through to content | chrome takes the click |
| Mask cleared             | `CHROME > CONTENT` | chrome takes the click   | chrome takes the click |
| After the page navigates | `CHROME > CONTENT` | falls through to content | chrome takes the click |

The last row matters most: a content webview does not re-raise itself when it navigates, so the raise is
stable and does not need re-applying on every page load.
