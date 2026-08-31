use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

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
    let output = Command::new("rojo")
        .arg("sourcemap")
        .arg(project)
        .arg("-o")
        .arg(SOURCEMAP_FILE)
        .current_dir(project_dir)
        .output()
        .with_context(|| toolchain::missing_tool_context("rojo"))?;

    Ok(SourcemapResult {
        success: output.status.success(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

pub fn generate_index(project_dir: &Path, project: &Path) -> Result<SourcemapIndex> {
    let output = Command::new("rojo")
        .arg("sourcemap")
        .arg(project)
        .arg("--absolute")
        .current_dir(project_dir)
        .output()
        .with_context(|| toolchain::missing_tool_context("rojo"))?;
    anyhow::ensure!(
        output.status.success(),
        "Rojo sourcemap failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
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

#[derive(Debug, Clone, Default)]
pub struct SourcemapIndex {
    source_to_game: HashMap<PathBuf, String>,
    game_paths: HashSet<String>,
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
        self.source_to_game.get(source_path).map(String::as_str)
    }

    pub fn contains_game_path(&self, game_path: &str) -> bool {
        self.game_paths.contains(game_path)
    }

    pub fn game_path_for_logical_source(
        &self,
        project_dir: &Path,
        logical_path: &str,
    ) -> Option<&str> {
        let base = project_dir.join(logical_path);
        let candidates = [
            base.clone(),
            base.with_extension("luau"),
            base.with_extension("lua"),
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

        let lua_files = node
            .file_paths
            .iter()
            .filter(|path| is_lua_file(path))
            .map(|file_path| {
                if file_path.is_absolute() {
                    file_path.clone()
                } else {
                    project_dir.join(file_path)
                }
            })
            .collect::<Vec<_>>();

        if matches!(
            node.class_name.as_str(),
            "ModuleScript" | "Script" | "LocalScript"
        ) {
            self.script_files.extend(lua_files.iter().cloned());
        }

        if node.class_name == "ModuleScript" {
            let game_path = format!("@game/{}", instance_path.join("/"));
            self.game_paths.insert(game_path.clone());
            for source_path in lua_files {
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
            index.module_files.push(source.clone());
            index.script_files.push(source);
        }
        index
    }

    #[cfg(test)]
    pub(crate) fn add_script_files(&mut self, files: &[PathBuf]) {
        self.script_files.extend_from_slice(files);
        self.script_files.sort();
        self.script_files.dedup();
    }
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
    }
}
