---
phase: 07-dx-polish
plan: 01
subsystem: output
tags: [owo-colors, anyhow, clap, cli, exit-codes, error-handling]

# Dependency graph
requires:
  - phase: 04-output-foundation
    provides: output module with if_supports_color pattern, ColorChoice, OnceLock config
provides:
  - output::error_block() function with colored Error/Context/Fix labels
  - lint() returns Err with anyhow::bail! on violations (exit code 1 for CI)
  - ezpm format --check mode for CI-compatible format checking
  - Subprocess stdout/stderr passthrough before structured error block in non-verbose mode
affects: [08-integration-tests, Phase 7 Plan 02]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "write!(label, ...) owned-String pattern for multi-modifier owo-colors in eprintln! context"
    - "anyhow::bail! in summary section to convert lint success-path to CI-compatible exit code 1"
    - "Conditional Command::new(...).arg(--check) before src_path for check vs in-place dual mode"
    - "menu.rs format_code(src, false) for interactive in-place format without --check"

key-files:
  created: []
  modified:
    - rust-src/output.rs
    - rust-src/commands/quality.rs
    - rust-src/cli.rs
    - rust-src/main.rs
    - rust-src/menu.rs

key-decisions:
  - "error_block() uses write!(label, ...) to owned String (not chained .red().bold()) — owo-colors chaining creates temporaries in closures causing E0515; single-modifier approach compiles cleanly"
  - "error_block() labels use single color modifier (red/yellow/green) without bold — avoids owo-colors chaining lifetime issue while keeping visual distinction"
  - "format --check uses single Command::new() with conditional .arg('--check') — avoids duplicating verbose/capture branching across both check and non-check paths"
  - "menu.rs interactive format uses check=false — interactive menu triggers in-place formatting, not CI check mode"

patterns-established:
  - "write!(mut_string, '{}', value.if_supports_color(...)) for colored owned-String labels in output module"
  - "lint() and format_code() both return Err via anyhow::bail! on tool violations — consistent CI-exit-code pattern across quality commands"

requirements-completed: [ERR-01, ERR-02]

# Metrics
duration: 3min
completed: 2026-02-25
---

# Phase 7 Plan 01: DX Polish — Error Block and Exit Codes Summary

**Structured error_block() output function with colored Error/Context/Fix labels, lint() bail-on-violation for CI exit code 1, and ezpm format --check flag for unformatted-file detection without rewriting**

## Performance

- **Duration:** 3 min
- **Started:** 2026-02-25T07:05:38Z
- **Completed:** 2026-02-25T07:08:14Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments

- Added `output::error_block(error, context, fix)` to output.rs using `write!` to owned String to avoid owo-colors E0515 chaining limitation
- Fixed `lint()` to `anyhow::bail!("lint found violations")` on issues — CI pipelines now receive exit code 1 when selene or stylua find violations
- Added `--check` flag to Format subcommand in cli.rs; format_code() accepts `check: bool` and passes `--check` to stylua conditionally
- Fixed menu.rs to pass `check=false` for interactive in-place format (auto-fix deviation, caught by cargo check)

## Task Commits

Each task was committed atomically:

1. **Task 1: Add error_block() to output.rs and fix lint() exit code with subprocess output passthrough** - `cfa2c6c` (feat)
2. **Task 2: Add --check flag to Format command and update dispatch chain** - `135929f` (feat)

**Plan metadata:** (docs commit — see below)

## Files Created/Modified

- `rust-src/output.rs` - Added `pub fn error_block(error, context, fix)` with colored Error/Context/Fix labels using `write!` to owned String pattern
- `rust-src/commands/quality.rs` - lint() doc comment updated, selene/stylua warn replaced with error_block, stdout/stderr passthrough added in non-verbose path, summary section bails on issues; format_code() gains `check: bool` param with conditional --check arg
- `rust-src/cli.rs` - Format variant changed from unit struct to `Format { check: bool }` with --check arg
- `rust-src/main.rs` - Format dispatch updated to `Commands::Format { check }` with check passed to format_code
- `rust-src/menu.rs` - Interactive format call updated to `format_code(src, false)` (auto-fix)

## Decisions Made

- `error_block()` uses `write!(label, ...)` to an owned String for each label rather than chaining `.red().bold()` in the closure — owo-colors chaining creates temporaries referenced across the closure boundary (E0515). Single-modifier approach (`t.red()`, `t.yellow()`, `t.green()`) compiles cleanly with the `write!` indirection.
- format_code() builds a single `Command::new("stylua")` then conditionally appends `--check` before `src_path` — avoids duplicating the verbose/capture branching across both check and non-check modes.
- Interactive menu "format" option uses `check=false` — the menu triggers in-place reformatting, not CI-check mode.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] owo-colors E0515: chained color modifiers create temporaries in closures**
- **Found during:** Task 1 (adding error_block to output.rs)
- **Issue:** Plan specified `.red().bold()` chaining but owo-colors chained modifiers return references to intermediate temporaries, causing E0515 "cannot return value referencing temporary value"
- **Fix:** Used `write!(label, "{}", "Error:".if_supports_color(..., |t| t.red()))` pattern — single modifier per call, formatted to owned String. Bold omitted to avoid chaining; color alone provides visual distinction.
- **Files modified:** rust-src/output.rs
- **Verification:** `cargo check` passed cleanly after fix
- **Committed in:** `cfa2c6c` (Task 1 commit)

**2. [Rule 1 - Bug] menu.rs format_code() call missing required `check` argument**
- **Found during:** Task 2 (updating format_code signature)
- **Issue:** `rust-src/menu.rs:131` called `format_code(src)` with 1 argument after signature changed to 2; caught by `cargo check`
- **Fix:** Updated call to `format_code(src, false)` — interactive menu uses in-place formatting, not --check mode
- **Files modified:** rust-src/menu.rs
- **Verification:** `cargo check` passed cleanly
- **Committed in:** `135929f` (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (2 Rule 1 bugs)
**Impact on plan:** Both fixes essential for compilation. No scope creep — menu.rs is the expected secondary call site for format_code.

## Issues Encountered

None beyond the two auto-fixed compilation errors above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- ERR-01 (structured error messages) and ERR-02 (non-zero exit codes) requirements satisfied
- `output::error_block()` is available for other commands in Phase 7 Plan 02 (subprocess propagation, menu serve integration)
- CI pipelines can now use `ezpm lint` and `ezpm format --check` as blocking checks
- No blockers for Phase 7 Plan 02 or Phase 8 (Integration Tests)

---
*Phase: 07-dx-polish*
*Completed: 2026-02-25*

## Self-Check: PASSED

- rust-src/output.rs: FOUND
- rust-src/commands/quality.rs: FOUND
- rust-src/cli.rs: FOUND
- rust-src/main.rs: FOUND
- rust-src/menu.rs: FOUND
- Commit cfa2c6c: FOUND
- Commit 135929f: FOUND
