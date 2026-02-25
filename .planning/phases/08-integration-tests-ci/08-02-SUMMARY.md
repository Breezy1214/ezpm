---
phase: 08-integration-tests-ci
plan: 02
subsystem: testing
tags: [integration-tests, rust, serve, github-actions, ci, rust-cache, rokit, cargo-test]

# Dependency graph
requires:
  - phase: 08-01
    provides: tests/common/mod.rs shared helpers, 19-test integration suite baseline
  - phase: 06-serve-command
    provides: compiled ezpm serve command with 8-step startup and "Watching for changes" ready line
provides:
  - tests/serve_test.rs (serve start-wait-kill test, serve-no-config test)
  - .github/workflows/ci.yml (ubuntu-only, Swatinem/rust-cache@v2, Rokit install, cargo test + clippy + fmt)
affects: []

# Tech tracking
tech-stack:
  added:
    - Swatinem/rust-cache@v2 (GitHub Actions caching for ~/.cargo and ./target)
  patterns:
    - "Serve integration test: spawn child with piped stdout, BufReader line scan for ready line, deadline guard, kill+wait"
    - "CI: ubuntu-only single job consolidating test + clippy + fmt to reduce overhead"
    - "Rokit CI install: curl install.sh | bash, echo ~/.rokit/bin >> GITHUB_PATH, rokit install"

key-files:
  created:
    - tests/serve_test.rs
    - .planning/phases/08-integration-tests-ci/08-02-SUMMARY.md
  modified:
    - tests/common/mod.rs
    - tests/lint_test.rs
    - .github/workflows/ci.yml

key-decisions:
  - "Full rokit.toml in create_project(): added rojo, darklua, wally, wally-package-types, selene, lune alongside stylua — serve requires rojo+darklua, lint now also sees selene"
  - "lint_exits_zero_on_clean_code overrides init.luau with return {} — default init.luau has unused_variable (local util = require(...)) which selene now flags since selene is in rokit.toml"
  - "CI uses single ubuntu-only test job (not matrix) — reduces overhead; Swatinem/rust-cache eliminates most compile time"
  - "Branch protection rule rename: new job name is 'test' (was 'build-and-test') — requires one CI run before GitHub branch protection dropdown shows new name"

patterns-established:
  - "Pattern: serve test uses 30s deadline BufReader loop — prevents hang while still having time limit"
  - "Pattern: serve test uses --port 44872 (not default 34872) — avoids conflict with locally running Rojo"
  - "Pattern: child.stdout.take() + BufReader<ChildStdout> for streaming line-by-line ready detection"

requirements-completed: [TEST-02, TEST-03, TEST-04]

# Metrics
duration: 3min
completed: 2026-02-25
---

# Phase 8 Plan 02: Serve Integration Test + CI Pipeline Summary

**Serve start-wait-kill integration test (BufReader line scan + 30s deadline) and ubuntu-only CI with Swatinem/rust-cache@v2 + Rokit toolchain install + cargo test/clippy/fmt quality gates**

## Performance

- **Duration:** 3 min
- **Started:** 2026-02-25T15:54:29Z
- **Completed:** 2026-02-25T15:58:18Z
- **Tasks:** 2
- **Files modified:** 4 (2 created, 2 modified)

## Accomplishments
- Created `tests/serve_test.rs` with `serve_starts_and_shuts_down` (spawn, BufReader line scan for "Watching for changes" ready line, 30s deadline, kill+wait) and `serve_exits_nonzero_without_config` (bare dir test)
- Updated `tests/common/mod.rs` rokit.toml to include all 7 project tools (rojo, darklua, wally, wally-package-types, selene, lune, stylua) — serve requires rojo and darklua to reach the ready line
- Updated `.github/workflows/ci.yml` to ubuntu-only with Swatinem/rust-cache@v2, Rokit install step, and consolidated cargo test + clippy + fmt quality gates (removes 3-platform matrix and separate clippy/fmt jobs)
- Full integration suite: 21 tests across 7 test files, all passing

## Task Commits

Each task was committed atomically:

1. **Task 1: Create serve start-wait-kill integration test** - `71721b3` (feat)
2. **Task 2: Update CI workflow with rust-cache, Rokit, ubuntu-only** - `e267082` (feat)

**Plan metadata:** TBD (docs: complete plan)

