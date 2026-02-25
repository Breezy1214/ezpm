# Phase 5: Serve Services - Context

**Gathered:** 2026-02-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Async process orchestration and OS-native file watching encapsulated as testable service modules. Delivers a ProcessManager (spawn, track, kill child processes with no orphans) and a FileWatcher (OS-native events, debounce, categorized change batches). These are building blocks — the `ezpm serve` command that wires them together is Phase 6.

</domain>

<decisions>
## Implementation Decisions

### Watch scope & filtering
- Watch `src/` directory recursively — single root, not project-wide
- Only trigger on `.lua`, `.luau`, and `init.meta.json` files — all other file types ignored
- Hardcoded ignore list (.git/, node_modules/, Packages/, build output) with optional extra ignore patterns configurable in ezpm.toml
- Watch details (directory, patterns, ignores) logged only with `--verbose` flag — clean default output

### Event classification
- Watcher emits categorized events (e.g. LuaChange, MetaChange, FileCreated, FileDeleted) — not raw path + action
- Multiple changes within the 300ms debounce window are batched into a single event containing all affected paths
- Mixed event types in one batch stay combined (one batch, caller iterates) — no splitting by category
- Events delivered via async channel (tokio) — caller awaits on the receiver

### Shutdown behavior
- On Ctrl-C: send SIGTERM, wait 2 seconds, then SIGKILL if still alive
- No double-Ctrl-C override — always wait the full grace period
- Spawn each child in its own process group; kill the group to catch grandchild processes (no orphans)
- Process termination logging (which processes are being stopped) only with `--verbose` — silent shutdown by default

### Process failure policy
- ProcessManager reports child death to caller via channel — does NOT auto-restart
- Separate status channel for process lifecycle events (started, exited, crashed) — distinct from the file watcher's event channel
- If the file watcher hits an error (directory deleted, OS limit), it sends an error event and stops — fail-fast, no recovery attempts
- Child processes inherit the terminal directly (stdout/stderr) — ProcessManager manages lifecycle only, not output

### Claude's Discretion
- Async runtime choice and configuration (tokio expected)
- Exact event enum design and naming
- Internal debounce implementation strategy
- Cross-platform process group handling details
- Test harness design for both services

</decisions>

<specifics>
## Specific Ideas

- ProcessManager and FileWatcher should be independent service modules that Phase 6 composes — no coupling between them
- The async channel pattern should make both services easy to test (send synthetic events, assert behavior)
- Process groups are key to the "no orphans" guarantee — tools like Rojo may spawn their own children

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 05-serve-services*
*Context gathered: 2026-02-24*
