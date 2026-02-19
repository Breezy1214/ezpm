# EZ Project Manager

A CLI build tool for Roblox projects that handles the tedious parts of the development pipeline (path aliasing, require path correction, DarkLua transformation, and Rojo live sync) so you can write Luau and not think about build plumbing.

ezpm is written in Luau, runs on [Lune](https://lune-org.github.io/docs), and compiles to standalone binaries for Linux, macOS, and Windows.

## Key Features

- **Single-command dev server** — File watcher, DarkLua processing, sourcemap generation, and Rojo live sync all run together via `ezpm serve`.
- **Automatic require path fixing** — Scans your source tree and rewrites `require()` calls to use `@alias/` notation, eliminating broken or inconsistent paths.
- **Path alias management** — Define aliases in `ezpm.toml` once; `.darklua.json` and `.luaurc` are generated automatically to keep DarkLua and your LSP in sync.
- **Project scaffolding** — `ezpm init` detects existing config files, imports aliases, creates directory structure, and generates `default.project.json`.
- **Integrated toolchain** — Linting (Selene), formatting (StyLua), Wally package installation, and Moonwave docs from one interface.
- **Interactive menu** — Arrow-key navigation for all commands. Subcommands also work directly from the CLI.

## Installation

Requires [Rokit](https://github.com/rojo-rbx/rokit).

```bash
rokit add Breezy1214/ezpm
```

To update to the latest version:

```bash
rokit update ezpm
rokit install
```

### Manual installation

**From release binaries** — Download a prebuilt binary from [Releases](https://github.com/Breezy1214/sync/releases) for your platform (Linux, macOS, or Windows — x86_64 and aarch64). Extract and place `ezpm` on your PATH.

**From source:**

```bash
git clone https://github.com/Breezy1214/ezpm.git
cd ezpm
rokit install      # installs Lune, Rojo, DarkLua, Wally, etc.
lune run ezpm      # launches the interactive menu
```

## Quick Start

### New project

```bash
ezpm init          # scaffolds directories, generates ezpm.toml, .darklua.json, .luaurc
ezpm serve         # starts the dev server with file watching and Rojo sync
```

### Existing project

If you already have a Rojo project with a `.darklua.json`:

```bash
ezpm init          # detects existing files, offers to import aliases
ezpm serve         # ready to go
```

## Usage

Run `ezpm` with no arguments to open the interactive menu, or use subcommands directly:

```
ezpm                  Interactive menu
ezpm serve            Dev server (file watcher + DarkLua + Rojo)
ezpm fix-requires     One-shot require path correction
ezpm install          Rokit install + Wally packages + type generation
ezpm lint             Selene + StyLua --check
ezpm format           StyLua format
ezpm init             Scaffold project and generate ezpm.toml
ezpm alias add        Add a path alias
ezpm alias remove     Remove a path alias
ezpm alias list       List configured aliases
ezpm alias sync       Reload aliases from ezpm.toml and regenerate configs
ezpm docs             Moonwave documentation server
ezpm help             Print usage
```

### What `serve` does

1. Generates `build.project.json` from `default.project.json` (remaps source paths to the DarkLua output directory)
2. Generates a Rojo sourcemap
3. Fixes require paths across your source tree
4. Runs DarkLua to transform `@alias` requires into Roblox-compatible paths
5. Starts Rojo with live sync to Studio
6. Watches for file changes and re-runs steps 2-4 automatically

## Configuration

Configuration lives in `ezpm.toml`. If the file is absent, defaults are used.

```toml
[project]
name = "my-roblox-game"

[paths]
src = "src"
darklua_build = "darklua_build"

[display]
file_changes = true
docs_enabled = false
logs_enabled = true

[aliases]
Client = "src/client/"
Server = "src/server/"
Shared = "src/shared/"
Packages = "Packages/"
ServerPackages = "ServerPackages/"
```

### Aliases

Aliases are the core mechanism for clean require paths. Define them under `[aliases]` in `ezpm.toml` — this is the single source of truth. Both `.darklua.json` and `.luaurc` are regenerated from these aliases automatically. Do not edit those files by hand; use the CLI:

```bash
ezpm alias add       # interactive prompt for name and path
ezpm alias remove    # select from existing aliases
ezpm alias list      # display current aliases
ezpm alias sync      # reload aliases from ezpm.toml and regenerate .darklua.json / .luaurc
```

Use `alias sync` after manually editing aliases in `ezpm.toml` to regenerate the config files without re-running `init`.

### Toolchain

Tools are managed via [Rokit](https://github.com/rojo-rbx/rokit) and pinned in `rokit.toml`. The default toolchain includes:

| Tool | Purpose |
|------|---------|
| [Lune](https://github.com/lune-org/lune) | Luau runtime (runs ezpm itself) |
| [Rojo](https://github.com/rojo-rbx/rojo) | Studio sync |
| [DarkLua](https://github.com/seaofvoices/darklua) | Require path transformation and Lua optimization |
| [Wally](https://github.com/UpliftGames/wally) | Package manager |
| [Selene](https://github.com/Kampfkarren/selene) | Linter |
| [StyLua](https://github.com/JohnnyMorganz/StyLua) | Formatter |

## Limitations

- **Polling-based file watcher.** File change detection uses a 1-second polling interval, not OS-level filesystem events. Changes are picked up reliably but not instantly.
- **No relative require paths.** The require fixer does not support `./` or `../` paths. All requires must use `@alias/` notation or absolute source paths.
- **Rojo port is hardcoded.** The dev server uses Rojo's default port (34872). Running multiple instances simultaneously requires manual port management.
- **DarkLua is required.** The build pipeline assumes DarkLua for path transformation. There is no bypass for projects that don't need it.
- **Early stage.** The project is at v0.1.0. APIs and config format may change.

## Contributing

Contributions are welcome. All source lives in `src/`. The entrypoint is `src/init.luau`.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE.md) file for details.
