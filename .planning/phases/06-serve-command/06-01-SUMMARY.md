---
phase: 06-serve-command
plan: 01
subsystem: cli
tags: [tokio, async, clap, serve, spinners, rojo, darklua, file-watcher, process-manager]

# Dependency graph
requires:
  - phase: 05-serve-services
    provides: ProcessManager and FileWatcher services used in the 8-step startup sequence
  - phase: 04-output-foundation
    provides: output::start_spinner(), success(), error(), info(), print_line() used for step display
  - phase: 02-core-services
    provides: darklua_runner, sourcemap, meta_copier, require_fixer services called during startup
provides:
  - rust-src/commands/serve.rs with pub async fn run(config, cli_port) entry point
  - 8-step startup sequence with spinners and per-step timing
  - Port resolution (CLI --port > ezpm.toml serve.port > 34872 default)
  - Port conflict detection before Rojo launch
  - Summary banner after successful startup
  - Ctrl-C graceful shutdown (kill Rojo, drop watcher)
affects: [06-02, phase-8-integration-tests]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - tokio::runtime::Builder::new_multi_thread().enable_all().build().block_on() scoped to Serve arm in main.rs (not #[tokio::main])
    - tokio::task::spawn_blocking for sync subprocess calls (darklua, sourcemap, require_fixer) inside async fn
    - Sequential per-step spinner pattern with pb.finish_and_clear() before output::success/error
    - owo-colors fmt::Write approach to avoid borrow conflicts when chaining if_supports_color

key-files:
  created:
    - rust-src/commands/serve.rs
  modified:
    - rust-src/cli.rs
    - rust-src/main.rs
    - rust-src/commands/mod.rs

key-decisions:
  - "tokio runtime scoped to Serve arm via block_on — keeps fn main() synchronous, avoids paying for async runtime overhead in non-serve commands"
  - "spawn_blocking wraps all sync subprocess services (sourcemap, require_fixer, darklua) inside async run() — avoids blocking tokio worker threads"
  - "port_is_available() checked before any build steps — user gets immediate feedback on port conflicts without waiting through the 8-step sequence"
  - "owo-colors banner uses fmt::Write to build an owned String — avoids temporary lifetime issues from chaining if_supports_color"
  - "Plan 01 uses simple ctrl_c().await for the wait loop — Plan 02 replaces with tokio::select! watch loop"

patterns-established:
  - "Tokio runtime dispatch pattern: block_on scoped to Serve arm, not global #[tokio::main]"
  - "Spinner lifecycle: start_spinner -> pb.finish_and_clear() -> output::success or output::error"
  - "spawn_blocking for any std::process::Command-based service inside async context"

requirements-completed: [SERVE-01, SERVE-05, SERVE-07]

# Metrics
duration: 12min
completed: 2026-02-24
---

# Phase 06 Plan 01: Serve Command Startup Sequence Summary

**`ezpm serve` wired end-to-end: tokio dispatch in main.rs, `--port` CLI flag, 8-step startup with sequential spinners and timing, port conflict detection, and Ctrl-C graceful shutdown**

## Performance

- **Duration:** 12 min
- **Started:** 2026-02-24T00:00:00Z
- **Completed:** 2026-02-24T00:12:00Z
- **Tasks:** 2 (committed together — co-dependent for compilation)
- **Files modified:** 4 (cli.rs, main.rs, commands/mod.rs, commands/serve.rs)

## Accomplishments

- Added `Serve { port: Option<u16> }` struct variant to `Commands` in cli.rs with `--port`/`-p` flag
- Replaced the `Commands::Serve` placeholder in main.rs with a proper tokio runtime dispatch via `Runtime::block_on(serve::run(loaded_config, port))`
- Implemented `rust-src/commands/serve.rs` with the full 8-step startup pipeline: build.project.json generation, build dir clean, sourcemap, require fixing, DarkLua, meta file copy, FileWatcher start, Rojo launch
- Port resolution priority chain: `--port` CLI flag > `ezpm.toml serve.port` > 34872 default
- Port conflict detection with friendly error ("Port 34872 in use. Try: ezpm serve --port 34873") before any build steps run
- Summary banner ("ezpm serve  ready" + port + watch status + Ctrl-C hint) after all 8 steps complete
- `cargo build` and `cargo clippy -- -D warnings` pass with zero errors/warnings; all 63 tests pass

## Task Commits

Both tasks committed together since they depend on each other for compilation:

1. **Task 1 + Task 2: CLI flag, tokio dispatch, serve.rs** - `0785aa3` (feat)

**Plan metadata:** committed in docs commit below

## Files Created/Modified

- `rust-src/commands/serve.rs` - The serve command entry point: port resolution, 8 startup steps, banner, Ctrl-C shutdown
- `rust-src/cli.rs` - `Serve` variant upgraded from unit to struct with `port: Option<u16>` field and `--port`/`-p` flag
- `rust-src/main.rs` - `Commands::Serve` arm replaced: imports serve module, builds tokio runtime, calls `block_on(serve::run(...))`
- `rust-src/commands/mod.rs` - Added `pub mod serve;`

## Decisions Made

- **Tokio runtime scoped to Serve arm:** `block_on` in the `Commands::Serve` match arm keeps `fn main()` synchronous. All other commands (init, install, lint, etc.) remain synchronous and don't pay the multi-thread runtime overhead.
- **spawn_blocking for all sync services:** `sourcemap::generate_sourcemap`, `require_fixer::fix_requires`, and `darklua_runner::process_tree` all use `std::process::Command` (synchronous). Each is wrapped in `tokio::task::spawn_blocking` to avoid blocking the async runtime.
- **Port check before build steps:** If the port is occupied, the user sees an error immediately without waiting through the 8-step sequence.
- **fmt::Write for colored banner:** owo-colors `.if_supports_color()` chaining produces borrow conflicts when assigning to a variable. Using `std::fmt::Write` to write into an owned `String` resolves this without introducing extra crates.
- **Ctrl-C wait for Plan 01:** Simple `tokio::signal::ctrl_c().await` is used as the post-startup wait loop. Plan 02 will replace this with `tokio::select!` to also handle file change events and Rojo lifecycle events.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] owo-colors banner chaining caused borrow/lifetime errors**

