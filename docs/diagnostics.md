# Diagnostics

Every problem ezpm reports has a stable code. Match on the code, not the message: messages may change between versions, codes don't. `ezpm explain <code>` prints the same section in your terminal, and `ezpm explain --list` lists every code.

- An **error** makes the command exit 1.
- A **warning** never changes the exit code, but it always names something to fix.

Under GitHub Actions (`GITHUB_ACTIONS=true`), every diagnostic is also written as a workflow command on stderr, so it shows on the file in the pull request. With `--json`, diagnostics appear in the document's `diagnostics` array instead, each with `code`, `severity`, `message`, `file`, `line`, `hint` and `url`.

## Config

### `config.unknownField`

Warning. `ezpm.toml` has a key ezpm doesn't know, so the setting it was meant to change keeps its default.

```
warning: unknown field 'check.entry_point' in ezpm.toml. Did you mean 'entry_points'? (config.unknownField)
```

It is usually a typo. Rename the key to the one the message suggests, or remove it.

### `config.invalid`

Error. `ezpm.toml` isn't valid TOML, or a value has the wrong type, such as a string where `[serve] port` needs a number.

Fix the line the message names. Every command except `init` stops instead of running on defaults, so a broken config never overwrites `.luaurc`.

## Check

### `check.cycle`

Error. Modules require each other in a loop (A → B → C → A). Luau can't load a module that is still loading, so the cycle fails at runtime with a recursive-require error.

Move what both modules need into a third module, or pass the dependency in instead of requiring it.

### `check.forbiddenImport`

Error. A require crosses from one `[check.layers]` layer into another in a direction a `[[check.forbid]]` rule forbids, such as client requiring server.

Move the shared code into a layer both may use, or remove the require.

### `check.featureBoundary`

Error. One feature requires another. With `[check] feature_roots`, every folder directly under a root is a feature, and a feature may require only itself and the features listed in `shared_features`.

```toml
[check]
feature_roots = ["src/server/", "src/client/", "src/shared/"]
shared_features = ["Util"]
```

A feature keeps its name across roots, so `src/server/Shop` may require `src/shared/Shop` but not `src/shared/Inventory`. Move the shared code into a shared feature, list the target in `shared_features`, or remove the require.

### `check.unusedModule`

Warning. Nothing reachable from an entry point requires this module, so it never runs. Entry points come from `[check] entry_points`, or `src/<layer>/init.luau` when that is unset.

Delete the module, require it where it is needed, or add it to `entry_points` if it is loaded another way.

## Requires

### `require.ambiguous`

Error. A bare `require("Name")` matches several modules with that file name, so ezpm can't tell which one is meant and changes nothing.

Use an alias (`@Shared/Name`) or the full `@game/...` path for this require.

### `require.wouldRewrite`

Error. `ezpm fix-requires --check` found a require that `ezpm fix-requires` would rewrite: a bare name, an alias, or a path to a module that moved.

```
src/client/init.luau:3 - error: require("Signal") would become require("@game/ReplicatedStorage/Shared/Signal") (require.wouldRewrite)
```

Run `ezpm fix-requires`, or `ezpm serve`, and commit the result.

## Where

### `where.unmapped`

Error. Rojo's sourcemap has no instance for this path, so the file isn't synced into the game.

Check that the file exists, that a `$path` in the Rojo project mounts its folder, and that `globIgnorePaths` doesn't ignore it.

## Tools

### `tool.missing`

Error. A tool ezpm runs (Rojo, Selene, StyLua, Wally or Rokit) couldn't be started.

Install [Rokit](https://github.com/rojo-rbx/rokit), then run `ezpm install` to fetch the tools that `rokit.toml` pins.

### `tool.failed`

Error. A tool ezpm ran reported a failure. Its own output, printed above or in the JSON document, says what went wrong.
