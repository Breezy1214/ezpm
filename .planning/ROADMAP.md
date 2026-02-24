# Roadmap: EZPM Rust Rewrite

## Overview

A complete rewrite of EZPM from Luau/Lune to Rust. The journey: stand up the Rust project with config parsing and CI infrastructure first, build the core service engines (process management, file watching, require fixer) as independently testable modules, implement all the simple commands to reach feature parity on most workflows, tackle the serve pipeline as its own isolated phase (the highest-risk command), then ship with polished UX and cross-platform distribution. Every phase delivers something a developer can actually use or test.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Foundation** - Rust project scaffold, TOML config parsing, CI matrix, and integration test harness (completed 2026-02-24)
- [x] **Phase 2: Core Services** - ProcessManager, FileWatcher, config generators, and require fixer as independently testable modules (completed 2026-02-24)
- [ ] **Phase 3: Simple Commands** - All commands except serve: init, alias, install, fix-requires, lint, format, docs, interactive menu, version check
- [ ] **Phase 4: Serve Pipeline** - Full ezpm serve: startup pipeline, OS-native file watching, incremental builds, graceful shutdown
- [ ] **Phase 5: UX Polish and Distribution** - Progress indicators, colored output, structured errors, verbose/quiet flags, cross-platform binaries, CI/CD release

## Phase Details

### Phase 1: Foundation
**Goal**: Developers can compile and test the Rust binary with full TOML config compatibility and CI running on all 6 targets
**Depends on**: Nothing (first phase)
**Requirements**: CFG-01, DIST-01, DIST-02, DIST-03, DIST-04
**Success Criteria** (what must be TRUE):
  1. Running `cargo build` produces a working binary with no warnings
  2. A Luau-version ezpm.toml loads without error in the Rust binary (backward compatibility verified by test)
  3. GitHub Actions CI builds succeed for all 6 targets (Linux/macOS/Windows x86_64+aarch64)
  4. The binary is a single static executable with no runtime dependencies
  5. The binary is installable via Rokit (`rokit add Breezy1214/ezpm`)
**Plans**: 2 plans
Plans:
- [ ] 01-01-PLAN.md -- Rust project scaffold with CLI, TOML config parsing, interactive menu, and integration tests
- [ ] 01-02-PLAN.md -- GitHub Actions CI workflow and cross-platform release pipeline

### Phase 2: Core Services
**Goal**: All internal engines (process management, file watching, config generation, require path fixing) exist as independently tested modules
**Depends on**: Phase 1
**Requirements**: BUILD-01, BUILD-02, BUILD-03, BUILD-04, BUILD-05, BUILD-06, BUILD-07, BUILD-08, BUILD-09, TEST-01
**Success Criteria** (what must be TRUE):
  1. Unit tests pass for require path fixer (longest-match alias resolution, skip external/builtin aliases, no-op when no changes)
  2. Unit tests pass for config generators (correct .darklua.json and .luaurc produced from alias map)
  3. Unit tests pass for config parsing (semver comparison, TOML deserialization with optional fields)
  4. `ezpm fix-requires` scans source files, rewrites require paths to @alias notation, and displays all changes made
  5. DarkLua transformation executes on full source tree and on individual files without blocking the async runtime
**Plans**: 3 plans
Plans:
- [x] 02-01-PLAN.md -- Require path fixer module with TDD (alias resolution, skip lists, write-on-change)
- [x] 02-02-PLAN.md -- Config generators, DarkLua runner, Rojo sourcemap, meta copier, and semver utility
- [ ] 02-03-PLAN.md -- Gap closure: wire fix-requires CLI command to engine, fix traceability table

