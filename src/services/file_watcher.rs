//! FileWatcher — OS-native file watching with debouncing and categorized events.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use notify_debouncer_full::notify::event::EventKind;
use notify_debouncer_full::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, DebouncedEvent, RecommendedCache};
use tokio::sync::mpsc;

use crate::output;

// ─── Public types ─────────────────────────────────────────────────────────────
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FileChange {
    /// A `.lua` or `.luau` file was modified.
    LuaChange(PathBuf),
    /// An `init.meta.json` file was modified.
    MetaChange(PathBuf),
    /// A new file was created (any relevant type).
    FileCreated(PathBuf),
    /// A file was deleted (any relevant type).
    FileDeleted(PathBuf),
}

/// Top-level event type delivered over the mpsc channel.
#[derive(Debug)]
pub enum WatchEvent {
    /// A batch of file changes debounced into one event.
    ///
    /// Multiple changes within the 300ms window are batched together. Mixed
    /// event types stay combined — one batch, caller iterates.
    Changes(Vec<FileChange>),
    /// The watcher encountered an error and stopped.
    ///
    /// Per locked decision: fail-fast, no recovery attempts.
    Error(String),
}

// ─── FileWatcher ──────────────────────────────────────────────────────────────
pub struct FileWatcher {
    /// Holds the debouncer alive. Dropping this stops the watcher.
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

impl FileWatcher {
    pub fn new(watch_dir: &Path, extra_ignores: &[String]) -> Result<(FileWatcher, mpsc::Receiver<WatchEvent>)> {
        let (tx, rx) = mpsc::channel::<WatchEvent>(64);

        // Build the full ignore list (hardcoded + configurable extra ignores).
        let mut ignore_patterns: Vec<String> = vec![
            ".git".to_string(),
            "node_modules".to_string(),
            "Packages".to_string(),
        ];
        ignore_patterns.extend_from_slice(extra_ignores);

        // Clone tx into the closure — the original tx is consumed by ownership.
        let tx_clone = tx.clone();

        // Build the debouncer callback. This closure runs on notify's OS-managed
        // thread — `blocking_send` is the only safe bridge to tokio here.
        let callback = move |result: DebounceEventResult| {
            match result {
                Ok(events) => {
                    let changes = classify_events(&events, &ignore_patterns);
                    if !changes.is_empty() {
                        // Safe: notify callback runs on OS thread, not tokio worker.
                        // blocking_send would panic if called from an async context.
                        let _ = tx_clone.blocking_send(WatchEvent::Changes(changes));
                    }
                }
                Err(errors) => {
                    let msg = errors
                        .iter()
                        .map(|e| format!("{e}"))
                        .collect::<Vec<_>>()
                        .join("; ");
                    // Per locked decision: error event and stop — fail-fast.
                    let _ = tx_clone.blocking_send(WatchEvent::Error(msg));
                }
            }
        };

        let mut debouncer = new_debouncer(Duration::from_millis(300), None, callback)?;

        // Watch the directory recursively via OS-native events.
        debouncer.watch(watch_dir, RecursiveMode::Recursive)?;

        output::verbose_line(&format!(
            "Watching {} recursively for .lua, .luau, init.meta.json",
            watch_dir.display()
        ));

        Ok((FileWatcher { _debouncer: debouncer }, rx))
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────
fn classify_events(events: &[DebouncedEvent], ignore_patterns: &[String]) -> Vec<FileChange> {
    let mut seen: HashSet<FileChange> = HashSet::new();
    let mut result: Vec<FileChange> = Vec::new();

    for debounced in events {
        // DebouncedEvent derefs to Event, which has `paths: Vec<PathBuf>` and `kind: EventKind`.
        let kind = &debounced.event.kind;

        for path in &debounced.event.paths {
            // Filter: ignored directory component.
            if should_ignore(path, ignore_patterns) {
                continue;
            }

            // Filter: non-relevant file extension.
            if !is_relevant(path) {
                continue;
            }

            let change = match kind {
                EventKind::Create(_) => {
                    // init.meta.json may arrive as Create on macOS (atomic save = delete + create).
                    // Route through classify_modify so it becomes MetaChange, not FileCreated.
                    if path.file_name().is_some_and(|n| n == "init.meta.json") {
                        classify_modify(path)
                    } else {
                        Some(FileChange::FileCreated(path.clone()))
                    }
                }
                EventKind::Remove(_) => Some(FileChange::FileDeleted(path.clone())),
                EventKind::Modify(_) => classify_modify(path),
                // Pitfall 3: kqueue on macOS emits EventKind::Any instead of
                // specific Modify variants. Treat Any as a possible modify.
                EventKind::Any => classify_modify(path),
                // Access and Other events are not actionable for our use case.
                EventKind::Access(_) | EventKind::Other => None,
            };

            if let Some(change) = change {
                if seen.insert(change.clone()) {
                    result.push(change);
                }
            }
        }
    }

    let deleted_paths: HashSet<PathBuf> = result
        .iter()
        .filter_map(|c| match c {
            FileChange::FileDeleted(p) => Some(p.clone()),
            _ => None,
        })
        .collect();

    if !deleted_paths.is_empty() {
        result.retain(|c| match c {
            FileChange::FileDeleted(_) => true,
            FileChange::LuaChange(p)
            | FileChange::MetaChange(p)
            | FileChange::FileCreated(p) => !deleted_paths.contains(p),
        });
    }

    result
}

/// Classify a modify event by path extension.
fn classify_modify(path: &Path) -> Option<FileChange> {
    if path.file_name().is_some_and(|n| n == "init.meta.json") {
        Some(FileChange::MetaChange(path.to_path_buf()))
    } else {
        match path.extension().and_then(|e| e.to_str()) {
            Some("lua") | Some("luau") => Some(FileChange::LuaChange(path.to_path_buf())),
            _ => None,
        }
    }
}

/// Returns true if the file should be watched.
pub(crate) fn is_relevant(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("lua") | Some("luau")
    ) || path.file_name().is_some_and(|n| n == "init.meta.json")
}

/// Returns true if any component of the path matches an ignored directory name.
pub(crate) fn should_ignore(path: &Path, ignore_patterns: &[String]) -> bool {
    path.components().any(|component| {
        let s = component.as_os_str().to_string_lossy();
        ignore_patterns.iter().any(|pattern| s.as_ref() == pattern.as_str())
    })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use notify_debouncer_full::notify::event::{DataChange, EventKind, ModifyKind};
    use notify_debouncer_full::notify::Event;
    use notify_debouncer_full::DebouncedEvent;
    use tokio::time::timeout;

    use super::*;

    fn init_output() {
        // ok() silently ignores double-init across tests (OnceLock pattern).
        output::init(false, false, output::ColorChoice::Auto);
    }

    fn make_debounced_event(kind: EventKind, paths: Vec<PathBuf>) -> DebouncedEvent {
        let event = Event {
            kind,
            paths,
            attrs: Default::default(),
        };
        DebouncedEvent::new(event, std::time::Instant::now())
    }

    // ── Test 1: is_relevant ────────────────────────────────────────────────

    #[test]
    fn test_is_relevant() {
        init_output();

        assert!(is_relevant(Path::new("src/foo.lua")));
        assert!(is_relevant(Path::new("src/bar.luau")));
        assert!(is_relevant(Path::new("src/thing/init.meta.json")));

        assert!(!is_relevant(Path::new("src/foo.rs")));
        assert!(!is_relevant(Path::new("src/foo.txt")));
        assert!(!is_relevant(Path::new("src/data.json")));
        assert!(!is_relevant(Path::new("README.md")));
    }

    // ── Test 2: should_ignore ──────────────────────────────────────────────

    #[test]
    fn test_should_ignore() {
        init_output();

        let defaults = vec![
            ".git".to_string(),
            "node_modules".to_string(),
            "Packages".to_string(),
        ];

        // Hardcoded ignores.
        assert!(should_ignore(Path::new(".git/config"), &defaults));
        assert!(should_ignore(Path::new("node_modules/pkg/index.js"), &defaults));
        assert!(should_ignore(Path::new("Packages/lib.lua"), &defaults));

        // Non-ignored path.
        assert!(!should_ignore(Path::new("src/foo.lua"), &defaults));

        // Extra ignores from ezpm.toml.
        let mut with_extra = defaults.clone();
        with_extra.push("build".to_string());
        assert!(should_ignore(Path::new("build/output.lua"), &with_extra));
        assert!(!should_ignore(Path::new("src/foo.lua"), &with_extra));
    }

    // ── Test 3: classify_events — lua change ──────────────────────────────

    #[test]
    fn test_classify_events_lua_change() {
        init_output();

        let events = vec![make_debounced_event(
            EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            vec![PathBuf::from("src/foo.lua")],
        )];

        let result = classify_events(&events, &[]);

        assert_eq!(result.len(), 1);
        assert!(matches!(&result[0], FileChange::LuaChange(p) if p == Path::new("src/foo.lua")));
    }

    // ── Test 4: classify_events — irrelevant files filtered ───────────────

    #[test]
    fn test_classify_events_filters_irrelevant() {
        init_output();

        let events = vec![make_debounced_event(
            EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            vec![PathBuf::from("src/main.rs")],
        )];

        let result = classify_events(&events, &[]);

        assert!(result.is_empty(), "non-Lua files must be filtered out");
    }

    // ── Test 5: classify_events — EventKind::Any treated as modify ────────

    #[test]
    fn test_classify_events_any_kind_treated_as_modify() {
        init_output();

        // Pitfall 3: kqueue on macOS emits EventKind::Any — must be treated as modify.
        let events = vec![make_debounced_event(
            EventKind::Any,
            vec![PathBuf::from("src/module.luau")],
        )];

        let result = classify_events(&events, &[]);

        assert_eq!(result.len(), 1);
        assert!(
            matches!(&result[0], FileChange::LuaChange(p) if p == Path::new("src/module.luau")),
            "EventKind::Any on a .luau file must produce LuaChange"
        );
    }

    // ── Test 6: integration — watcher detects real file change ────────────

    #[tokio::test]
    async fn test_watcher_detects_file_change() {
        init_output();

        let tmp = tempfile::TempDir::new().expect("failed to create tempdir");
        let src_dir = tmp.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("failed to create src dir");

        let lua_file = src_dir.join("test.lua");
        std::fs::write(&lua_file, b"-- initial").expect("failed to write initial file");

        let (watcher, mut rx) = FileWatcher::new(&src_dir, &[]).expect("FileWatcher::new failed");

        // Allow the OS watcher to initialize before triggering a change.
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Modify the file to trigger a change event.
        std::fs::write(&lua_file, b"-- modified").expect("failed to write modified file");

        // Wait up to 2 seconds for the debounced event (300ms debounce + margin).
        let received = timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out waiting for WatchEvent — no event arrived within 2s")
            .expect("channel closed before event arrived");

        match received {
            WatchEvent::Changes(changes) => {
                assert!(
                    !changes.is_empty(),
                    "expected at least one FileChange in the batch"
                );
                // On macOS kqueue, overwriting a watched file after the watcher starts
                // may be reported as FileCreated rather than LuaChange (platform difference).
                // We accept both — the important thing is that the path is test.lua and
                // the event type is relevant (either a change or a create counts).
                let has_relevant_change = changes.iter().any(|c| {
                    let p = match c {
                        FileChange::LuaChange(p) | FileChange::FileCreated(p) => p,
                        _ => return false,
                    };
                    p.file_name().is_some_and(|n| n == "test.lua")
                });
                assert!(
                    has_relevant_change,
                    "expected LuaChange or FileCreated for test.lua, got {:?}",
                    changes
                );
            }
            WatchEvent::Error(e) => panic!("unexpected watcher error: {e}"),
        }

        // Explicit drop to stop the watcher (documents intent).
        drop(watcher);
    }

    // ── Test 6b: classify_events — init.meta.json Create becomes MetaChange ──

    #[test]
    fn test_classify_events_meta_create_becomes_meta_change() {
        init_output();

        let events = vec![make_debounced_event(
            EventKind::Create(notify_debouncer_full::notify::event::CreateKind::File),
            vec![PathBuf::from("src/MyModule/init.meta.json")],
        )];

        let result = classify_events(&events, &[]);

        assert_eq!(result.len(), 1);
        assert!(
            matches!(&result[0], FileChange::MetaChange(p) if p == Path::new("src/MyModule/init.meta.json")),
            "Create event for init.meta.json must produce MetaChange, got {:?}",
            result
        );
    }

    // ── Test 8b: classify_events — delete-wins deduplication ─────────────

    #[test]
    fn test_classify_events_delete_wins_over_lua_change() {
        init_output();

        // Simulate macOS race: same file gets both Modify and Remove in one debounce batch.
        let events = vec![
            make_debounced_event(
                EventKind::Modify(ModifyKind::Data(DataChange::Any)),
                vec![PathBuf::from("src/foo.lua")],
            ),
            make_debounced_event(
                EventKind::Remove(notify_debouncer_full::notify::event::RemoveKind::File),
                vec![PathBuf::from("src/foo.lua")],
            ),
        ];

        let result = classify_events(&events, &[]);

        // Only FileDeleted should remain — LuaChange is suppressed.
        assert_eq!(result.len(), 1, "expected 1 event after delete-wins dedup, got {:?}", result);
        assert!(
            matches!(&result[0], FileChange::FileDeleted(p) if p == Path::new("src/foo.lua")),
            "expected FileDeleted for src/foo.lua, got {:?}",
            result
        );
    }

    // ── Test 7: integration — non-Lua files do not trigger events ─────────

    #[tokio::test]
    async fn test_watcher_ignores_non_lua_files() {
        init_output();

        let tmp = tempfile::TempDir::new().expect("failed to create tempdir");
        let src_dir = tmp.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("failed to create src dir");

        let (_watcher, mut rx) = FileWatcher::new(&src_dir, &[]).expect("FileWatcher::new failed");

        // Allow the OS watcher to initialize.
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Write a non-Lua file — must be silently ignored.
        let txt_file = src_dir.join("test.txt");
        std::fs::write(&txt_file, b"not lua").expect("failed to write .txt file");

        // Wait with a short timeout — no event should arrive.
        let result = timeout(Duration::from_millis(800), rx.recv()).await;

        assert!(
            result.is_err(),
            "expected timeout (no event for .txt file), but got an event"
        );
    }
}
