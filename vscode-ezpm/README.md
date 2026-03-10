# ezpm VS Code Extension

This extension wraps the `ezpm` CLI and preserves CLI behavior. It does not reimplement command logic in TypeScript.

## Requirements

- `ezpm` installed and available on `PATH`, or
- `ezpm.binaryPath` configured to an absolute binary path.

## Settings

- `ezpm.binaryPath`: absolute path to `ezpm` binary. Empty means `PATH` lookup.
- `ezpm.servePort`: optional default port used for `ezpm serve -p <port>`. `0` disables override.
- `ezpm.autoRefreshDiagnostics`: when enabled, refreshes diagnostics on `.lua` and `.luau` save.

## Commands

- `ezpm: Start Serve` -> `ezpm serve` (or `ezpm serve -p <servePort>`)
- `ezpm: Stop Serve`
- `ezpm: Serve Status`
- `ezpm: Check` -> `ezpm check`
- `ezpm: Refresh Diagnostics` -> `ezpm check --json`
- `ezpm: Fix Requires` -> `ezpm fix-requires`
- `ezpm: Install` -> `ezpm install`
- `ezpm: Lint` -> `ezpm lint`
- `ezpm: Format` -> `ezpm format`
- `ezpm: Format (Check Only)` -> `ezpm format --check`
- `ezpm: Setup Wally Packages` -> `ezpm setup-wally-packages`
- `ezpm: Docs` -> `ezpm docs`
- `ezpm: Init (Terminal)` -> runs `ezpm init` in integrated terminal
- `ezpm: Alias (Terminal)` -> runs `ezpm alias` in integrated terminal

## Diagnostics

Diagnostics come from `ezpm check --json`.

- Circular dependencies: Error diagnostics
- Architecture rule violations: Error diagnostics
- Unused modules: Warning diagnostics

When JSON output is malformed or missing, diagnostics are cleared and an error is shown.

## Development

```bash
cd vscode-ezpm
npm install
npm run compile
npm test
```

Press `F5` in VS Code to launch an Extension Development Host.

## Troubleshooting

- Error: binary missing
  - Install `ezpm` or set `ezpm.binaryPath`.
  - Use command palette and run `Preferences: Open Settings (UI)`, then search for `ezpm.binaryPath`.
- `serve` appears stuck running
  - Use `ezpm: Stop Serve`.
  - The extension sends `SIGINT` first, then `SIGKILL` after a timeout.
- No diagnostics after refresh
  - Check `ezpm` output channel for stderr/stdout details.
  - Run `ezpm check --json` manually in the project root to validate CLI output.
