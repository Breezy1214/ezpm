---
phase: 03-simple-commands
plan: 05
subsystem: cli
tags: [clap, inquire, ureq, mpsc, semver, ascii-art, interactive-menu]

# Dependency graph
requires:
  - phase: 03-01
    provides: init command handler (run_init)
  - phase: 03-02
    provides: install/quality command handlers (install_tools, setup_wally_packages, lint, format_code, docs)
  - phase: 03-03
    provides: init wizard complete with rojo project generation
  - phase: 03-04
    provides: alias command handlers (alias_add, alias_remove, alias_list, alias_sync)
  - phase: 02-03
    provides: require_fixer service and FixRequires dispatch
provides:
  - Full command dispatch in main.rs: all 9 commands wired to handlers
  - Background version check thread with 2s timeout using mpsc channel
  - Version check disable via EZPM_NO_UPDATE_CHECK env var or check_updates=false config
  - Interactive menu with ASCII EZPM logo, category headers, and 13 executable items
  - run_command() dispatcher in menu.rs loads config fresh per invocation
affects: [04-serve-command, 05-release]

# Tech tracking
tech-stack:
  added: [ureq v3 (read_to_string no-arg API), std::sync::mpsc, std::thread]
  patterns: [background thread for non-blocking network check, mpsc channel for thread result, version check footer to stderr only]

key-files:
  created: []
  modified:
    - rust-src/main.rs
    - rust-src/menu.rs

key-decisions:
  - "ureq v3 read_to_string() takes no arguments and returns String directly (vs v2 which wrote into &mut String)"
  - "Version check footer goes to stderr so it never corrupts stdout piping"
  - "src path extracted once from config before dispatch and passed as &str to all handlers"
  - "Menu run_command() loads config fresh each invocation so alias changes take effect without restart"
  - "Category header items use empty command_key; selecting one loops back silently (Pitfall 1 from RESEARCH.md)"

patterns-established:
  - "Background thread pattern: spawn + mpsc::channel, recv_timeout(2s) on footer print"
  - "Menu dispatch: flat MENU_ITEMS array with (label, cmd_key) pairs, empty key = header"
  - "All command errors printed as 'Error: {e}' and exit(1); version footer printed even on error"

requirements-completed: [CLI-01, CLI-02, CLI-05, CLI-09]

# Metrics
duration: 4min
completed: 2026-02-24
---

# Phase 3 Plan 05: Integration — CLI Dispatch, Menu Upgrade, and Version Check Summary

**Full CLI wiring of all 9 Phase 3 commands in main.rs with background version check thread, plus interactive menu upgraded with ASCII EZPM logo and category-grouped command list**

## Performance

- **Duration:** 4 min
- **Started:** 2026-02-24T17:58:07Z
- **Completed:** 2026-02-24T18:02:00Z
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments
- Wired all command handlers (init, install, setup-wally-packages, lint, format, docs, fix-requires, alias add/remove/list/sync, serve) into main.rs dispatch
- Added background version check thread using mpsc channel with 2-second timeout so it never blocks user commands
- EZPM_NO_UPDATE_CHECK env var and check_updates=false config both disable the version check correctly
- Upgraded menu.rs with ASCII art EZPM logo printed before the menu and 5 category sections (Project Setup, Alias Management, Code Quality, Build Tools, Development)
- Menu run_command() dispatcher reloads config fresh per invocation for live alias changes

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire all command handlers into main.rs with background version check** - `bd46704` (feat)
2. **Task 2: Upgrade interactive menu with category headers and ASCII logo** - `6776f4d` (feat)
3. **Task 3: Verify end-to-end compilation and help output** - `b091cfb` (chore)

**Plan metadata:** _(docs commit after SUMMARY.md)_

## Files Created/Modified
- `rust-src/main.rs` - Full command dispatch with background version check; all 9 commands wired to handlers
- `rust-src/menu.rs` - ASCII logo, 5 category sections, 13 executable items, run_command() dispatcher

## Decisions Made
- ureq v3 `read_to_string()` takes no arguments and returns `String` directly — the plan's code was written for ureq v2 API (`read_to_string(&mut body)`). Fixed inline during Task 1.
- Version check footer written to stderr exclusively so it does not corrupt piped stdout output.
- `src` path is extracted once from config before the command dispatch match and passed as `&str` to avoid redundant config lookups.
- `run_command()` in menu.rs loads config fresh on each call rather than once at menu startup, so alias changes made during a session are reflected immediately on the next menu selection.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed ureq v3 read_to_string API mismatch**
- **Found during:** Task 1 (Wire command handlers into main.rs)
- **Issue:** Plan snippet used `read_to_string(&mut body)` (ureq v2 API). ureq v3 changed the method to take no arguments and return `Result<String>` directly. The import `use std::io::Read` was also unnecessary.
- **Fix:** Replaced `.read_to_string(&mut body).ok()?` with `.read_to_string().ok()?`; removed `use std::io::Read`
- **Files modified:** rust-src/main.rs
- **Verification:** `cargo check` passed after fix
- **Committed in:** `bd46704` (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 — bug from API version mismatch)
**Impact on plan:** Necessary correctness fix. No scope creep.

## Issues Encountered
- ureq v3 changed its `read_to_string` method signature — handled automatically per deviation Rule 1.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All Phase 3 commands are fully wired and functional
- Interactive menu is complete with ASCII logo and category headers
- Background version check is implemented and correctly gated
- Phase 4 (serve command with concurrent process orchestration) can begin
- Blocker noted in STATE.md: concurrent tokio process orchestration for serve needs research during Phase 4 planning

---
*Phase: 03-simple-commands*
*Completed: 2026-02-24*

## Self-Check: PASSED

- FOUND: rust-src/main.rs
- FOUND: rust-src/menu.rs
- FOUND: .planning/phases/03-simple-commands/03-05-SUMMARY.md
- FOUND commit: bd46704 (feat(03-05): wire all command handlers into main.rs with background version check)
- FOUND commit: 6776f4d (feat(03-05): upgrade interactive menu with category headers and ASCII logo)
- FOUND commit: b091cfb (chore(03-05): verify end-to-end compilation and help output)