### Phase 3: Simple Commands
**Goal**: Users can run every EZPM command except serve: project initialization, alias management, tool installation, require fixing, linting, formatting, and docs
**Depends on**: Phase 2
**Requirements**: CLI-01, CLI-02, CLI-05, CLI-09, CFG-02, CFG-03, CFG-04, CFG-05, CFG-06, CFG-07, CFG-08, INIT-01, INIT-02, INIT-03, INIT-04, INIT-05, INIT-06, INIT-07, INST-01, INST-02, INST-03, INST-04, QUAL-01, QUAL-02, QUAL-03, QUAL-04
**Success Criteria** (what must be TRUE):
  1. Running `ezpm` with no arguments displays an interactive arrow-key menu listing all commands
  2. Running `ezpm init` in an empty directory creates ezpm.toml, default.project.json, rokit.toml, and source directory scaffolding with interactive prompts
  3. Running `ezpm alias add`, `ezpm alias remove`, `ezpm alias list`, and `ezpm alias sync` manage aliases and regenerate .darklua.json and .luaurc correctly
  4. Running `ezpm install` runs Rokit, Wally (if wally.toml exists), and wally-package-types in sequence
  5. Running `ezpm lint` invokes Selene and StyLua --check (skipping gracefully if not installed); `ezpm format` runs StyLua on source directory
**Plans**: 5 plans
Plans:
- [ ] 03-01-PLAN.md — Config infrastructure: ureq dependency, TOML serialization, check_updates field, commands module scaffold
- [ ] 03-02-PLAN.md — Install + quality commands: rokit/wally/selene/stylua/moonwave subprocess wrappers
- [ ] 03-03-PLAN.md — Init command: interactive wizard with file detection, alias import, scaffolding, and file generation
- [ ] 03-04-PLAN.md — Alias commands: add, remove, list, sync with auto-regeneration of .darklua.json and .luaurc
- [ ] 03-05-PLAN.md — CLI wiring + menu upgrade: all handlers in main.rs, background version check, category-grouped menu with ASCII logo

### Phase 4: Serve Pipeline
**Goal**: Developers can run `ezpm serve` for their full Roblox dev loop — build, watch, instant change propagation, and clean shutdown
**Depends on**: Phase 3
**Requirements**: SERVE-01, SERVE-02, SERVE-03, SERVE-04, SERVE-05, SERVE-06, SERVE-07, SERVE-08, SERVE-09, SERVE-10, SERVE-11, TEST-02
**Success Criteria** (what must be TRUE):
  1. Running `ezpm serve` executes the full startup sequence (clean build dir, generate sourcemap, fix requires, DarkLua full build, start Rojo) and Rojo connects on the configured port
  2. Saving a .lua or .luau file triggers a rebuild within 500ms (OS-native events, not polling) and Roblox Studio reflects the change
  3. Creating a new file triggers require fix, sourcemap regeneration, and DarkLua build for the affected directory
  4. Pressing Ctrl-C kills all child processes (Rojo, DarkLua), releases the port, and exits cleanly — no orphaned processes remain
  5. Integration tests verify the serve startup/shutdown pipeline and fix-requires pipeline against known inputs
**Plans**: TBD

### Phase 5: UX Polish and Distribution
**Goal**: Every command produces polished, informative output; the binary ships on all 6 platforms installable via Rokit
**Depends on**: Phase 4
**Requirements**: CLI-03, CLI-04, CLI-06, CLI-07, CLI-08, TEST-03
**Success Criteria** (what must be TRUE):
  1. Terminal output uses color (respecting NO_COLOR) and progress spinners appear during init, install, and serve startup
  2. Failed commands display structured error messages with context and a suggested fix; all failures exit with non-zero code
  3. `--verbose` produces detailed output; `--quiet` suppresses non-essential output; both work on every command
  4. GitHub Actions releases a new version with binaries for all 6 targets when a version bump is pushed
  5. CI runs the full test suite on every PR before merging
**Plans**: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4 → 5

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Foundation | 2/2 | Complete   | 2026-02-24 |
| 2. Core Services | 3/3 | Complete   | 2026-02-24 |
| 3. Simple Commands | 3/5 | In Progress|  |
| 4. Serve Pipeline | 0/TBD | Not started | - |
| 5. UX Polish and Distribution | 0/TBD | Not started | - |
