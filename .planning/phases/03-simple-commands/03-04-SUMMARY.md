---
phase: 03-simple-commands
plan: 04
subsystem: config
tags: [inquire, toml, aliases, config-gen, darklua, luaurc]

# Dependency graph
requires:
  - phase: 03-simple-commands
    provides: 03-01 config infrastructure (save_ezpm_toml, load_config, write_config_files)

provides:
  - alias_add handler (CFG-02): prompts for name+path, normalizes trailing slash, saves ezpm.toml, auto-regenerates .darklua.json and .luaurc, optionally creates directory
  - alias_remove handler (CFG-03): MultiSelect checklist to pick aliases to remove, confirms deletion, saves ezpm.toml, auto-regenerates config files
  - alias_list handler (CFG-04): aligned table output sorted alphabetically with total count
  - alias_sync handler (CFG-05): reloads ezpm.toml from disk and regenerates .darklua.json + .luaurc
  - rust-src/commands/alias.rs module exported from commands/mod.rs

affects: [03-05, 04-serve]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - All alias mutations call both save_ezpm_toml AND write_config_files to ensure ezpm.toml and .darklua.json/.luaurc stay in sync
    - Load-modify-save cycle for alias mutations: load_config -> mutate HashMap -> save_ezpm_toml -> write_config_files
    - Trailing slash normalization on alias paths (Pitfall 5 prevention) applied in alias_add
    - Alphabetical sort on alias display for consistent UX across alias_list and alias_remove

key-files:
  created:
    - rust-src/commands/alias.rs
  modified:
    - rust-src/commands/mod.rs

key-decisions:
  - "All four alias functions written atomically in a single file to avoid partial implementation states"
  - "alias_list takes &Option<HashMap<String, String>> parameter for testability without I/O in callers"
  - "alias_remove re-reads project_name/src/darklua_build from cfg after partial move of aliases field — Rust partial move semantics allow this pattern"

patterns-established:
  - "Pattern: Load-modify-save-regenerate cycle for alias mutations (load_config -> mutate -> save_ezpm_toml -> write_config_files)"
  - "Pattern: Trailing slash normalization at point of input, not at point of use"

requirements-completed: [CFG-02, CFG-03, CFG-04, CFG-05]

# Metrics
duration: 3min
completed: 2026-02-24
---

# Phase 3 Plan 04: Alias Commands Summary

**Four alias management commands (add/remove/list/sync) with auto-regeneration of .darklua.json and .luaurc on every mutation**

## Performance

- **Duration:** 3 min
- **Started:** 2026-02-24T17:50:29Z
- **Completed:** 2026-02-24T17:53:29Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- Implemented alias_add with sequential prompts, trailing slash normalization (Pitfall 5), config save, auto-regeneration, and optional directory creation
- Implemented alias_remove with MultiSelect checklist, confirmation prompt, and auto-regeneration after removal
- Implemented alias_list with aligned table format (max-width padding) sorted alphabetically
- Implemented alias_sync that reloads ezpm.toml and triggers config file regeneration
- All four functions call write_config_files after every mutation, ensuring .darklua.json and .luaurc are always in sync with ezpm.toml

## Task Commits

Each task was committed atomically:

1. **Task 1 + 2: Implement all four alias handlers (alias_add, alias_remove, alias_list, alias_sync)** - `d79fa73` (feat)

**Plan metadata:** (docs commit to follow)

## Files Created/Modified

- `rust-src/commands/alias.rs` - All four alias command handlers (206 lines)
- `rust-src/commands/mod.rs` - Added pub mod alias export

## Decisions Made

- Wrote all four functions atomically in the initial file to ensure consistency across the load-modify-save-regenerate pattern rather than two separate commits that could leave the file in a partial state
- alias_list accepts `&Option<HashMap<String, String>>` to allow callers (main.rs) to pass already-loaded config aliases without extra disk I/O
- alias_sync calls load_config() (not accepting aliases from caller) to fulfill the CFG-05 requirement of explicitly reloading from disk

## Deviations from Plan

None - plan executed exactly as written. All four functions implemented as specified. Both tasks merged into a single commit for implementation efficiency (all functions were written together in one file creation).

## Issues Encountered

None. Cargo check passed on first attempt with no new errors (pre-existing dead_code warning in install.rs is unrelated to this plan).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- alias.rs complete with all four CFG-02 through CFG-05 requirement handlers
- commands/mod.rs exports alias module, ready for main.rs CLI dispatch wiring
- Phase 3 Wave 2 alias plan complete — 03-05 (quality commands) is the remaining Wave 2 plan
- All alias mutations consistently use the load-modify-save-regenerate pattern established here

## Self-Check: PASSED

- FOUND: rust-src/commands/alias.rs (206 lines, contains alias_add, alias_remove, alias_list, alias_sync)
- FOUND: rust-src/commands/mod.rs (contains pub mod alias)
- FOUND: commit d79fa73 (Task 1+2)
- cargo check passes with no errors (1 pre-existing unrelated warning in install.rs)
- save_ezpm_toml called in alias_add and alias_remove (lines 61, 148)
- write_config_files called in alias_add, alias_remove, alias_sync (lines 64, 151, 201)
- load_config called in alias_add, alias_remove, alias_sync (lines 34, 86, 191)

---
*Phase: 03-simple-commands*
*Completed: 2026-02-24*
