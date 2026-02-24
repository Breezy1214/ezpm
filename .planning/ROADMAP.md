# Roadmap: EZPM Rust Rewrite

## Milestones

- ✅ **v1.0 MVP** — Phases 1-3 (shipped 2026-02-24)
- 📋 **v1.1** — Phases 4-5 (planned)

## Phases

<details>
<summary>✅ v1.0 MVP (Phases 1-3) — SHIPPED 2026-02-24</summary>

- [x] Phase 1: Foundation (2/2 plans) — completed 2026-02-24
- [x] Phase 2: Core Services (3/3 plans) — completed 2026-02-24
- [x] Phase 3: Simple Commands (5/5 plans) — completed 2026-02-24

See: `.planning/milestones/v1.0-ROADMAP.md` for full details.

</details>

### 📋 v1.1 (Planned)

- [ ] **Phase 4: Serve Pipeline** - Full ezpm serve: startup pipeline, OS-native file watching, incremental builds, graceful shutdown
- [ ] **Phase 5: UX Polish and Distribution** - Progress indicators, colored output, structured errors, verbose/quiet flags, cross-platform binaries, CI/CD release

## Phase Details

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

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Foundation | v1.0 | 2/2 | Complete | 2026-02-24 |
| 2. Core Services | v1.0 | 3/3 | Complete | 2026-02-24 |
| 3. Simple Commands | v1.0 | 5/5 | Complete | 2026-02-24 |
| 4. Serve Pipeline | v1.1 | 0/TBD | Not started | - |
| 5. UX Polish and Distribution | v1.1 | 0/TBD | Not started | - |
