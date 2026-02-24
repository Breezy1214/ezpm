# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-02-24)

**Core value:** Every current EZPM workflow must work identically (or better) in the Rust version — zero regression on the developer experience that Roblox users depend on.
**Current focus:** Phase 3 — Simple Commands

## Current Position

Phase: 3 of 5 (Simple Commands)
Plan: 5 of 5 in current phase
Status: In Progress
Last activity: 2026-02-24 — Completed 03-04: Alias command handlers (alias_add, alias_remove, alias_list, alias_sync)

Progress: [████████░░] 75%

## Performance Metrics

**Velocity:**
- Total plans completed: 5
- Average duration: 10 min
- Total execution time: 0.8 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 01-foundation | 2 | 37 min | 18 min |
| 02-core-services | 2 | 10 min | 5 min |
| 03-simple-commands | 4 | 15 min | 4 min |

**Recent Trend:**
- Last 5 plans: 35 min, 2 min, 8 min, 2 min, 2 min
- Trend: fast

*Updated after each plan completion*
| Phase 02-core-services P03 | 2 | 2 tasks | 2 files |
| Phase 03-simple-commands P01 | 2 | 2 tasks | 4 files |
| Phase 03-simple-commands P02 | 2 | 2 tasks | 3 files |
| Phase 03-simple-commands P03 | 5 | 2 tasks | 2 files |
| Phase 03-simple-commands P04 | 3 | 2 tasks | 2 files |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [Pre-phase]: Rewrite in Rust (not incremental port) — clean break enables proper architecture and type safety
- [Pre-phase]: Event-driven file watcher (notify crate) — eliminates 1-second polling lag, primary UX improvement
- [Pre-phase]: Keep TOML config format — backward compatible with existing ezpm.toml files, no user migration
- [Pre-phase]: Interactive menu + subcommands — menu for discovery, subcommands for scripting
- [Pre-phase]: Thorough test suite from day one — current Luau version has zero tests
- [01-01]: import_aliases_from_dir(path) API over cwd-based for testability without set_current_dir
- [01-01]: Removed Help variant from Commands enum — clap provides built-in help, duplicates cause startup panic
- [01-01]: serde_ignored::deserialize takes owned Deserializer (not &mut ref) — compiler-suggested fix
- [01-02]: Tag-push trigger (v*) for releases — explicit operator control, conventional for Rust tooling
- [01-02]: Native ARM runners for Linux aarch64 (ubuntu-24.04-arm) and macOS aarch64 (macos-latest) — no QEMU
- [01-02]: macOS x86_64 uses macos-13 (Intel) since macos-latest is now ARM-only
- [01-02]: Clippy and fmt jobs run only on ubuntu — platform-independent checks, avoids 3x CI cost
- [02-01]: OnceLock static regex instead of lazy_static/once_cell — stdlib pattern (Rust 1.70+), no extra crate
- [02-01]: process_file_content is a pure function (no FS I/O) — all business logic testable without tempdir
- [02-01]: str::replace for require rewriting — matches Luau content:gsub semantics, handles multiple occurrences
- [02-01]: Trailing slash normalisation on alias real paths in build_sorted_src_aliases (Pitfall 3 prevention)
- [02-01]: Alphabetical tie-breaking on equal-length alias paths for deterministic sort (Pitfall 6 prevention)
- [02-02]: DarkluaResult type reused in sourcemap.rs — both tools share stdout/stderr/exit_code structure, no duplication
- [02-02]: Synchronous std::process::Command — async subprocess orchestration deferred to Phase 4
- [02-02]: .darklua.json has NO sources map — Pitfall 1, uses bare convert_require with current: { name: "luau" }
- [02-02]: char array pattern ['v', 'V'] for trim_start_matches (idiomatic Rust, clippy-clean)
- [Phase 02-03]: Match Commands::FixRequires before generic cmd arm; use unreachable!() in generic arm for compiler-verified exhaustiveness
- [Phase 02-03]: Config loaded once before match as Option<EzpmConfig>, consumed via unwrap_or_default() in FixRequires handler — avoids double-loading
- [03-01]: ureq = "3" added as synchronous HTTP client — no async runtime needed for single background thread
- [03-01]: Separate EzpmTomlOutput serialization struct from EzpmConfig — controls TOML field order without affecting deserialization
- [03-01]: check_updates placed in DisplayConfig alongside docs_enabled, logs_enabled (boolean feature flag pattern)
- [03-01]: aliases BTreeMap declared last in EzpmTomlOutput to prevent TOML table ordering error (Pitfall 3)
- [03-02]: install_tools uses Path::new('wally.toml').exists() gate before running wally — prevents wally install failure on projects without wally
- [03-02]: lint() returns Ok even when lint issues found — lint output is informational, not fatal (matches Luau runLinting)
- [03-02]: setup_wally_packages runs two sourcemap passes (before and after wally-package-types) — matches Luau setupWallyPackages
- [03-03]: Rojo project tree built as serde_json::Map with sorted alias iteration for deterministic output
- [03-03]: Alias import from .darklua.json uses MultiSelect with all-selected-by-default (CONTEXT.md checklist pattern)
- [03-03]: Non-src aliases detected by src_prefix_slash check — skipped in Rojo tree without separate blocklist
- [03-04]: All four alias functions written atomically in one file — ensures consistent load-modify-save-regenerate pattern
- [03-04]: alias_list takes &Option<HashMap> parameter for testability and caller flexibility
- [03-04]: alias_sync explicitly calls load_config() to fulfill CFG-05 disk-reload requirement

### Pending Todos

None yet.

### Blockers/Concerns

- [Phase 4 flag]: Concurrent tokio process orchestration for serve (DarkLua + Rojo + file watcher) needs deeper research during phase 4 planning — validate specific stream draining pattern and DarkLua per-file invocation
- [Phase 5 flag]: cargo-dist 0.31.0 aarch64-pc-windows-msvc target support status unconfirmed — verify during phase 5 planning, drop target if unsupported

## Session Continuity

Last session: 2026-02-24
Stopped at: Completed 03-04-PLAN.md — Alias command handlers (alias_add, alias_remove, alias_list, alias_sync)
Resume file: None
