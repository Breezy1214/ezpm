# Requirements: EZPM Rust Rewrite

**Defined:** 2026-02-24
**Core Value:** Every current EZPM workflow must work identically (or better) in the Rust version — zero regression on the developer experience that Roblox users depend on.

## v1.1 Requirements

Requirements for v1.1 Dev Server & Polish. Each maps to roadmap phases.

### Dev Server

- [x] **SERVE-01**: User can run `ezpm serve` to start a full build pipeline with Rojo live sync
- [x] **SERVE-02**: File watcher detects changes using OS-native events (not polling) with <100ms latency
- [x] **SERVE-03**: Incremental rebuild on file change — per-file require fixing + DarkLua for .lua/.luau files
- [x] **SERVE-04**: Sourcemap regenerates on file create/remove; meta files copy on init.meta.json change
- [x] **SERVE-05**: Rojo subprocess launches on configurable port (default 34872) from ezpm.toml
- [x] **SERVE-06**: Ctrl-C gracefully terminates all child processes, releases ports, exits cleanly
- [x] **SERVE-07**: Progress spinners display during 8-step initial build sequence

### Output

- [x] **OUT-01**: All terminal output uses colored text with automatic NO_COLOR and non-TTY detection
- [x] **OUT-02**: Centralized output module — no direct println!/eprintln! in command handlers
- [x] **OUT-03**: Progress spinners display during multi-step operations (serve startup, install)

### Errors

- [x] **ERR-01**: Structured error messages include context and suggested fixes for common failures
- [x] **ERR-02**: Non-zero exit codes on lint/format failure for CI compatibility
- [x] **ERR-03**: All subprocess calls propagate exit codes through error handling

### CLI Flags

- [x] **CLI-01**: `--verbose` flag enables detailed output for debugging
- [x] **CLI-02**: `--quiet` flag suppresses non-error output for CI/scripting
- [x] **CLI-03**: `--color Always/Auto/Never` flag overrides automatic color detection

### Testing

- [x] **TEST-01**: Integration tests validate `fix-requires`, `init`, and `alias` command pipelines
- [x] **TEST-02**: Integration tests verify exit code contracts for all commands
- [x] **TEST-03**: CI pipeline runs full test suite (unit + integration) on every PR
- [x] **TEST-04**: Rust build cache in CI for faster pipeline execution

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Dev Server (Advanced)

- **SERVE-08**: Watch mode without Rojo (`ezpm watch` — build only, no sync)
- **SERVE-09**: Multiple concurrent serve instances with named ports
- **SERVE-10**: `fix-requires --dry-run` mode to preview changes without writing

### Extensibility

- **EXT-01**: Plugin hooks for custom build steps in the serve pipeline

## Out of Scope

| Feature | Reason |
|---------|--------|
| HTTP hot-reload server | Requires implementing Roblox plugin protocol; Rojo handles this |
| DarkLua watch mode (long-lived process) | EZPM controls all DarkLua invocations for predictable incremental rebuilds |
| GUI/desktop application | CLI-only tool; interactive menu covers discoverability |
| Package registry | Wally handles this; duplicating fragments the ecosystem |
| Auto-install Rokit | Bootstrap paradox (need Rokit to install EZPM) |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| SERVE-01 | Phase 6 | Complete |
| SERVE-02 | Phase 5 | Complete |
| SERVE-03 | Phase 6 | Complete |
| SERVE-04 | Phase 6 | Complete |
| SERVE-05 | Phase 6 | Complete |
| SERVE-06 | Phase 5 | Complete |
| SERVE-07 | Phase 6 | Complete |
| OUT-01 | Phase 4 | Complete |
| OUT-02 | Phase 4 | Complete |
| OUT-03 | Phase 4 | Complete |
| ERR-01 | Phase 7 | Complete |
| ERR-02 | Phase 7 | Complete |
| ERR-03 | Phase 7 | Complete |
| CLI-01 | Phase 4 | Complete |
| CLI-02 | Phase 4 | Complete |
| CLI-03 | Phase 4 | Complete |
| TEST-01 | Phase 8 | Complete |
| TEST-02 | Phase 8 | Complete |
| TEST-03 | Phase 8 | Complete |
| TEST-04 | Phase 8 | Complete |

**Coverage:**
- v1.1 requirements: 20 total
- Mapped to phases: 20
- Unmapped: 0

---
*Requirements defined: 2026-02-24*
*Last updated: 2026-02-24 — traceability filled during v1.1 roadmap creation*
