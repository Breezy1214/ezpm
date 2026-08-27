# ezpm

**One CLI for your whole Roblox dev pipeline.** Rojo, DarkLua, Wally, Selene, and StyLua wired together.

```bash
ezpm serve
```

File watching, require-path fixing, DarkLua transforms, sourcemap generation, and Rojo live sync, all in one process.

## Why

| Problem | ezpm |
|---|---|
| Broken `require()` paths after moving files | Auto-rewrites requires to `@alias/` notation |
| Hand-syncing `.darklua.json`, `.luaurc`, and Rojo configs | One `[aliases]` table in `ezpm.toml` — configs regenerated for you |
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

`ezpm init` scans the project root for `*.project.json` files. If it finds any, it keeps your existing Rojo project as the source of truth (recording it under `[rojo]` in `ezpm.toml`), infers your source root from its path mappings, and offers to import aliases from `.luaurc` or `.darklua.json`. If it finds none, it scaffolds `default.project.json`, the `src/` tree, `rokit.toml`, `.darklua.json`, and `.luaurc` from scratch.

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
2. Generates the build Rojo project (`build.project.json`) from your template, remapping source paths to the DarkLua output directory
3. Cleans the build directory
4. Generates a Rojo sourcemap from your source project
5. Fixes require paths across the source tree
6. Runs DarkLua to turn `@alias/` requires into Roblox paths
7. Copies `init.meta.json` files into the build tree
8. Starts Rojo live sync against the generated project
9. Watches `src/` (`.lua`, `.luau`, `init.meta.json`, `*.model.json`) and re-runs steps 4–7 on change

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

Rewrites `require()` calls to your `@alias/` paths — `require(game.ReplicatedStorage.Shared.Utils.Signal)` becomes `require("@Shared/Utils/Signal")`. When `luau-lsp` 1.65.0+ auto-imports insert `@game/...` string requires, alias-equivalent paths are normalized back to `@alias/...`; unmatched or intentional `@game` requires are left alone. Relative (`./`, `../`) requires are unsupported and left untouched.

## Configuration

Everything lives in `ezpm.toml`. Only `[aliases]` really matters; the rest has defaults.

```toml
[project]
name = "my-game"

[paths]
src = "src"
darklua_build = "darklua_build"

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

Aliases drive `.darklua.json` and `.luaurc`. After editing them by hand, run `ezpm alias sync` to regenerate both.

**`require_fix_mode`** — how aggressively the watcher rewrites requires:

- **`strict`** — full-tree fix on every change. Maximum correctness.
- **`hybrid`** — single-file on edits, full-tree on deletes/renames. Good default.
- **`fast`** — single-file only. Fastest on large trees; may leave stale requires after structural changes.

### `[rojo]` (optional)

By default ezpm reads `default.project.json` and writes `build.project.json`, remapping `src` → `darklua_build`. Override any of that:

```toml
[rojo]
project = "game.project.json"            # your hand-maintained template
generated_project = "build.project.json" # what Rojo actually serves

[[rojo.path_maps]]
source = "src"
build = "darklua_build"

[[rojo.path_maps]]
source = "vendor"
build = "darklua_build/vendor"
```

Paths must be project-relative with no `..` traversal, and the generated project must differ from the template. `path_maps` replaces the default `src` → `darklua_build` mapping entirely, so include it if you still need it; longer source prefixes match first. `ezpm init` writes this section for you when it adopts an existing project.

### `[darklua]` (optional)

Omit it and ezpm uses the defaults below. Include it and it becomes the source of truth, written **verbatim** to `.darklua.json` — your rules replace the defaults rather than merging with them.

```toml
[darklua]
process = [
    { rule = "convert_require", current = { name = "luau" }, target = { name = "roblox", rojo_sourcemap = "sourcemap.json", indexing_style = "find_first_child" } },
    "make_assignment_local",
    "compute_expression",
    "remove_unused_if_branch",
    "remove_unused_while",
    "filter_after_early_return",
    "remove_nil_declaration",
    "remove_empty_do",
]

[darklua.loaders]
"**/*.model.json" = "copy"
```

Notes ([rules reference](https://darklua.com/docs/rules/)):

- **`convert_require`** rewrites `@alias/...` requires into Roblox paths. Keep it, or alias requires will not resolve — ezpm warns if it is missing. Your aliases are injected into its `current.sources` automatically, so you do not list them here.
- **`make_assignment_local`** lowers Luau's [`const`](https://rfcs.luau.org/const-keyword.html) keyword to `local` so the build runs on Roblox.
- The `**/*.model.json` loader copies Rojo model files unchanged instead of letting DarkLua 0.19 treat them as Lua modules.

Run `ezpm alias sync` after editing to regenerate `.darklua.json`.

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
| [DarkLua](https://github.com/seaofvoices/darklua) | Require transforms | 0.19.0 |
| [Wally](https://github.com/UpliftGames/wally) | Package manager | 0.3.2 |
| [wally-package-types](https://github.com/JohnnyMorganz/wally-package-types) | Package type re-exports | 1.6.2 |
| [Selene](https://github.com/Kampfkarren/selene) | Linter | 0.31.0 |
| [StyLua](https://github.com/JohnnyMorganz/StyLua) | Formatter | 2.5.2 |

`setup-wally-packages` wipes and reinstalls package directories, so it refuses to touch anything that is not a real top-level directory in the project root — no symlinks, no paths inside your source tree.

## VS Code extension

`vscode-ezpm/` wraps the CLI: serve start/stop/status, `ezpm check` results as inline diagnostics (cycles and rule violations as errors, unused modules as warnings), and every other command from the palette. Point it at a binary with `ezpm.binaryPath`, or leave it empty to use `PATH`. See [its README](vscode-ezpm/README.md).

## Limitations

- Requires `@alias/` notation — no `./` or `../` relative requires
- Needs a Rojo project file in the project root (`default.project.json`, or one named under `[rojo]`)
- DarkLua is a hard dependency of the build pipeline
- v0.2 — the config format may still evolve

## Contributing

Rust source in `src/`, entrypoint `src/main.rs`, tests in `tests/`. Run `cargo test` before opening a PR.

## License

MIT — see [LICENSE](LICENSE).
