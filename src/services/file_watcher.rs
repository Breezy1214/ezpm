use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use notify_debouncer_full::notify::event::{CreateKind, EventKind, ModifyKind, RemoveKind};
use notify_debouncer_full::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{
    new_debouncer, DebounceEventResult, DebouncedEvent, Debouncer, RecommendedCache,
};
use tokio::sync::mpsc;

use crate::output;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FileChange {
    LuaChange(PathBuf),
    MetaChange(PathBuf),
    FileCreated(PathBuf),
    FileDeleted(PathBuf),
    DirectoryCreated(PathBuf),
    DirectoryRemoved(PathBuf),
    RojoProjectChange(PathBuf),
    ConfigChange(PathBuf),
}

#[derive(Debug, Clone)]
pub struct WatchTargets {
    pub source_root: PathBuf,
    pub project_files: Vec<PathBuf>,
    pub config_file: Option<PathBuf>,
    pub generated_roots: Vec<PathBuf>,
}

impl WatchTargets {
    pub fn source_only(source_root: impl Into<PathBuf>) -> Self {
        Self {
            source_root: source_root.into(),
            project_files: Vec::new(),
            config_file: None,
            generated_roots: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub enum WatchEvent {
    Changes(Vec<FileChange>),
    Error(String),
}

pub struct FileWatcher {
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

impl FileWatcher {
    pub fn new(
        watch_dir: &Path,
        extra_ignores: &[String],
    ) -> Result<(FileWatcher, mpsc::Receiver<WatchEvent>)> {
        Self::with_targets(
            WatchTargets::source_only(watch_dir.to_path_buf()),
            extra_ignores,
        )
    }

    pub fn with_targets(
        targets: WatchTargets,
        extra_ignores: &[String],
    ) -> Result<(FileWatcher, mpsc::Receiver<WatchEvent>)> {
        let (tx, rx) = mpsc::channel::<WatchEvent>(64);

        let mut ignore_patterns: Vec<String> = vec![
            ".git".to_string(),
            "node_modules".to_string(),
            "Packages".to_string(),
        ];
        ignore_patterns.extend_from_slice(extra_ignores);
        let rules = ClassificationRules::new(&targets);

        let tx_clone = tx.clone();

        let callback = move |result: DebounceEventResult| match result {
            Ok(events) => {
                let changes = classify_events_with_rules(&events, &ignore_patterns, &rules);
                if !changes.is_empty() {
                    let _ = tx_clone.blocking_send(WatchEvent::Changes(changes));
                }
            }
            Err(errors) => {
                let msg = errors
                    .iter()
                    .map(|e| format!("{e}"))
                    .collect::<Vec<_>>()
                    .join("; ");
                let _ = tx_clone.blocking_send(WatchEvent::Error(msg));
            }
        };

        let mut debouncer = new_debouncer(Duration::from_millis(100), None, callback)?;

        debouncer.watch(&targets.source_root, RecursiveMode::Recursive)?;

        let mut watched_parents = HashSet::new();
        for control_file in targets
            .project_files
            .iter()
            .chain(targets.config_file.iter())
        {
            if control_file.starts_with(&targets.source_root) {
                continue;
            }
            if let Some(parent) = control_file.parent() {
                let parent = if parent.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    parent
                };
                if watched_parents.insert(parent.to_path_buf()) {
                    debouncer.watch(parent, RecursiveMode::NonRecursive)?;
                }
            }
        }

        output::verbose_line(&format!(
            "Watching {} recursively for .lua, .luau, init.meta.json, *.model.json",
            targets.source_root.display()
        ));

        Ok((
            FileWatcher {
                _debouncer: debouncer,
            },
            rx,
        ))
    }
}

#[cfg(test)]
fn classify_events(events: &[DebouncedEvent], ignore_patterns: &[String]) -> Vec<FileChange> {
    classify_events_with_rules(events, ignore_patterns, &ClassificationRules::unscoped())
}

#[derive(Debug, Clone)]
struct ClassificationRules {
    source_root: Option<PathBuf>,
    project_files: std::collections::HashMap<PathBuf, PathBuf>,
    config_file: Option<(PathBuf, PathBuf)>,
    generated_roots: Vec<PathBuf>,
}

impl ClassificationRules {
    fn new(targets: &WatchTargets) -> Self {
        Self {
            source_root: Some(clean_path(&targets.source_root)),
            project_files: targets
                .project_files
                .iter()
                .map(|p| (clean_path(p), p.clone()))
                .collect(),
            config_file: targets
                .config_file
                .as_ref()
                .map(|p| (clean_path(p), p.clone())),
            generated_roots: targets
                .generated_roots
                .iter()
                .map(|p| clean_path(p))
                .collect(),
        }
    }

    #[cfg(test)]
    fn unscoped() -> Self {
        Self {
            source_root: None,
            project_files: std::collections::HashMap::new(),
            config_file: None,
            generated_roots: Vec::new(),
        }
    }

    fn control_change(&self, path: &Path) -> Option<FileChange> {
        if let Some((_, original)) = self
            .config_file
            .as_ref()
            .filter(|(normalized, _)| normalized == path)
        {
            Some(FileChange::ConfigChange(original.clone()))
        } else {
            self.project_files
                .get(path)
                .map(|original| FileChange::RojoProjectChange(original.clone()))
        }
    }

    fn is_generated(&self, path: &Path) -> bool {
        self.generated_roots
            .iter()
            .any(|root| path.starts_with(root))
    }

    fn is_in_source(&self, path: &Path) -> bool {
        self.source_root
            .as_ref()
            .is_none_or(|root| path.starts_with(root))
    }
}

fn clean_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };

    absolute.canonicalize().unwrap_or_else(|_| {
        absolute
            .parent()
            .and_then(|parent| parent.canonicalize().ok())
            .and_then(|parent| absolute.file_name().map(|name| parent.join(name)))
            .unwrap_or(absolute)
    })
}

fn classify_events_with_rules(
    events: &[DebouncedEvent],
    ignore_patterns: &[String],
    rules: &ClassificationRules,
) -> Vec<FileChange> {
    let mut seen: HashSet<FileChange> = HashSet::new();
    let mut result: Vec<FileChange> = Vec::new();

    for debounced in events {
        let kind = &debounced.event.kind;

        for path in &debounced.event.paths {
            let normalized_path = clean_path(path);

            if rules.is_generated(&normalized_path) {
                continue;
            }

            if let Some(change) = rules.control_change(&normalized_path) {
                if seen.insert(change.clone()) {
                    result.push(change);
                }
                continue;
            }

            if !rules.is_in_source(&normalized_path) {
                continue;
            }

            if should_ignore(path, ignore_patterns) {
                continue;
            }

            let change = match kind {
                EventKind::Create(create_kind) => match create_kind {
                    CreateKind::Folder => Some(FileChange::DirectoryCreated(path.clone())),
                    CreateKind::Any => {
                        if path.is_dir() {
                            Some(FileChange::DirectoryCreated(path.clone()))
                        } else if is_relevant(path) {
                            if is_copied_metadata_file(path) {
                                classify_modify(path)
                            } else {
                                Some(FileChange::FileCreated(path.clone()))
                            }
                        } else {
                            None
                        }
                    }
                    _ => {
                        if !is_relevant(path) {
                            None
                        } else if is_copied_metadata_file(path) {
                            classify_modify(path)
                        } else {
                            Some(FileChange::FileCreated(path.clone()))
                        }
                    }
                },
                EventKind::Remove(remove_kind) => match remove_kind {
                    RemoveKind::Folder => Some(FileChange::DirectoryRemoved(path.clone())),
                    RemoveKind::Any => {
                        if is_relevant(path) {
                            Some(FileChange::FileDeleted(path.clone()))
                        } else if path.extension().is_none() {
                            Some(FileChange::DirectoryRemoved(path.clone()))
                        } else {
                            None
                        }
                    }
                    _ => {
                        if is_relevant(path) {
                            Some(FileChange::FileDeleted(path.clone()))
                        } else {
                            None
                        }
                    }
                },
                EventKind::Modify(ModifyKind::Name(_)) => {
                    if path.is_dir() {
                        Some(FileChange::DirectoryCreated(path.clone()))
                    } else if path.exists() {
                        if is_relevant(path) {
                            Some(FileChange::FileCreated(path.clone()))
                        } else {
                            None
                        }
                    } else if is_relevant(path) {
                        Some(FileChange::FileDeleted(path.clone()))
                    } else if path.extension().is_none() {
                        Some(FileChange::DirectoryRemoved(path.clone()))
                    } else {
                        None
                    }
                }
                EventKind::Modify(_) => {
                    if is_relevant(path) {
                        classify_modify(path)
                    } else {
                        None
                    }
                }
                EventKind::Any => {
                    if is_relevant(path) {
                        classify_modify(path)
                    } else {
                        None
                    }
                }
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
            FileChange::FileDeleted(p) | FileChange::DirectoryRemoved(p) => Some(p.clone()),
            _ => None,
        })
        .collect();

    if !deleted_paths.is_empty() {
        let still_exists: HashSet<&PathBuf> = deleted_paths.iter().filter(|p| p.exists()).collect();

        result.retain(|c| match c {
            FileChange::FileDeleted(p) | FileChange::DirectoryRemoved(p) => {
                !still_exists.contains(p)
            }
            FileChange::LuaChange(p)
            | FileChange::MetaChange(p)
            | FileChange::FileCreated(p)
            | FileChange::DirectoryCreated(p)
            | FileChange::RojoProjectChange(p)
            | FileChange::ConfigChange(p) => !deleted_paths.contains(p) || still_exists.contains(p),
        });

        for path in &still_exists {
            let has_non_delete = result.iter().any(|c| {
                let p = match c {
                    FileChange::LuaChange(p)
                    | FileChange::MetaChange(p)
                    | FileChange::FileCreated(p)
                    | FileChange::DirectoryCreated(p)
                    | FileChange::RojoProjectChange(p)
                    | FileChange::ConfigChange(p) => p,
                    _ => return false,
                };
                p == *path
            });

            if !has_non_delete {
                if path.is_dir() {
                    result.push(FileChange::DirectoryCreated((*path).clone()));
                } else if let Some(change) = classify_modify(path) {
                    result.push(change);
                } else if is_relevant(path) {
                    result.push(FileChange::FileCreated((*path).clone()));
                }
            }
        }
    }

    result
}

fn classify_modify(path: &Path) -> Option<FileChange> {
    if is_copied_metadata_file(path) {
        Some(FileChange::MetaChange(path.to_path_buf()))
    } else {
        match path.extension().and_then(|e| e.to_str()) {
            Some("lua") | Some("luau") => Some(FileChange::LuaChange(path.to_path_buf())),
            _ => None,
        }
    }
}

pub(crate) fn is_relevant(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("lua") | Some("luau")
    ) || is_copied_metadata_file(path)
}

fn is_copied_metadata_file(path: &Path) -> bool {
    path.file_name().is_some_and(|name| {
        let name = name.to_string_lossy();
        name == "init.meta.json" || name.ends_with(".model.json")
    })
}

pub(crate) fn should_ignore(path: &Path, ignore_patterns: &[String]) -> bool {
    path.components().any(|component| {
        let s = component.as_os_str().to_string_lossy();
        ignore_patterns
            .iter()
            .any(|pattern| s.as_ref() == pattern.as_str())
    })
}

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

