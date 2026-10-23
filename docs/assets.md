---
status: IMPLEMENTED
last_updated: 2026-10-23
scope: Brand artwork, the interface and brand fonts under root `assets/`, and the app icons generated from them in `tauri/icons/`.
---

# Assets

This document catalogs the files under `assets/` and explains how to choose and use them. [Brand](guidelines/brand.md) records why they look the way they do. Paths are relative to the repository root.

## Image naming

`light` and `dark` describe the artwork, not the application theme:

- `*-light.svg` is bright artwork. Place it on a dark background.
- `*-dark.svg` is dark artwork. Place it on a bright background.

Choose an image by the background actually behind it, not by a theme name or the user's colour-scheme preference.

## Logos

Logo files are transparent SVG artwork. Keep the aspect ratio when resizing.

| File                             | Artwork                   | Dimensions | Use                                        |
| -------------------------------- | ------------------------- | ---------- | ------------------------------------------ |
| `assets/logo/logo-light.svg`     | Mark and wordmark, bright | 963 x 256  | Full identification on dark backgrounds.   |
| `assets/logo/logo-dark.svg`      | Mark and wordmark, dark   | 963 x 256  | Full identification on bright backgrounds. |
| `assets/logo/wordmark-light.svg` | Wordmark alone, bright    | 691 x 256  | Where the mark already appears nearby.     |
| `assets/logo/wordmark-dark.svg`  | Wordmark alone, dark      | 691 x 256  | As above, on bright backgrounds.           |
| `assets/logo/mark-light.svg`     | Mark, bright              | 256 x 256  | Compact placement.                         |
| `assets/logo/mark-dark.svg`      | Mark, dark                | 256 x 256  | As above, on bright backgrounds.           |

Use the full logo when there is room and the name must be visible, and the mark when there is not:

```html
<img src="/assets/logo/logo-light.svg" alt="Haku" width="241" height="64" />
```

The URL assumes the consumer serves the file at that path; where it goes is the consumer's choice.

### Animated logos

