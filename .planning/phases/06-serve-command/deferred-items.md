# Deferred Items — Phase 06-serve-command

## Out-of-scope issues discovered during 06-03 execution

### config_compat integration test failure

**Test:** `tests/config_compat.rs::luau_format_ezpm_toml_loads_without_error`

**Symptom:** Test reads the actual project `ezpm.toml` and asserts `project.name == "ez-project-manager"`. The test fails because `ezpm.toml` was modified during UAT testing (project name changed from "ez-project-manager" to "s", plus many extra fields added).

**Root cause:** Pre-existing modification — `ezpm.toml` was written during UAT of `ezpm serve`. Not caused by 06-03 changes.

**Evidence:** `git diff ezpm.toml` shows project.name changed from "ez-project-manager" to "s" and additional config sections added. The test was passing on commit 8876413 when checked in isolation.

**Fix needed:** Restore `ezpm.toml` to its canonical form (`project.name = "ez-project-manager"` with minimal content), or update the test to not read the live project file. Recommend restoring `ezpm.toml` since the file is tracked and should reflect the actual project config.
