# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-02-24)

**Core value:** Every current EZPM workflow must work identically (or better) in the Rust version — zero regression on the developer experience that Roblox users depend on.
**Current focus:** Milestone v1.1 Dev Server & Polish — Phase 5: Serve Services

## Current Position

Phase: 5 (Serve Services)
Plan: 2/2 complete
Status: Phase 5 Complete — ProcessManager (Plan 01) and FileWatcher (Plan 02) both implemented and tested
Last activity: 2026-02-25 — Phase 5 Plan 2 complete (FileWatcher with OS-native events, 300ms debounce, categorized events, 7 tests; SERVE-02 satisfied)

```
Progress: [####------] 3/8 phases complete (v1.0 shipped, v1.1 in progress)
```

## Performance Metrics

**Velocity:**
- Total plans completed: 10
- Average duration: 6 min
- Total execution time: ~1 hour

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 01-foundation | 2 | 37 min | 18 min |
| 02-core-services | 3 | 12 min | 4 min |
| 03-simple-commands | 5 | 15 min | 3 min |
| 04-output-foundation | 2 (of 2) | 10 min | 5 min |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Summary: 9 key decisions made during v1.0, all marked Good.

**v1.1 phase decisions:**
- Combined Cargo Foundation + Output Layer + CLI Global Flags into Phase 4 (Output Foundation) — all three are output-layer prerequisites with no async involvement; building together avoids a thin one-plan Cargo phase
- Phase 5 (Serve Services) is isolated to ProcessManager + FileWatcher only — keeps async complexity contained before serve assembly
- Phase 7 (DX Polish) depends on Phase 4 but NOT Phase 5/6 — error handling and exit codes are independent of the async serve pipeline
- Phase 8 (Integration Tests) is last — requires stable binary from all prior phases

**Phase 4 Plan 1 decisions:**
- ColorArg placed in cli.rs (not output.rs) — CLI parsing and output rendering are separate concerns; mapping happens in main() match block
- print_line() and print_stderr() added as neutral variants for alias rows/help text/version footer blank lines — these are neither success/error/info/warn but still need --quiet suppression
- Version footer update notification uses output::info() (stdout ▸ prefix) not raw eprintln — gets branded cyan prefix and obeys --quiet
- OnceLock global state is correct pattern for CLI binary — not a library; output module in lib.rs consumed exclusively by binary

**Phase 4 Plan 2 decisions:**
- Logo println! calls kept as direct println! with if_supports_color — output module lacks a raw-colored-line variant; plan explicitly specified this implementation pattern
- Subprocess verbosity pattern established: is_verbose() -> .status() (pass-through) vs .output() (capture), applied uniformly across install.rs and quality.rs
- pb.suspend(|| output::warn(...)) used for warnings during spinner spin to prevent terminal corruption

**Phase 5 Plan 1 decisions:**
- ProcessManager stores Child directly in HashMap — kill_all() takes &mut self (exclusive), no shared state/Arc<Mutex> needed in Phase 5
- No background tokio::spawn wait tasks in spawn() — Phase 6 serve loop handles process death via select! on the event channel receiver
- Child processes inherit terminal stdin/stdout/stderr — ProcessManager manages lifecycle only, not I/O
- Windows fallback: child.kill().await on each direct child with #[cfg(windows)] gate (process groups are Unix-only)

**Phase 5 Plan 2 decisions:**
- FileChange derives Hash+Eq to allow HashSet deduplication within a single debounce batch — editor save bursts can produce duplicate paths even after debouncing
- Integration test accepts FileCreated alongside LuaChange — kqueue on macOS reports first write to a watched file as Create rather than Modify when file existed before watcher started
- is_some_and() used instead of map_or(false, ...) — clippy::unnecessary-map-or enforced at -D warnings level

### Pending Todos

None.

### Blockers/Concerns

- [Phase 5 flag RESOLVED]: notify-debouncer-full 0.7.0 uses 3-arg new_debouncer(timeout, tick_rate, handler) — verified and implemented
- [Phase 6 flag]: DarkLua invocation model — decide between (a) long-lived `darklua process --watch` subprocess vs. (b) EZPM-managed per-file invocations via `process_file()`. Decision must be made at start of Phase 6 planning.
- [Phase 6 flag]: tokio::select! shutdown loop with CancellationToken propagation — validate exact implementation variant during Phase 6 planning
- [Phase 6 flag]: Rojo port config field name — verify exact field in live EzpmConfig struct (`config.rojo_port` vs `config.serve.port`) before Phase 6
- [Phase 8 flag]: EZPM_NO_UPDATE_CHECK env var — CONFIRMED already implemented in main.rs (std::env::var check on line 83)

## Session Continuity

Last session: 2026-02-25
Stopped at: Completed 05-02-PLAN.md — FileWatcher implemented with notify-debouncer-full 0.7.0, 300ms debounce, categorized events (LuaChange/MetaChange/FileCreated/FileDeleted), 7 tests passing; Phase 5 complete
Resume file: None
Next action: Phase 6 — Serve command assembly (compose FileWatcher + ProcessManager in tokio::select! loop)
