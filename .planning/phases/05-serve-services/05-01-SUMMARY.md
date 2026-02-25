---
phase: 05-serve-services
plan: 01
subsystem: infra
tags: [tokio, nix, process-manager, async, sigterm, sigkill, process-groups]

# Dependency graph
requires:
  - phase: 04-output-foundation
    provides: output module (verbose_line, is_verbose, ColorChoice) used for process lifecycle logging
provides:
  - ProcessManager struct with spawn(), kill_all(), and Drop in rust-src/services/process_manager.rs
  - ProcessEvent enum (Started, Exited, Crashed) via tokio mpsc channel
  - tokio, tokio-util, nix, notify-debouncer-full dependencies in Cargo.toml
affects:
  - 05-02 (FileWatcher plan — inherits async deps; add pub mod file_watcher to services/mod.rs)
  - 06-serve-command (wires ProcessManager + FileWatcher into ezpm serve)

# Tech tracking
tech-stack:
  added:
    - tokio 1.49.0 (rt-multi-thread, macros, process, signal, time, sync features)
    - tokio-util 0.7.18 (rt feature; CancellationToken available for Phase 6)
    - nix 0.31.1 (signal feature; killpg for Unix process group signals)
    - notify-debouncer-full 0.7.0 (for FileWatcher in Plan 02)
  patterns:
    - Process group isolation: spawn with process_group(0) so PID == PGID; killpg targets whole group
    - SIGTERM -> 2s grace period -> SIGKILL shutdown sequence via tokio::time::timeout
    - mpsc channel lifecycle reporting: caller owns receiver, ProcessManager holds sender
    - #[cfg(unix)] / #[cfg(windows)] split for platform-specific signal code

key-files:
  created:
    - rust-src/services/process_manager.rs
  modified:
    - Cargo.toml
    - Cargo.lock
    - rust-src/services/mod.rs
    - rust-src/commands/alias.rs
    - rust-src/commands/init.rs

key-decisions:
  - "ProcessManager stores Child directly in HashMap (no Arc<Mutex> wrapper) — kill_all() takes exclusive &mut self, no concurrent access needed in Phase 5"
  - "No background wait tasks spawned in spawn() — caller (Phase 6 serve loop) handles monitoring via select! on the event channel receiver"
  - "Child processes inherit terminal stdin/stdout/stderr — ProcessManager manages lifecycle only, not I/O"
  - "Drop impl uses start_kill() (sync, non-blocking) as best-effort fallback — callers must call kill_all().await for clean SIGTERM shutdown"

patterns-established:
  - "Pattern: process_group(0) + killpg — every child process spawned in isolated process group to prevent orphan grandchildren"
  - "Pattern: output::verbose_line for all lifecycle events (spawn, stop, SIGKILL escalation) — clean default, detailed with --verbose"

requirements-completed: [SERVE-06]

# Metrics
duration: 8min
completed: 2026-02-25
---

# Phase 5 Plan 01: ProcessManager Summary

**tokio-based ProcessManager with process-group-aware SIGTERM/SIGKILL shutdown, mpsc lifecycle events, and 4 passing unit tests**

## Performance

- **Duration:** 8 min
- **Started:** 2026-02-25T00:59:50Z
- **Completed:** 2026-02-25T01:07:30Z
- **Tasks:** 2 (Task 2 tests co-located in Task 1 file and committed together)
- **Files modified:** 6

## Accomplishments

- Implemented ProcessManager with Unix process-group-aware graceful shutdown (SIGTERM → 2s grace → SIGKILL) using nix::sys::signal::killpg
- Added lifecycle event reporting via tokio::sync::mpsc channel (Started, Exited, Crashed events)
- Achieved 4 passing unit tests covering spawn+kill, timing grace period, nonexistent command error, and empty kill_all
- Added all 4 Phase 5 async dependencies (tokio, tokio-util, nix, notify-debouncer-full) in one Cargo.toml commit

## Task Commits

Each task was committed atomically:

1. **Task 1: Add async dependencies and implement ProcessManager** - `f0f3e5f` (feat)
   - Note: Task 2 unit tests were co-located in process_manager.rs and included in this commit

**Plan metadata:** (final docs commit — see below)

## Files Created/Modified

- `rust-src/services/process_manager.rs` — ProcessManager struct, ProcessEvent enum, spawn(), kill_all(), Drop impl, and 4 unit tests
- `Cargo.toml` — Added tokio, tokio-util, nix, notify-debouncer-full
- `Cargo.lock` — Updated with 36 new packages locked
- `rust-src/services/mod.rs` — Added `pub mod process_manager`
- `rust-src/commands/alias.rs` — Auto-fix: needless_splitn -> split (pre-existing clippy warning)
- `rust-src/commands/init.rs` — Auto-fix: for_kv_map pattern (pre-existing clippy warning)

## Decisions Made

- ProcessManager stores `tokio::process::Child` directly in `HashMap<String, ManagedProcess>` — `kill_all()` takes `&mut self` (exclusive access), no shared state needed
- No background `tokio::spawn` wait tasks in spawn() — caller (Phase 6 serve loop) handles process death notification via the event channel
- All child I/O (stdin/stdout/stderr) set to `Stdio::inherit()` — terminal passthrough; ProcessManager does not buffer or parse subprocess output
- Windows fallback: `child.kill().await` on each direct child (no process groups on Windows) with `#[cfg(windows)]` gate

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed pre-existing clippy warnings blocking `cargo clippy -- -D warnings`**

- **Found during:** Task 1 verification (clippy check)
- **Issue:** `needless_splitn` in `alias.rs:123` and `for_kv_map` in `init.rs:168` — both pre-existing from Phase 4, not caused by Phase 5 changes. These caused clippy to exit with error code 101, blocking the plan's "zero warnings" success criterion.
- **Fix:** Changed `label.splitn(2, " -> ").next()` to `label.split(" -> ").next()` in alias.rs; changed `for (_alias_name, alias_path) in &aliases` to `for alias_path in aliases.values()` in init.rs.
- **Files modified:** `rust-src/commands/alias.rs`, `rust-src/commands/init.rs`
- **Verification:** `cargo clippy -- -D warnings` passes with zero errors/warnings
- **Committed in:** `f0f3e5f` (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 — pre-existing lint)
**Impact on plan:** Required for plan success criterion ("cargo clippy -- -D warnings produces no warnings on new code"). Trivial two-line changes; no scope creep.

## Issues Encountered

None — build succeeded on first attempt after fixing pre-existing clippy warnings.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- ProcessManager is ready for Phase 5 Plan 02 (FileWatcher) — they are independent service modules
- `services/mod.rs` has `pub mod process_manager` — Plan 02 adds `pub mod file_watcher`
- notify-debouncer-full 0.7.0 is already in Cargo.toml for Plan 02 to use immediately
- Phase 6 will add `#[tokio::main]` to main.rs and wire ProcessManager + FileWatcher into the serve command

## Self-Check: PASSED

- FOUND: rust-src/services/process_manager.rs
- FOUND: Cargo.toml (with tokio, tokio-util, nix, notify-debouncer-full)
- FOUND: rust-src/services/mod.rs (with pub mod process_manager)
- FOUND: 05-01-SUMMARY.md
- FOUND: commit f0f3e5f (feat(05-01): add async deps and implement ProcessManager)

---
*Phase: 05-serve-services*
*Completed: 2026-02-25*
