# Codebase guidelines

**Status:** APPROVED
**Last updated:** 2026-05-26

This document outlines patterns, conventions and guidelines to follow when working on this codebase.

## Naming conventions

- Files & Folders: kebab-case (`user-service.ts`, `api-routes/`)
- Variables & Functions: camelCase (`getUserById`, `isValid`)
- Classes & Types: PascalCase (`UserService`, `ApiResponse`)
- Constants & Enums: UPPER_SNAKE_CASE (`MAX_RETRY_ATTEMPTS`)
- Booleans: prefix with is/has/can/should (`isLoading`, `hasPermission`)
- Arrays: plural nouns (`users`, `items`)
- Functions: verb phrases (`calculateTotal`, `fetchUser`)

## Architecture & design

- Single Responsibility: each function/class does ONE thing well
- High Cohesion: logic that belongs together stays together
- Loose Coupling: minimize dependencies between modules
- Prefer dependency injection over hard-coding
- Prefer using folders to group related files, with an `index.ts` entrypoint

## Coding good/bad practices

- Functions should be small and clearly nameable
- Prefer `const` over `let`; never use `var`
- Never use magic numbers or strings; extract to named constants
- Code should be self-explanatory; use descriptive names
- Design for predictable, deterministic behavior
- Avoid God Objects, Feature Envy, and excessive method chaining.

Function parameters:

- 0-1 parameters: excellent
- 2 parameters: acceptable
- 3+ parameters: refactor into typed object

## Testing

- Test behavior, edge cases, and error paths (not just happy paths).
- One behavior per test. Use descriptive names (e.g., `shouldReturnEmptyWhenNull`).
- Test files are colocated with the code they test.

## Documentation

- Comments explain WHY, not WHAT or HOW.
- Remove and avoid redundant comments, decorations and outdated docs.
- Use JSDoc/TSDoc for public APIs.
