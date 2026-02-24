---
phase: 03-simple-commands
plan: 02
subsystem: cli
tags: [rust, subprocess, rokit, wally, selene, stylua, moonwave, std-process]

# Dependency graph
requires:
  - phase: 02-core-services
    provides: sourcemap::generate_sourcemap reused by setup_wally_packages
  - phase: 03-01
    provides: commands/ module skeleton and config infrastructure

provides:
  - install_tools() handler — rokit install + conditional wally install + wally-package-types
  - setup_wally_packages() handler — full clean-reinstall cycle with two sourcemap passes
  - lint() handler — selene + stylua --check with graceful skip when tools absent
  - format_code() handler — stylua in-place with graceful skip
  - docs() handler — moonwave dev gated on docs_enabled config flag

affects: [03-03, 03-04, main.rs wiring plans]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "is_tool_available() helper — Command::new(tool).arg('--version').output() for silent tool check"
    - "Pass-through subprocess — .status() for long-running tools (rokit, wally, stylua, moonwave)"
    - "Graceful tool skip — check availability before invoking, print hint, return Ok"
    - "docs_enabled gate — boolean config flag guards optional feature (docs command)"

key-files:
  created:
    - rust-src/commands/install.rs
    - rust-src/commands/quality.rs
  modified:
    - rust-src/commands/mod.rs

key-decisions:
  - "is_tool_available helper marked #[allow(dead_code)] in install.rs — it's a defined helper pattern from the plan spec, will lose warning when wired in main.rs"
  - "install_tools uses Path::new('wally.toml').exists() gate before running wally — prevents wally install failure on projects without wally"
  - "setup_wally_packages always runs two sourcemap passes — matches Luau setupWallyPackages behavior exactly"
  - "lint() returns Ok even when issues found — lint output is informational, not a fatal error (matches Luau runLinting)"

patterns-established:
  - "Pattern: Graceful tool skip — check availability, print message, return Ok; never error if optional tool missing"
  - "Pattern: .status() for pass-through, .output() for silent capture — Pitfall 4 from RESEARCH.md respected"

requirements-completed: [INST-01, INST-02, INST-03, INST-04, QUAL-01, QUAL-02, QUAL-03, QUAL-04]

# Metrics
duration: 2min
completed: 2026-02-24
---

# Phase 3 Plan 02: Install and Quality Command Handlers Summary

**Subprocess wrapper handlers for rokit/wally install cycle and selene/stylua/moonwave quality tools, with graceful skip when optional tools are absent**

## Performance

- **Duration:** 2 min
- **Started:** 2026-02-24T17:50:25Z
- **Completed:** 2026-02-24T17:52:40Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments

- install_tools runs rokit install, then conditionally wally install + wally-package-types for Packages/ and ServerPackages/
- setup_wally_packages does full clean-reinstall cycle: removes lock/sourcemap/package-dirs, wally install, two rojo sourcemap passes bracketing wally-package-types
- lint runs selene and stylua --check on src, skips gracefully (prints hint) when either or both tools not installed
- format_code runs stylua in-place, skips gracefully with install hint when stylua absent
- docs gates on docs_enabled config flag, launches moonwave dev as blocking pass-through

## Task Commits

Each task was committed atomically:

1. **Task 1: Implement install_tools and setup_wally_packages handlers** - `b81a37a` (feat)
2. **Task 2: Implement lint, format_code, and docs handlers** - `edb7b0d` (feat)

**Plan metadata:** (pending — docs commit)

## Files Created/Modified

- `rust-src/commands/install.rs` - install_tools() and setup_wally_packages() public functions
- `rust-src/commands/quality.rs` - lint(), format_code(), and docs() public functions
- `rust-src/commands/mod.rs` - Added pub mod quality; (pub mod install; added in Task 1)

## Decisions Made

- `is_tool_available` in install.rs marked `#[allow(dead_code)]` — it is specified in the plan as a defined helper pattern. It matches quality.rs's identical helper (which IS used). The warning disappears once main.rs is wired to call install_tools.
- `lint()` returns `Ok(())` even when issues are found — lint output is informational, matching Luau `runLinting` which returns a boolean but does not throw.
- `setup_wally_packages` always runs two sourcemap passes (before and after wally-package-types) — exact match to Luau `setupWallyPackages`.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- install.rs and quality.rs ready to be wired into main.rs match arms
- Both modules exported from commands/ — `use ezpm::commands::install;` / `use ezpm::commands::quality;` works
- All 8 requirements (INST-01 through INST-04, QUAL-01 through QUAL-04) satisfied

---
*Phase: 03-simple-commands*
*Completed: 2026-02-24*
