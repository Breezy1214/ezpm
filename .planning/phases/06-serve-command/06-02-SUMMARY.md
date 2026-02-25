---
phase: 06-serve-command
plan: 02
subsystem: cli
tags: [tokio, async, select, file-watcher, process-manager, darklua, sourcemap, require-fixer, meta-copier, incremental-rebuild]

# Dependency graph
requires:
  - phase: 06-01
    provides: serve.rs with 8-step startup, watcher_rx and process_rx receivers ready for select! loop
  - phase: 05-serve-services
    provides: FileWatcher (WatchEvent/FileChange types), ProcessManager (ProcessEvent types)
  - phase: 02-core-services
    provides: darklua_runner::process_file, sourcemap::generate_sourcemap, require_fixer::fix_single_file, meta_copier
provides:
  - Full tokio::select! watch loop in rust-src/commands/serve.rs replacing the temporary ctrl_c().await
  - handle_changes: batch detection (1 vs >1 files), per-file or summary output line
  - handle_lua_change: require_fixer::fix_single_file + spawn_blocking(darklua_runner::process_file) with recovery tracking
  - handle_meta_change: copy single meta file to build equivalent path
  - handle_file_created: spawn_blocking(sourcemap::generate_sourcemap) + optional Lua rebuild
  - handle_file_deleted: build file deletion + spawn_blocking(sourcemap::generate_sourcemap)
  - handle_process_event: Rojo auto-restart once on crash; second crash logs only
  - Non-fatal rebuild errors: WatchEvent::Error and ctrl_c are the only loop-exit conditions
affects: [phase-8-integration-tests]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - tokio::select! 3-arm loop: watcher_rx.recv(), process_rx.recv(), ctrl_c() — fully async event routing
    - spawn_blocking for all subprocess-backed services (sourcemap, darklua) inside async handlers
    - &Path instead of &PathBuf in async function signatures (clippy::ptr_arg compliance)
    - failed_files HashSet<PathBuf> for recovery tracking: remove() returns bool, insert() with to_path_buf()
    - Non-fatal error pattern: rebuild failures call output::error() inline and keep the loop alive

key-files:
  created: []
  modified:
    - rust-src/commands/serve.rs

key-decisions:
  - "&Path instead of &PathBuf for all async handler function signatures — clippy::ptr_arg enforcement; callers auto-deref PathBuf to Path"
  - "Non-fatal rebuild errors — individual LuaChange/MetaChange failures print error inline and continue watching; only WatchEvent::Error exits the loop"
  - "Recovery detection via failed_files.remove(path) — returns bool indicating prior failure; shows distinct 'fixed' message vs normal 'Rebuilt' on next success"
  - "Batch detection threshold: changes.len() > 1 — single-file events get per-file timing line, multi-file batches get a single summary line after all files processed"

patterns-established:
  - "Async handler functions take &Path (not &PathBuf) per clippy::ptr_arg — use path.to_path_buf() for HashSet insertion and spawn_blocking moves"
  - "spawn_blocking required for any std::process::Command service inside async context: darklua_runner::process_file and sourcemap::generate_sourcemap"
  - "require_fixer::fix_single_file called directly (no spawn_blocking) — pure in-process string manipulation, no subprocess, microseconds"

requirements-completed: [SERVE-03, SERVE-04]

# Metrics
duration: 3min
completed: 2026-02-25
---

# Phase 06 Plan 02: Serve Watch Loop Summary

**tokio::select! watch loop with 3-arm event routing: incremental Lua rebuilds via require_fixer + DarkLua, sourcemap regeneration on file create/delete, Rojo auto-restart on crash, and batch detection for multi-file changes**

## Performance

- **Duration:** 3 min
- **Started:** 2026-02-25T04:34:52Z
- **Completed:** 2026-02-25T04:38:00Z
- **Tasks:** 1
- **Files modified:** 1 (rust-src/commands/serve.rs)

## Accomplishments

