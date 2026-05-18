# AGENTS.md

Codebase reference for AI agents working on Haku.

## Project

Haku is a lightweight desktop browser built with Tauri v2 (Rust backend) and a TypeScript/Vite/TailwindCSS frontend.

## Commands

```bash
pnpm dev          # Vite dev server on :1420
pnpm build        # tsc + vite build → dist/
pnpm typecheck    # TypeScript type checking
pnpm test         # Vitest single run
pnpm test <file>  # Test a single file
pnpm test:watch   # Vitest watch mode
pnpm lint:check   # Prettier --check
pnpm lint:fix     # Prettier --write
pnpm tauri        # Tauri CLI
```

## Directory structure

```
haku/
├── src/
│   ├── index.html          # Entrypoint
│   ├── core/               # Business logic only (no UI)
│   │   └── tabs/           # Tab management feature
│   │       ├── index.ts    # Public exports
│   │       ├── state.ts    # Tab state (pure functions)
│   │       ├── state.test.ts
│   │       └── webview.ts  # Tauri command wrappers
│   ├── shared/             # Reusable code, no cross-sibling imports
│   │   ├── constants/      # DEFAULT_TAB_URL, SEARCH_URL, etc.
│   │   ├── tauri/          # invoke, listen, getCurrentWindow re-exports
│   │   └── types/          # Domain types (Tab, ManagedTab, TabState, etc.)
│   └── ui/
│       ├── scripts/        # Glue: user interactions ↔ core logic
│       │   ├── index.ts    # DOMContentLoaded → initTabs + initWindowControls
│       │   ├── tabs/       # Tab event handlers and DOM rendering
│       │   └── window-controls.ts
│       └── styles/
│           ├── index.css   # Entry (imports tailwind + components + theme)
│           ├── components.css
│           └── theme.css   # CSS variables, light/dark
├── tauri/                  # Rust/Cargo desktop integration
│   └── src/
│       ├── lib.rs          # Tauri commands (6 total)
│       └── main.rs         # Calls haku_lib::run()
├── plugins/vite/icons/     # Vite plugin: <i class="ic-*"> → inline SVG
├── .docs/
│   ├── architecture.md
│   └── guidelines.md
├── vite.config.ts
├── vitest.config.ts
└── tsconfig.json           # Path aliases: @core/*, @shared/*, @ui/*
```

## Architecture

Dependency direction is strictly one-way:

```
index.html → ui → core → shared
                         └── (types & constants: import nothing)
```

**Hard rules:**

- Types and constants depend on nothing — they are leaves.
- No circular dependencies, ever.
- `core/` = pure logic, no UI. `ui/` = UI only, delegates logic to core. `shared/` = reusable utilities with no behavior.
- Each folder under `core/` handles one feature. Expose via `index.ts` barrel.

## Tauri commands (Rust → TS bridge)

Defined in `tauri/src/lib.rs`, invoked from `src/core/tabs/webview.ts`:

| Command                                | Purpose                                     |
| -------------------------------------- | ------------------------------------------- |
| `ensure_browser_webview(url, bounds)`  | Create or reposition the browser webview    |
| `set_browser_bounds(bounds)`           | Reposition webview                          |
| `navigate_browser(url)`                | Navigate webview to URL                     |
| `reload_browser()`                     | Reload current page                         |
| `open_browser_devtools()`              | Open devtools                               |
| `update_tab_info(title, favicon, url)` | Update tab metadata, emits `tab-info` event |

## Key types (`@shared/types`)

- `TabState` — immutable interface with getters and navigation methods
- `ManagedTab` — internal tab with history array and index
- `Website` — single history entry (`title`, `url`, `favicon?`, `order`)
- `BrowserBounds` — rect for webview positioning (`x`, `y`, `width`, `height`)
- `TabInfoEvent` — payload from Tauri `tab-info` event

## Data flow

```
User interaction
  → ui/scripts/tabs/index.ts (event handler)
  → core/tabs/state.ts (pure state update)
  → core/tabs/webview.ts (Tauri invoke)
  → Tauri Rust backend

Tauri backend
  → emit("tab-info", { title, favicon, url })
  → ui listen() handler
  → renderTabs() DOM update
```

## Naming conventions

| Thing                 | Convention               | Example                   |
| --------------------- | ------------------------ | ------------------------- |
| Files & folders       | kebab-case               | `tab-state.ts`            |
| Variables & functions | camelCase                | `createTabState`          |
| Classes & types       | PascalCase               | `ManagedTab`              |
| Constants & enums     | UPPER_SNAKE_CASE         | `DEFAULT_TAB_URL`         |
| Booleans              | is/has/can/should prefix | `canGoBack`               |
| Arrays                | plural nouns             | `tabs`                    |
| Functions             | verb phrases             | `resolveNavigationTarget` |

## Coding conventions

- Prefer `const` over `let`; never `var`
- No magic numbers/strings — extract to named constants
- Functions: 0–1 params ideal, 2 acceptable, 3+ → refactor to typed object
- Self-documenting names over comments; comments explain WHY only
- Single responsibility per function/module
- Small, clearly nameable functions

## Testing

- Test files colocated with source (`.test.ts` suffix)
- One behavior per test, descriptive names
- Test behavior, edge cases, error paths — not just happy paths
- No mocking of internal pure logic; mock only Tauri boundaries

## Code style

Prettier config (`.prettierrc`): 120 printWidth, double quotes, trailing commas. Run `pnpm lint:fix` before committing.

TypeScript strict mode is on. No unused locals or parameters.

## Icons

SVG icons use an `ic-` prefix class on `<i>` elements. The `plugins/vite/icons` Vite plugin inlines them at build time. Define new icons in `plugins/vite/icons/data.ts`.
