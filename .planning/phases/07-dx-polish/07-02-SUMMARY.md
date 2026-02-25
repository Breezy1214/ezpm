---
phase: 07-dx-polish
plan: 02
subsystem: cli
tags: [rust, subprocess, tokio, install, menu, wally, exit-codes]

# Dependency graph
requires:
  - phase: 07-01
    provides: format_code(src, check) signature; error_block() output function
  - phase: 06-serve-command
    provides: serve::run(cfg, port) async function dispatched from menu
provides:
  - wally-package-types exit code checking with output::warn on failure (all 4 call sites)
  - Live menu serve dispatch via tokio block_on(serve::run(Some(cfg), None))
  - Menu format dispatch confirmed as format_code(src, false)
affects: [phase 08-integration-tests]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - Non-fatal subprocess warning: capture .status()/.output(), check !success(), pb.suspend(|| output::warn(...))
    - Menu tokio dispatch: new_multi_thread().enable_all().build().context(...)?.block_on(async_fn)

key-files:
  created: []
  modified:
    - rust-src/commands/install.rs
    - rust-src/menu.rs

key-decisions:
  - "wally-package-types failure is non-fatal (output::warn not bail!) — packages installed, only types missing; overall command exits 0"
  - "Menu serve dispatch uses scoped tokio runtime (new_multi_thread) matching main.rs Serve arm pattern; port=None because interactive menu has no port flag"
  - "Menu format call confirmed check=false — interactive menu always formats in-place, never CI check mode"

patterns-established:
  - "Non-fatal subprocess pattern: let result = Command.status()?; if !result.success() { pb.suspend(|| output::warn(...)); }"
  - "Menu async dispatch: scoped tokio runtime per command invocation via block_on"

requirements-completed: [ERR-03]

# Metrics
duration: 2min
completed: 2026-02-25
---

# Phase 7 Plan 02: Subprocess Error Propagation and Menu Serve Dispatch Summary

**wally-package-types exit code checking with non-fatal output::warn across all 4 call sites in install.rs; menu serve stub replaced with live tokio block_on(serve::run) dispatch**

## Performance

- **Duration:** 2 min
- **Started:** 2026-02-25T00:00:00Z
- **Completed:** 2026-02-25T00:02:00Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- All 4 wally-package-types call sites in install.rs now check exit status and print output::warn on failure (non-fatal — install still succeeds)
- Menu "serve" option no longer shows a stub message — it dispatches to the real serve pipeline via tokio runtime block_on
- Menu "format" dispatch confirmed correct at format_code(src, false) from Plan 01

## Task Commits

Each task was committed atomically:

1. **Task 1: Fix wally-package-types exit code handling in install.rs** - `d8ac596` (fix)
2. **Task 2: Replace menu serve stub with tokio dispatch and update format call** - `ccbeed3` (feat)

**Plan metadata:** (docs commit follows)

## Files Created/Modified
- `rust-src/commands/install.rs` - 4 wally-package-types call sites now check exit status; non-fatal output::warn on failure; pb.suspend() used during spinner
- `rust-src/menu.rs` - Serve stub replaced with tokio runtime block_on(serve::run(Some(cfg), None)); Context added to anyhow import

## Decisions Made
- wally-package-types failure is non-fatal (output::warn, not bail!) — packages are installed, only types are missing; overall install/setup-wally-packages command still exits 0
- Menu serve dispatch uses scoped tokio runtime matching main.rs Serve arm pattern; port=None because the interactive menu has no port flag
- Menu format call was already format_code(src, false) from Plan 01 — confirmed correct, no change needed

## Deviations from Plan

None - plan executed exactly as written. The format call was already updated in Plan 01 (the plan noted this as a potential finding, confirmed correct).

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Phase 7 (DX Polish) complete — error_block(), non-zero exit codes, format --check, subprocess exit code propagation, and live menu serve all done
- Phase 8 (Integration Tests) can now proceed — stable binary with full ERR-01, ERR-02, ERR-03 coverage
- ERR-03 requirement fully satisfied: all subprocess calls in install.rs propagate exit codes

---
*Phase: 07-dx-polish*
*Completed: 2026-02-25*
