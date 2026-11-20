use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::services::toolchain;

pub const SOURCEMAP_FILE: &str = "sourcemap.json";

#[derive(Debug)]
pub struct SourcemapResult {
    pub success: bool,
    pub stderr: String,
}

pub fn generate_sourcemap_for_project(
    project_dir: &Path,
    project: &Path,
) -> Result<SourcemapResult> {
    let output = toolchain::rojo("sourcemap", project, project_dir)
        .arg("-o")
        .arg(SOURCEMAP_FILE)
        .output()
        .map_err(|error| toolchain::spawn_error("rojo", error))?;

    Ok(SourcemapResult {
        success: output.status.success(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

pub fn generate_index(project_dir: &Path, project: &Path) -> Result<SourcemapIndex> {
    let output = toolchain::rojo("sourcemap", project, project_dir)
        .arg("--absolute")
        .output()
        .map_err(|error| toolchain::spawn_error("rojo", error))?;
    if !output.status.success() {
        return Err(crate::error::EzpmError::tool(format!(
            "Rojo sourcemap failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
        .into());
    }
    SourcemapIndex::parse(project_dir, &output.stdout)
}

#[derive(Debug, Deserialize)]
struct SourcemapNode {
    name: String,
    #[serde(rename = "className")]
    class_name: String,
    #[serde(rename = "filePaths", default)]
    file_paths: Vec<PathBuf>,
    #[serde(default)]
    children: Vec<SourcemapNode>,
}

/// Where a file lands in the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// Dotted instance path, such as `ReplicatedStorage.Shared.Signal`.
    pub instance_path: String,
    pub class_name: String,
}

#[derive(Debug, Clone, Default)]
pub struct SourcemapIndex {
    placements: HashMap<PathBuf, Placement>,
    source_to_game: HashMap<PathBuf, String>,
    game_paths: HashSet<String>,
    game_to_source: HashMap<String, PathBuf>,
    module_files: Vec<PathBuf>,
    script_files: Vec<PathBuf>,
}

impl SourcemapIndex {
    pub fn parse(project_dir: &Path, contents: &[u8]) -> Result<Self> {
        let root: SourcemapNode =
            serde_json::from_slice(contents).context("Rojo emitted an invalid sourcemap")?;
        anyhow::ensure!(
            root.class_name == "DataModel",
            "Rojo project root is {}; an absolute @game path requires a DataModel project",
            root.class_name
        );

        let mut index = Self::default();
        let mut instance_path = Vec::new();
        for child in &root.children {
            index.visit(project_dir, child, &mut instance_path)?;
        }
        index.module_files.sort();
        index.module_files.dedup();
        index.script_files.sort();
        index.script_files.dedup();
        Ok(index)
    }

    pub fn game_path(&self, source_path: &Path) -> Option<&str> {
        self.source_to_game
            .get(&normalize_source_path(source_path))
            .map(String::as_str)
    }

    /// The instance a file becomes, for any file Rojo reads (scripts, models, JSON, meta files).
    pub fn placement(&self, source_path: &Path) -> Option<&Placement> {
        self.placements.get(&normalize_source_path(source_path))
    }

    pub fn contains_game_path(&self, game_path: &str) -> bool {
        self.game_paths.contains(game_path)
    }

    pub fn source_for_game_path(&self, game_path: &str) -> Option<&Path> {
        self.game_to_source.get(game_path).map(PathBuf::as_path)
    }

    pub fn game_path_for_logical_source(
        &self,
        project_dir: &Path,
        logical_path: &str,
    ) -> Option<&str> {
        let base = project_dir.join(logical_path);
        let with_suffix = |suffix: &str| {
            let mut path = base.clone().into_os_string();
            path.push(suffix);
            PathBuf::from(path)
        };
        let candidates = [
            base.clone(),
            with_suffix(".luau"),
            with_suffix(".lua"),
            base.join("init.luau"),
            base.join("init.lua"),
        ];
        candidates.iter().find_map(|path| self.game_path(path))
    }

    pub fn source_files(&self) -> &[PathBuf] {
        &self.module_files
    }

    pub fn script_files(&self) -> &[PathBuf] {
        &self.script_files
    }

    fn visit(
        &mut self,
        project_dir: &Path,
        node: &SourcemapNode,
        instance_path: &mut Vec<String>,
    ) -> Result<()> {
        instance_path.push(node.name.clone());

        let file_paths: Vec<PathBuf> = node
            .file_paths
            .iter()
            .map(|file_path| normalize_source_path(&project_dir.join(file_path)))
            .collect();
        for path in &file_paths {
            self.placements
                .entry(path.clone())
                .or_insert_with(|| Placement {
                    instance_path: instance_path.join("."),
                    class_name: node.class_name.clone(),
                });
        }

        let lua_files = file_paths
            .into_iter()
            .filter(|path| is_lua_file(path))
            .collect::<Vec<_>>();

        // Vendored Wally sources are resolvable modules but are never rewritten:
        // edits there would be lost on the next `wally install`.
        if matches!(
            node.class_name.as_str(),
            "ModuleScript" | "Script" | "LocalScript"
        ) {
            self.script_files.extend(
                lua_files
                    .iter()
                    .filter(|path| !is_wally_index_path(path))
                    .cloned(),
            );
        }

        if node.class_name == "ModuleScript" {
            let game_path = format!("@game/{}", instance_path.join("/"));
            self.game_paths.insert(game_path.clone());
            for source_path in lua_files {
                self.game_to_source
                    .entry(game_path.clone())
                    .or_insert_with(|| source_path.clone());
                if let Some(existing) = self
                    .source_to_game
                    .insert(source_path.clone(), game_path.clone())
                {
                    anyhow::ensure!(
                        existing == game_path,
                        "{} maps to both {} and {} in the Rojo sourcemap",
                        source_path.display(),
                        existing,
                        game_path
                    );
                }
                self.module_files.push(source_path);
            }
        }

        for child in &node.children {
            self.visit(project_dir, child, instance_path)?;
        }
        instance_path.pop();
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn from_pairs(pairs: &[(PathBuf, &str)]) -> Self {
        let mut index = Self::default();
        for (source, game) in pairs {
            let source = source.clone();
            index
                .source_to_game
                .insert(source.clone(), (*game).to_string());
            index.game_paths.insert((*game).to_string());
            index
                .game_to_source
                .insert((*game).to_string(), source.clone());
            index.module_files.push(source.clone());
            index.script_files.push(source);
        }
        index.module_files.sort();
        index
    }

    #[cfg(test)]
    pub(crate) fn add_script_files(&mut self, files: &[PathBuf]) {
        self.script_files.extend_from_slice(files);
        self.script_files.sort();
        self.script_files.dedup();
    }
}

pub(crate) fn is_wally_index_path(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == "_Index")
}

fn normalize_source_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    let path: &Path = &path
        .to_str()
        .and_then(|path| path.strip_prefix("//?/"))
        .map(|path| PathBuf::from(format!(r"\\?\{}", path.replace('/', r"\"))))
        .unwrap_or_else(|| path.to_path_buf());

    dunce::simplified(path).to_path_buf()
}

fn is_lua_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("lua") | Some("luau")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn indexes_module_sources_by_their_rojo_instance_path() {
        let dir = TempDir::new().expect("temp dir");
        let source = dir.path().join("src/features/init.luau");
        std::fs::create_dir_all(source.parent().expect("source parent")).expect("create source");
        std::fs::write(&source, "return {}\n").expect("write source");
        let map = br#"{
  "name": "Game",
  "className": "DataModel",
  "children": [{
    "name": "ReplicatedStorage",
    "className": "ReplicatedStorage",
    "children": [{
      "name": "Features",
      "className": "ModuleScript",
      "filePaths": ["src/features/init.luau"]
    }]
  }, {
    "name": "ServerScriptService",
    "className": "ServerScriptService",
    "children": [{
      "name": "Main",
      "className": "Script",
      "filePaths": ["src/main.server.luau"]
    }]
  }]
}"#;

        let index = SourcemapIndex::parse(dir.path(), map).expect("load sourcemap");
        assert_eq!(
            index.game_path(&source),
            Some("@game/ReplicatedStorage/Features")
        );
        assert!(index
            .script_files()
            .contains(&dir.path().join("src/main.server.luau")));
        assert!(!index
            .source_files()
            .contains(&dir.path().join("src/main.server.luau")));
        assert_eq!(
            index.placement(&dir.path().join("src/main.server.luau")),
            Some(&Placement {
                instance_path: "ServerScriptService.Main".to_string(),
                class_name: "Script".to_string(),
            })
        );
    }

    #[test]
    fn logical_source_lookup_keeps_dotted_module_names() {
        let dir = TempDir::new().expect("temp dir");
        let button = dir.path().join("src/Button.luau");
        let story = dir.path().join("src/Button.story.luau");
        let index = SourcemapIndex::from_pairs(&[
            (button, "@game/ReplicatedStorage/Button"),
            (story, "@game/ReplicatedStorage/Button.story"),
        ]);

        assert_eq!(
            index.game_path_for_logical_source(dir.path(), "src/Button.story"),
            Some("@game/ReplicatedStorage/Button.story")
        );
    }
}
