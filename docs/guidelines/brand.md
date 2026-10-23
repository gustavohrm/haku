---
status: IMPLEMENTED
last_updated: 2026-10-22
scope: Haku's name, logo, wordmark, colours, and typefaces, and why each was chosen.
---

# Brand

This guideline records Haku's brand decisions and how to apply them. [Assets](../assets.md) catalogs the files that carry them and how to use each one.

## Name

The product is called **Haku**, pronounced HAH-koo and written with a capital H in prose. The repository, the package, and the Tauri `productName` use it, and the bundle identifier is `com.hakubrowser.app`.

### What it means

The name was coined from Japanese as **ha**yai (速い, "fast") + **ku**romu (クローム, "Chrome"). That coinage is history, not the story the name tells: nobody can recover it from the word, "fast" is a claim every browser makes, and "Chrome" names a competitor's trademark and one engine.

The story the name tells is one the word already carries:

- **Hawaiian _haku_**: to arrange, to put in order, to braid, as in braiding a lei ([Wehewehe Wikiwiki](https://wehe.colo.hawaii.edu/?q=haku)). Many tabs braided through a few webviews is how Haku works; [Webview pool](../specs/webview-pool.md) describes it. This is the primary story.
- **Japanese 白 (_haku_)**: white, blank. It suits a light, monochrome browser.

### Why it was kept

Haku was judged against new candidates on 2026-10-22 and kept. It is two syllables, spelled one way, easy to say across languages, and no browser uses the name.

Its known weaknesses, accepted with the name:

- **Haiku.** Haiku OS ships its own browser, and the two names are one letter apart and easy to mishear.
- **Japanese 吐く (_haku_)** means "to vomit" or "to spit out". It is a homophone; 白 is the stronger reading of the name alone.
- **Search noise.** Characters in _Spirited Away_ and _Naruto_ share the name; "Haku browser" searches cleanly.
- **Haku 2**, a small Mac and browser automation tool for AI agents, is in a nearby field.
- **Trademark.** A "Haku, Inc." holds marks for search-engine software. Their status was not checked, and nothing here is legal clearance.

Rejected alternatives:

| Name   | Meaning                                      | Why it lost                                                                      |
| ------ | -------------------------------------------- | -------------------------------------------------------------------------------- |
| Karumi | 軽み, "lightness", Bashō's late poetic ideal | Shares its name with a Madrid software consultancy; .com and .app taken.         |
| Omote  | 面, a Noh mask: one actor, many roles        | English readers say "oh-mote"; omote.dev is a live product.                      |
| Washi  | 和紙, Japanese paper, light and strong       | Reads as "washy" in English; weaker story than Haku's.                           |
| Others | Kumo, Kiri, Tabi, Balsa, Loupe, Hane, Mayfly | A browser or desktop app already uses the name, or the domains or spelling fail. |

## Logo

### Mark

The mark is the **lei knot**: two closed strands braided through each other, crossing four times and alternating over and under. It draws the name's story — _haku_, to braid — and Haku's mechanism, many tabs woven through a few webviews.

Its geometry is fixed, in units of a 100-unit box centred on the knot:

- Each strand's centreline is the polar curve r(θ) = 28 ± 11 · sin 2θ; one strand takes `+`, the other `−`. They cross where sin 2θ = 0.
- Strands are 11 units wide. At each crossing the strand underneath is cut by a gap of 3 units on either side of the strand on top, and outer corners are softened by 0.8 units.
- The gaps are real cut-outs, not strokes in a background colour, so the mark works on any background.
- A **small variant** (`mark-small-*`) is for 32 px and below: strands 15 units wide, gaps 5.5. The regular mark's gaps shrink under a pixel at 16 px and the knot reads as a blob.

Keep the strands at or below the small variant's weight. The knot has four-fold rotational symmetry; heavier strokes with wide gaps turn its inner negative space into a hooked cross.

Rejected marks, from the exploration of 2026-10-22:

| Mark        | Idea                                                | Why it lost                                                                 |
| ----------- | --------------------------------------------------- | --------------------------------------------------------------------------- |
| White hole  | A white disc with a symmetrical cut and tail on top | A circle broken at the top by a vertical line is the standard power symbol. |
| Drop        | The white hole with its tail fused to the disc      | Reads as a generic water drop and tells nothing about Haku.                 |
| Lei rosette | The same braid with six crossings                   | Close to OpenAI's interlaced mark, and muddy at small sizes.                |

### App icon and favicon

The app icon and favicon are the bright mark on a dark plate: `#0a0a0a`, corners rounded to 22.5% of the side, the mark at 62% of the side (74% for the small variant at 32 px and below). A bare mark disappears on one of the two taskbar themes, white on light or black on dark; the plate keeps it visible on both, and dark is the brand's primary look.

## Wordmark

The wordmark is lowercase **haku**, set in Outfit Medium (weight 500), tracked to −0.02 em with the font's kerning, and converted to outlines so it never depends on the font being installed. Lowercase matches the round, soft mark; capitals would add a stiff vertical to a word that is otherwise all bowls and arches.

In the full logo, the mark sits to the left of the wordmark at 1.12 times the height of its ascenders, centred on them, with a gap of 0.32 times the mark's height.

## Colours

Haku is monochrome. The interface takes its greys from the `@codenhub/styles` palette and adds none of its own ([Code guidelines](code.md#styling)); the brand uses the same values. The palette is defined in OKLCH, and those values are the source; the hex values are their sRGB conversions, for tools that take only hex. Every colour has zero chroma, which is what makes the palette monochrome.

| Role           | Dark                           | Light                          | Palette step          |
| -------------- | ------------------------------ | ------------------------------ | --------------------- |
| Ground         | `oklch(14.5% 0 0)` · `#0a0a0a` | `oklch(98.5% 0 0)` · `#fafafa` | `neutral-950`, `-50`  |
| Chrome         | `oklch(20.5% 0 0)` · `#171717` | `oklch(97% 0 0)` · `#f5f5f5`   | `neutral-900`, `-100` |
| Raised         | `oklch(26.9% 0 0)` · `#262626` | `oklch(92.2% 0 0)` · `#e5e5e5` | `neutral-800`, `-200` |
| Secondary text | `oklch(70.8% 0 0)` · `#a1a1a1` | `oklch(37.1% 0 0)` · `#404040` | `neutral-400`, `-700` |
| Text and mark  | `oklch(98.5% 0 0)` · `#fafafa` | `oklch(14.5% 0 0)` · `#0a0a0a` | `neutral-50`, `-950`  |

Dark is the brand's primary look: screenshots, the app icon, and anything presented first are dark, with the bright mark. The application's theme setting is separate and defaults to following the operating system.

## Typefaces

- **Interface: Inter**, bundled with the application and set as `--font-default` in `src/styles/index.css`, with the system stack as the fallback.
- **Brand: Outfit**, for the wordmark and for display text on brand surfaces such as a website or release notes. It is a geometric sans whose round bowls echo the mark. It is not used for interface text: its `I`, `l` and `1` are nearly identical at small sizes.

Both are licensed under the SIL Open Font License 1.1.

### Interface typeface

The interface's typeface is bundled rather than taken from the system because the chrome is sized to its font: the bars are 1.75 rem tall at a 13 px base. The system stack gives Segoe UI on Windows, SF Pro on macOS, and whatever a Linux distribution ships, often the much wider DejaVu Sans, so tab titles would truncate and controls align differently on each platform. A bundled font measures the same everywhere, and it loads from the application, not the network.

Inter was chosen for being designed for interfaces at small sizes, for covering Latin, Cyrillic and Greek, and for its tabular figures. Coden's other products use it too. It is the variable font, one 352 KB file covering every weight. Its `I` and `l` are both plain bars; Inter's `cv05` feature gives the `l` a tail where telling them apart matters.

| Typeface     | Why it lost                                                                                           |
| ------------ | ----------------------------------------------------------------------------------------------------- |
| System stack | Different fonts and widths on each platform, which the compact chrome cannot absorb.                  |
| Segoe UI     | Microsoft's license does not allow redistributing it, and it does not exist off Windows.              |
| Open Sans    | No advantage over Inter at interface sizes, and dated.                                                |
| Poppins      | Wide, with near-identical `I`, `l` and `1`, and geometric like Outfit, so the pair reads as one face. |

### Brand typeface

Rejected brand typefaces:

| Typeface            | Why it lost                                                                  |
| ------------------- | ---------------------------------------------------------------------------- |
| Geist               | Clean, but it is Vercel's identity and already common.                       |
| Instrument Sans     | Narrow, and its `I`, `l` and `1` are hard to tell apart.                     |
| Hanken Grotesk      | Too neutral to carry an identity.                                            |
| Zen Kaku Gothic New | Plain Latin letters; worth revisiting for a Japanese lockup with ハク or 白. |