    #[tokio::test]
    async fn test_watcher_detects_file_change() {
        init_output();

        let tmp = tempfile::TempDir::new().expect("failed to create tempdir");
        let src_dir = tmp.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("failed to create src dir");

        let lua_file = src_dir.join("test.lua");
        std::fs::write(&lua_file, b"-- initial").expect("failed to write initial file");

        let (watcher, mut rx) = FileWatcher::new(&src_dir, &[]).expect("FileWatcher::new failed");

        tokio::time::sleep(Duration::from_millis(100)).await;

        std::fs::write(&lua_file, b"-- modified").expect("failed to write modified file");

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

        drop(watcher);
    }

    #[test]
    fn test_classify_events_delete_wins_when_file_truly_gone() {
        init_output();

        let events = vec![
            make_debounced_event(
                EventKind::Modify(ModifyKind::Data(DataChange::Any)),
                vec![PathBuf::from("src/nonexistent_file.lua")],
            ),
            make_debounced_event(
                EventKind::Remove(notify_debouncer_full::notify::event::RemoveKind::File),
                vec![PathBuf::from("src/nonexistent_file.lua")],
            ),
        ];

        let result = classify_events(&events, &[]);

        assert_eq!(
            result.len(),
            1,
            "expected 1 event after dedup, got {:?}",
            result
        );
        assert!(
            matches!(&result[0], FileChange::FileDeleted(p) if p == Path::new("src/nonexistent_file.lua")),
            "expected FileDeleted when file is truly gone, got {:?}",
            result
        );
    }

