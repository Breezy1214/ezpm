# Requirements: EZPM Rust Rewrite

**Defined:** 2026-02-24
**Core Value:** Every current EZPM workflow must work identically (or better) in the Rust version — zero regression on the developer experience that Roblox users depend on.

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### CLI Foundation

- [ ] **CLI-01**: User sees interactive arrow-key menu when running `ezpm` with no arguments
- [ ] **CLI-02**: User can run any command directly as a subcommand (e.g., `ezpm serve`, `ezpm init`)
- [ ] **CLI-03**: User sees colored terminal output with ANSI codes; NO_COLOR env var respected
- [ ] **CLI-04**: CLI returns proper non-zero exit codes on failure for CI pipeline compatibility
- [ ] **CLI-05**: User sees version check notification on startup when a newer version is available on GitHub
- [ ] **CLI-06**: User sees structured error messages with context, source location, and suggested fixes
- [ ] **CLI-07**: User sees progress spinners/bars during multi-step operations (init, install, serve startup)
- [ ] **CLI-08**: User can pass `--verbose` or `--quiet` flags to control output verbosity
- [ ] **CLI-09**: `ezpm help` and `-h`/`--help` flags display all available commands and usage

### Configuration

- [x] **CFG-01**: Tool reads ezpm.toml in the same format as the Luau version (backward compatible)
- [ ] **CFG-02**: User can add an alias via `ezpm alias add` with interactive prompts
- [ ] **CFG-03**: User can remove an alias via `ezpm alias remove` with selection prompt
- [ ] **CFG-04**: User can list all configured aliases via `ezpm alias list`
- [ ] **CFG-05**: User can sync aliases from ezpm.toml via `ezpm alias sync`, regenerating .darklua.json and .luaurc
- [x] **CFG-06**: Tool auto-generates .darklua.json from configured aliases with correct path-require rules
- [x] **CFG-07**: Tool auto-generates .luaurc from configured aliases for Luau LSP path resolution
- [x] **CFG-08**: User can configure Rojo port in ezpm.toml under `[serve]` section (default 34872)

### Project Setup

- [x] **INIT-01**: User can initialize a new project via `ezpm init` with interactive prompts for source directory
- [x] **INIT-02**: Init auto-detects existing project files (.darklua.json, rokit.toml, wally.toml, default.project.json) and skips creating them
- [x] **INIT-03**: Init offers to import aliases from existing .darklua.json during setup
- [x] **INIT-04**: Init creates scaffolding directories based on configured aliases
- [x] **INIT-05**: Init generates ezpm.toml with default aliases (Client, Server, Shared, Packages, ServerPackages)
- [x] **INIT-06**: Init optionally generates rokit.toml with default tool versions
- [x] **INIT-07**: Init generates default.project.json with correct Rojo file tree structure
- [x] **INST-01**: User can run `ezpm install` to execute Rokit install for all pinned tools
- [x] **INST-02**: Install runs Wally package installation if wally.toml exists
- [x] **INST-03**: Install runs wally-package-types for type generation after Wally install
- [x] **INST-04**: User can run `ezpm setup-wally-packages` to clear old packages, install, generate sourcemap, and run wally-package-types

### Build Pipeline

- [x] **BUILD-01**: User can run `ezpm fix-requires` to scan all .lua/.luau files and rewrite require paths to @alias notation
- [x] **BUILD-02**: Require fixer matches paths against configured aliases using longest-match-first strategy
- [x] **BUILD-03**: Require fixer skips external aliases (Packages/, ServerPackages/) and built-in Roblox aliases (@self, @game)
- [x] **BUILD-04**: Require fixer only writes files to disk when changes are actually made
- [x] **BUILD-05**: Require fixer displays all changes made to the user
- [x] **BUILD-06**: Tool executes DarkLua transformation on full source tree (darklua process src/ build/)
- [x] **BUILD-07**: Tool executes DarkLua transformation on individual files for incremental builds
- [x] **BUILD-08**: Tool generates Rojo sourcemap via `rojo sourcemap . -o sourcemap.json`
- [x] **BUILD-09**: Tool copies init.meta.json files from source to build directory

### Dev Server

- [ ] **SERVE-01**: User can run `ezpm serve` which executes full build pipeline then starts file watching and Rojo
- [ ] **SERVE-02**: Serve generates build.project.json from default.project.json with paths remapped to build directory
- [ ] **SERVE-03**: Serve cleans old DarkLua build directory before rebuilding
- [ ] **SERVE-04**: File watcher uses OS-native events (inotify/FSEvents/ReadDirectoryChangesW) for instant change detection
- [ ] **SERVE-05**: File watcher debounces rapid events (300-500ms window) to prevent pipeline floods
- [ ] **SERVE-06**: On file change: regenerates sourcemap, fixes requires for changed file, runs DarkLua for changed directory
- [ ] **SERVE-07**: On file create: fixes requires, regenerates sourcemap, runs DarkLua build
- [ ] **SERVE-08**: On directory remove: removes from build, regenerates sourcemap
- [ ] **SERVE-09**: Rojo serve subprocess starts on configured port (default 34872) with build.project.json
- [ ] **SERVE-10**: Graceful shutdown on Ctrl-C kills all child processes (Rojo, DarkLua) — no orphaned processes
- [ ] **SERVE-11**: Graceful shutdown releases port so next `ezpm serve` works immediately

### Code Quality

- [x] **QUAL-01**: User can run `ezpm lint` which executes Selene and StyLua --check
- [x] **QUAL-02**: Lint skips gracefully if Selene or StyLua is not installed
- [x] **QUAL-03**: User can run `ezpm format` which executes StyLua on source directory
- [x] **QUAL-04**: User can run `ezpm docs` to launch Moonwave documentation server (config-gated)

