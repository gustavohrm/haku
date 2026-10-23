---
status: IMPLEMENTED
last_updated: 2026-10-22
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

| File                               | Artwork                           | Dimensions | Use                                             |
| ---------------------------------- | --------------------------------- | ---------- | ----------------------------------------------- |
| `assets/logo/logo-light.svg`       | Mark and wordmark, bright         | 963 x 256  | Full identification on dark backgrounds.        |
| `assets/logo/logo-dark.svg`        | Mark and wordmark, dark           | 963 x 256  | Full identification on bright backgrounds.      |
| `assets/logo/wordmark-light.svg`   | Wordmark alone, bright            | 691 x 256  | Where the mark already appears nearby.          |
| `assets/logo/wordmark-dark.svg`    | Wordmark alone, dark              | 691 x 256  | As above, on bright backgrounds.                |
| `assets/logo/mark-light.svg`       | Mark, bright                      | 256 x 256  | Compact placement above 32 px.                  |
| `assets/logo/mark-dark.svg`        | Mark, dark                        | 256 x 256  | As above, on bright backgrounds.                |
| `assets/logo/mark-small-light.svg` | Mark with heavier strands, bright | 256 x 256  | 32 px and below, where the regular gaps vanish. |
| `assets/logo/mark-small-dark.svg`  | Mark with heavier strands, dark   | 256 x 256  | As above, on bright backgrounds.                |

Use the full logo when there is room and the name must be visible, and the mark when there is not:

```html
<img src="/assets/logo/logo-light.svg" alt="Haku" width="241" height="64" />
```

The URL assumes the consumer serves the file at that path; where it goes is the consumer's choice.

## App icon and favicons

| File                             | Content                                            | Use                                                 |
| -------------------------------- | -------------------------------------------------- | --------------------------------------------------- |
| `assets/icon/app-icon.svg`       | Bright mark on the dark plate, 1024 x 1024         | Source artwork for the app icon above 32 px.        |
| `assets/icon/app-icon-small.svg` | The small mark on the plate, 1024 x 1024           | Source artwork for the app icon at 32 px and below. |
| `assets/icon/app-icon.png`       | `app-icon.svg` rendered at 1024 px                 | Input to `tauri icon`.                              |
| `assets/favicon/favicon.ico`     | 16, 24, 32, 48, 64 and 256 px; small mark up to 32 | Default favicon.                                    |
| `assets/favicon/favicon.svg`     | The small mark on the plate                        | Favicon for browsers that take SVG.                 |

`favicon.ico` needs no `<link>` tag when it is served at the site root, since browsers request `/favicon.ico` by default. Offer the SVG explicitly:

```html
<link rel="icon" href="/assets/favicon/favicon.svg" type="image/svg+xml" />
```

### Regenerating the app icons

`tauri/icons/` holds the icons the bundler and the window use; `tauri.conf.json` lists them. They are generated from `assets/icon/`:

1. `pnpm tauri icon assets/icon/app-icon.png -o tauri/icons`.
2. Delete what the command adds that Haku does not ship: `tauri/icons/android/`, `tauri/icons/ios/`, and `tauri/icons/64x64.png`.
3. Replace `tauri/icons/32x32.png` with `app-icon-small.svg` rendered at 32 px, and rebuild `tauri/icons/icon.ico` with the same sizes as `assets/favicon/favicon.ico`: `app-icon-small.svg` at 16, 24 and 32 px, `app-icon.svg` above. `tauri icon` scales one source to every size, which puts the regular mark at 16 px, where it blurs.

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
