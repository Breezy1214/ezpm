# Phase 7: DX Polish - Context

**Gathered:** 2026-02-25
**Status:** Ready for planning

<domain>
## Phase Boundary

Every command failure is informative and machine-readable — structured errors with suggested fixes, correct exit codes throughout, subprocess error propagation. Also wires the interactive menu "serve" option to serve::run() via tokio runtime.

</domain>

<decisions>
## Implementation Decisions

### Error message presentation
- Labeled block style with three sections: Error, Context, Fix
- Colored labels: "Error:" in red, "Context:" in yellow, "Fix:" in green
- Respects `--no-color` flag and `NO_COLOR` environment variable
- No JSON error format — text output is sufficient for CI pipelines

### Suggested fix depth
- Descriptive hint + copy-paste command when one exists; falls back to hint-only when no single command fixes it
- Only curate fixes for known/common failures (tool not found, bad path, wrong format) — unknown errors show Error + Context only, no guessing
- Raw subprocess output passed through first, then ezpm's Error/Context/Fix block appended below
- Generic install suggestions — do not detect toolchain manager (aftman/foreman/rokit)

### Exit code design
- Binary 0/1: exit 0 for success, exit 1 for any failure
- All subprocess failures normalize to exit code 1 — ezpm owns its exit codes, does not pass through tool-specific codes
- No categorized exit codes

### Claude's Discretion
- Verbosity tiers for --quiet and --verbose flags (how error sections expand/collapse per tier)
- Partial failure semantics (e.g., lint finds some violations — exit 1 or 0)
- `ezpm format` (without --check) exit code when files were reformatted — follow CLI conventions (rustfmt, prettier patterns)
- Menu-serve integration implementation details

</decisions>

<specifics>
## Specific Ideas

- Error block example the user approved:
  ```
  Error: Selene not found
  Context: Running `ezpm lint` requires Selene
  Fix: Install Selene: `aftman add johnnymorganz/selene`
  ```
- Subprocess failure example:
  ```
  [selene stderr output here]

  Error: Selene lint failed (exit code 1)
  Context: 3 violations found in src/
  Fix: Run `ezpm fix` to auto-fix, or review above
  ```

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 07-dx-polish*
*Context gathered: 2026-02-25*