The animated files are self-contained SVG with CSS animations, the same size as their static counterparts. [Brand](guidelines/brand.md#animation) describes the motion.

| File                                       | Animation                           | Dimensions | Use                                     |
| ------------------------------------------ | ----------------------------------- | ---------- | --------------------------------------- |
| `assets/logo/animated/mark-in-light.svg`   | Mark draws in once, 1.2 s, bright   | 256 x 256  | Splash or first appearance on dark.     |
| `assets/logo/animated/mark-in-dark.svg`    | As above, dark                      | 256 x 256  | As above, on bright backgrounds.        |
| `assets/logo/animated/logo-in-light.svg`   | Mark draws in, then wordmark, 1.4 s | 963 x 256  | Full identification appearing, on dark. |
| `assets/logo/animated/logo-in-dark.svg`    | As above, dark                      | 963 x 256  | As above, on bright backgrounds.        |
| `assets/logo/animated/mark-loop-light.svg` | Mark draws, holds, erases; 3.2 s    | 256 x 256  | Loading or waiting states, on dark.     |
| `assets/logo/animated/mark-loop-dark.svg`  | As above, dark                      | 256 x 256  | As above, on bright backgrounds.        |

They animate in an `<img>` tag, as a CSS background, or inline. An in animation plays once, when the image is first displayed, and then stays on the static logo; to replay it, load the file again under a new URL, such as with a changed query string. Every file shows the static logo, without motion, when the viewer's system asks for reduced motion.

```html
<img src="/assets/logo/animated/mark-loop-light.svg" alt="Loading" width="64" height="64" />
```

## App icon and favicons

| File                         | Content                                    | Use                                 |
| ---------------------------- | ------------------------------------------ | ----------------------------------- |
| `assets/icon/app-icon.svg`   | Bright mark on the dark plate, 1024 x 1024 | Source artwork for the app icon.    |
| `assets/icon/app-icon.png`   | `app-icon.svg` rendered at 1024 px         | Input to `tauri icon`.              |
| `assets/favicon/favicon.ico` | `app-icon.svg` at 16, 24, 32, 48, 64, 256  | Default favicon.                    |
| `assets/favicon/favicon.svg` | Same artwork as `app-icon.svg`             | Favicon for browsers that take SVG. |

`favicon.ico` needs no `<link>` tag when it is served at the site root, since browsers request `/favicon.ico` by default. Offer the SVG explicitly:

```html
<link rel="icon" href="/assets/favicon/favicon.svg" type="image/svg+xml" />
```

### Regenerating the app icons

`tauri/icons/` holds the icons the bundler and the window use; `tauri.conf.json` lists them. They are generated from `assets/icon/`:

1. `pnpm tauri icon assets/icon/app-icon.png -o tauri/icons`.
2. Delete what the command adds that Haku does not ship: `tauri/icons/android/`, `tauri/icons/ios/`, and `tauri/icons/64x64.png`.
3. Rebuild `tauri/icons/icon.ico` with the same sizes as `assets/favicon/favicon.ico`, each rendered from `app-icon.svg` at that size rather than scaled down from one large image, and replace `tauri/icons/32x32.png` with `app-icon.svg` rendered at 32 px. A render at the target size keeps the strands and gaps as sharp as the pixel grid allows; `tauri icon` scales one 1024 px source down, which softens them further.

## Fonts

Inter is the interface typeface: the application bundles it and sets it as the default font. Outfit is the brand typeface, for the wordmark and display text on brand surfaces, and the application does not load it. Both are variable fonts covering weights 100 to 900 in one file.

### Inter

| File                                     | Format  |
| ---------------------------------------- | ------- |
| `assets/fonts/inter/InterVariable.woff2` | WOFF2   |
| `assets/fonts/inter/InterVariable.ttf`   | TTF     |
| `assets/fonts/inter/LICENSE.txt`         | License |

These are the upright files of the official Inter 4.1 release, under their upstream names; Haku sets no italic. Besides weight, they carry an optical-size axis (`opsz`, 14 to 32) that browsers apply from the font size on their own. `src/styles/index.css` loads the WOFF2 through Vite, which copies it into the build:

```css
@font-face {
  font-family: "Inter";
  src: url("../../assets/fonts/inter/InterVariable.woff2") format("woff2");
  font-style: normal;
  font-weight: 100 900;
  font-display: block;
}
```

`block` hides text for a short period, about three seconds in Chromium, while the font loads, and falls back to the system stack if it has not loaded by then. Because the file is local, it loads in well under that period, so in practice the chrome paints once, in Inter, instead of painting in the fallback's different widths and then swapping. If the file ever failed to load, the chrome would stay blank for that period before falling back. A web page loading Inter over the network should use `swap` instead.

### Outfit

| File                                        | Format  |
| ------------------------------------------- | ------- |
| `assets/fonts/outfit/Outfit-Variable.woff2` | WOFF2   |
| `assets/fonts/outfit/Outfit-Variable.woff`  | WOFF    |
| `assets/fonts/outfit/Outfit-Variable.ttf`   | TTF     |
| `assets/fonts/outfit/LICENSE.txt`           | License |

Serve WOFF2 with WOFF as a fallback; keep the TTF for tools that need TrueType.

```css
@font-face {
  font-family: "Outfit";
  src:
    url("/assets/fonts/outfit/Outfit-Variable.woff2") format("woff2"),
    url("/assets/fonts/outfit/Outfit-Variable.woff") format("woff");
  font-style: normal;
  font-weight: 100 900;
  font-display: swap;
}
```

### Font licenses

Inter is © 2016 The Inter Project Authors and Outfit is © 2021 The Outfit Project Authors, both under the SIL Open Font License 1.1. Keep each family's `LICENSE.txt` with its files when bundling or redistributing them; the license files hold the complete terms.