    #[test]
    fn test_classify_events_synthesize_event_for_replaced_file() {
        init_output();

        let tmp = tempfile::TempDir::new().expect("failed to create tempdir");
        let lua_file = tmp.path().join("checkout.lua");
        std::fs::write(&lua_file, b"-- restored").expect("failed to write file");

        let events = vec![make_debounced_event(
            EventKind::Remove(notify_debouncer_full::notify::event::RemoveKind::File),
            vec![lua_file.clone()],
        )];

        let result = classify_events(&events, &[]);

        assert_eq!(
            result.len(),
            1,
            "expected 1 synthetic event, got {:?}",
            result
        );
        assert!(
            matches!(&result[0], FileChange::LuaChange(p) if p == &lua_file),
            "expected synthetic LuaChange for replaced file, got {:?}",
            result
        );
    }

    #[test]
    fn test_classify_events_directory_rename() {
        init_output();

        let tmp = tempfile::TempDir::new().expect("failed to create tempdir");
        let new_dir = tmp.path().join("NewFolder");
        std::fs::create_dir_all(&new_dir).expect("failed to create dir");

        let old_dir = tmp.path().join("OldFolder");

        let events = vec![
            make_debounced_event(
                EventKind::Modify(ModifyKind::Name(
                    notify_debouncer_full::notify::event::RenameMode::From,
                )),
                vec![old_dir.clone()],
            ),
            make_debounced_event(
                EventKind::Modify(ModifyKind::Name(
                    notify_debouncer_full::notify::event::RenameMode::To,
                )),
                vec![new_dir.clone()],
            ),
        ];

        let result = classify_events(&events, &[]);

        assert_eq!(
            result.len(),
            2,
            "expected 2 events for dir rename, got {:?}",
            result
        );
        assert!(
            result
                .iter()
                .any(|c| matches!(c, FileChange::DirectoryRemoved(p) if p == &old_dir)),
            "expected DirectoryRemoved for old path, got {:?}",
            result
        );
        assert!(
            result
                .iter()
                .any(|c| matches!(c, FileChange::DirectoryCreated(p) if p == &new_dir)),
            "expected DirectoryCreated for new path, got {:?}",
            result
        );
    }

