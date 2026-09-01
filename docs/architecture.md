# App Architecture

**Status:** DRAFT
**Last updated:** 2026-05-26

This document explains what goes where in this app and the reason why if not obvious.

## Tauri

The `tauri/` folder contains the default Rust/Cargo project Tauri automatically generates. By default, the folder is named `src-tauri/` but we renamed to follow the app's naming convention and cause it's more clean.

## App

Everything pertinent to the browser's behavior and features should be inside `src/`. Below is a simplified view of the file/folder structure.

```
src/
├── assets/         # Assets, duh
├── core/           # All logic and what makes the app work
├── shared/         # Code that is reusable across its siblings
├── ui/             # Bridge between the user and the core
│   ├── scripts/    # UI behavior
│   └── styles/     # UI appearance
└── index.html      # Home page/entrypoint
```

Following explanations cover better complex structures.

### Core structure (`src/core/`)

Each folder here deals with a single feature. Boudaries and separations of concerns must always be respected. Any code that implements something that directly talks to the user or is reused internally, doesn't belong here.

### Shared structure (`src/shared/`)

Reusable code that can be imported anywhere in the app code.

- `constants/` - Magic values multiple places need to agree on
- `tauri/` - Centralize place for Tauri functions and connections
- `types/` - Types, interfaces and domain shapes

### UI structure (`src/ui/`)

This concerns actual UI behavior and looks. Here's where the appearance from the whole app comes from. If something doesn't directly control HOW the UI behaves or looks, but WHAT the UI should display, it doesn't belong in `src/ui/`.

#### `scripts/`

This is what glues the features and all the core code with the user interactions. While the `src/core/` worries about logic and rules, the `src/ui/scripts/` focus on translating that to the user.

#### `styles/`

The shared CSS that controls the UI. Mostly TailwindCSS, but pure CSS when needed.

- `components.css` - Component and element-specific styles (e.g. buttons, inputs, containers, etc)
- `index.css` - Entry point + generic global styles
- `theme.css` - Tokens and variables + light/dark theme definition

## Boundaries

Below is an idea of the direction that should be followed:

- index.html -> ui -> core
- ui/core -> shared

Repeated code should be avoided, repeated logic should NEVER happen. Unless it's a very isolated edge-case, ALWAYS prefer abstracting the code into a reusable module or shared helper.

### Hard rules

- **Types and constants depend on nothing.** They are the leaves of the graph. Every other folder can import from them. They are data, shape, not behavior.
- **No circular dependencies, EVER.** If two pieces need each other, one of them is in the wrong folder or there's a missing abstraction.