- **Found during:** Task 2 (serve.rs implementation)
- **Issue:** Chaining `.if_supports_color().if_supports_color()` to achieve bold+green returned a value referencing a temporary, causing `E0515`/`E0716` compilation errors
- **Fix:** Used `std::fmt::Write` to format both colored segments into an owned `String` in a block scope, avoiding the borrow conflict
- **Files modified:** rust-src/commands/serve.rs (banner section)
- **Verification:** `cargo build` and `cargo clippy -- -D warnings` both pass
- **Committed in:** `0785aa3` (Task 1+2 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 - Bug)
**Impact on plan:** Necessary for compilation. No scope creep. owo-colors limitations with chained if_supports_color are a known constraint; the fmt::Write workaround is idiomatic Rust.

## Issues Encountered

None beyond the owo-colors borrow issue documented above.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- Plan 02 (06-02) can now implement the `tokio::select!` watch loop using the `watcher_rx` and `process_rx` receivers already created in this plan
- The `src_to_build_path()` helper in serve.rs is already exposed (`pub(crate)`) for use by the watch loop
- All 8 startup steps are implemented and tested; Plan 02 only needs to replace the `ctrl_c().await` block with the select! loop

## Self-Check: PASSED

| Check | Result |
|-------|--------|
| `rust-src/commands/serve.rs` exists | FOUND |
| `rust-src/cli.rs` exists | FOUND |
| `rust-src/main.rs` exists | FOUND |
| `rust-src/commands/mod.rs` exists | FOUND |
| `06-01-SUMMARY.md` exists | FOUND |
| Commit `0785aa3` exists | FOUND |
| serve.rs line count >= 120 | 361 lines |

---
*Phase: 06-serve-command*
*Completed: 2026-02-24*