## Files Created/Modified
- `tests/serve_test.rs` - Serve start-wait-kill test (BufReader line scan, 30s deadline, --port 44872)
- `tests/common/mod.rs` - Updated rokit.toml in create_project() to include all 7 tools
- `tests/lint_test.rs` - Fixed lint_exits_zero_on_clean_code to override init.luau with `return {}`
- `.github/workflows/ci.yml` - Ubuntu-only CI with rust-cache, Rokit, cargo test/clippy/fmt

## Decisions Made

- **Full rokit.toml in create_project():** The serve test needs rojo and darklua to execute steps 3 (sourcemap via Rojo) and 5 (DarkLua) of the 8-step startup. Adding all tools to rokit.toml now means selene also runs in lint tests — requiring the lint happy-path test to use selene-clean code (`return {}`).

- **lint_exits_zero_on_clean_code fix:** The default `src/client/init.luau` from `create_project()` has `local util = require("src/shared/util")` — selene (now installed) flags this as `unused_variable`. The test overrides this file with `return {}\n` which passes both stylua formatting and selene lint checks.

- **CI ubuntu-only single job:** Consolidates test + clippy + fmt into one job, eliminating the 3-platform matrix and separate clippy/fmt jobs. Swatinem/rust-cache eliminates most compilation time, keeping CI fast.

- **Branch protection rename:** The job was renamed from `build-and-test` to `test`. GitHub branch protection rules require one passing CI run before the new job name appears in the protection dropdown. This is a manual GitHub UI step (Pitfall 6 from RESEARCH.md).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Expanded create_project() rokit.toml to include all tools**
- **Found during:** Task 1 (serve_starts_and_shuts_down test)
- **Issue:** The serve 8-step startup invokes rojo (sourcemap, step 3) and darklua (step 5). create_project()'s rokit.toml only had `stylua`. Rokit shims return "Failed to find tool" error without rokit.toml entries, causing serve to fail at step 3.
- **Fix:** Added rojo@7.6.1, darklua@0.17.3, wally@0.3.2, wally-package-types@1.6.2, selene@0.30.0, lune@0.10.4 to the rokit.toml written by create_project().
- **Files modified:** tests/common/mod.rs
- **Verification:** `serve_starts_and_shuts_down` passes (serve reaches ready line in ~0.5s)
- **Committed in:** 71721b3 (Task 1 commit)

**2. [Rule 1 - Bug] Fixed lint_exits_zero_on_clean_code regression from selene addition**
- **Found during:** Task 1 (running full test suite after common/mod.rs update)
- **Issue:** Adding selene to rokit.toml caused selene to run in lint tests. The default init.luau (`local util = require("src/shared/util")`) triggers selene unused_variable warning → non-zero exit → test failure.
- **Fix:** Override init.luau in `lint_exits_zero_on_clean_code` with `return {}\n` before running lint. This file is both stylua-formatted and selene-clean.
- **Files modified:** tests/lint_test.rs
- **Verification:** All 3 lint tests pass; lint_exits_zero_on_clean_code passes with selene installed
- **Committed in:** 71721b3 (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (1 Rule 2 missing critical, 1 Rule 1 bug)
**Impact on plan:** Both auto-fixes necessary for correctness. Rule 2 fix enabled serve test to work; Rule 1 fix restored lint test correctness after Rule 2 change. No scope creep.

## Issues Encountered
- serve_starts_and_shuts_down timed out initially because rokit.toml lacked rojo/darklua — serve exited at step 3 (sourcemap). Fixed by expanding rokit.toml in create_project().
- lint_exits_zero_on_clean_code regressed after adding selene to rokit.toml — selene flagged the unused `util` variable in the default init.luau. Fixed by using `return {}` in the lint happy-path test.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Full integration test suite: 21 tests, 0 failures (serve, fix-requires, init, alias, format, lint, install)
- CI workflow ready to run on PR — requires one passing CI run to update GitHub branch protection rule names
- Phase 8 complete — all integration tests and CI pipeline in place

## Self-Check: PASSED

- FOUND: tests/serve_test.rs
- FOUND: .github/workflows/ci.yml
- FOUND: tests/common/mod.rs
- FOUND: tests/lint_test.rs
- FOUND: commit 71721b3 (Task 1)
- FOUND: commit e267082 (Task 2)

---
*Phase: 08-integration-tests-ci*
*Completed: 2026-02-25*
