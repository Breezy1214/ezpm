# ezpm

**One CLI for your whole Roblox dev pipeline.** Rojo, Wally, Selene, and StyLua wired together.

```bash
ezpm serve
```

File watching, require conversion, sourcemap generation, and Rojo live sync, all in one process.

## Why

| Problem | ezpm |
|---|---|
| Broken `require()` paths after moving files | Auto-rewrites requires to Rojo's `@game/...` path |
| Hand-syncing `.luaurc` and Rojo configs | One `[aliases]` table in `ezpm.toml` |
| Five terminal tabs for one dev loop | `ezpm serve` runs everything |
| Toolchain drift between machines | Rokit-managed pins, auto-synced to ezpm's tested versions |
| Circular deps and layer violations | `ezpm check` catches cycles, forbidden imports, and dead code |

## Install

```bash
rokit add Breezy1214/ezpm
```

No Rokit? Grab a binary from [Releases](https://github.com/Breezy1214/ezpm/releases) (Linux/macOS/Windows, x86_64 and aarch64), or `cargo install --path .`.

Update with `rokit update ezpm && rokit install`.

## Quick start

```bash
ezpm init     # scaffold a new project, or adopt an existing Rojo one
ezpm serve    # dev server is live
```

`ezpm init` scans the project root for `*.project.json` files. If it finds any, it keeps your existing Rojo project as the source of truth (recording it under `[rojo]` in `ezpm.toml`), infers your source root from its path mappings, and imports aliases from `.luaurc` when available. If it finds none, it scaffolds `default.project.json`, the `src/` tree, `rokit.toml`, and `.luaurc` from scratch.

```bash
ezpm init --dry-run   # print what init would create or change; writes nothing
```

If `init` fails or you cancel it part-way through, it puts every file and folder back as it was.

## Commands

```
ezpm                                 Interactive menu (arrow-key navigation)
ezpm serve [-p <port>]               Watch + transform + Rojo live sync
ezpm init [--dry-run]                Create or adopt a project
ezpm check [PATHS]                   Dependency analysis (cycles, layer and feature rules, unused modules)
ezpm fix-requires [--check|--dry-run] [PATHS]
                                     Rewrite requires to @game/... form, or only report them
ezpm build [ROJO_ARGS]               rojo build on the project, e.g. ezpm build -o game.rbxl
ezpm sourcemap [ROJO_ARGS]           rojo sourcemap on the project, e.g. ezpm sourcemap -o sourcemap.json
ezpm where <PATHS>                   Where files land in the game, and the require for each
ezpm explain <code> | --list         What a diagnostic code means, and how to fix it
ezpm install                         Rokit tools + Wally packages + type generation
ezpm setup-wally-packages            Staged reinstall of Wally deps + sourcemap/types
ezpm lint                            Selene + StyLua --check
ezpm format [--check]                StyLua format (or verify only, for CI)
ezpm alias add <Name> <path> [--create-dir]
ezpm alias remove <Name>...
ezpm alias list | sync               Manage path aliases (add/remove prompt when run without arguments)
ezpm hook stop [--format cursor]     Stop hook for coding agents (see below)
```

Global flags: `--json`, `--verbose`, `--quiet`, `--color <auto|always|never>`.

### Exit codes, JSON and CI

| Exit | Meaning |
|---|---|
| 0 | Done (warnings included) |
| 1 | The project has errors, or a tool failed |
| 2 | The command line is wrong |
| 130 / 143 / 129 | Stopped by Ctrl-C / SIGTERM / a closed terminal |

`build` and `sourcemap` exit with Rojo's own code.

With `--json`, every command prints one JSON document on one line of stdout and nothing else. A failure before a command has anything to show prints `{"error": {"kind", "message", "hint"}}`, plus `code` (`tool.missing`, `tool.failed`) when a tool is at fault. A wrong command line under `--json` is such an error too, and `ezpm.toml` warnings are added to the document's `diagnostics` array. `serve --json` instead prints one event per line: `ready`, `synced`, `rebuilt`, `fixFailed`, `sourcemapFailed`, `configReloaded`, `rojoRestarted`, `rojoCrashed`, `watcherError`, `stopped`, and `rojo` for each line Rojo prints.

Every problem has a stable code, such as `check.cycle`. Match on the code, not the message. [docs/diagnostics.md](docs/diagnostics.md) lists them all, and `ezpm explain <code>` prints the same text in your terminal. Under GitHub Actions, diagnostics are also written as workflow commands, so they show on the pull request's files:

```yaml
- uses: CompeyDev/setup-rokit@v0.2.1
- run: ezpm check
- run: ezpm fix-requires --check
- run: ezpm build -o game.rbxl
```

### `ezpm serve`

1. Syncs `rokit.toml` tool pins that lag behind ezpm's bundled versions, then runs `rokit install`
2. Generates `.luaurc` from `ezpm.toml`
3. Generates a Rojo sourcemap
4. Resolves shorthand requires to Roblox `@game/...` string paths
5. Starts Rojo live sync against your project file
6. Watches source files and repairs requires after edits, moves, and renames

Port comes from `-p`, then `[serve] port`, then `34872`. If it's already taken, ezpm reports it up front instead of letting Rojo fail.

### `ezpm check`

Static analysis of your require graph:

- **Circular dependencies** — A → B → C → A
- **Architecture violations** — client importing server, per your `[check.forbid]` rules
- **Feature boundaries** — one feature requiring another, per `[check] feature_roots`
- **Unused modules** — nothing requires them

```bash
ezpm check                        # human-readable
ezpm check --json                 # for CI and editors
ezpm check src/shared/Shop        # only findings about these files or folders
```

### `ezpm fix-requires`

Resolves simple module names and editor aliases to Roblox-supported string requires. For example, `require("Signal")` becomes `require("@game/ReplicatedStorage/Shared/Utils/Signal")`. Existing paths are repaired when uniquely identifiable modules move. If two modules have the same name, ezpm reports the ambiguity instead of guessing.

```bash
ezpm fix-requires             # rewrite them
ezpm fix-requires --dry-run   # list each rewrite with its line; write nothing
ezpm fix-requires --check     # same, but exit 1 if there are any (for CI)
```

### `ezpm build` and `ezpm sourcemap`

Each syncs `.luaurc` from `ezpm.toml`, then runs Rojo on the project that `[rojo] project` selects, passing your arguments through: `ezpm build -o game.rbxl`, `ezpm build --watch -o game.rbxl`, `ezpm sourcemap -o sourcemap.json`. Put ezpm's own flags first (`ezpm --json build -o game.rbxl`); everything after the command goes to Rojo.

### `ezpm where`

```
$ ezpm where src/shared/Utils/Signal.luau
src/shared/Utils/Signal.luau → ReplicatedStorage.Shared.Utils.Signal (ModuleScript)
  require("@game/ReplicatedStorage/Shared/Utils/Signal")
```

A file that Rojo doesn't sync is reported as `where.unmapped`, with exit code 1.

## Configuration

Everything lives in `ezpm.toml`. Only `[aliases]` really matters; the rest has defaults.

```toml
[project]
name = "my-game"

[paths]
src = "src"

[display]
file_changes = true
logs_enabled = true
check_updates = true      # or set EZPM_NO_UPDATE_CHECK=1; always off when CI is set

[aliases]
Client = "src/client/"
Server = "src/server/"
Shared = "src/shared/"
Packages = "Packages/"
ServerPackages = "ServerPackages/"

[serve]
port = 34872
```

`ezpm.toml` is the configuration source of truth: `[rojo].project` selects the Rojo project and `[aliases]` generates `.luaurc`. The selected project's Rojo sourcemap is the sole authority for mapping source files to Roblox instances. `ezpm serve` reads it directly from Rojo and refreshes its in-memory index whenever project topology changes.

### `[rojo]` (optional)

By default ezpm serves `default.project.json` directly. To use another project:

```toml
[rojo]
project = "game.project.json"
```

The fixer supports bare module names, `@Alias/module`, `@game/Service/module`, `@self/module`, and relative string requires. Rojo serves the source tree directly.

### `[check]` (optional)

```toml
[check]
# Roots for unused-module analysis. Omitted: ezpm auto-detects src/<layer>/init.lua(u).
entry_points = ["src/client/init.luau", "src/server/init.luau"]

[check.layers]
client = "src/client/"
server = "src/server/"
shared = "src/shared/"

[[check.forbid]]
from = "client"
to = "server"
reason = "Client must never import server modules"
```

- `from` / `to` must match `[check.layers]` keys exactly.
- Layer matching is path-prefix based — keep the trailing `/`.
- Only cross-layer imports are checked; same-layer imports are always allowed.
- With no `entry_points` and no `src/<layer>/init.lua(u)`, every module counts as reachable.

To keep features from depending on each other, name the folders that hold them:

```toml
[check]
feature_roots = ["src/server/", "src/client/", "src/shared/"]
shared_features = ["Util"]
```

Every folder directly under a root is a feature. A feature may require itself and the `shared_features`, and nothing else. A feature keeps its name across roots, so `src/server/Shop` may require `src/shared/Shop` but not `src/shared/Inventory`.

Unknown keys in `ezpm.toml` get a `config.unknownField` warning that suggests the closest key.

## Toolchain

Managed through [Rokit](https://github.com/rojo-rbx/rokit). `ezpm serve` bumps any pin older than ezpm's tested version and reinstalls; newer pins are left alone.

| Tool | Role | Tested |
|---|---|---|
| [Rojo](https://github.com/rojo-rbx/rojo) | Studio sync | 7.7.0 |
| [Wally](https://github.com/UpliftGames/wally) | Package manager | 0.3.2 |
| [wally-package-types](https://github.com/JohnnyMorganz/wally-package-types) | Package type re-exports | 1.6.2 |
| [Selene](https://github.com/Kampfkarren/selene) | Linter | 0.31.0 |
| [StyLua](https://github.com/JohnnyMorganz/StyLua) | Formatter | 2.5.2 |

`setup-wally-packages` installs into a temporary directory before updating package files in place, preserving existing directories and unchanged files for Rojo watchers.

## VS Code extension

`vscode-ezpm/` wraps the CLI: serve start/stop/status, `ezpm check` results as inline diagnostics on the line of the offending require (cycles, rule and feature violations as errors, unused modules as warnings), and every other command from the palette. Point it at a binary with `ezpm.binaryPath`, or leave it empty to use `PATH`. See [its README](vscode-ezpm/README.md).

## Coding agents

`ezpm init` adds a short block of rules to `AGENTS.md`, between `<!-- ezpm -->` markers, and imports it into `CLAUDE.md` with `@AGENTS.md`. The rules tell an agent how requires work here and to run `ezpm check` and `ezpm fix-requires --check` on the files it changes. Running init again replaces an outdated block and keeps your text around it. Pass `--no-agent-rules` to skip this, or run `ezpm init --agent-rules` in a project that is already set up.

An agent can still forget to run the checks, so `ezpm hook stop` runs them on the Luau files changed in git when the agent ends its turn. It sends each new error back as the agent's next prompt and never repeats one. `ezpm init --agent-hook` registers it for Claude Code in `.claude/settings.json`. For other agents, add it to their Stop event by hand:

| Agent | File | Event | Command |
|---|---|---|---|
| Claude Code | `.claude/settings.json` | `Stop` | `ezpm hook stop` |
| Codex | `.codex/hooks.json` | `Stop` | `ezpm hook stop` |
| Cursor | `.cursor/hooks.json` | `stop` | `ezpm hook stop --format cursor` |
| Copilot | `.github/hooks/ezpm.json` | `agentStop` | `ezpm hook stop` |

## Limitations

- Dynamic (non-string) requires are passed through unchanged
- Needs a Rojo project file in the project root (`default.project.json`, or one named under `[rojo]`)
- One Rojo project per `ezpm.toml`; roblox-ts and multi-place setups aren't supported yet
- Pre-1.0 — the config format may still evolve; see [CHANGELOG.md](CHANGELOG.md)

## Contributing

Rust source in `src/`, entrypoint `src/main.rs`, tests in `tests/`. Run `cargo test` before opening a PR.

## License

MIT — see [LICENSE](LICENSE).