    fn scoped_rules(tmp: &tempfile::TempDir) -> ClassificationRules {
        ClassificationRules::new(&WatchTargets {
            source_root: tmp.path().join("src"),
            project_files: vec![tmp.path().join("default.project.json")],
            config_file: Some(tmp.path().join("ezpm.toml")),
            generated_roots: vec![tmp.path().join("darklua_build")],
        })
    }

    #[test]
    fn test_control_file_modifications_are_distinct() {
        let tmp = tempfile::TempDir::new().unwrap();
        let project = tmp.path().join("default.project.json");
        let config = tmp.path().join("ezpm.toml");
        let events = vec![make_debounced_event(
            EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            vec![project.clone(), config.clone()],
        )];

        let result = classify_events_with_rules(&events, &[], &scoped_rules(&tmp));
        assert!(result.contains(&FileChange::RojoProjectChange(project)));
        assert!(result.contains(&FileChange::ConfigChange(config)));
    }

    #[test]
    fn test_atomic_project_replacement_is_deduplicated() {
        let tmp = tempfile::TempDir::new().unwrap();
        let project = tmp.path().join("default.project.json");
        let events = vec![
            make_debounced_event(EventKind::Remove(RemoveKind::File), vec![project.clone()]),
            make_debounced_event(
                EventKind::Modify(ModifyKind::Name(
                    notify_debouncer_full::notify::event::RenameMode::To,
                )),
                vec![project.clone()],
            ),
            make_debounced_event(EventKind::Create(CreateKind::File), vec![project.clone()]),
        ];

        let result = classify_events_with_rules(&events, &[], &scoped_rules(&tmp));
        assert_eq!(result, vec![FileChange::RojoProjectChange(project)]);
    }

    #[test]
    fn test_generated_output_and_parent_siblings_do_not_loop() {
        let tmp = tempfile::TempDir::new().unwrap();
        let generated = tmp.path().join("darklua_build/generated.luau");
        let unrelated = tmp.path().join("README.md");
        let events = vec![make_debounced_event(
            EventKind::Create(CreateKind::File),
            vec![generated, unrelated],
        )];

        let result = classify_events_with_rules(&events, &[], &scoped_rules(&tmp));
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_watcher_detects_atomic_control_file_replacement() {
        init_output();
        let tmp = tempfile::TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let project = tmp.path().join("default.project.json");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(&project, b"{}").unwrap();

        let targets = WatchTargets {
            source_root: src,
            project_files: vec![project.clone()],
            config_file: Some(tmp.path().join("ezpm.toml")),
            generated_roots: vec![tmp.path().join("darklua_build")],
        };
        let (_watcher, mut rx) = FileWatcher::with_targets(targets, &[]).unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        let replacement = tmp.path().join(".default.project.json.tmp");
        std::fs::write(&replacement, br#"{"name":"changed"}"#).unwrap();
        std::fs::rename(replacement, &project).unwrap();

        let received = timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out waiting for atomic project replacement")
            .expect("watch channel closed");
        assert!(matches!(
            received,
            WatchEvent::Changes(ref changes)
                if changes == &vec![FileChange::RojoProjectChange(project)]
        ));
    }
}
