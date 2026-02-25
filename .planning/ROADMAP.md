# Roadmap: EZPM Rust Rewrite

## Milestones

- ✅ **v1.0 MVP** — Phases 1-3 (shipped 2026-02-24)
- 📋 **v1.1 Dev Server & Polish** — Phases 4-8 (in progress)

## Phases

<details>
<summary>✅ v1.0 MVP (Phases 1-3) — SHIPPED 2026-02-24</summary>

- [x] Phase 1: Foundation (2/2 plans) — completed 2026-02-24
- [x] Phase 2: Core Services (3/3 plans) — completed 2026-02-24
- [x] Phase 3: Simple Commands (5/5 plans) — completed 2026-02-24

See: `.planning/milestones/v1.0-ROADMAP.md` for full details.

</details>

### 📋 v1.1 Dev Server & Polish

- [x] **Phase 4: Output Foundation** - Cargo dependencies, centralized output module, NO_COLOR support, global --verbose/--quiet/--color flags
- [x] **Phase 5: Serve Services** - Async ProcessManager and FileWatcher service modules (prerequisites for serve) (completed 2026-02-25)
- [x] **Phase 6: Serve Command** - Full ezpm serve: startup pipeline, incremental rebuilds, Rojo live sync, graceful shutdown (completed 2026-02-25)
- [x] **Phase 7: DX Polish** - Structured error messages, non-zero exit codes, subprocess error propagation applied to all commands (completed 2026-02-25)
- [ ] **Phase 8: Integration Tests and CI** - assert_cmd integration tests, CI test suite, Rust build cache

## Phase Details

### Phase 4: Output Foundation
**Goal**: All terminal output is colored, consistent, and flag-controlled — a single output module that every command inherits
**Depends on**: Phase 3 (v1.0 complete)
**Requirements**: OUT-01, OUT-02, OUT-03, CLI-01, CLI-02, CLI-03
**Success Criteria** (what must be TRUE):
  1. Terminal output in all commands shows colored text; piped output and NO_COLOR=1 produce plain text automatically
  2. Running `ezpm --quiet install` suppresses non-error output; `ezpm --verbose init` shows detailed step output
  3. `ezpm --color Never` forces plain output in a TTY; `ezpm --color Always` forces color in a pipe
  4. Progress spinners appear during multi-step operations (serve startup, install) and auto-hide when output is piped
  5. No `println!` or `eprintln!` calls remain in command handlers — all output routes through the output module
**Plans**: 2 plans
Plans:
- [x] 04-01-PLAN.md — Output module, CLI flags, and main.rs migration
- [x] 04-02-PLAN.md — Migrate all command handlers, menu, services, and add spinners

### Phase 5: Serve Services
**Goal**: Async process orchestration and OS-native file watching are encapsulated as testable service modules
**Depends on**: Phase 4
**Requirements**: SERVE-02, SERVE-06
**Success Criteria** (what must be TRUE):
  1. A file change is detected within 100ms using OS-native events (inotify/kqueue/ReadDirectoryChangesW) — not polling
  2. Rapid saves (editor atomic save bursts of 3-8 events) collapse into a single rebuild trigger via 300ms debounce
  3. All spawned child processes terminate and release ports when the process manager is dropped or `kill_all()` is called — no orphans survive
**Plans**: 2 plans
Plans:
- [x] 05-01-PLAN.md — Async dependencies and ProcessManager service (spawn, kill_all, lifecycle channel)
- [x] 05-02-PLAN.md — FileWatcher service (OS-native events, 300ms debounce, categorized event delivery)

### Phase 6: Serve Command
**Goal**: Developers can run `ezpm serve` and get their full Roblox build loop — instant startup, file-change rebuilds, and clean shutdown
**Depends on**: Phase 5
**Requirements**: SERVE-01, SERVE-03, SERVE-04, SERVE-05, SERVE-07
**Success Criteria** (what must be TRUE):
  1. `ezpm serve` executes the 8-step initial build sequence with per-step progress spinners, then starts Rojo on the port configured in ezpm.toml
  2. Saving a `.lua` or `.luau` file triggers per-file require fixing and DarkLua within 100ms; the change appears in Roblox Studio without manual action
  3. Creating or deleting a file triggers sourcemap regeneration; changing an `init.meta.json` triggers its copy to the build directory
  4. Pressing Ctrl-C kills Rojo and all DarkLua subprocesses, releases the port, and exits with code 0 — verified by checking no process holds the port afterward
**Plans**: 3 plans
Plans:
- [x] 06-01-PLAN.md — CLI --port flag, tokio dispatch, 8-step startup sequence with spinners and summary banner
- [x] 06-02-PLAN.md — tokio::select! watch loop with incremental rebuild handlers, batch detection, and Rojo auto-restart
- [ ] 06-03-PLAN.md — Gap closure: meta Create event reclassification, delete-wins dedup, init.* display names

### Phase 7: DX Polish
**Goal**: Every command failure is informative and machine-readable — structured errors with suggested fixes, correct exit codes throughout
**Depends on**: Phase 4
**Requirements**: ERR-01, ERR-02, ERR-03
**Gap Closure:** Closes requirement gaps ERR-01/02/03 + integration gap (menu.rs serve stub)
**Success Criteria** (what must be TRUE):
  1. Running `ezpm lint` when Selene finds violations exits with a non-zero code — `echo $?` returns non-zero; CI pipelines block on failure
  2. Running `ezpm format --check` when files are unformatted exits non-zero; CI pipelines block on failure
  3. A failed subprocess call (e.g., missing tool, wrong path) displays an error message with the failure context and a specific suggested fix — not a raw Rust panic or opaque error string
  4. All subprocess exit codes propagate through the error chain — no command silently swallows a tool failure and exits 0
  5. Interactive menu "serve" option dispatches to serve::run() via tokio runtime — no stub placeholder
**Plans**: 2 plans
Plans:
- [x] 07-01-PLAN.md — Error block helper, lint exit codes, format --check flag
- [ ] 07-02-PLAN.md — Subprocess propagation audit, menu serve dispatch, menu format update

### Phase 8: Integration Tests and CI
**Goal**: The CLI contracts for all major commands are verified by automated tests that run on every PR in under 2 minutes
**Depends on**: Phase 7
**Requirements**: TEST-01, TEST-02, TEST-03, TEST-04
**Gap Closure:** Closes requirement gaps TEST-01/02/03/04
**Success Criteria** (what must be TRUE):
  1. `cargo test` runs integration tests for `ezpm init`, `ezpm fix-requires`, and `ezpm alias` subcommands against real filesystem fixtures with TempDir isolation
  2. Integration tests verify exit code contracts — commands that should fail return non-zero, commands that should succeed return 0
  3. A PR opened against main triggers the full test suite (unit + integration) in GitHub Actions; the suite must pass before merge is allowed
  4. CI pipeline completes in under 2 minutes due to Rust build cache (`Swatinem/rust-cache`)
**Plans**: TBD

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Foundation | v1.0 | 2/2 | Complete | 2026-02-24 |
| 2. Core Services | v1.0 | 3/3 | Complete | 2026-02-24 |
| 3. Simple Commands | v1.0 | 5/5 | Complete | 2026-02-24 |
| 4. Output Foundation | v1.1 | Complete    | 2026-02-24 | 2026-02-24 |
| 5. Serve Services | 2/2 | Complete   | 2026-02-25 | - |
| 6. Serve Command | 3/3 | Complete   | 2026-02-25 | - |
| 7. DX Polish | 2/2 | Complete   | 2026-02-25 | - |
| 8. Integration Tests and CI | v1.1 | 0/TBD | Not started | - |
