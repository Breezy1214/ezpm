# ezpm

**Stop wrestling with Rojo, DarkLua, and broken require paths. Just build your game.**

ezpm is a single CLI that replaces your entire Roblox dev pipeline. One command starts your dev server, fixes your requires, syncs to Studio, and watches for changes — no glue scripts, no config juggling.

```bash
ezpm serve
```

That's it. File watcher, require path fixing, DarkLua transforms, sourcemap generation, and Rojo live sync — all running together.

## Why ezpm?

Without ezpm, a typical Roblox project means manually wiring together Rojo, DarkLua, Wally, Selene, and StyLua. You're writing shell scripts to glue them together, debugging broken require paths when you move files, and restarting processes every time something drifts out of sync.

**ezpm handles all of that in a single binary.**

| Problem | ezpm solution |
|---|---|
| Broken `require()` paths after moving files | Auto-rewrites all requires using `@alias` notation |
| Manually syncing DarkLua + `.luaurc` + Rojo configs | One `[aliases]` table in `ezpm.toml` — configs regenerated automatically |
| Juggling 5 terminal tabs for your dev loop | `ezpm serve` runs everything in one process |
| "Works on my machine" toolchain drift | Rokit-managed toolchain with pinned versions |
| Circular dependencies creeping in | `ezpm check` catches cycles, architecture violations, and dead code |

## Get Started in 30 Seconds

### Install

```bash
rokit add Breezy1214/ezpm
```

> No Rokit? Grab a binary from [Releases](https://github.com/Breezy1214/ezpm/releases) (Linux, macOS, Windows) or build from source with `cargo install --path .`

### New project

```bash
ezpm init     # scaffolds everything: dirs, ezpm.toml, darklua config, luaurc
ezpm serve    # dev server is live — start coding
```

### Existing project

```bash
ezpm init     # detects your existing config, imports aliases
ezpm serve    # picks up right where you left off
```

## What You Get

### `ezpm serve` — Your entire dev loop, one command

1. Generates `build.project.json` from your Rojo project (remaps source paths to the DarkLua output directory)
2. Generates a Rojo sourcemap
3. Fixes require paths across your source tree
4. Runs DarkLua to transform `@alias` requires into Roblox-compatible paths
5. Starts Rojo live sync to Studio
6. Watches for file changes and re-runs steps 2-4 automatically

### `ezpm check` — Catch problems before they hit production

Static analysis of your `require()` graph with zero config:

- **Circular dependencies** — A -> B -> C -> A? Caught.
- **Architecture violations** — Client importing Server? Blocked.
- **Unused modules** — Dead code that nobody requires? Found.

```bash
ezpm check          # human-readable output
ezpm check --json   # pipe into CI
```

### `ezpm fix-requires` — Clean up your codebase in one shot

Scans every file and rewrites `require()` calls to use your `@alias/` paths. No more `require(game.ReplicatedStorage.Shared.Utils.Signal)` — just `require("@Shared/Utils/Signal")`.

### Everything else

```
ezpm install     Rokit + Wally packages + type generation
ezpm lint        Selene + StyLua --check
ezpm format      StyLua format
ezpm alias       Add, remove, list, or sync path aliases
ezpm docs        Moonwave documentation server
ezpm             Interactive menu (arrow-key navigation)
```

## Configuration

All config lives in `ezpm.toml`. Sensible defaults out of the box — only edit what you need.

```toml
[project]
name = "my-game"

[aliases]
Client = "src/client/"
Server = "src/server/"
Shared = "src/shared/"
Packages = "Packages/"

[serve]
port = 34872
require_fix_mode = "hybrid"   # strict | hybrid | fast
```

**`require_fix_mode`** controls how aggressively the file watcher rewrites requires:

- **`strict`** — Full-tree fix on every change. Maximum correctness.
- **`hybrid`** — Single-file fix on edits, full-tree fix on deletes/renames. Best default.
- **`fast`** — Single-file fix only. Maximum speed for large projects.

### Architecture rules (optional)

```toml
[check.layers]
client = "src/client/"
server = "src/server/"
shared = "src/shared/"

[[check.forbid]]
from = "client"
to = "server"
reason = "Client must never import server modules"
```

## Toolchain

ezpm integrates with the standard Roblox toolchain, managed via [Rokit](https://github.com/rojo-rbx/rokit):

| Tool | Role |
|---|---|
| [Rojo](https://github.com/rojo-rbx/rojo) | Studio sync |
| [DarkLua](https://github.com/seaofvoices/darklua) | Require transforms |
| [Wally](https://github.com/UpliftGames/wally) | Package manager |
| [Selene](https://github.com/Kampfkarren/selene) | Linter |
| [StyLua](https://github.com/JohnnyMorganz/StyLua) | Formatter |

## Limitations

- Requires `@alias/` notation — no `./` or `../` relative requires
- Requires `default.project.json` in the project root (run `ezpm init`)
- DarkLua is a hard dependency for the build pipeline
- v0.2 — config format may still evolve

## Contributing

Contributions welcome. Rust source is in `src/`, entrypoint is `src/main.rs`.

## License

MIT — see [LICENSE](LICENSE).
