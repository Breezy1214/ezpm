# ezpm

**One CLI for your whole Roblox dev pipeline.** Rojo, Wally, Selene, and StyLua wired together.

```bash
ezpm serve
```

File watching, require conversion, sourcemap generation, and Rojo live sync, all in one process.

## Why

| Problem | ezpm |
|---|---|
| Broken `require()` paths after moving files | Auto-rewrites requires to `@alias/` notation |
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

## Commands

```
ezpm                      Interactive menu (arrow-key navigation)
ezpm serve [-p <port>]    Watch + transform + Rojo live sync
ezpm init [--dry-run]     Create or adopt a project
ezpm check [--json]       Dependency analysis (cycles, layer rules, unused modules)
ezpm fix-requires         Rewrite requires across the source tree to @alias/ form
ezpm install              Rokit tools + Wally packages + type generation
ezpm setup-wally-packages Clean reinstall of Wally deps + sourcemap/types
ezpm lint                 Selene + StyLua --check
ezpm format [--check]     StyLua format (or verify only, for CI)
ezpm alias <add|remove|list|sync>   Manage path aliases
ezpm docs                 Moonwave documentation server
```

Global flags: `--verbose`, `--quiet`, `--color <auto|always|never>`.

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
- **Unused modules** — nothing requires them

```bash
ezpm check          # human-readable
ezpm check --json   # for CI
```

### `ezpm fix-requires`

Resolves simple module names and editor aliases to Roblox-supported string requires. For example, `require("Signal")` becomes `require("@game/ReplicatedStorage/Shared/Utils/Signal")`. Existing paths are repaired when uniquely identifiable modules move. If two modules have the same name, ezpm reports the ambiguity instead of guessing.

## Configuration

Everything lives in `ezpm.toml`. Only `[aliases]` really matters; the rest has defaults.

```toml
[project]
name = "my-game"

[paths]
src = "src"

[display]
file_changes = true
docs_enabled = false      # must be true for `ezpm docs`
logs_enabled = true
check_updates = true      # or set EZPM_NO_UPDATE_CHECK=1

[aliases]
Client = "src/client/"
Server = "src/server/"
Shared = "src/shared/"
Packages = "Packages/"
ServerPackages = "ServerPackages/"

[serve]
port = 34872
require_fix_mode = "hybrid"   # strict | hybrid | fast
```

`ezpm.toml` is the source of truth. Aliases drive require resolution and generated `.luaurc`; `ezpm serve` keeps it synchronized automatically.

**`require_fix_mode`** — how aggressively the watcher rewrites requires:

- **`strict`** — full-tree fix on every change. Maximum correctness.
- **`hybrid`** — single-file on edits, full-tree on deletes/renames. Good default.
- **`fast`** — single-file only. Fastest on large trees; may leave stale requires after structural changes.

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

## Toolchain

Managed through [Rokit](https://github.com/rojo-rbx/rokit). `ezpm serve` bumps any pin older than ezpm's tested version and reinstalls; newer pins are left alone.

| Tool | Role | Tested |
|---|---|---|
| [Rojo](https://github.com/rojo-rbx/rojo) | Studio sync | 7.7.0 |
| [Wally](https://github.com/UpliftGames/wally) | Package manager | 0.3.2 |
| [wally-package-types](https://github.com/JohnnyMorganz/wally-package-types) | Package type re-exports | 1.6.2 |
| [Selene](https://github.com/Kampfkarren/selene) | Linter | 0.31.0 |
| [StyLua](https://github.com/JohnnyMorganz/StyLua) | Formatter | 2.5.2 |

`setup-wally-packages` wipes and reinstalls package directories, so it refuses to touch anything that is not a real top-level directory in the project root — no symlinks, no paths inside your source tree.

## VS Code extension

`vscode-ezpm/` wraps the CLI: serve start/stop/status, `ezpm check` results as inline diagnostics (cycles and rule violations as errors, unused modules as warnings), and every other command from the palette. Point it at a binary with `ezpm.binaryPath`, or leave it empty to use `PATH`. See [its README](vscode-ezpm/README.md).

## Limitations

- Dynamic (non-string) requires are passed through unchanged
- Needs a Rojo project file in the project root (`default.project.json`, or one named under `[rojo]`)
- v0.2 — the config format may still evolve

## Contributing

Rust source in `src/`, entrypoint `src/main.rs`, tests in `tests/`. Run `cargo test` before opening a PR.

## License

MIT — see [LICENSE](LICENSE).