- Replaced the temporary `tokio::signal::ctrl_c().await` block with a full 3-arm `tokio::select!` loop
- `handle_changes` dispatches each `FileChange` variant to the correct rebuild handler with batch detection (>1 files = single summary line)
- `handle_lua_change` calls `require_fixer::fix_single_file` (sync, in-process) then `spawn_blocking(darklua_runner::process_file)` with recovery tracking via `failed_files` HashSet
- `handle_meta_change` copies the single changed meta file to its build-directory equivalent path
- `handle_file_created` regenerates sourcemap + runs Lua rebuild pipeline if the created file is .lua/.luau
- `handle_file_deleted` deletes the corresponding build file then regenerates sourcemap
- `handle_process_event` implements Rojo auto-restart: restarts once on crash, second crash logs without restart
- `WatchEvent::Error` and Ctrl-C break the loop; all rebuild failures are non-fatal
- `cargo build` and `cargo clippy -- -D warnings` pass; all 63+8 tests pass

## Task Commits

1. **Task 1: tokio::select! watch loop with FileChange event routing** - `b8df6f8` (feat)

**Plan metadata:** (committed in docs commit below)

## Files Created/Modified

- `rust-src/commands/serve.rs` - Full watch loop implementation: 3-arm select!, 5 async handler functions, failed_files tracking, Rojo auto-restart, cleanup on exit

## Decisions Made

- **`&Path` over `&PathBuf` in handler signatures:** clippy::ptr_arg enforces this. Callers pass `&PathBuf` from `FileChange` enum variants; Rust auto-derefs at the call site. `to_path_buf()` used for `HashSet::insert` and `spawn_blocking` moves.
- **Non-fatal rebuild errors:** Individual file rebuild failures call `output::error()` inline and the loop continues. Only `WatchEvent::Error` (OS watcher failure) and Ctrl-C break the loop — matching the CONTEXT.md "serve keeps running on individual rebuild failures" decision.
- **Recovery tracking via `failed_files.remove()`:** `HashSet::remove()` returns `bool` — `true` means the file had previously failed and now succeeds, triggering the distinct `"{file} fixed (Nms)"` message.
- **Batch output threshold:** `changes.len() > 1` flags batch mode. In batch mode, per-handler functions suppress individual output lines; `handle_changes` prints a single `"Rebuilt N files (Nms)"` summary after all handlers complete.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `&PathBuf` changed to `&Path` across all handler signatures**

- **Found during:** Task 1 (clippy pass after cargo build)
- **Issue:** `cargo clippy -- -D warnings` flagged `ptr_arg` warning: handler functions declared `path: &PathBuf` where `&Path` is more idiomatic
- **Fix:** Changed `&PathBuf` to `&Path` in `handle_lua_change`, `handle_meta_change`, `handle_file_created`, `handle_file_deleted`; replaced `path.clone()` with `path.to_path_buf()` for `HashSet::insert` calls and the `spawn_blocking` move
- **Files modified:** rust-src/commands/serve.rs
- **Verification:** `cargo build` and `cargo clippy -- -D warnings` both pass with zero errors
- **Committed in:** `b8df6f8` (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 - Bug)
**Impact on plan:** Required for `cargo clippy -- -D warnings` to pass. No behavior change — Rust auto-derefs `&PathBuf` to `&Path` at call sites. No scope creep.

## Issues Encountered

None beyond the clippy ptr_arg issue documented above.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- `ezpm serve` is fully implemented: 8-step startup + full tokio::select! watch loop + graceful Ctrl-C shutdown
- SERVE-03 (incremental file-change rebuilds) and SERVE-04 (sourcemap/meta file handling) are satisfied
- Phase 7 (DX Polish) and Phase 8 (Integration Tests) can now proceed
- Integration tests can test the full serve lifecycle by running `ezpm serve` as a subprocess and asserting rebuild output on file changes

## Self-Check: PASSED

| Check | Result |
|-------|--------|
| `rust-src/commands/serve.rs` exists | FOUND |
| Commit `b8df6f8` exists | FOUND |
| `cargo build` passes | PASSED |
| `cargo clippy -- -D warnings` passes | PASSED |
| All 63+8 tests pass | PASSED |
| serve.rs contains `tokio::select!` | FOUND |
| serve.rs contains `handle_changes` | FOUND |
| serve.rs contains `handle_process_event` | FOUND |
| serve.rs contains `failed_files` HashSet | FOUND |

---
*Phase: 06-serve-command*
*Completed: 2026-02-25*
