# Security

## Reporting a vulnerability

Report it privately through [GitHub's private vulnerability reporting](https://github.com/gustavohrm/haku/security/advisories/new), not in a public issue. Describe the impact, the commit or version you found it in, and include the smallest reproduction you have.

Haku loads arbitrary remote pages, so the boundary between a page and the application is what matters most: a way for a page to reach the interface, call a command, read another tab's state, or present its content as Haku's own is in scope. [Architecture § Security](docs/architecture.md#security) describes what that boundary is meant to guarantee.

## Supported versions

Haku has no releases yet. Fixes land on `main`.
