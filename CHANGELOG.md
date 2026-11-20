# Changelog

All notable changes to ezpm are listed here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- `ezpm build` and `ezpm sourcemap` sync `.luaurc`, then run `rojo build` or `rojo sourcemap` on the project with your arguments, and exit with Rojo's code.
- `ezpm where <paths>` prints where each file lands in the game and the `@game/...` require for it.
- `ezpm fix-requires --dry-run` lists every rewrite with its line, and `--check` also exits 1 when there are any, for CI. Both take paths to report on only those files.
- `ezpm check <paths>` reports only the findings about those files or folders. A path that doesn't exist is a usage error, so a typo can't make CI pass.
- `[check] feature_roots` and `shared_features`: `check` reports a feature that requires another feature (`check.featureBoundary`).
- Every problem has a stable diagnostic code, documented in `docs/diagnostics.md`. `ezpm explain <code>` prints the same text, and `ezpm explain --list` lists every code.
- Under GitHub Actions, diagnostics are also written as workflow commands, so they show on the pull request's files.
- A global `--json` flag. Every command prints one JSON document, and `serve --json` prints one event per line, with Rojo's output forwarded as `rojo` events.
- `ezpm alias add <Name> <path> [--create-dir]` and `ezpm alias remove <Name>...` work without prompts.
- `ezpm init` adds rules for coding agents to `AGENTS.md` and imports them into `CLAUDE.md`. `--agent-rules` writes only those files, and `--no-agent-rules` skips them.
- `ezpm hook stop` checks the Luau files an agent changed when it ends its turn and hands new errors back to it. `ezpm init --agent-hook` registers it for Claude Code.
- `ezpm build` and `ezpm sourcemap` in the interactive menu.

### Changed

- Exit codes are 0 for done, 1 for project or tool errors, and 2 for a wrong command line. `serve` exits 130 on Ctrl-C, 143 on SIGTERM and 129 when the terminal closes, instead of 0.
- Unknown keys in `ezpm.toml` suggest the closest known key.
- `check --json` adds the line of each violating require, a `feature_violations` array and a `diagnostics` array. Existing fields are unchanged.
- The VS Code extension puts check diagnostics on the line of the offending require and shows feature violations.
- `ezpm init` puts every file and folder back as it was when it fails or is cancelled part-way through.
- The update check is skipped when `CI` is set.
- Spinners are hidden with `--quiet`.
- A malformed `ezpm.toml` is an error instead of falling back to defaults, which then overwrote `.luaurc`. `init` still tolerates it.
- `.luaurc` writes replace only `aliases` and keep your other keys. Config files are written atomically.

### Fixed

- `serve` notices when Rojo exits on its own and restarts it once, and a panic no longer leaves Rojo running.
- `serve` no longer stops on a watcher error, drops changes when a sourcemap refresh fails, or rewrites requires project-wide when an unrelated file is deleted and another created.
- `check` follows `@game/...` requires, so it no longer passes with zero edges after `fix-requires` has run. It ignores commented-out requires and reports self-requires as cycles.
- `fix-requires` no longer redirects alias requires such as `@Packages/Signal` to a local module of the same name, resolves dotted names like `Button.story`, and never rewrites vendored Wally sources.
- `lint` and `format --check` fail when their tools aren't installed.
- `init` no longer produces `src//client/` from a source directory ending in `/`.

## [0.3.2] - 2026-11-10

### Added

- `ezpm serve` checks that its port is free before starting Rojo, and offers to stop the process holding it or use the next free port.

### Fixed

- Port handling on Windows and Linux.
- Ctrl-C during `serve` startup stops Rojo too.

## [0.3.1] - 2026-09-15

### Removed

- The `ezpm docs` command and Darklua support.

### Changed

- Wally package setup keeps existing directories and unchanged files, so Rojo's watcher isn't disturbed.

### Fixed

- Path normalization for Windows in the sourcemap.

## [0.3.0] - 2026-08-28

### Added

- `[rojo] project` selects which Rojo project ezpm uses. The Rojo sourcemap decides where each file lands.

[Unreleased]: https://github.com/Breezy1214/ezpm/compare/v0.3.2...HEAD
[0.3.2]: https://github.com/Breezy1214/ezpm/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/Breezy1214/ezpm/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/Breezy1214/ezpm/releases/tag/v0.3.0