### Distribution

- [x] **DIST-01**: Project produces cross-platform binaries for 6 targets (Linux/macOS/Windows, x86_64/aarch64)
- [x] **DIST-02**: Binary is a single static executable with no runtime dependencies
- [x] **DIST-03**: GitHub Actions CI/CD pipeline builds and releases binaries on version bump
- [x] **DIST-04**: Binary is installable via Rokit (`rokit add Breezy1214/ezpm`)

### Testing

- [x] **TEST-01**: Unit tests cover config parsing, alias resolution, require path fixing logic, and semver comparison
- [ ] **TEST-02**: Integration tests cover command execution pipelines (init, serve startup/shutdown, fix-requires)
- [ ] **TEST-03**: CI pipeline runs full test suite on every PR

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### CLI Enhancements

- **CLI-10**: `--dry-run` flag for fix-requires to preview changes without writing
- **CLI-11**: Shell completion generation (bash, zsh, fish) via clap built-in
- **CLI-12**: `ezpm alias add` supports non-interactive mode via args (`ezpm alias add Client src/client/`)

### Advanced Features

- **ADV-01**: Config validation command (`ezpm check`) validates ezpm.toml, .darklua.json, .luaurc consistency
- **ADV-02**: Watch mode for `fix-requires` standalone (without full serve pipeline)
- **ADV-03**: Dry-run import preview during `init` showing what aliases would be imported

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Built-in Rojo replacement | Rojo is a complex, maintained tool with its own protocol and plugin; EZPM orchestrates it |
| Plugin/extension API | Adds significant API surface and versioning burden for v1 |
| Built-in Luau language server | LSP is a separate concern; EZPM generates .luaurc so existing LSPs work |
| Package registry / publishing | Wally handles this; duplicating it fragments the ecosystem |
| GUI / desktop application | Roblox developers are technical enough for CLI; interactive menu covers discoverability |
| Hot reload without Rojo | Would require implementing Roblox plugin protocol; not feasible |
| Auto-install Rokit | Bootstrap paradox — you need Rokit to install EZPM |
| Multiple project profiles | Adds config complexity; power users can use multiple project directories |
| Real-time DarkLua error streaming | Complicates subprocess management; capture full stderr and display with context instead |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| CLI-01 | Phase 3 | Pending |
| CLI-02 | Phase 3 | Pending |
| CLI-03 | Phase 5 | Pending |
| CLI-04 | Phase 5 | Pending |
| CLI-05 | Phase 3 | Pending |
| CLI-06 | Phase 5 | Pending |
| CLI-07 | Phase 5 | Pending |
| CLI-08 | Phase 5 | Pending |
| CLI-09 | Phase 3 | Pending |
| CFG-01 | Phase 1 | Complete |
| CFG-02 | Phase 3 | Pending |
| CFG-03 | Phase 3 | Pending |
| CFG-04 | Phase 3 | Pending |
| CFG-05 | Phase 3 | Pending |
| CFG-06 | Phase 3 | Complete |
| CFG-07 | Phase 3 | Complete |
| CFG-08 | Phase 3 | Complete |
| INIT-01 | Phase 3 | Complete |
| INIT-02 | Phase 3 | Complete |
| INIT-03 | Phase 3 | Complete |
| INIT-04 | Phase 3 | Complete |
| INIT-05 | Phase 3 | Complete |
| INIT-06 | Phase 3 | Complete |
| INIT-07 | Phase 3 | Complete |
| INST-01 | Phase 3 | Complete |
| INST-02 | Phase 3 | Complete |
| INST-03 | Phase 3 | Complete |
| INST-04 | Phase 3 | Complete |
| BUILD-01 | Phase 2 | Complete |
| BUILD-02 | Phase 2 | Complete |
| BUILD-03 | Phase 2 | Complete |
| BUILD-04 | Phase 2 | Complete |
| BUILD-05 | Phase 2 | Complete |
| BUILD-06 | Phase 2 | Complete |
| BUILD-07 | Phase 2 | Complete |
| BUILD-08 | Phase 2 | Complete |
| BUILD-09 | Phase 2 | Complete |
| SERVE-01 | Phase 4 | Pending |
| SERVE-02 | Phase 4 | Pending |
| SERVE-03 | Phase 4 | Pending |
| SERVE-04 | Phase 4 | Pending |
| SERVE-05 | Phase 4 | Pending |
| SERVE-06 | Phase 4 | Pending |
| SERVE-07 | Phase 4 | Pending |
| SERVE-08 | Phase 4 | Pending |
| SERVE-09 | Phase 4 | Pending |
| SERVE-10 | Phase 4 | Pending |
| SERVE-11 | Phase 4 | Pending |
| QUAL-01 | Phase 3 | Complete |
| QUAL-02 | Phase 3 | Complete |
| QUAL-03 | Phase 3 | Complete |
| QUAL-04 | Phase 3 | Complete |
| DIST-01 | Phase 1 | Complete |
| DIST-02 | Phase 1 | Complete |
| DIST-03 | Phase 1 | Complete |
| DIST-04 | Phase 1 | Complete |
| TEST-01 | Phase 2 | Complete |
| TEST-02 | Phase 4 | Pending |
| TEST-03 | Phase 5 | Pending |

**Coverage:**
- v1 requirements: 55 total
- Mapped to phases: 55
- Unmapped: 0

---
*Requirements defined: 2026-02-24*
*Last updated: 2026-02-24 after roadmap creation — all requirements mapped*
