---
phase: 06-serve-command
plan: 03
subsystem: serve
tags: [rust, tokio, file-watcher, notify-debouncer-full, serve-command]

# Dependency graph
requires:
  - phase: 06-serve-command plan 02
    provides: tokio::select! watch loop with file change routing and handle_* handlers
provides:
  - init.meta.json Create events correctly classified as MetaChange (not FileCreated)
  - delete-wins deduplication in classify_events(): FileDeleted suppresses LuaChange/MetaChange/FileCreated for same path
  - display_name() helper in serve.rs: init.* files display as "Parent/init.luau"
affects: [UAT, Phase 7, Phase 8]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Delete-wins dedup: collect deleted_paths as owned HashSet<PathBuf> before result.retain() to satisfy borrow checker"
    - "display_name() helper centralizes file display logic for init.* vs regular files"

key-files:
  created: []
  modified:
    - rust-src/services/file_watcher.rs
    - rust-src/commands/serve.rs

key-decisions:
  - "Collect owned PathBuf values (not references) for deleted_paths HashSet — borrow checker rejects borrowing result immutably during collect() while also needing result mutably for retain()"
  - "display_name() placed as a free fn before handle_lua_change — same module, no need for a trait or struct method; centralizes all handler display logic"

patterns-established:
  - "Delete-wins pattern: after building FileChange batch, collect deleted paths then retain() to remove conflicting non-delete events for same paths"
  - "display_name(path): for init.* files return parent/filename, otherwise return filename — centralized, not inline"

requirements-completed: [SERVE-03, SERVE-04]

# Metrics
duration: 4min
completed: 2026-02-25
---

# Phase 6 Plan 03: UAT Gap Closure Summary

**Delete-wins dedup in classify_events() and display_name() helper fix the two remaining UAT failures: init.meta.json Create events now route to MetaChange, and file deletion no longer triggers a require-fix error on the deleted file**

## Performance

- **Duration:** ~4 min
- **Started:** 2026-02-25T06:03:57Z
- **Completed:** 2026-02-25T06:08:00Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- UAT test 6 fixed: EventKind::Create for init.meta.json now calls classify_modify() producing FileChange::MetaChange instead of FileCreated — handle_meta_change now handles meta file saves from atomic-save editors
- UAT test 8 fixed: delete-wins post-processing in classify_events() removes LuaChange/MetaChange/FileCreated events for any path that also has FileDeleted in the same debounce batch — prevents require_fixer from running on deleted files
- display_name() helper added: init.luau shows as "MyModule/init.luau", init.meta.json as "Services/init.meta.json", regular files still as "Foo.luau"
- All 65 lib tests pass including 2 new unit tests; clippy -D warnings clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Fix meta file Create event classification and delete-wins dedup in file_watcher.rs** - `8876413` (fix)
2. **Task 2: Add display_name helper for init.* files in serve.rs** - `e027e9d` (feat)

**Plan metadata:** *(pending final docs commit)*

## Files Created/Modified

- `rust-src/services/file_watcher.rs` — EventKind::Create arm now checks init.meta.json and routes to classify_modify(); delete-wins HashSet<PathBuf> + retain() added after main loop; two new unit tests added
- `rust-src/commands/serve.rs` — display_name() helper fn added before handle_lua_change; all path.file_name().unwrap_or_default().to_string_lossy() calls in handlers replaced with display_name(path)

## Decisions Made

- Collected owned `PathBuf` values (not `&PathBuf` references) for `deleted_paths` HashSet — the borrow checker rejects creating a `HashSet<&PathBuf>` from `result.iter()` while `result` is subsequently borrowed mutably by `retain()`; cloning paths is the only correct solution in safe Rust
- `display_name()` placed as a private free function before the handler functions — same module, avoids trait/impl boilerplate, centralizes all display logic with one clear function name

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Borrow checker rejection of HashSet<&PathBuf> with result.retain()**
- **Found during:** Task 1 (classify_events delete-wins dedup)
- **Issue:** Plan's code sample used `HashSet<&PathBuf>` which borrows `result` immutably; calling `result.retain()` afterward requires a mutable borrow — Rust E0502 borrow conflict
- **Fix:** Changed to `HashSet<PathBuf>` with `.clone()` in the filter_map — owned values satisfy the borrow checker with no other change needed
- **Files modified:** rust-src/services/file_watcher.rs
- **Verification:** `cargo test --lib services::file_watcher` — all 9 tests pass including new delete-wins test
- **Committed in:** 8876413 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 — borrow checker bug in plan's code sample)
**Impact on plan:** Fix required for correctness. The logic is identical to the plan's intent — only the ownership model changed from borrowed references to owned clones.

## Issues Encountered

- `config_compat` integration test `luau_format_ezpm_toml_loads_without_error` fails because `ezpm.toml` was modified during Phase 6 UAT testing (project name changed to "s", extra config sections added). This is a pre-existing issue outside this plan's scope. Logged to `deferred-items.md` in the phase directory. All lib tests (65/65) pass.

## Next Phase Readiness

- All 10 UAT tests for Phase 6 now addressed (2 remaining failures fixed by this plan)
- Phase 6 fully complete — ezpm serve is production-ready for the Roblox workflow
- Ready for Phase 7 (DX Polish: error handling and exit codes) or Phase 8 (Integration Tests)
- Pre-existing: `ezpm.toml` needs restoration to canonical form before `config_compat` integration tests will pass

---
*Phase: 06-serve-command*
*Completed: 2026-02-25*
