use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use eframe::egui::{Pos2, Rect};

use crate::app::{CopiedSystemEntry, InteractionKind, SystemsCatalogApp, MAP_GRID_SPACING, MAP_NODE_SIZE};
use crate::models::DatabaseColumnInput;
use crate::plugins::{parse_llm_detailed_import_file, plugin_by_name};
use crate::project_store::{
    DatabaseColumnFile, InteractionFile, ProjectFile, ProjectSettings, ProjectTechItem,
    ProjectZone, ProjectZoneOffset, SystemFile, SystemNoteFile,
};

impl SystemsCatalogApp {
    fn spawn_position_for_new_system(
        &self,
        parent_id: Option<i64>,
    ) -> Option<(Pos2, Option<(i64, Pos2)>)> {
        if parent_id.is_some() {
            return self
                .find_next_free_child_spawn_position(parent_id)
                .map(|position| (position, None));
        }

        let Some(selected_zone_id) = self.selected_zone_id else {
            return Some((self.find_next_free_root_spawn_position(), None));
        };

        let Some(zone) = self.zones.iter().find(|zone| zone.id == selected_zone_id) else {
            return Some((self.find_next_free_root_spawn_position(), None));
        };

        let zone_min_x = zone.x + 12.0;
        let zone_min_y = zone.y + 12.0;
        let zone_max_x = (zone.x + zone.width - MAP_NODE_SIZE.x - 12.0).max(zone_min_x);
        let zone_max_y = (zone.y + zone.height - MAP_NODE_SIZE.y - 12.0).max(zone_min_y);

        let mut candidate = Pos2::new(zone_min_x, zone_min_y);
        for _ in 0..220 {
            let clamped = self.clamp_node_position(Rect::NOTHING, candidate, MAP_NODE_SIZE);
            let inside_zone = clamped.x >= zone_min_x
                && clamped.x <= zone_max_x
                && clamped.y >= zone_min_y
                && clamped.y <= zone_max_y;

            if inside_zone && !self.spawn_position_overlaps(clamped) {
                let offset = Pos2::new(clamped.x - zone.x, clamped.y - zone.y);
                return Some((clamped, Some((selected_zone_id, offset))));
            }

            candidate.x += MAP_GRID_SPACING;
            if candidate.x > zone_max_x {
                candidate.x = zone_min_x;
                candidate.y += MAP_GRID_SPACING;
            }
            if candidate.y > zone_max_y {
                break;
            }
        }

        Some((self.find_next_free_root_spawn_position(), None))
    }

    pub(super) fn bulk_convert_selected_system_types(&mut self, target_type: &str) {
        let normalized_target = Self::normalize_system_type(target_type);

        let mut target_ids = self.selected_map_system_ids.clone();
        if let Some(selected_id) = self.selected_system_id {
            target_ids.insert(selected_id);
        }

        if target_ids.is_empty() {
            self.status_message = "Select one or more systems first".to_owned();
            return;
        }

        let systems_by_id = self
            .systems
            .iter()
            .map(|system| (system.id, system.clone()))
            .collect::<HashMap<_, _>>();

        let mut converted_count = 0usize;
        let result = (|| -> anyhow::Result<()> {
            for system_id in &target_ids {
                let Some(system) = systems_by_id.get(system_id) else {
                    continue;
                };

                if Self::normalize_system_type(system.system_type.as_str()) == normalized_target {
                    continue;
                }

                let route_methods = if normalized_target == "api" {
                    system.route_methods.as_deref()
                } else {
                    None
                };

                self.repo.update_system_details(
                    system.id,
                    system.name.as_str(),
                    system.description.as_str(),
                    system.naming_root,
                    system.naming_delimiter.as_str(),
                    normalized_target.as_str(),
                    route_methods,
                )?;

                if normalized_target != "database" {
                    self.repo
                        .replace_database_columns_for_system(system.id, &[])?;
                }

                self.mark_system_as_dirty(system.id);
                converted_count += 1;
            }

            self.refresh_systems()?;
            if let Some(selected_id) = self.selected_system_id {
                self.load_selected_data(selected_id)?;
            }
            Ok(())
        })();

        match result {
            Ok(_) => {
                self.status_message = format!(
                    "Converted {} system(s) to {}",
                    converted_count,
                    normalized_target
                );
            }
            Err(error) => {
                self.status_message = format!("Failed bulk type conversion: {error}");
            }
        }
    }

    fn write_file_if_changed(path: &Path, bytes: &[u8]) -> anyhow::Result<bool> {
        if let Ok(existing) = std::fs::read(path) {
            if existing == bytes {
                return Ok(false);
            }
        }

        std::fs::write(path, bytes)?;
        Ok(true)
    }

    fn is_filesystem_project_path(path: &str) -> bool {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return false;
        }

        let target = PathBuf::from(trimmed);
        !target
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("db"))
            .unwrap_or(false)
    }

    fn autosave_filesystem_project(&mut self) -> anyhow::Result<()> {
        if !Self::is_filesystem_project_path(self.current_catalog_path.as_str()) {
            return Ok(());
        }

        let root = PathBuf::from(self.current_catalog_path.as_str());
        self.export_catalog_to_filesystem_project(&root)
    }

    fn system_id_from_relative_path(relative_path: &str) -> Option<i64> {
        if !relative_path.starts_with("systems/") {
            return None;
        }

        let file_stem = Path::new(relative_path).file_stem()?.to_str()?;
        let (_, id_text) = file_stem.rsplit_once("__")?;
        id_text.parse::<i64>().ok()
    }

    fn git_changed_system_ids(root: &Path) -> Option<HashSet<i64>> {
        let changed_paths = Self::git_changed_paths(root)?;
        let mut changed_ids = HashSet::new();

        for effective_path in changed_paths {
            if !effective_path.starts_with("systems/") {
                continue;
            }

            let Some(system_id) = Self::system_id_from_relative_path(effective_path.as_str()) else {
                return None;
            };

            changed_ids.insert(system_id);
        }

        Some(changed_ids)
    }

    fn git_changed_paths(root: &Path) -> Option<HashSet<String>> {
        let root_text = root.to_str()?;

        let inside_work_tree = Command::new("git")
            .args(["-C", root_text, "rev-parse", "--is-inside-work-tree"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|value| value.trim().eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        if !inside_work_tree {
            return None;
        }

        let output = Command::new("git")
            .args([
                "-C",
                root_text,
                "status",
                "--porcelain",
                "--untracked-files=all",
                "--",
                "systems",
            ])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8(output.stdout).ok()?;
        let mut changed_paths = HashSet::new();

        for line in stdout.lines() {
            if line.len() < 4 {
                continue;
            }

            let status = &line[..2];
            let raw_path = line[3..].trim();

            let effective_path = if (status.starts_with('R') || status.starts_with('C'))
                && raw_path.contains(" -> ")
            {
                raw_path.rsplit(" -> ").next().unwrap_or(raw_path)
            } else {
                raw_path
            };

            changed_paths.insert(effective_path.to_owned());
        }

        Some(changed_paths)
    }

    fn git_is_repo(root: &Path) -> bool {
        let Some(root_text) = root.to_str() else {
            return false;
        };

        Command::new("git")
            .args(["-C", root_text, "rev-parse", "--is-inside-work-tree"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|value| value.trim().eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    }

    pub(super) fn detect_version_control_status(&mut self) {
        let path = self.current_catalog_path.trim().to_owned();
        if path.is_empty() {
            self.git_repo_detect_path.clear();
            self.git_repo_detected_for_path = None;
            self.status_message = "No project path selected".to_owned();
            return;
        }

        let root = PathBuf::from(path.as_str());
        let detected = Self::git_is_repo(&root);
        self.git_repo_detect_path = path;
        self.git_repo_detected_for_path = Some(detected);
        self.status_message = if detected {
            "Version control detected (git repository present)".to_owned()
        } else {
            "No git repository found for this project".to_owned()
        };
    }

    pub(super) fn enable_version_control(&mut self) {
        if !Self::is_filesystem_project_path(self.current_catalog_path.as_str()) {
            self.status_message = "Version control is only available for filesystem projects".to_owned();
            return;
        }

        let root = PathBuf::from(self.current_catalog_path.as_str());
        let Some(root_text) = root.to_str() else {
            self.status_message = "Invalid project path".to_owned();
            return;
        };

        if Self::git_is_repo(&root) {
            self.git_repo_detect_path = root_text.to_owned();
            self.git_repo_detected_for_path = Some(true);
            self.status_message = "Git repository already initialized".to_owned();
            return;
        }

        let result = (|| -> anyhow::Result<()> {
            let init = Command::new("git")
                .args(["-C", root_text, "init"])
                .output()?;
            if !init.status.success() {
                return Err(anyhow::anyhow!(
                    "git init failed: {}",
                    String::from_utf8_lossy(&init.stderr).trim()
                ));
            }

            let add = Command::new("git")
                .args(["-C", root_text, "add", "-A"])
                .output()?;
            if !add.status.success() {
                return Err(anyhow::anyhow!(
                    "git add failed: {}",
                    String::from_utf8_lossy(&add.stderr).trim()
                ));
            }

            let commit = Command::new("git")
                .args(["-C", root_text, "commit", "-m", "first commit"])
                .output()?;
            if !commit.status.success() {
                let stderr = String::from_utf8_lossy(&commit.stderr);
                let stdout = String::from_utf8_lossy(&commit.stdout);
                let text = format!("{} {}", stdout.trim(), stderr.trim());
                if text.contains("nothing to commit") {
                    return Ok(());
                }
                return Err(anyhow::anyhow!("git commit failed: {}", text.trim()));
            }

            Ok(())
        })();

        match result {
            Ok(_) => {
                self.git_repo_detect_path = root_text.to_owned();
                self.git_repo_detected_for_path = Some(true);
                self.status_message = "Git version control initialized".to_owned();
            }
            Err(error) => {
                self.status_message = format!("Failed to enable version control: {error}");
            }
        }
    }

    pub(super) fn commit_project_changes(&mut self) {
        if !Self::is_filesystem_project_path(self.current_catalog_path.as_str()) {
            self.status_message = "Commit is only available for filesystem projects".to_owned();
            return;
        }

        let root = PathBuf::from(self.current_catalog_path.as_str());
        let Some(root_text) = root.to_str() else {
            self.status_message = "Invalid project path".to_owned();
            return;
        };

        if !Self::git_is_repo(&root) {
            self.git_repo_detect_path = root_text.to_owned();
            self.git_repo_detected_for_path = Some(false);
            self.status_message = "No git repository found. Enable Version Control first".to_owned();
            return;
        }
        self.git_repo_detect_path = root_text.to_owned();
        self.git_repo_detected_for_path = Some(true);

        let message = format!(
            "catalog commit: {} dirty, {} new",
            self.dirty_system_ids.len(),
            self.new_system_ids.len()
        );

        let result = (|| -> anyhow::Result<()> {
            let add = Command::new("git")
                .args(["-C", root_text, "add", "-A"])
                .output()?;
            if !add.status.success() {
                return Err(anyhow::anyhow!(
                    "git add failed: {}",
                    String::from_utf8_lossy(&add.stderr).trim()
                ));
            }

            let commit = Command::new("git")
                .args(["-C", root_text, "commit", "-m", message.as_str()])
                .output()?;
            if !commit.status.success() {
                let stderr = String::from_utf8_lossy(&commit.stderr);
                let stdout = String::from_utf8_lossy(&commit.stdout);
                let text = format!("{} {}", stdout.trim(), stderr.trim());
                if text.contains("nothing to commit") {
                    return Ok(());
                }
                return Err(anyhow::anyhow!("git commit failed: {}", text.trim()));
            }

            Ok(())
        })();

        match result {
            Ok(_) => {
                self.status_message = format!("Committed project changes ({})", message);
            }
            Err(error) => {
                self.status_message = format!("Commit failed: {error}");
            }
        }
    }

    pub(super) fn rollback_project_changes(&mut self) {
        if !Self::is_filesystem_project_path(self.current_catalog_path.as_str()) {
            self.status_message = "Rollback is only available for filesystem projects".to_owned();
            return;
        }

        let root = PathBuf::from(self.current_catalog_path.as_str());
        let Some(root_text) = root.to_str() else {
            self.status_message = "Invalid project path".to_owned();
            return;
        };

        if !Self::git_is_repo(&root) {
            self.git_repo_detect_path = root_text.to_owned();
            self.git_repo_detected_for_path = Some(false);
            self.status_message = "No git repository found. Enable Version Control first".to_owned();
            return;
        }
        self.git_repo_detect_path = root_text.to_owned();
        self.git_repo_detected_for_path = Some(true);

        let changed_count = Self::git_changed_system_ids(&root)
            .map(|ids| ids.len())
            .unwrap_or(0);

        let result = (|| -> anyhow::Result<()> {
            let restore = Command::new("git")
                .args(["-C", root_text, "restore", "--staged", "--worktree", "."])
                .output()?;
            if !restore.status.success() {
                return Err(anyhow::anyhow!(
                    "git restore failed: {}",
                    String::from_utf8_lossy(&restore.stderr).trim()
                ));
            }

            let clean = Command::new("git")
                .args(["-C", root_text, "clean", "-fd"])
                .output()?;
            if !clean.status.success() {
                return Err(anyhow::anyhow!(
                    "git clean failed: {}",
                    String::from_utf8_lossy(&clean.stderr).trim()
                ));
            }

            let reload_path = self.current_catalog_path.clone();
            self.load_catalog_from_path_with_options(reload_path.as_str(), true)?;
            Ok(())
        })();

        match result {
            Ok(_) => {
                self.status_message =
                    format!("Rollback complete ({} changed system file(s) restored)", changed_count);
            }
            Err(error) => {
                self.status_message = format!("Rollback failed: {error}");
            }
        }
    }

    pub(super) fn maybe_autosave_project(&mut self, now_secs: f64) {
        const AUTOSAVE_INTERVAL_SECS: f64 = 2.0;

        if !self.project_autosave_enabled {
            return;
        }

        if !Self::is_filesystem_project_path(self.current_catalog_path.as_str()) {
            return;
        }

        let should_save = self
            .project_last_autosave_at_secs
            .map(|last| now_secs - last >= AUTOSAVE_INTERVAL_SECS)
            .unwrap_or(true);

        if !should_save {
            return;
        }

        self.project_last_autosave_at_secs = Some(now_secs);
        if let Err(error) = self.autosave_filesystem_project() {
            self.status_message = format!("Autosave failed: {error}");
        }
    }

    fn import_from_plugin_path(&mut self, plugin_name: &str, input_path: &Path) -> anyhow::Result<usize> {
        let Some(plugin) = plugin_by_name(plugin_name) else {
            return Err(anyhow::anyhow!("Unknown plugin: {}", plugin_name));
        };
        let definition = plugin.definition();
        if definition.name != plugin_name {
            return Err(anyhow::anyhow!(
                "Plugin name mismatch: expected '{}', got '{}'",
                plugin_name,
                definition.name
            ));
        }
        match definition.input_type {
            crate::plugins::PluginInputType::FileSystem => {}
        }

        let drafts = plugin.transform_file(input_path)?;
        if drafts.is_empty() {
            return Ok(0);
        }

        let expected_system_type = definition.system_type;
        if !expected_system_type.trim().is_empty()
            && drafts
                .iter()
                .any(|draft| !draft.system_type.eq_ignore_ascii_case(expected_system_type))
        {
            return Err(anyhow::anyhow!(
                "Plugin '{}' produced a system type outside '{}'",
                definition.display_name,
                expected_system_type
            ));
        }

        let mut created_ids = Vec::new();
        let mut created_ids_by_key = HashMap::<String, i64>::new();
        let mut pending = drafts.into_iter().collect::<Vec<_>>();

        while !pending.is_empty() {
            let mut next_pending = Vec::new();
            let mut progress = false;

            for draft in pending {
                let parent_id = match draft.parent_source_key.as_deref() {
                    Some(parent_key) => match created_ids_by_key.get(parent_key).copied() {
                        Some(parent_id) => Some(parent_id),
                        None => {
                            next_pending.push(draft);
                            continue;
                        }
                    },
                    None => None,
                };

                let system_id = self.repo.create_system(
                    draft.name.as_str(),
                    draft.description.as_str(),
                    parent_id,
                    draft.system_type.as_str(),
                    draft.route_methods.as_deref(),
                )?;

                self.mark_system_as_new(system_id);

                if !draft.database_columns.is_empty() {
                    self.repo
                        .replace_database_columns_for_system(system_id, &draft.database_columns)?;
                    self.mark_system_as_dirty(system_id);
                }

                if let Some(source_key) = draft.source_key {
                    created_ids_by_key.insert(source_key, system_id);
                }

                created_ids.push(system_id);
                progress = true;
            }

            if !progress {
                return Err(anyhow::anyhow!(
                    "Plugin import failed to resolve parent hierarchy"
                ));
            }

            pending = next_pending;
        }

        self.refresh_systems()?;
        for system_id in &created_ids {
            let position = self.find_next_free_root_spawn_position();
            self.map_positions.insert(*system_id, position);
            self.persist_map_position(*system_id, position);
        }

        if let Some(first) = created_ids.first().copied() {
            self.selected_system_id = Some(first);
            let _ = self.load_selected_data(first);
        }

        Ok(created_ids.len())
    }

    fn referenced_columns_for_system(&self, system_id: i64) -> HashSet<String> {
        let mut referenced = HashSet::new();

        for link in &self.all_links {
            if link.source_system_id == system_id {
                if let Some(column_name) = link.source_column_name.as_deref() {
                    let normalized = column_name.trim();
                    if !normalized.is_empty() {
                        referenced.insert(normalized.to_owned());
                    }
                }
            }

            if link.target_system_id == system_id {
                if let Some(column_name) = link.target_column_name.as_deref() {
                    let normalized = column_name.trim();
                    if !normalized.is_empty() {
                        referenced.insert(normalized.to_owned());
                    }
                }
            }
        }

        referenced
    }

    fn merge_ddl_columns_preserving_references(
        mut imported_columns: Vec<DatabaseColumnInput>,
        referenced_columns: &HashSet<String>,
    ) -> Vec<DatabaseColumnInput> {
        let mut known = imported_columns
            .iter()
            .map(|column| column.column_name.trim().to_ascii_lowercase())
            .collect::<HashSet<_>>();

        let mut next_position = imported_columns.len() as i64;
        for referenced in referenced_columns {
            let normalized = referenced.trim();
            if normalized.is_empty() {
                continue;
            }

            let key = normalized.to_ascii_lowercase();
            if known.contains(&key) {
                continue;
            }

            imported_columns.push(DatabaseColumnInput {
                position: next_position,
                column_name: normalized.to_owned(),
                column_type: "text".to_owned(),
                constraints: Some("preserved for existing link references".to_owned()),
            });
            known.insert(key);
            next_position += 1;
        }

        imported_columns
    }

    fn normalize_table_identifier(value: &str) -> String {
        value
            .trim()
            .trim_matches('`')
            .trim_matches('"')
            .trim_matches('[')
            .trim_matches(']')
            .to_ascii_lowercase()
    }

    fn table_identifier_parts(value: &str) -> (String, Option<String>, String) {
        let normalized = Self::normalize_table_identifier(value);
        let segments = normalized
            .split('.')
            .map(str::trim)
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>();

        if segments.is_empty() {
            return (normalized, None, String::new());
        }

        let leaf = segments.last().map(|segment| (*segment).to_owned()).unwrap_or_default();
        let schema = if segments.len() >= 2 {
            Some(segments[0].to_owned())
        } else {
            None
        };

        (normalized, schema, leaf)
    }

    fn preselect_database_system_for_ddl_table(
        &self,
        ddl_table_name: &str,
        database_systems: &[crate::models::SystemRecord],
        systems_by_id: &HashMap<i64, crate::models::SystemRecord>,
    ) -> Option<i64> {
        let (ddl_full, ddl_schema, ddl_leaf) = Self::table_identifier_parts(ddl_table_name);
        if ddl_leaf.is_empty() {
            return None;
        }

        let full_matches = database_systems
            .iter()
            .filter(|system| Self::normalize_table_identifier(system.name.as_str()) == ddl_full)
            .map(|system| system.id)
            .collect::<Vec<_>>();

        if full_matches.len() == 1 {
            return full_matches.first().copied();
        }

        let leaf_matches = database_systems
            .iter()
            .filter(|system| {
                let (_, _, system_leaf) = Self::table_identifier_parts(system.name.as_str());
                system_leaf == ddl_leaf
            })
            .map(|system| system.id)
            .collect::<Vec<_>>();

        if leaf_matches.len() == 1 {
            return leaf_matches.first().copied();
        }

        if leaf_matches.len() > 1 {
            if let Some(schema_name) = ddl_schema {
                let parent_name_matches = leaf_matches
                    .iter()
                    .filter_map(|system_id| {
                        let system = systems_by_id.get(system_id)?;
                        let parent_id = system.parent_id?;
                        let parent = systems_by_id.get(&parent_id)?;
                        let parent_name = Self::normalize_table_identifier(parent.name.as_str());
                        if parent_name == schema_name {
                            Some(*system_id)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();

                if parent_name_matches.len() == 1 {
                    return parent_name_matches.first().copied();
                }
            }
        }

        None
    }

    fn apply_ddl_mapping(
        &mut self,
        drafts: Vec<crate::plugins::PluginSystemDraft>,
        targets: Vec<Option<i64>>,
        plugin_display_name: &str,
    ) -> anyhow::Result<(usize, usize)> {
        if drafts.len() != targets.len() {
            return Err(anyhow::anyhow!(
                "{} mapping state mismatch ({} drafts vs {} targets)",
                plugin_display_name,
                drafts.len(),
                targets.len()
            ));
        }

        self.refresh_systems()?;

        let mut used_existing_targets = HashSet::new();
        for target in targets.iter().flatten() {
            if !used_existing_targets.insert(*target) {
                return Err(anyhow::anyhow!(
                    "One existing system is mapped to multiple DDL tables. Use unique targets."
                ));
            }
        }

        let systems_by_id = self
            .systems
            .iter()
            .map(|system| (system.id, system.clone()))
            .collect::<HashMap<_, _>>();

        let mut created_ids = Vec::new();
        let mut updated_count = 0_usize;

        for (draft, target_system_id) in drafts.into_iter().zip(targets.into_iter()) {
            if let Some(existing_id) = target_system_id {
                let Some(existing) = systems_by_id.get(&existing_id) else {
                    return Err(anyhow::anyhow!(
                        "Mapped target system {} no longer exists",
                        existing_id
                    ));
                };

                self.repo.update_system_details(
                    existing.id,
                    existing.name.as_str(),
                    draft.description.as_str(),
                    existing.naming_root,
                    existing.naming_delimiter.as_str(),
                    "database",
                    None,
                )?;

                let referenced_columns = self.referenced_columns_for_system(existing.id);
                let merged_columns =
                    Self::merge_ddl_columns_preserving_references(draft.database_columns, &referenced_columns);

                self.repo
                    .replace_database_columns_for_system(existing.id, &merged_columns)?;

                self.mark_system_as_dirty(existing.id);
                updated_count += 1;
            } else {
                let new_id = self.repo.create_system(
                    draft.name.as_str(),
                    draft.description.as_str(),
                    None,
                    "database",
                    None,
                )?;

                self.mark_system_as_new(new_id);

                if !draft.database_columns.is_empty() {
                    self.repo
                        .replace_database_columns_for_system(new_id, &draft.database_columns)?;
                }

                created_ids.push(new_id);
            }
        }

        self.refresh_systems()?;
        for system_id in &created_ids {
            let position = self.find_next_free_root_spawn_position();
            self.map_positions.insert(*system_id, position);
            self.persist_map_position(*system_id, position);
        }

        if let Some(first) = created_ids.first().copied() {
            self.selected_system_id = Some(first);
            let _ = self.load_selected_data(first);
        }

        Ok((created_ids.len(), updated_count))
    }

    pub(super) fn apply_pending_ddl_table_mapping(&mut self) {
        let drafts = std::mem::take(&mut self.pending_ddl_drafts);
        let targets = std::mem::take(&mut self.pending_ddl_target_system_ids);

        let result = self.apply_ddl_mapping(drafts, targets, "DDL Plugin");
        self.show_ddl_table_mapping_modal = false;

        match result {
            Ok((created_count, updated_count)) => {
                self.status_message = format!(
                    "DDL Plugin synced: {} created, {} updated",
                    created_count, updated_count
                );
            }
            Err(error) => {
                self.status_message = format!("DDL Plugin mapping failed: {error}");
            }
        }
    }

    pub(super) fn cancel_pending_ddl_table_mapping(&mut self) {
        self.pending_ddl_drafts.clear();
        self.pending_ddl_target_system_ids.clear();
        self.show_ddl_table_mapping_modal = false;
        self.status_message = "DDL import mapping canceled".to_owned();
    }

    pub(super) fn import_database_tables_from_ddl_path(&mut self, input_path: &Path) {
        let result = (|| -> anyhow::Result<(String, usize, usize)> {
            let Some(plugin) = plugin_by_name("plugin.ddl") else {
                return Err(anyhow::anyhow!("Unknown plugin: plugin.ddl"));
            };
            let definition = plugin.definition();

            self.refresh_systems()?;

            let drafts = plugin.transform_file(input_path)?;
            if drafts.is_empty() {
                return Ok((definition.display_name.to_owned(), 0, 0));
            }

            let existing_database_systems = self
                .systems
                .iter()
                .filter(|system| system.system_type.eq_ignore_ascii_case("database"))
                .cloned()
                .collect::<Vec<_>>();

            if !existing_database_systems.is_empty() {
                let systems_by_id = self
                    .systems
                    .iter()
                    .map(|system| (system.id, system.clone()))
                    .collect::<HashMap<_, _>>();

                let targets = drafts
                    .iter()
                    .map(|draft| {
                        self.preselect_database_system_for_ddl_table(
                            draft.name.as_str(),
                            &existing_database_systems,
                            &systems_by_id,
                        )
                    })
                    .collect::<Vec<_>>();

                self.pending_ddl_drafts = drafts;
                self.pending_ddl_target_system_ids = targets;
                self.open_modal(crate::app::AppModal::DdlTableMapping);
                self.status_message =
                    "DDL import mapping ready; review preselected matches and apply".to_owned();
                return Ok((definition.display_name.to_owned(), 0, 0));
            }

            let targets = vec![None; drafts.len()];
            let (created_count, updated_count) =
                self.apply_ddl_mapping(drafts, targets, definition.display_name)?;
            Ok((definition.display_name.to_owned(), created_count, updated_count))
        })();

        match result {
            Ok((display_name, created_count, updated_count)) => {
                if self.show_ddl_table_mapping_modal {
                    return;
                }

                self.status_message = format!(
                    "{} synced: {} created, {} updated",
                    display_name, created_count, updated_count
                );
            }
            Err(error) => {
                self.status_message = format!("DDL Plugin import failed: {error}");
            }
        }
    }

    pub(super) fn import_api_routes_from_openapi_path(&mut self, input_path: &Path) {
        let plugin_display_name = plugin_by_name("plugin.openapi")
            .map(|plugin| plugin.definition().display_name.to_owned())
            .unwrap_or_else(|| "OpenAPI Plugin".to_owned());

        let result = (|| -> anyhow::Result<(usize, usize)> {
            let Some(plugin) = plugin_by_name("plugin.openapi") else {
                return Err(anyhow::anyhow!("Unknown plugin: plugin.openapi"));
            };

            let mut drafts = plugin.transform_file(input_path)?;
            if drafts.is_empty() {
                return Ok((0, 0));
            }

            drafts.sort_by(|left, right| {
                let left_depth = left
                    .source_key
                    .as_deref()
                    .map(|key| key.split('/').filter(|segment| !segment.is_empty()).count())
                    .unwrap_or(0);
                let right_depth = right
                    .source_key
                    .as_deref()
                    .map(|key| key.split('/').filter(|segment| !segment.is_empty()).count())
                    .unwrap_or(0);
                left_depth
                    .cmp(&right_depth)
                    .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            });

            self.refresh_systems()?;
            let mut api_systems = self
                .systems
                .iter()
                .filter(|system| system.system_type.eq_ignore_ascii_case("api"))
                .cloned()
                .collect::<Vec<_>>();

            let mut created_count = 0_usize;
            let mut updated_count = 0_usize;
            let mut created_ids = Vec::new();
            let mut id_by_source_key = HashMap::<String, i64>::new();

            for draft in drafts {
                let parent_id = draft
                    .parent_source_key
                    .as_deref()
                    .and_then(|key| id_by_source_key.get(key).copied());

                let existing = api_systems
                    .iter()
                    .find(|system| {
                        system.parent_id == parent_id
                            && system.name.eq_ignore_ascii_case(draft.name.as_str())
                    })
                    .cloned();

                if let Some(existing) = existing {
                    let existing_methods = existing
                        .route_methods
                        .as_deref()
                        .unwrap_or("")
                        .trim()
                        .to_ascii_uppercase();
                    let draft_methods = draft
                        .route_methods
                        .as_deref()
                        .unwrap_or("")
                        .trim()
                        .to_ascii_uppercase();

                    if existing.description != draft.description
                        || existing_methods != draft_methods
                        || existing.system_type != "api"
                    {
                        self.repo.update_system_details(
                            existing.id,
                            existing.name.as_str(),
                            draft.description.as_str(),
                            existing.naming_root,
                            existing.naming_delimiter.as_str(),
                            "api",
                            draft.route_methods.as_deref(),
                        )?;
                        self.mark_system_as_dirty(existing.id);
                        updated_count += 1;
                    }

                    if let Some(source_key) = draft.source_key {
                        id_by_source_key.insert(source_key, existing.id);
                    }
                } else {
                    let system_id = self.repo.create_system(
                        draft.name.as_str(),
                        draft.description.as_str(),
                        parent_id,
                        "api",
                        draft.route_methods.as_deref(),
                    )?;

                    self.mark_system_as_new(system_id);
                    created_count += 1;
                    created_ids.push((system_id, parent_id));

                    api_systems.push(crate::models::SystemRecord {
                        id: system_id,
                        name: draft.name.clone(),
                        description: draft.description,
                        parent_id,
                        map_x: None,
                        map_y: None,
                        line_color_override: None,
                        naming_root: false,
                        naming_delimiter: "/".to_owned(),
                        system_type: "api".to_owned(),
                        route_methods: draft.route_methods.clone(),
                    });

                    if let Some(source_key) = draft.source_key {
                        id_by_source_key.insert(source_key, system_id);
                    }
                }
            }

            self.refresh_systems()?;
            for (system_id, parent_id) in created_ids {
                if let Some((position, _)) = self.spawn_position_for_new_system(parent_id) {
                    self.map_positions.insert(system_id, position);
                    self.persist_map_position(system_id, position);
                }
            }

            Ok((created_count, updated_count))
        })();

        match result {
            Ok((created_count, updated_count)) => {
                self.status_message = format!(
                    "{} synced: {} created, {} updated",
                    plugin_display_name, created_count, updated_count
                );
            }
            Err(error) => {
                self.status_message = format!("{} import failed: {error}", plugin_display_name);
            }
        }
    }

    pub(super) fn import_llm_detailed_map_from_path(&mut self, input_path: &Path) {
        match parse_llm_detailed_import_file(input_path) {
            Ok((systems, interactions)) => {
                if systems.is_empty() {
                    self.status_message = "LLM Detailed Import: no systems found".to_owned();
                    return;
                }

                self.pending_llm_detailed_system_drafts = systems;
                self.pending_llm_detailed_interaction_drafts = interactions;
                self.pending_llm_detailed_root_name = input_path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("Imported Systems")
                    .to_owned();
                self.open_modal(crate::app::AppModal::LlmDetailedImport);
                self.status_message = "LLM detailed import ready; provide root name and apply"
                    .to_owned();
            }
            Err(error) => {
                self.status_message = format!("LLM Detailed Import failed: {error}");
            }
        }
    }

    fn normalize_interaction_kind_label(value: Option<&str>) -> crate::app::InteractionKind {
        let normalized = value.unwrap_or("standard").trim().to_ascii_lowercase();
        match normalized.as_str() {
            "pull" => crate::app::InteractionKind::Pull,
            "push" => crate::app::InteractionKind::Push,
            "bidirectional" | "bi" | "both" => crate::app::InteractionKind::Bidirectional,
            _ => crate::app::InteractionKind::Standard,
        }
    }

    fn place_tree_node_position(&mut self, system_id: i64, desired: Pos2) -> Pos2 {
        let mut candidate = self.clamp_node_position(
            eframe::egui::Rect::NOTHING,
            desired,
            MAP_NODE_SIZE,
        );
        let mut attempts = 0;
        while self.spawn_position_overlaps(candidate) && attempts < 250 {
            candidate.y += MAP_GRID_SPACING;
            candidate = self.clamp_node_position(
                eframe::egui::Rect::NOTHING,
                candidate,
                MAP_NODE_SIZE,
            );
            attempts += 1;
        }

        self.map_positions.insert(system_id, candidate);
        self.persist_map_position(system_id, candidate);
        candidate
    }

    fn layout_imported_tree_under_root(&mut self, root_id: i64, imported_system_ids: &HashSet<i64>) {
        if imported_system_ids.is_empty() {
            return;
        }

        let root_position = self.find_next_free_root_spawn_position();
        let root_position = self.place_tree_node_position(root_id, root_position);
        let root_row_y = root_position.y;

        let mut children_by_parent = HashMap::<i64, Vec<i64>>::new();
        let mut system_names_by_id = HashMap::<i64, String>::new();
        for system in &self.systems {
            system_names_by_id.insert(system.id, system.name.to_ascii_lowercase());
            if !imported_system_ids.contains(&system.id) {
                continue;
            }

            let Some(parent_id) = system.parent_id else {
                continue;
            };

            if imported_system_ids.contains(&parent_id) {
                children_by_parent.entry(parent_id).or_default().push(system.id);
            }
        }

        for child_ids in children_by_parent.values_mut() {
            child_ids.sort_by_key(|system_id| {
                system_names_by_id
                    .get(system_id)
                    .cloned()
                    .unwrap_or_default()
            });
        }

        let mut next_row = 2.0_f32;
        let mut stack = children_by_parent
            .get(&root_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .rev()
            .map(|child_id| (child_id, 1usize))
            .collect::<Vec<_>>();

        while let Some((node_id, depth)) = stack.pop() {
            let desired = Pos2::new(
                root_position.x + (depth as f32 * MAP_GRID_SPACING * 2.0),
                root_row_y + (next_row * MAP_GRID_SPACING),
            );
            self.place_tree_node_position(node_id, desired);
            next_row += 2.0;

            if let Some(children) = children_by_parent.get(&node_id) {
                for child_id in children.iter().rev() {
                    stack.push((*child_id, depth + 1));
                }
            }
        }
    }

    fn sorted_children_for_subtree(&self, root_id: i64) -> HashMap<i64, Vec<i64>> {
        let subtree_ids = self.system_and_descendant_ids(root_id);
        let mut children_by_parent = HashMap::<i64, Vec<i64>>::new();
        let mut system_names_by_id = HashMap::<i64, String>::new();

        for system in &self.systems {
            system_names_by_id.insert(system.id, system.name.to_ascii_lowercase());

            if !subtree_ids.contains(&system.id) || system.id == root_id {
                continue;
            }

            let Some(parent_id) = system.parent_id else {
                continue;
            };

            if subtree_ids.contains(&parent_id) {
                children_by_parent.entry(parent_id).or_default().push(system.id);
            }
        }

        for child_ids in children_by_parent.values_mut() {
            child_ids.sort_by_key(|system_id| {
                system_names_by_id
                    .get(system_id)
                    .cloned()
                    .unwrap_or_default()
            });
        }

        children_by_parent
    }

    pub(super) fn layout_selected_subsystem_file_tree(&mut self) {
        let Some(root_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let Some(root_position) = self.effective_map_position(root_id) else {
            self.status_message = "Selected system has no map position".to_owned();
            return;
        };

        let children_by_parent = self.sorted_children_for_subtree(root_id);
        let affected_count = self.system_and_descendant_ids(root_id).len();
        if affected_count <= 1 {
            self.status_message = "Selected system has no descendants to lay out".to_owned();
            return;
        }

        self.push_map_undo_snapshot();

        let mut next_row = 2.0_f32;
        let mut moved_count = 0usize;
        let mut stack = children_by_parent
            .get(&root_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .rev()
            .map(|child_id| (child_id, 1usize))
            .collect::<Vec<_>>();

        while let Some((node_id, depth)) = stack.pop() {
            let desired = Pos2::new(
                root_position.x + (depth as f32 * MAP_GRID_SPACING * 2.0),
                root_position.y + (next_row * MAP_GRID_SPACING),
            );
            self.place_tree_node_position(node_id, desired);
            moved_count += 1;
            next_row += 2.0;

            if let Some(children) = children_by_parent.get(&node_id) {
                for child_id in children.iter().rev() {
                    stack.push((*child_id, depth + 1));
                }
            }
        }

        if moved_count > 0 {
            self.mark_project_as_dirty();
        }
        self.status_message = format!(
            "Applied file tree layout to {} subsystem{}",
            moved_count,
            if moved_count == 1 { "" } else { "s" }
        );
    }

    pub(super) fn layout_selected_subsystem_regular_tree(&mut self) {
        let Some(root_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let Some(root_position) = self.effective_map_position(root_id) else {
            self.status_message = "Selected system has no map position".to_owned();
            return;
        };

        let children_by_parent = self.sorted_children_for_subtree(root_id);
        let affected_count = self.system_and_descendant_ids(root_id).len();
        if affected_count <= 1 {
            self.status_message = "Selected system has no descendants to lay out".to_owned();
            return;
        }

        self.push_map_undo_snapshot();

        let mut moved_count = 0usize;
        let mut stack = vec![root_id];
        let mut positions = HashMap::<i64, Pos2>::new();
        positions.insert(root_id, root_position);

        while let Some(parent_id) = stack.pop() {
            let Some(parent_position) = positions.get(&parent_id).copied() else {
                continue;
            };

            let Some(children) = children_by_parent.get(&parent_id).cloned() else {
                continue;
            };

            for (index, child_id) in children.iter().enumerate() {
                let desired = Pos2::new(
                    parent_position.x + (MAP_GRID_SPACING * 2.0),
                    parent_position.y + ((index as f32 + 1.0) * MAP_GRID_SPACING * 2.0),
                );
                let placed = self.place_tree_node_position(*child_id, desired);
                moved_count += 1;
                positions.insert(*child_id, placed);
                stack.push(*child_id);
            }
        }

        if moved_count > 0 {
            self.mark_project_as_dirty();
        }
        self.status_message = format!(
            "Applied regular tree layout to {} subsystem{}",
            moved_count,
            if moved_count == 1 { "" } else { "s" }
        );
    }

    pub(super) fn apply_pending_llm_detailed_import(&mut self) {
        let root_name = self.pending_llm_detailed_root_name.trim().to_owned();
        if root_name.is_empty() {
            self.status_message = "Root name is required".to_owned();
            return;
        }

        let systems = std::mem::take(&mut self.pending_llm_detailed_system_drafts);
        let interactions = std::mem::take(&mut self.pending_llm_detailed_interaction_drafts);

        let result = (|| -> anyhow::Result<(usize, usize)> {
            let root_id = self
                .repo
                .create_system(root_name.as_str(), "LLM detailed import root", None, "service", None)?;
            self.mark_system_as_new(root_id);

            let mut created_ids_by_key = HashMap::<String, i64>::new();
            let mut created_system_ids = vec![root_id];
            let mut pending = systems;

            while !pending.is_empty() {
                let mut next_pending = Vec::new();
                let mut progress = false;

                for draft in pending {
                    let parent_id = match draft.parent_source_key.as_deref() {
                        Some(parent_key) => match created_ids_by_key.get(parent_key).copied() {
                            Some(parent_id) => Some(parent_id),
                            None => {
                                next_pending.push(draft);
                                continue;
                            }
                        },
                        None => Some(root_id),
                    };

                    let system_id = self.repo.create_system(
                        draft.name.as_str(),
                        draft.description.as_str(),
                        parent_id,
                        Self::normalize_system_type(draft.system_type.as_str()).as_str(),
                        draft.route_methods.as_deref(),
                    )?;
                    self.mark_system_as_new(system_id);
                    created_system_ids.push(system_id);

                    if let Some(source_key) = draft.source_key {
                        created_ids_by_key.insert(source_key, system_id);
                    }
                    progress = true;
                }

                if !progress {
                    return Err(anyhow::anyhow!(
                        "LLM detailed import failed to resolve system hierarchy"
                    ));
                }
                pending = next_pending;
            }

            let mut aggregated = HashMap::<(i64, i64), (crate::app::InteractionKind, String, String)>::new();

            for interaction in interactions {
                let Some(source_id) = created_ids_by_key.get(interaction.source_key.as_str()).copied() else {
                    continue;
                };
                let Some(target_id) = created_ids_by_key.get(interaction.target_key.as_str()).copied() else {
                    continue;
                };
                if source_id == target_id {
                    continue;
                }

                let kind = Self::normalize_interaction_kind_label(interaction.kind.as_deref());
                let key = if source_id <= target_id {
                    (source_id, target_id)
                } else {
                    (target_id, source_id)
                };

                let entry = aggregated.entry(key).or_insert_with(|| {
                    (
                        kind,
                        interaction.label.clone(),
                        interaction.note.clone(),
                    )
                });

                let existing_kind = entry.0;
                entry.0 = match (existing_kind, kind) {
                    (crate::app::InteractionKind::Bidirectional, _)
                    | (_, crate::app::InteractionKind::Bidirectional) => {
                        crate::app::InteractionKind::Bidirectional
                    }
                    (crate::app::InteractionKind::Pull, crate::app::InteractionKind::Push)
                    | (crate::app::InteractionKind::Push, crate::app::InteractionKind::Pull) => {
                        crate::app::InteractionKind::Bidirectional
                    }
                    (_, next_kind) => next_kind,
                };

                if entry.1.trim().is_empty() && !interaction.label.trim().is_empty() {
                    entry.1 = interaction.label;
                }
                if entry.2.trim().is_empty() && !interaction.note.trim().is_empty() {
                    entry.2 = interaction.note;
                }
            }

            let mut created_interactions = 0usize;
            for ((source_id, target_id), (kind, label, note)) in aggregated {
                self.repo.create_link(
                    source_id,
                    target_id,
                    label.trim(),
                    Self::interaction_kind_to_setting_value(kind),
                    None,
                    None,
                )?;

                if !note.trim().is_empty() {
                    if let Some(link_id) = self
                        .repo
                        .list_links()?
                        .into_iter()
                        .find(|link| link.source_system_id == source_id && link.target_system_id == target_id)
                        .map(|link| link.id)
                    {
                        self.repo.update_link_details(
                            link_id,
                            label.trim(),
                            note.trim(),
                            Self::interaction_kind_to_setting_value(kind),
                            None,
                            None,
                        )?;
                    }
                }

                created_interactions += 1;
            }

            self.refresh_systems()?;
            let mut imported_ids = created_system_ids.into_iter().collect::<HashSet<_>>();
            imported_ids.insert(root_id);
            self.layout_imported_tree_under_root(root_id, &imported_ids);

            Ok((created_ids_by_key.len() + 1, created_interactions))
        })();

        self.show_llm_detailed_import_modal = false;

        match result {
            Ok((system_count, interaction_count)) => {
                self.status_message = format!(
                    "LLM Detailed Import complete: {} systems, {} interactions",
                    system_count, interaction_count
                );
            }
            Err(error) => {
                self.status_message = format!("LLM Detailed Import failed: {error}");
            }
        }
    }

    pub(super) fn cancel_pending_llm_detailed_import(&mut self) {
        self.pending_llm_detailed_system_drafts.clear();
        self.pending_llm_detailed_interaction_drafts.clear();
        self.pending_llm_detailed_root_name.clear();
        self.show_llm_detailed_import_modal = false;
        self.status_message = "LLM detailed import canceled".to_owned();
    }

    pub(super) fn import_llm_systems_from_path(&mut self, input_path: &Path) {
        let plugin_display_name = plugin_by_name("plugin.llm")
            .map(|plugin| plugin.definition().display_name.to_owned())
            .unwrap_or_else(|| "LLM Import Plugin".to_owned());

        match self.import_from_plugin_path("plugin.llm", input_path) {
            Ok(count) => {
                self.status_message =
                    format!("{} imported {} system(s)", plugin_display_name, count);
            }
            Err(error) => {
                self.status_message = format!("{} import failed: {error}", plugin_display_name);
            }
        }
    }

    fn slugify_segment(value: &str) -> String {
        let mut slug = String::new();
        for character in value.chars() {
            if character.is_ascii_alphanumeric() {
                slug.push(character.to_ascii_lowercase());
            } else if (character == ' ' || character == '-' || character == '_') && !slug.ends_with('_') {
                slug.push('_');
            }
        }

        let slug = slug.trim_matches('_');
        if slug.is_empty() {
            "item".to_owned()
        } else {
            slug.to_owned()
        }
    }

    fn system_relative_file_path(
        &self,
        system_id: i64,
        system_by_id: &HashMap<i64, crate::models::SystemRecord>,
    ) -> String {
        let Some(system) = system_by_id.get(&system_id) else {
            return format!("systems/system__{}.json", system_id);
        };

        let short_slug = |value: &str| {
            let mut slug = Self::slugify_segment(value);
            if slug.chars().count() > 24 {
                slug = slug.chars().take(24).collect::<String>();
            }
            if slug.is_empty() {
                "item".to_owned()
            } else {
                slug
            }
        };

        let parent_prefix = system
            .parent_id
            .and_then(|parent_id| system_by_id.get(&parent_id))
            .map(|parent| format!("p{}", short_slug(parent.name.as_str())))
            .unwrap_or_else(|| "root".to_owned());

        let system_slug = short_slug(system.name.as_str());
        format!("systems/{}_{}__{}.json", parent_prefix, system_slug, system.id)
    }

    fn existing_project_system_paths(root: &Path) -> Vec<String> {
        let project_path = root.join("Project.json");
        let Ok(project_text) = std::fs::read_to_string(project_path) else {
            return Vec::new();
        };

        let Ok(project) = serde_json::from_str::<ProjectFile>(project_text.as_str()) else {
            return Vec::new();
        };

        project.systems_paths
    }

    fn remove_empty_system_dirs_upward(
        mut current: Option<&Path>,
        systems_root: &Path,
    ) -> anyhow::Result<()> {
        while let Some(directory) = current {
            if directory == systems_root || !directory.starts_with(systems_root) {
                break;
            }

            let mut entries = std::fs::read_dir(directory)?;
            if entries.next().is_some() {
                break;
            }

            std::fs::remove_dir(directory)?;
            current = directory.parent();
        }

        Ok(())
    }

    fn remove_stale_system_files(
        root: &Path,
        systems_root: &Path,
        previous_paths: &[String],
        current_paths: &[String],
    ) -> anyhow::Result<()> {
        let current_set = current_paths.iter().map(String::as_str).collect::<HashSet<_>>();

        for relative_path in previous_paths {
            if !relative_path.starts_with("systems/") {
                continue;
            }
            if current_set.contains(relative_path.as_str()) {
                continue;
            }

            let absolute_path = root.join(relative_path.replace('/', std::path::MAIN_SEPARATOR_STR));
            if !absolute_path.is_file() {
                continue;
            }

            std::fs::remove_file(&absolute_path)?;
            Self::remove_empty_system_dirs_upward(absolute_path.parent(), systems_root)?;
        }

        Ok(())
    }

    fn export_catalog_to_filesystem_project(&mut self, root: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(root)?;

        let systems_root = root.join("systems");
        let interactions_root = root.join("interactions");
        std::fs::create_dir_all(&systems_root)?;
        std::fs::create_dir_all(&interactions_root)?;

        let previous_system_paths = if self.manage_system_json_hierarchy {
            Self::existing_project_system_paths(root)
        } else {
            Vec::new()
        };

        let system_by_id = self
            .systems
            .iter()
            .cloned()
            .map(|system| (system.id, system))
            .collect::<HashMap<_, _>>();

        let mut systems_paths = Vec::new();
        for system in &self.systems {
            let relative_path = self.system_relative_file_path(system.id, &system_by_id);
            let absolute_path = root.join(relative_path.replace('/', std::path::MAIN_SEPARATOR_STR));
            if let Some(parent) = absolute_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let (map_x, map_y) = self
                .map_positions
                .get(&system.id)
                .map(|position| (Some(position.x), Some(position.y)))
                .unwrap_or((system.map_x, system.map_y));

            let notes = self
                .repo
                .list_notes_for_system(system.id)?
                .into_iter()
                .map(|note| SystemNoteFile {
                    id: note.id,
                    body: note.body,
                    updated_at: note.updated_at,
                })
                .collect::<Vec<_>>();

            let database_columns = self
                .database_columns_by_system
                .get(&system.id)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|column| DatabaseColumnFile {
                    position: column.position,
                    column_name: column.column_name,
                    column_type: column.column_type,
                    constraints: column.constraints,
                })
                .collect::<Vec<_>>();

            let mut system_file = SystemFile {
                id: system.id,
                name: system.name.clone(),
                description: system.description.clone(),
                parent_id: system.parent_id,
                calculated_name: self.naming_path_for_system(system.id),
                map_x,
                map_y,
                line_color_override: system.line_color_override.clone(),
                naming_root: system.naming_root,
                naming_delimiter: system.naming_delimiter.clone(),
                system_type: system.system_type.clone(),
                route_methods: system.route_methods.clone(),
                tech_ids: self
                    .system_tech_ids_by_system
                    .get(&system.id)
                    .cloned()
                    .unwrap_or_default(),
                notes,
                database_columns,
            };
            self.apply_system_file_entity_schema(system, &mut system_file);

            let bytes = serde_json::to_vec_pretty(&system_file)?;
            Self::write_file_if_changed(&absolute_path, &bytes)?;
            systems_paths.push(relative_path);
        }

        if self.manage_system_json_hierarchy {
            Self::remove_stale_system_files(
                root,
                &systems_root,
                previous_system_paths.as_slice(),
                systems_paths.as_slice(),
            )?;
        }

        let mut interactions_paths = Vec::new();
        for interaction in &self.all_links {
            let file_name = format!(
                "{}__to__{}__{}.json",
                interaction.source_system_id, interaction.target_system_id, interaction.id
            );
            let relative_path = format!("interactions/{file_name}");
            let absolute_path = interactions_root.join(file_name);

            let interaction_file = InteractionFile {
                id: interaction.id,
                source_system_id: interaction.source_system_id,
                target_system_id: interaction.target_system_id,
                label: interaction.label.clone(),
                note: interaction.note.clone(),
                kind: interaction.kind.clone(),
                source_column_name: interaction.source_column_name.clone(),
                target_column_name: interaction.target_column_name.clone(),
            };

            let bytes = serde_json::to_vec_pretty(&interaction_file)?;
            Self::write_file_if_changed(&absolute_path, &bytes)?;
            interactions_paths.push(relative_path);
        }

        let project = ProjectFile {
            schema_version: 1,
            systems_paths,
            interactions_paths,
            tech_catalog: self
                .tech_catalog
                .iter()
                .cloned()
                .map(|tech| ProjectTechItem {
                    id: tech.id,
                    name: tech.name,
                    description: tech.description,
                    documentation_link: tech.documentation_link,
                    color: tech.color,
                    display_priority: tech.display_priority,
                })
                .collect(),
            zones: self
                .zones
                .iter()
                .cloned()
                .map(|zone| ProjectZone {
                    id: zone.id,
                    name: zone.name,
                    x: zone.x,
                    y: zone.y,
                    width: zone.width,
                    height: zone.height,
                    color: zone.color,
                    render_priority: zone.render_priority,
                    parent_zone_id: zone.parent_zone_id,
                    minimized: zone.minimized,
                    representative_system_id: zone.representative_system_id,
                })
                .collect(),
            zone_offsets: self
                .zone_offsets_by_system
                .iter()
                .map(|(system_id, (zone_id, offset))| ProjectZoneOffset {
                    zone_id: *zone_id,
                    system_id: *system_id,
                    offset_x: offset.x,
                    offset_y: offset.y,
                })
                .collect(),
            settings: ProjectSettings {
                autosave_enabled: self.project_autosave_enabled,
                manage_system_json_hierarchy: self.manage_system_json_hierarchy,
                has_git: self.git_repo_detected_for_path.unwrap_or(false),
                map_zoom: self.map_zoom,
                map_pan_x: self.map_pan.x,
                map_pan_y: self.map_pan.y,
                map_world_width: self.map_world_size.x,
                map_world_height: self.map_world_size.y,
                snap_to_grid: self.snap_to_grid,
            },
        };

        let project_path = root.join("Project.json");
        let project_bytes = serde_json::to_vec_pretty(&project)?;
        Self::write_file_if_changed(&project_path, &project_bytes)?;

        self.clear_system_change_flags();

        Ok(())
    }

    fn import_filesystem_project_from_root(
        &mut self,
        root: &Path,
        force_full_sync: bool,
    ) -> anyhow::Result<()> {
        let project_path = root.join("Project.json");
        let project_text = std::fs::read_to_string(&project_path)?;
        let project: ProjectFile = serde_json::from_str(project_text.as_str())?;

        let git_changed_paths = Self::git_changed_paths(root);
        let git_changed_system_ids = Self::git_changed_system_ids(root);
        let use_git_change_filter = !force_full_sync && git_changed_system_ids.is_some();
        let git_changed_system_ids = git_changed_system_ids.unwrap_or_default();
        let git_changed_paths = git_changed_paths.unwrap_or_default();

        let project_changed_non_system = force_full_sync
            || (use_git_change_filter
            && git_changed_paths.iter().any(|path| {
                path.eq_ignore_ascii_case("Project.json") || path.starts_with("interactions/")
            }));

        let path_by_system_id = project
            .systems_paths
            .iter()
            .filter_map(|relative_path| {
                Self::system_id_from_relative_path(relative_path.as_str())
                    .map(|system_id| (system_id, relative_path.clone()))
            })
            .collect::<HashMap<_, _>>();

        let existing_systems = self.repo.list_systems()?;
        let existing_by_id = existing_systems
            .into_iter()
            .map(|system| (system.id, system))
            .collect::<HashMap<_, _>>();
        let file_ids = path_by_system_id.keys().copied().collect::<HashSet<_>>();

        for existing_id in existing_by_id.keys().copied().collect::<Vec<_>>() {
            if !file_ids.contains(&existing_id) {
                self.repo.delete_system(existing_id)?;
            }
        }

        let existing_ids = existing_by_id.keys().copied().collect::<HashSet<_>>();
        let mut systems_to_load_ids = if use_git_change_filter {
            file_ids
                .difference(&existing_ids)
                .copied()
                .chain(git_changed_system_ids.iter().copied())
                .filter(|system_id| file_ids.contains(system_id))
                .collect::<HashSet<_>>()
        } else {
            file_ids.clone()
        };

        if project_changed_non_system {
            systems_to_load_ids = file_ids.clone();
        }

        let mut systems = Vec::new();
        for system_id in &systems_to_load_ids {
            let Some(relative_path) = path_by_system_id.get(system_id) else {
                continue;
            };
            let absolute_path = root.join(relative_path.replace('/', std::path::MAIN_SEPARATOR_STR));
            let file_text = std::fs::read_to_string(&absolute_path)?;
            let mut system: SystemFile = serde_json::from_str(file_text.as_str())?;
            self.normalize_loaded_system_file_for_entity(&mut system);
            systems.push(system);
        }

        let mut inserted_ids = HashSet::new();
        for system in &systems {
            if let Some(existing) = existing_by_id.get(&system.id) {
                let changed = existing.name != system.name
                    || existing.description != system.description
                    || existing.naming_root != system.naming_root
                    || existing.naming_delimiter != system.naming_delimiter
                    || existing.system_type != system.system_type
                    || existing.route_methods != system.route_methods
                    || existing.map_x != system.map_x
                    || existing.map_y != system.map_y
                    || existing.line_color_override != system.line_color_override;

                if changed {
                    self.repo.update_system_details(
                        system.id,
                        system.name.as_str(),
                        system.description.as_str(),
                        system.naming_root,
                        system.naming_delimiter.as_str(),
                        system.system_type.as_str(),
                        system.route_methods.as_deref(),
                    )?;
                    self.repo.update_system_position_optional(system.id, system.map_x, system.map_y)?;
                    self.repo.update_system_line_color_override(
                        system.id,
                        system.line_color_override.as_deref(),
                    )?;
                }
            } else {
                self.repo.insert_system_with_id(
                    system.id,
                    system.name.as_str(),
                    system.description.as_str(),
                    None,
                    system.map_x,
                    system.map_y,
                    system.line_color_override.as_deref(),
                    system.naming_root,
                    system.naming_delimiter.as_str(),
                    system.system_type.as_str(),
                    system.route_methods.as_deref(),
                )?;
                inserted_ids.insert(system.id);
            }
        }

        for system in &systems {
            let current_parent = existing_by_id.get(&system.id).and_then(|existing| existing.parent_id);
            if inserted_ids.contains(&system.id) || current_parent != system.parent_id {
                self.repo.update_system_parent(system.id, system.parent_id)?;
            }
        }

        if !use_git_change_filter || project_changed_non_system {
            if systems_to_load_ids.len() != file_ids.len() {
                systems.clear();
                for relative_path in &project.systems_paths {
                    let absolute_path =
                        root.join(relative_path.replace('/', std::path::MAIN_SEPARATOR_STR));
                    let file_text = std::fs::read_to_string(&absolute_path)?;
                    let mut system: SystemFile = serde_json::from_str(file_text.as_str())?;
                    self.normalize_loaded_system_file_for_entity(&mut system);
                    systems.push(system);
                }
            }

            self.repo.clear_non_system_catalog_data()?;

            for tech in &project.tech_catalog {
                self.repo.insert_tech_item_with_id(
                    tech.id,
                    tech.name.as_str(),
                    tech.description.as_deref(),
                    tech.documentation_link.as_deref(),
                    tech.color.as_deref(),
                    tech.display_priority,
                )?;
            }

            for system in &systems {
                for note in &system.notes {
                    self.repo.insert_note_with_id(
                        note.id,
                        system.id,
                        note.body.as_str(),
                        note.updated_at.as_str(),
                    )?;
                }

                let columns = system
                    .database_columns
                    .iter()
                    .map(|column| DatabaseColumnInput {
                        position: column.position,
                        column_name: column.column_name.clone(),
                        column_type: column.column_type.clone(),
                        constraints: column.constraints.clone(),
                    })
                    .collect::<Vec<_>>();

                self.repo.replace_database_columns_for_system(system.id, &columns)?;
                self.repo
                    .replace_system_tech_assignments(system.id, system.tech_ids.as_slice())?;
            }

            for relative_path in &project.interactions_paths {
                let absolute_path = root.join(relative_path.replace('/', std::path::MAIN_SEPARATOR_STR));
                let file_text = std::fs::read_to_string(&absolute_path)?;
                let interaction: InteractionFile = serde_json::from_str(file_text.as_str())?;
                self.repo.insert_link_with_id(
                    interaction.id,
                    interaction.source_system_id,
                    interaction.target_system_id,
                    interaction.label.as_str(),
                    interaction.note.as_str(),
                    interaction.kind.as_str(),
                    interaction.source_column_name.as_deref(),
                    interaction.target_column_name.as_deref(),
                )?;
            }

            for zone in &project.zones {
                self.repo.insert_zone_with_id(
                    zone.id,
                    zone.name.as_str(),
                    zone.x,
                    zone.y,
                    zone.width,
                    zone.height,
                    zone.color.as_deref(),
                    zone.render_priority,
                    None,
                    false,
                    zone.representative_system_id,
                )?;
            }

            for zone in &project.zones {
                self.repo.update_zone(
                    zone.id,
                    zone.name.as_str(),
                    zone.x,
                    zone.y,
                    zone.width,
                    zone.height,
                    zone.color.as_deref(),
                    zone.render_priority,
                    zone.parent_zone_id,
                    zone.minimized,
                    zone.representative_system_id,
                )?;
            }

            for offset in &project.zone_offsets {
                self.repo.upsert_zone_system_offset(
                    offset.zone_id,
                    offset.system_id,
                    offset.offset_x,
                    offset.offset_y,
                )?;
            }
        } else {
            for system in &systems {
                self.repo.delete_notes_for_system(system.id)?;
                for note in &system.notes {
                    self.repo.insert_note_with_id(
                        note.id,
                        system.id,
                        note.body.as_str(),
                        note.updated_at.as_str(),
                    )?;
                }

                let columns = system
                    .database_columns
                    .iter()
                    .map(|column| DatabaseColumnInput {
                        position: column.position,
                        column_name: column.column_name.clone(),
                        column_type: column.column_type.clone(),
                        constraints: column.constraints.clone(),
                    })
                    .collect::<Vec<_>>();
                self.repo.replace_database_columns_for_system(system.id, &columns)?;
                self.repo
                    .replace_system_tech_assignments(system.id, system.tech_ids.as_slice())?;
            }
        }

        self.refresh_systems()?;
        self.clear_selection();

        let safe_zoom = if project.settings.map_zoom.is_finite() {
            project.settings.map_zoom
        } else {
            1.0
        };
        let safe_pan_x = if project.settings.map_pan_x.is_finite() {
            project.settings.map_pan_x
        } else {
            0.0
        };
        let safe_pan_y = if project.settings.map_pan_y.is_finite() {
            project.settings.map_pan_y
        } else {
            0.0
        };
        let safe_world_w = if project.settings.map_world_width.is_finite() {
            project.settings.map_world_width
        } else {
            crate::app::MAP_WORLD_SIZE.x
        };
        let safe_world_h = if project.settings.map_world_height.is_finite() {
            project.settings.map_world_height
        } else {
            crate::app::MAP_WORLD_SIZE.y
        };

        self.map_zoom = safe_zoom.clamp(crate::app::MAP_MIN_ZOOM, crate::app::MAP_MAX_ZOOM);
        self.map_pan.x = safe_pan_x;
        self.map_pan.y = safe_pan_y;
        self.map_world_size.x = safe_world_w.clamp(
            crate::app::MAP_WORLD_MIN_SIZE.x,
            crate::app::MAP_WORLD_MAX_SIZE.x,
        );
        self.map_world_size.y = safe_world_h.clamp(
            crate::app::MAP_WORLD_MIN_SIZE.y,
            crate::app::MAP_WORLD_MAX_SIZE.y,
        );
        self.snap_to_grid = project.settings.snap_to_grid;
        self.project_autosave_enabled = project.settings.autosave_enabled;
        self.manage_system_json_hierarchy = project.settings.manage_system_json_hierarchy;
        self.git_repo_detect_path = root.to_string_lossy().to_string();
        self.git_repo_detected_for_path = Some(project.settings.has_git);
        self.project_last_autosave_at_secs = None;
        self.clear_system_change_flags();
        self.settings_dirty = true;

        Ok(())
    }

    pub(super) fn load_catalog_from_path_with_options(
        &mut self,
        path: &str,
        force_full_sync: bool,
    ) -> anyhow::Result<()> {
        let target = PathBuf::from(path);

        let is_sqlite_catalog = target
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("db"))
            .unwrap_or(false);

        if is_sqlite_catalog {
            return Err(anyhow::anyhow!(
                "Direct .db loading is disabled. Create a new project and use Migration source DB for one-time import."
            ));
        } else {
            self.import_filesystem_project_from_root(&target, force_full_sync)?;
        }

        Ok(())
    }

    pub(super) fn load_catalog_from_path(&mut self, path: &str) -> anyhow::Result<()> {
        self.load_catalog_from_path_with_options(path, false)
    }

    fn catalog_directory_name_from_project_name(name: &str) -> String {
        let mut slug = String::new();
        for character in name.chars() {
            if character.is_ascii_alphanumeric() {
                slug.push(character.to_ascii_lowercase());
            } else if character == ' ' || character == '-' || character == '_' {
                if !slug.ends_with('_') {
                    slug.push('_');
                }
            }
        }

        let slug = slug.trim_matches('_');
        if slug.is_empty() {
            "catalog_project".to_owned()
        } else {
            slug.to_owned()
        }
    }

    fn spawn_route_method_systems(
        &mut self,
        route_system_id: i64,
        methods: &HashSet<String>,
    ) -> anyhow::Result<()> {
        if methods.is_empty() {
            return Ok(());
        }

        let Some(route_position) = self.effective_map_position(route_system_id) else {
            return Ok(());
        };

        let mut existing_rects = self
            .systems
            .iter()
            .filter_map(|system| {
                self.effective_map_position(system.id)
                    .map(|position| eframe::egui::Rect::from_min_size(position, MAP_NODE_SIZE))
            })
            .collect::<Vec<_>>();

        let ordered_methods = Self::supported_http_methods()
            .iter()
            .filter(|method| methods.contains(**method))
            .map(|method| (*method).to_owned())
            .collect::<Vec<_>>();

        let mut created_any = false;
        for method in ordered_methods {
            let already_exists = self
                .systems
                .iter()
                .any(|system| system.parent_id == Some(route_system_id) && system.name.eq_ignore_ascii_case(method.as_str()));
            if already_exists {
                continue;
            }

            let child_id =
                self.repo
                    .create_system(method.as_str(), "", Some(route_system_id), "service", None)?;
            self.mark_system_as_new(child_id);

            let base_x = route_position.x + (MAP_GRID_SPACING * 2.0);
            let mut candidate = eframe::egui::Pos2::new(base_x, route_position.y + MAP_GRID_SPACING);

            loop {
                let candidate_rect = eframe::egui::Rect::from_min_size(candidate, MAP_NODE_SIZE);
                let blocked = existing_rects.iter().any(|rect| rect.intersects(candidate_rect));
                if !blocked {
                    break;
                }
                candidate.y += MAP_GRID_SPACING;
            }

            let clamped = self.clamp_node_position(eframe::egui::Rect::NOTHING, candidate, MAP_NODE_SIZE);
            self.map_positions.insert(child_id, clamped);
            self.persist_map_position(child_id, clamped);
            existing_rects.push(eframe::egui::Rect::from_min_size(clamped, MAP_NODE_SIZE));
            created_any = true;
        }

        if created_any {
            self.refresh_systems()?;
        }

        Ok(())
    }

    pub(super) fn create_zone_from_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) {
        let clamped_width = width.max(24.0);
        let clamped_height = height.max(24.0);
        let parent_zone_id =
            self.zone_parent_for_rect(x, y, clamped_width, clamped_height, None);

        // Create a temporary zone record to determine representative system
        let temp_zone = crate::models::ZoneRecord {
            id: 0, // Temporary ID
            name: String::new(),
            x,
            y,
            width: clamped_width,
            height: clamped_height,
            color: None,
            render_priority: self.selected_zone_render_priority,
            parent_zone_id,
            minimized: false,
            representative_system_id: None,
        };

        // Temporarily add to zones list to calculate system IDs
        self.zones.push(temp_zone);
        let zone_system_ids = self.zone_system_ids(0);
        self.zones.pop(); // Remove temporary zone

        // Determine default zone name based on representative system
        let zone_name = if let Some(system_ids) = zone_system_ids {
            // Find if there's a common ancestor (representative)
            let mut candidates: Vec<i64> = system_ids
                .iter()
                .copied()
                .filter(|candidate_id| {
                    system_ids
                        .iter()
                        .all(|system_id| self.system_is_ancestor_or_self(*candidate_id, *system_id))
                })
                .collect();
            candidates.sort_unstable();

            if let Some(&representative_id) = candidates.first() {
                self.system_name_by_id(representative_id)
            } else {
                format!("Zone {}", self.zones.len() + 1)
            }
        } else {
            format!("Zone {}", self.zones.len() + 1)
        };
        let color_value = Some(Self::color_to_setting_value(self.selected_zone_color));

        let result = self
            .repo
            .create_zone(
                zone_name.as_str(),
                x,
                y,
                clamped_width,
                clamped_height,
                color_value.as_deref(),
                self.selected_zone_render_priority,
                parent_zone_id,
                false,
                None,
            )
            .and_then(|new_zone_id| {
                self.refresh_systems()?;
                self.select_zone(new_zone_id);
                Ok(())
            });

        match result {
            Ok(_) => {
                self.mark_project_as_dirty();
                self.status_message = "Zone created".to_owned();
            }
            Err(error) => {
                self.status_message = format!("Failed to create zone: {error}");
            }
        }
    }

    pub(super) fn update_selected_zone_properties(&mut self) {
        let Some(zone_id) = self.selected_zone_id else {
            return;
        };

        let Some(existing) = self.zones.iter().find(|zone| zone.id == zone_id).cloned() else {
            return;
        };

        let name = self.selected_zone_name.trim();
        let name = if name.is_empty() { "Zone" } else { name };

        let color_value = Some(Self::color_to_setting_value(self.selected_zone_color));
        let result = self
            .repo
            .update_zone(
                zone_id,
                name,
                existing.x,
                existing.y,
                existing.width,
                existing.height,
                color_value.as_deref(),
                self.selected_zone_render_priority,
                self.selected_zone_parent_zone_id,
                self.selected_zone_minimized,
                self.selected_zone_representative_system_id,
            )
            .and_then(|_| self.refresh_systems());

        match result {
            Ok(_) => {
                self.mark_project_as_dirty();
            }
            Err(_) => {
                self.status_message = "Failed to update zone".to_owned();
            }
        }
    }

    pub(super) fn update_selected_zone_geometry(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) {
        let Some(zone_id) = self.selected_zone_id else {
            return;
        };

        let Some(existing) = self.zones.iter().find(|zone| zone.id == zone_id).cloned() else {
            return;
        };

        let clamped_width = width.max(24.0);
        let clamped_height = height.max(24.0);
        let clamped_x = x.max(0.0);
        let clamped_y = y.max(0.0);

        let max_x = (self.map_world_size.x - clamped_width).max(0.0);
        let max_y = (self.map_world_size.y - clamped_height).max(0.0);

        let name = self.selected_zone_name.trim();
        let name = if name.is_empty() { "Zone" } else { name };
        let color_value = Some(Self::color_to_setting_value(self.selected_zone_color));

        let result = self
            .repo
            .update_zone(
                zone_id,
                name,
                clamped_x.min(max_x),
                clamped_y.min(max_y),
                clamped_width,
                clamped_height,
                color_value.as_deref(),
                self.selected_zone_render_priority,
                existing.parent_zone_id,
                self.selected_zone_minimized,
                self.selected_zone_representative_system_id,
            )
            .and_then(|_| self.refresh_systems())
            .and_then(|_| {
                self.select_zone(zone_id);
                Ok(())
            });

        match result {
            Ok(_) => {
                self.mark_project_as_dirty();
            }
            Err(_) => {
                self.status_message = "Failed to update zone geometry".to_owned();
            }
        }
    }

    pub(super) fn toggle_zone_minimized(&mut self, zone_id: i64) {
        let Some(existing) = self.zones.iter().find(|zone| zone.id == zone_id).cloned() else {
            return;
        };

        let representative = self.zone_resolved_representative_system_id(zone_id);
        let next_minimized = !existing.minimized;
        let next_representative = representative.or(existing.representative_system_id);

        if !existing.minimized && next_representative.is_none() {
            self.status_message =
                "Cannot minimize zone: choose a representative ancestor system".to_owned();
            return;
        }

        let nested_child_ids = self.zone_nested_child_ids(zone_id);
        let nested_children = self
            .zones
            .iter()
            .filter(|zone| nested_child_ids.contains(&zone.id))
            .cloned()
            .collect::<Vec<_>>();
        let mut affected_representative_ids = HashSet::new();
        if let Some(representative_id) = self.zone_resolved_representative_system_id(zone_id) {
            affected_representative_ids.insert(representative_id);
        }
        for child in &nested_children {
            if let Some(representative_id) = self.zone_resolved_representative_system_id(child.id) {
                affected_representative_ids.insert(representative_id);
            }
        }

        let result = self
            .repo
            .update_zone(
                zone_id,
                existing.name.as_str(),
                existing.x,
                existing.y,
                existing.width,
                existing.height,
                existing.color.as_deref(),
                existing.render_priority,
                existing.parent_zone_id,
                next_minimized,
                next_representative,
            )
            .and_then(|_| {
                for child in &nested_children {
                    self.repo.update_zone(
                        child.id,
                        child.name.as_str(),
                        child.x,
                        child.y,
                        child.width,
                        child.height,
                        child.color.as_deref(),
                        child.render_priority,
                        child.parent_zone_id,
                        next_minimized,
                        child.representative_system_id,
                    )?;
                }
                Ok(())
            })
            .and_then(|_| self.refresh_systems())
            .and_then(|_| {
                if !next_minimized {
                    for representative_id in &affected_representative_ids {
                        self.auto_collapsed_zone_representative_ids
                            .remove(representative_id);

                        if self.collapsed_system_ids.contains(representative_id) {
                            self.on_disclosure_click(*representative_id);
                        }
                    }
                }
                self.select_zone(zone_id);
                Ok(())
            });

        match result {
            Ok(_) => {
                self.mark_project_as_dirty();
                self.status_message = if next_minimized {
                    "Zone minimized".to_owned()
                } else {
                    "Zone expanded".to_owned()
                };
            }
            Err(error) => {
                self.status_message = format!("Failed to toggle zone state: {error}");
            }
        }
    }

    pub(super) fn delete_selected_zone(&mut self) {
        let Some(zone_id) = self.selected_zone_id else {
            return;
        };

        let zone = self.zones.iter().find(|zone| zone.id == zone_id).cloned();

        if let Some(zone) = zone {
            let bound_system_ids = self
                .zone_offsets_by_system
                .iter()
                .filter_map(|(system_id, (bound_zone_id, offset))| {
                    if *bound_zone_id != zone_id {
                        return None;
                    }

                    let absolute = eframe::egui::Pos2::new(zone.x + offset.x, zone.y + offset.y);
                    Some((*system_id, absolute))
                })
                .collect::<Vec<_>>();

            for (system_id, absolute) in bound_system_ids {
                self.map_positions.insert(system_id, absolute);
                self.persist_map_position(system_id, absolute);
            }
        }

        let result = self.repo.delete_zone(zone_id).and_then(|_| self.refresh_systems());

        match result {
            Ok(_) => {
                self.mark_project_as_dirty();
                self.selected_zone_id = None;
                self.selected_zone_name.clear();
                self.selected_zone_render_priority = 1;
                self.selected_zone_parent_zone_id = None;
                self.selected_zone_minimized = false;
                self.selected_zone_representative_system_id = None;
                self.status_message = "Zone deleted".to_owned();
            }
            Err(error) => {
                self.status_message = format!("Failed to delete zone: {error}");
            }
        }
    }

    fn escape_clipboard_field(value: &str) -> String {
        value
            .replace('\\', "\\\\")
            .replace('\t', "\\t")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    }

    fn unescape_clipboard_field(value: &str) -> String {
        let mut result = String::new();
        let mut chars = value.chars();

        while let Some(ch) = chars.next() {
            if ch != '\\' {
                result.push(ch);
                continue;
            }

            match chars.next() {
                Some('t') => result.push('\t'),
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('\\') => result.push('\\'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        }

        result
    }

    pub(super) fn copied_systems_payload(&self) -> Option<String> {
        if self.copied_system_entries.is_empty() {
            return None;
        }

        let mut payload = String::from("systems-catalog-cards:v2\n");
        for entry in &self.copied_system_entries {
            let escaped_name = Self::escape_clipboard_field(entry.name.as_str());
            let escaped_description = Self::escape_clipboard_field(entry.description.as_str());
            let parent_index = entry
                .parent_index
                .map(|index| index.to_string())
                .unwrap_or_else(|| "-1".to_owned());
            payload.push_str(
                format!(
                    "{}\t{}\t{}\t{}\t{}\n",
                    escaped_name,
                    escaped_description,
                    parent_index,
                    entry.relative_x,
                    entry.relative_y
                )
                .as_str(),
            );
        }

        Some(payload)
    }

    pub(super) fn load_copied_systems_from_payload(&mut self, payload: &str) -> bool {
        let trimmed = payload.trim_end();
        if let Some(rest) = trimmed.strip_prefix("systems-catalog-cards:v2") {
            let entries = rest
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .filter_map(|line| {
                    let columns = line.split('\t').collect::<Vec<_>>();
                    if columns.len() < 5 {
                        return None;
                    }

                    let name = Self::unescape_clipboard_field(columns[0]);
                    if name.trim().is_empty() {
                        return None;
                    }

                    let description = Self::unescape_clipboard_field(columns[1]);
                    let parent_index = columns[2]
                        .parse::<isize>()
                        .ok()
                        .and_then(|index| if index >= 0 { Some(index as usize) } else { None });
                    let relative_x = columns[3].parse::<f32>().ok().unwrap_or(0.0);
                    let relative_y = columns[4].parse::<f32>().ok().unwrap_or(0.0);

                    Some(CopiedSystemEntry {
                        name,
                        description,
                        parent_index,
                        relative_x,
                        relative_y,
                    })
                })
                .collect::<Vec<_>>();

            if entries.is_empty() {
                return false;
            }

            self.copied_system_entries = entries;
            return true;
        }

        let Some(rest) = trimmed.strip_prefix("systems-catalog-cards:v1") else {
            return false;
        };

        let copied = rest
            .lines()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| name.to_owned())
            .collect::<Vec<_>>();

        if copied.is_empty() {
            return false;
        }

        self.copied_system_entries = copied
            .into_iter()
            .enumerate()
            .map(|(index, name)| CopiedSystemEntry {
                name,
                description: String::new(),
                parent_index: None,
                relative_x: index as f32 * 36.0,
                relative_y: 0.0,
            })
            .collect();
        true
    }

    fn create_system_with_generated_unique_name(
        &mut self,
        base_name: &str,
        description: &str,
        parent_id: Option<i64>,
    ) -> anyhow::Result<i64> {
        let base = base_name.trim();
        if base.is_empty() {
            return Err(anyhow::anyhow!("System name is required"));
        }

        if let Ok(id) = self
            .repo
            .create_system(base, description, parent_id, "service", None)
        {
            self.mark_system_as_new(id);
            return Ok(id);
        }

        for attempt in 1..=250 {
            let candidate = if attempt == 1 {
                format!("{base} Copy")
            } else {
                format!("{base} Copy {attempt}")
            };

            if let Ok(id) = self
                .repo
                .create_system(candidate.as_str(), description, parent_id, "service", None)
            {
                self.mark_system_as_new(id);
                return Ok(id);
            }
        }

        Err(anyhow::anyhow!(
            "Unable to create unique system name for '{}'",
            base
        ))
    }

    pub(super) fn copy_selected_map_systems(&mut self) {
        let mut source_ids = self.selected_map_system_ids.clone();
        if source_ids.is_empty() {
            if let Some(system_id) = self.selected_system_id {
                source_ids.insert(system_id);
            }
        }

        if source_ids.is_empty() {
            self.status_message =
                "Copy failed: select one or more cards (or a system) first".to_owned();
            return;
        }

        let mut copied_systems = self
            .systems
            .iter()
            .filter(|system| source_ids.contains(&system.id))
            .cloned()
            .collect::<Vec<_>>();

        copied_systems.sort_by_key(|system| system.id);

        if copied_systems.is_empty() {
            self.status_message = "Copy failed: no matching systems found".to_owned();
            return;
        }

        let id_to_index = copied_systems
            .iter()
            .enumerate()
            .map(|(index, system)| (system.id, index))
            .collect::<HashMap<_, _>>();

        let copied_positions = copied_systems
            .iter()
            .map(|system| {
                self.map_positions
                    .get(&system.id)
                    .copied()
                    .or_else(|| match (system.map_x, system.map_y) {
                        (Some(x), Some(y)) => Some(eframe::egui::Pos2::new(x, y)),
                        _ => None,
                    })
                    .unwrap_or(eframe::egui::Pos2::new(0.0, 0.0))
            })
            .collect::<Vec<_>>();

        let min_x = copied_positions
            .iter()
            .map(|position| position.x)
            .fold(f32::INFINITY, f32::min);
        let min_y = copied_positions
            .iter()
            .map(|position| position.y)
            .fold(f32::INFINITY, f32::min);

        self.copied_system_entries = copied_systems
            .iter()
            .enumerate()
            .map(|(index, system)| {
                let position = copied_positions[index];
                CopiedSystemEntry {
                    name: system.name.clone(),
                    description: system.description.clone(),
                    parent_index: system
                        .parent_id
                        .and_then(|parent_id| id_to_index.get(&parent_id).copied()),
                    relative_x: position.x - min_x,
                    relative_y: position.y - min_y,
                }
            })
            .collect();

        self.status_message = format!(
            "Copied {} card(s) to clipboard",
            self.copied_system_entries.len()
        );
    }

    pub(super) fn paste_copied_systems(&mut self) {
        if self.copied_system_entries.is_empty() {
            self.status_message =
                "Paste failed: clipboard is empty (use Ctrl+C first)".to_owned();
            return;
        }

        let parent_id = self.selected_system_id;
        let parent_tech_ids = parent_id
            .and_then(|id| self.system_tech_ids_by_system.get(&id).cloned())
            .unwrap_or_default();

        let entries_to_create = self.copied_system_entries.clone();
        let result = (|| -> anyhow::Result<usize> {
            let mut created_ids_by_entry = HashMap::<usize, i64>::new();
            let mut created_order = Vec::<usize>::new();
            let mut remaining = (0..entries_to_create.len()).collect::<HashSet<_>>();

            while !remaining.is_empty() {
                let mut progress = false;
                let pending = remaining.iter().copied().collect::<Vec<_>>();

                for entry_index in pending {
                    let entry = &entries_to_create[entry_index];

                    let effective_parent_id = match entry.parent_index {
                        Some(parent_entry_index) => {
                            let Some(created_parent_id) =
                                created_ids_by_entry.get(&parent_entry_index).copied()
                            else {
                                continue;
                            };

                            Some(created_parent_id)
                        }
                        None => parent_id,
                    };

                    let new_id = self.create_system_with_generated_unique_name(
                        entry.name.as_str(),
                        entry.description.as_str(),
                        effective_parent_id,
                    )?;

                    if effective_parent_id == parent_id {
                        for tech_id in &parent_tech_ids {
                            let _ = self.repo.add_tech_to_system(new_id, *tech_id);
                        }
                    }

                    created_ids_by_entry.insert(entry_index, new_id);
                    created_order.push(entry_index);
                    remaining.remove(&entry_index);
                    progress = true;
                }

                if !progress {
                    return Err(anyhow::anyhow!(
                        "Unable to resolve clipboard hierarchy for paste"
                    ));
                }
            }

            self.refresh_systems()?;

            let anchor_position = if parent_id.is_some() {
                self.find_next_free_child_spawn_position(parent_id)
                    .unwrap_or_else(|| self.find_next_free_root_spawn_position())
            } else {
                self.find_next_free_root_spawn_position()
            };

            let mut pasted_set = HashSet::new();
            for entry_index in created_order {
                let Some(created_id) = created_ids_by_entry.get(&entry_index).copied() else {
                    continue;
                };

                let entry = &entries_to_create[entry_index];
                let desired_position = eframe::egui::Pos2::new(
                    anchor_position.x + entry.relative_x,
                    anchor_position.y + entry.relative_y,
                );

                let node_size = self
                    .systems
                    .iter()
                    .find(|system| system.id == created_id)
                    .map(|_system| crate::app::MAP_NODE_SIZE)
                    .unwrap_or(crate::app::MAP_NODE_SIZE);

                let clamped = self.clamp_node_position(
                    eframe::egui::Rect::NOTHING,
                    desired_position,
                    node_size,
                );

                self.map_positions.insert(created_id, clamped);
                self.persist_map_position(created_id, clamped);
                pasted_set.insert(created_id);
            }

            if let Some(first_created_id) = created_ids_by_entry.values().next().copied() {
                self.selected_system_id = Some(first_created_id);
                let _ = self.load_selected_data(first_created_id);
            }

            self.selected_map_system_ids = pasted_set;

            Ok(created_ids_by_entry.len())
        })();

        match result {
            Ok(count) => {
                if let Some(parent_id) = parent_id {
                    self.status_message = format!(
                        "Pasted {} card(s) as children of '{}'",
                        count,
                        self.system_name_by_id(parent_id)
                    );
                } else {
                    self.status_message = format!("Pasted {} card(s) at root", count);
                }
            }
            Err(error) => {
                self.status_message = format!("Failed to paste cards: {error}");
            }
        }
    }

    pub(super) fn update_selected_system_details(&mut self) {
        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let edited_name = self.edited_system_name.trim();
        if edited_name.is_empty() {
            self.status_message = "System name is required".to_owned();
            return;
        }

        let naming_delimiter = self.selected_system_naming_delimiter.trim();
        let naming_delimiter = if naming_delimiter.is_empty() {
            "/"
        } else {
            naming_delimiter
        };

        let system_type = Self::normalize_system_type(self.selected_system_type.as_str());
        let route_methods_set_for_spawn = if system_type == "api" {
            let mut methods = self.selected_system_route_methods.clone();
            methods.extend(self.inferred_api_methods_from_children(system_id));
            methods
        } else {
            HashSet::new()
        };
        let route_methods = if system_type == "api" {
            Self::route_methods_storage_from_set(&route_methods_set_for_spawn)
        } else {
            None
        };

        let database_columns_for_save = if system_type == "database" {
            self.selected_database_columns
                .iter()
                .filter_map(|column| {
                    let column_name = column.column_name.trim();
                    let column_type = column.column_type.trim();
                    if column_name.is_empty() || column_type.is_empty() {
                        return None;
                    }

                    let constraints = column
                        .constraints
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned);

                    Some((column_name.to_owned(), column_type.to_owned(), constraints))
                })
                .enumerate()
                .map(|(position, (column_name, column_type, constraints))| DatabaseColumnInput {
                    position: position as i64,
                    column_name,
                    column_type,
                    constraints,
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let result = self
            .repo
            .update_system_details(
                system_id,
                edited_name,
                self.edited_system_description.trim(),
                self.selected_system_naming_root,
                naming_delimiter,
                system_type.as_str(),
                route_methods.as_deref(),
            )
            .and_then(|_| {
                if system_type == "database" {
                    self.repo
                        .replace_database_columns_for_system(system_id, &database_columns_for_save)
                } else {
                    Ok(())
                }
            })
            .and_then(|_| self.refresh_systems())
            .and_then(|_| self.load_selected_data(system_id))
            .and_then(|_| self.spawn_route_method_systems(system_id, &route_methods_set_for_spawn))
            .and_then(|_| self.load_selected_data(system_id));

        match result {
            Ok(_) => {
                self.mark_system_as_dirty(system_id);
                self.status_message = "System details updated".to_owned();
            }
            Err(error) => {
                self.status_message = format!("Failed to update system details: {error}")
            }
        }
    }

    pub(super) fn select_note_for_edit(&mut self, note_id: i64) {
        self.selected_note_id_for_edit = Some(note_id);
        self.note_text = self
            .selected_notes
            .iter()
            .find(|note| note.id == note_id)
            .map(|note| note.body.clone())
            .unwrap_or_default();
    }

    pub(super) fn create_note_for_selected_system(&mut self) {
        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        match self.repo.create_note(system_id, "") {
            Ok(_) => {
                if let Err(error) = self.load_selected_data(system_id) {
                    self.status_message = format!("Failed to load notes: {error}");
                    return;
                }

                if let Some(first_note) = self.selected_notes.first() {
                    self.select_note_for_edit(first_note.id);
                }

                self.status_message = "Note created".to_owned();
            }
            Err(error) => self.status_message = format!("Failed to create note: {error}"),
        }
    }

    pub(super) fn save_note(&mut self) {
        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let result = if let Some(note_id) = self.selected_note_id_for_edit {
            self.repo.update_note(note_id, self.note_text.trim())
        } else {
            self.repo.create_note(system_id, self.note_text.trim())
        }
        .and_then(|_| self.load_selected_data(system_id));

        match result {
            Ok(_) => self.status_message = "Note saved".to_owned(),
            Err(error) => self.status_message = format!("Failed to save note: {error}"),
        }
    }

    pub(super) fn delete_selected_note(&mut self) {
        let Some(note_id) = self.selected_note_id_for_edit else {
            self.status_message = "Select a note first".to_owned();
            return;
        };

        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let result = self
            .repo
            .delete_note(note_id)
            .and_then(|_| self.load_selected_data(system_id));

        match result {
            Ok(_) => self.status_message = "Note deleted".to_owned(),
            Err(error) => self.status_message = format!("Failed to delete note: {error}"),
        }
    }

    pub(super) fn export_catalog(&mut self) {
        if !self.has_unsaved_project_changes() {
            self.show_save_catalog_modal = false;
            self.status_message = "No unsaved project changes to save".to_owned();
            return;
        }

        let path = self.save_catalog_path.trim().to_owned();
        if path.is_empty() {
            self.status_message = "Save path is required".to_owned();
            return;
        }

        let destination_path = PathBuf::from(path.as_str());
        let export_result = self.export_catalog_to_filesystem_project(&destination_path);

        match export_result {
            Ok(_) => {
                self.push_recent_catalog_path(path.as_str());
                self.load_catalog_path = path.clone();
                self.current_catalog_path = path.clone();
                if self.current_catalog_name.trim().is_empty() {
                    self.current_catalog_name = Self::catalog_name_from_path(path.as_str());
                }
                self.settings_dirty = true;
                self.show_save_catalog_modal = false;
                self.status_message = format!("Filesystem project saved to {}", path);
            }
            Err(error) => self.status_message = format!("Failed to save project: {error}"),
        }
    }

    pub(super) fn import_catalog(&mut self) {
        let path = self.load_catalog_path.trim().to_owned();
        if path.is_empty() {
            self.status_message = "Load path is required".to_owned();
            return;
        }

        let result = self.load_catalog_from_path(path.as_str());

        match result {
            Ok(_) => {
                self.push_recent_catalog_path(path.as_str());
                self.save_catalog_path = path.clone();
                self.current_catalog_path = path.clone();
                self.current_catalog_name = Self::catalog_name_from_path(path.as_str());
                self.settings_dirty = true;
                self.show_load_catalog_modal = false;
                self.status_message = format!("Project loaded from {}", path);
            }
            Err(error) => self.status_message = format!("Failed to load project: {error}"),
        }
    }

    pub(super) fn switch_to_recent_catalog(&mut self, path: &str) {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            self.status_message = "Select a project path first".to_owned();
            return;
        }

        if trimmed == self.current_catalog_path.trim() {
            self.status_message = "Project is already open".to_owned();
            return;
        }

        let target = PathBuf::from(trimmed);
        if !target.exists() {
            self.recent_catalog_paths
                .retain(|recent_path| recent_path.trim() != trimmed);
            self.settings_dirty = true;
            self.status_message = format!("Project path no longer exists: {}", trimmed);
            return;
        }

        if target.is_dir() && !target.join("Project.json").exists() {
            self.recent_catalog_paths
                .retain(|recent_path| recent_path.trim() != trimmed);
            self.settings_dirty = true;
            self.status_message = format!(
                "Project folder is missing Project.json and was removed from recents: {}",
                trimmed
            );
            return;
        }

        match self.load_catalog_from_path(trimmed) {
            Ok(_) => {
                self.push_recent_catalog_path(trimmed);
                self.save_catalog_path = trimmed.to_owned();
                self.load_catalog_path = trimmed.to_owned();
                self.current_catalog_path = trimmed.to_owned();
                self.current_catalog_name = Self::catalog_name_from_path(trimmed);
                self.pending_catalog_switch_path = None;
                self.settings_dirty = true;
                self.status_message = format!("Switched to project '{}'", self.current_catalog_name);
            }
            Err(error) => {
                self.status_message = format!("Failed to switch project: {error}");
            }
        }
    }

    pub(super) fn create_named_catalog(&mut self) {
        let project_name = self.new_catalog_name.trim().to_owned();
        if project_name.is_empty() {
            self.status_message = "Project name is required".to_owned();
            return;
        }

        let target_directory = if self.new_catalog_directory.trim().is_empty() {
            self.recent_catalog_paths
                .first()
                .and_then(|path| Path::new(path).parent().map(Path::to_path_buf))
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."))
        } else {
            PathBuf::from(self.new_catalog_directory.trim())
        };

        if let Err(error) = std::fs::create_dir_all(&target_directory) {
            self.status_message = format!("Failed to create project directory: {error}");
            return;
        }

        let path = if self.new_catalog_directory.trim().is_empty() {
            let directory_name = Self::catalog_directory_name_from_project_name(project_name.as_str());
            target_directory.join(directory_name)
        } else {
            target_directory.clone()
        };
        let path_text = path.to_string_lossy().to_string();
        let migration_source_db_path = self.new_catalog_migration_db_path.trim().to_owned();

        let migration_requested = !migration_source_db_path.is_empty();
        if migration_requested {
            let migration_source = PathBuf::from(migration_source_db_path.as_str());
            let looks_like_db = migration_source
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| {
                    ext.eq_ignore_ascii_case("db")
                        || ext.eq_ignore_ascii_case("sqlite")
                        || ext.eq_ignore_ascii_case("sqlite3")
                })
                .unwrap_or(false);
            if !looks_like_db {
                self.status_message = "Migration source must be a .db/.sqlite/.sqlite3 file"
                    .to_owned();
                return;
            }

            if !migration_source.exists() {
                self.status_message = "Migration source DB path does not exist".to_owned();
                return;
            }
        }

        let result = (|| -> anyhow::Result<()> {
            if migration_requested {
                self.repo.import_catalog_from_path(migration_source_db_path.as_str())?;
                self.refresh_systems()?;
            } else {
                self.repo.clear_catalog_data()?;
                self.refresh_systems()?;
            }

            self.export_catalog_to_filesystem_project(&path)?;
            Ok(())
        })();

        match result {
            Ok(_) => {
                self.clear_selection();
                self.current_catalog_name = project_name;
                self.current_catalog_path = path_text.clone();
                self.new_catalog_directory = target_directory.to_string_lossy().to_string();
                self.new_catalog_migration_db_path.clear();
                self.save_catalog_path = path_text.clone();
                self.load_catalog_path = path_text.clone();
                self.push_recent_catalog_path(path_text.as_str());
                self.show_add_system_modal = false;
                self.show_add_tech_modal = false;
                self.show_save_catalog_modal = false;
                self.show_load_catalog_modal = false;
                self.show_new_catalog_confirm_modal = false;
                self.settings_dirty = true;
                self.status_message = if migration_requested {
                    format!(
                        "Created project '{}' from DB migration",
                        self.current_catalog_name
                    )
                } else {
                    format!("Created project '{}'", self.current_catalog_name)
                };
            }
            Err(error) => {
                self.status_message = format!("Failed to create new project: {error}");
            }
        }
    }

    pub(super) fn update_selected_system_line_color_override(&mut self) {
        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let override_value = self
            .selected_system_line_color_override
            .map(Self::color_to_setting_value);

        let result = self
            .repo
            .update_system_line_color_override(system_id, override_value.as_deref())
            .and_then(|_| self.refresh_systems())
            .and_then(|_| self.load_selected_data(system_id));

        match result {
            Ok(_) => {
                self.mark_system_as_dirty(system_id);
                self.status_message = "System line color override updated".to_owned();
            }
            Err(error) => {
                self.status_message = format!("Failed to update line color override: {error}")
            }
        }
    }

    pub(super) fn update_selected_link(&mut self) {
        let Some(link_id) = self.selected_link_id_for_edit else {
            self.status_message = "Select an interaction first".to_owned();
            return;
        };

        let Some(existing_link) = self.selected_links.iter().find(|link| link.id == link_id) else {
            self.status_message = "Selected interaction is no longer available".to_owned();
            return;
        };

        let label = self.edited_link_label.trim();
        let note = self.edited_link_note.trim();
        let kind = Self::interaction_kind_to_setting_value(self.edited_link_kind);
        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let selected_target_system_id = self
            .selected_interaction_transfer_target_id
            .or(self.new_link_target_id)
            .unwrap_or(existing_link.target_system_id);

        let mut next_source_system_id = existing_link.source_system_id;
        let mut next_target_system_id = existing_link.target_system_id;

        if existing_link.source_system_id == system_id {
            next_source_system_id = selected_target_system_id;
        }
        if existing_link.target_system_id == system_id {
            next_target_system_id = selected_target_system_id;
        }

        // Fallback: if the selected system is not one endpoint (unexpected), treat this as
        // direct target reassignment for the edited interaction.
        if existing_link.source_system_id != system_id
            && existing_link.target_system_id != system_id
            && selected_target_system_id != existing_link.target_system_id
        {
            next_target_system_id = selected_target_system_id;
        }

        let endpoints_changed = next_source_system_id != existing_link.source_system_id
            || next_target_system_id != existing_link.target_system_id;

        if endpoints_changed && next_source_system_id == next_target_system_id {
            self.status_message = "Interaction cannot point to the same system on both ends"
                .to_owned();
            return;
        }

        let source_column_name = if endpoints_changed {
            None
        } else {
            self.edited_link_source_column_name.clone()
        };
        let target_column_name = if endpoints_changed {
            None
        } else {
            self.edited_link_target_column_name.clone()
        };

        let duplicate_target = self
            .all_links
            .iter()
            .find(|link| {
                link.id != link_id
                    && link.source_system_id == next_source_system_id
                    && link.target_system_id == next_target_system_id
            })
            .map(|link| link.id);

        if endpoints_changed {
            if let Some(existing_id) = duplicate_target {
                self.status_message = format!(
                    "Interaction already exists for this source/target pair (#{existing_id})"
                );
                return;
            }
        }

        let result = (|| -> anyhow::Result<()> {
            if endpoints_changed {
                self.repo.update_link_endpoints(
                    link_id,
                    next_source_system_id,
                    next_target_system_id,
                )?;
                self.edited_link_source_column_name = None;
                self.edited_link_target_column_name = None;
            }

            self.repo.update_link_details(
                link_id,
                label,
                note,
                kind,
                source_column_name.as_deref(),
                target_column_name.as_deref(),
            )?;

            Ok(())
        })()
            .and_then(|_| self.refresh_systems())
            .and_then(|_| self.load_selected_data(system_id));

        match result {
            Ok(_) => {
                self.mark_project_as_dirty();
                self.status_message = if endpoints_changed {
                    "Interaction updated and endpoint moved".to_owned()
                } else {
                    "Interaction updated".to_owned()
                };
            }
            Err(error) => self.status_message = format!("Failed to update interaction: {error}"),
        }
    }

    fn merge_interaction_kind_values(existing_kind: &str, incoming_kind: &str) -> String {
        let existing = Self::interaction_kind_from_setting_value(existing_kind);
        let incoming = Self::interaction_kind_from_setting_value(incoming_kind);

        let merged = match (existing, incoming) {
            (InteractionKind::Bidirectional, _) | (_, InteractionKind::Bidirectional) => {
                InteractionKind::Bidirectional
            }
            (InteractionKind::Pull, InteractionKind::Push)
            | (InteractionKind::Push, InteractionKind::Pull) => InteractionKind::Bidirectional,
            (_, next) => next,
        };

        Self::interaction_kind_to_setting_value(merged).to_owned()
    }

    fn merge_interaction_text(existing: &str, incoming: &str, delimiter: &str) -> String {
        let existing_trimmed = existing.trim();
        let incoming_trimmed = incoming.trim();

        if existing_trimmed.is_empty() {
            return incoming_trimmed.to_owned();
        }
        if incoming_trimmed.is_empty() || existing_trimmed.eq_ignore_ascii_case(incoming_trimmed) {
            return existing_trimmed.to_owned();
        }

        format!("{}{}{}", existing_trimmed, delimiter, incoming_trimmed)
    }

    pub(super) fn transfer_selected_system_interactions(&mut self) {
        let Some(source_system_id) = self.selected_system_id else {
            self.status_message = "Select a source system first".to_owned();
            return;
        };

        let Some(target_system_id) = self.selected_interaction_transfer_target_id else {
            self.status_message = "Select a target system first".to_owned();
            return;
        };

        if source_system_id == target_system_id {
            self.status_message = "Source and target systems must be different".to_owned();
            return;
        }

        let result = (|| -> anyhow::Result<(usize, usize, usize)> {
            let mut links = self.repo.list_links()?;
            let mut moved_count = 0usize;
            let mut merged_count = 0usize;
            let mut dropped_self_count = 0usize;

            let transfer_candidates = links
                .iter()
                .filter(|link| {
                    link.source_system_id == source_system_id || link.target_system_id == source_system_id
                })
                .cloned()
                .collect::<Vec<_>>();

            for link in transfer_candidates {
                let next_source = if link.source_system_id == source_system_id {
                    target_system_id
                } else {
                    link.source_system_id
                };
                let next_target = if link.target_system_id == source_system_id {
                    target_system_id
                } else {
                    link.target_system_id
                };

                if next_source == next_target {
                    self.repo.delete_link(link.id)?;
                    links.retain(|existing| existing.id != link.id);
                    dropped_self_count += 1;
                    continue;
                }

                let duplicate = links
                    .iter()
                    .find(|existing| {
                        existing.id != link.id
                            && existing.source_system_id == next_source
                            && existing.target_system_id == next_target
                    })
                    .cloned();

                if let Some(existing) = duplicate {
                    let merged_kind = Self::merge_interaction_kind_values(
                        existing.kind.as_str(),
                        link.kind.as_str(),
                    );
                    let merged_label =
                        Self::merge_interaction_text(existing.label.as_str(), link.label.as_str(), " | ");
                    let merged_note =
                        Self::merge_interaction_text(existing.note.as_str(), link.note.as_str(), "\n\n");

                    self.repo.update_link_details(
                        existing.id,
                        merged_label.as_str(),
                        merged_note.as_str(),
                        merged_kind.as_str(),
                        existing.source_column_name.as_deref(),
                        existing.target_column_name.as_deref(),
                    )?;
                    self.repo.delete_link(link.id)?;

                    links.retain(|item| item.id != link.id);
                    if let Some(entry) = links.iter_mut().find(|item| item.id == existing.id) {
                        entry.label = merged_label;
                        entry.note = merged_note;
                        entry.kind = merged_kind;
                    }

                    merged_count += 1;
                } else {
                    self.repo
                        .update_link_endpoints(link.id, next_source, next_target)?;
                    if let Some(entry) = links.iter_mut().find(|item| item.id == link.id) {
                        entry.source_system_id = next_source;
                        entry.target_system_id = next_target;
                    }
                    moved_count += 1;
                }
            }

            self.refresh_systems()?;
            if let Some(selected_id) = self.selected_system_id {
                self.load_selected_data(selected_id)?;
            }

            Ok((moved_count, merged_count, dropped_self_count))
        })();

        match result {
            Ok((moved_count, merged_count, dropped_self_count)) => {
                if moved_count > 0 || merged_count > 0 || dropped_self_count > 0 {
                    self.mark_project_as_dirty();
                }
                self.status_message = format!(
                    "Transferred interactions: {} moved, {} merged, {} removed self-links",
                    moved_count, merged_count, dropped_self_count
                );
            }
            Err(error) => {
                self.status_message = format!("Failed to transfer interactions: {error}");
            }
        }
    }

    pub(super) fn delete_selected_link(&mut self) {
        let Some(link_id) = self.selected_link_id_for_edit else {
            self.status_message = "Select an interaction first".to_owned();
            return;
        };

        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let result = self
            .repo
            .delete_link(link_id)
            .and_then(|_| self.refresh_systems())
            .and_then(|_| self.load_selected_data(system_id));

        match result {
            Ok(_) => {
                self.mark_project_as_dirty();
                self.status_message = "Interaction removed".to_owned();
            }
            Err(error) => self.status_message = format!("Failed to remove interaction: {error}"),
        }
    }

    pub(super) fn update_selected_catalog_tech(&mut self) {
        let Some(tech_id) = self.selected_catalog_tech_id_for_edit else {
            self.status_message = "Select a technology first".to_owned();
            return;
        };

        let name = self.edited_tech_name.trim();
        if name.is_empty() {
            self.status_message = "Technology name is required".to_owned();
            return;
        }

        let description = Self::text_to_option(&self.edited_tech_description);
        let documentation_link = Self::text_to_option(&self.edited_tech_documentation_link);
        let color = self.edited_tech_color.map(Self::color_to_setting_value);
        let display_priority = self.edited_tech_display_priority;

        let result = self
            .repo
            .update_tech_item(
                tech_id,
                name,
                description,
                documentation_link,
                color.as_deref(),
                display_priority,
            )
            .and_then(|_| {
                self.refresh_systems().and_then(|_| {
                    if let Some(system_id) = self.selected_system_id {
                        self.load_selected_data(system_id)?;
                    }
                    Ok(())
                })
            });

        match result {
            Ok(_) => self.status_message = "Technology updated".to_owned(),
            Err(error) => self.status_message = format!("Failed to update technology: {error}"),
        }
    }

    pub(super) fn delete_selected_catalog_tech(&mut self) {
        let Some(tech_id) = self.selected_catalog_tech_id_for_edit else {
            self.status_message = "Select a technology first".to_owned();
            return;
        };

        let result = self.repo.delete_tech_item(tech_id).and_then(|_| {
            self.refresh_systems().and_then(|_| {
                if let Some(system_id) = self.selected_system_id {
                    self.load_selected_data(system_id)?;
                }
                Ok(())
            })
        });

        match result {
            Ok(_) => self.status_message = "Technology removed from catalog".to_owned(),
            Err(error) => self.status_message = format!("Failed to remove technology: {error}"),
        }
    }

    pub(super) fn remove_tech_from_selected_system(&mut self, tech_id: i64) {
        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let result = self
            .repo
            .remove_tech_from_system(system_id, tech_id)
            .and_then(|_| self.refresh_systems())
            .and_then(|_| self.load_selected_data(system_id));

        match result {
            Ok(_) => {
                self.mark_system_as_dirty(system_id);
                self.status_message = "Technology removed from system".to_owned();
            }
            Err(error) => {
                self.status_message = format!("Failed to remove technology from system: {error}")
            }
        }
    }

    pub(super) fn update_selected_system_parent(&mut self) {
        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let current_parent_id = self
            .systems
            .iter()
            .find(|system| system.id == system_id)
            .and_then(|system| system.parent_id);

        if current_parent_id == self.selected_system_parent_id {
            self.status_message = "Parent unchanged".to_owned();
            return;
        }

        if let Some(parent_id) = self.selected_system_parent_id {
            if self.would_create_parent_cycle(system_id, parent_id) {
                self.status_message = "Invalid parent: this would create a cycle".to_owned();
                return;
            }
        }

        let result = (|| -> anyhow::Result<()> {
            self.repo
                .update_system_parent(system_id, self.selected_system_parent_id)?;
            self.mark_system_as_dirty(system_id);
            self.refresh_systems()?;
            self.load_selected_data(system_id)?;
            Ok(())
        })();

        match result {
            Ok(_) => {
                self.status_message = "Parent updated".to_owned();
            }
            Err(error) => self.status_message = format!("Failed to update parent: {error}"),
        }
    }

    pub(super) fn delete_selected_system(&mut self) {
        let mut target_ids = self.selected_map_system_ids.clone();
        if let Some(system_id) = self.selected_system_id {
            target_ids.insert(system_id);
        }

        if target_ids.is_empty() {
            self.status_message = "Select one or more systems first".to_owned();
            return;
        }

        let mut ordered_ids = target_ids.iter().copied().collect::<Vec<_>>();
        ordered_ids.sort_unstable();

        let deleted_count = ordered_ids.len();
        let deleted_name = if deleted_count == 1 {
            ordered_ids
                .first()
                .copied()
                .map(|system_id| self.system_name_by_id(system_id))
        } else {
            None
        };

        let result = (|| -> anyhow::Result<()> {
            for system_id in &ordered_ids {
                self.repo.delete_system(*system_id)?;
            }
            self.refresh_systems()?;
            Ok(())
        })();

        match result {
            Ok(_) => {
                self.map_link_click_source = None;
                self.map_link_drag_from = None;
                self.map_interaction_drag_from = None;
                self.selected_map_system_ids.clear();

                if let Some(name) = deleted_name {
                    self.status_message = format!("Deleted system: {name}");
                } else {
                    self.status_message = format!("Deleted {deleted_count} systems");
                }
            }
            Err(error) => self.status_message = format!("Failed to delete systems: {error}"),
        }
    }

    pub(super) fn create_tech_item(&mut self) {
        let name = self.new_tech_name.trim();
        if name.is_empty() {
            self.status_message = "Technology name is required".to_owned();
            return;
        }

        let description = Self::text_to_option(&self.new_tech_description);
        let documentation_link = Self::text_to_option(&self.new_tech_documentation_link);
        let color = self.new_tech_color.map(Self::color_to_setting_value);
        let display_priority = self.new_tech_display_priority;

        let result = self
            .repo
            .create_tech_item(
                name,
                description,
                documentation_link,
                color.as_deref(),
                display_priority,
            )
            .and_then(|_| self.refresh_systems());

        match result {
            Ok(_) => {
                self.new_tech_name.clear();
                self.new_tech_description.clear();
                self.new_tech_documentation_link.clear();
                self.new_tech_color = None;
                self.new_tech_display_priority = 0;
                self.status_message = "Technology saved to catalog".to_owned();
            }
            Err(error) => {
                self.status_message = format!("Failed to save technology: {error}");
            }
        }
    }

    pub(super) fn add_selected_tech_to_system(&mut self) {
        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let Some(tech_id) = self.selected_tech_id_for_assignment else {
            self.status_message = "Select a technology to assign".to_owned();
            return;
        };

        let result = self
            .repo
            .add_tech_to_system(system_id, tech_id)
            .and_then(|_| self.refresh_systems())
            .and_then(|_| self.load_selected_data(system_id));

        match result {
            Ok(_) => {
                self.mark_system_as_dirty(system_id);
                self.selected_tech_id_for_assignment = None;
                self.status_message = "Technology assigned to system".to_owned();
            }
            Err(error) => {
                self.status_message = format!("Failed to assign technology: {error}");
            }
        }
    }

    pub(super) fn create_system(&mut self) {
        let name = self.new_system_name.trim();
        if name.is_empty() {
            self.status_message = "System name is required".to_owned();
            return;
        }

        let parent_id = self.new_system_parent_id;
        let description = self.new_system_description.trim();
        let system_type = Self::normalize_system_type(self.new_system_type.as_str());
        let route_methods = if system_type == "api" {
            Self::route_methods_storage_from_set(&self.new_system_route_methods)
        } else {
            None
        };
        let route_methods_set_for_spawn = if system_type == "api" {
            self.new_system_route_methods.clone()
        } else {
            HashSet::new()
        };
        let assigned_tech_ids = self
            .new_system_assigned_tech_ids
            .iter()
            .copied()
            .collect::<Vec<_>>();

        let result = self
            .repo
            .create_system(
                name,
                description,
                parent_id,
                system_type.as_str(),
                route_methods.as_deref(),
            )
            .and_then(|new_id| {
                self.mark_system_as_new(new_id);
                for tech_id in &assigned_tech_ids {
                    self.repo.add_tech_to_system(new_id, *tech_id)?;
                }

                self.refresh_systems()?;

                if let Some((spawn_position, zone_binding)) =
                    self.spawn_position_for_new_system(parent_id)
                {
                    self.map_positions.insert(new_id, spawn_position);
                    self.persist_map_position(new_id, spawn_position);

                    if let Some((zone_id, offset)) = zone_binding {
                        self.assign_system_to_zone_offset(new_id, zone_id, offset);
                        self.persist_system_zone_offset(new_id, zone_id, offset);
                    }
                }

                self.spawn_route_method_systems(new_id, &route_methods_set_for_spawn)?;

                Ok(())
            });

        match result {
            Ok(_) => {
                self.new_system_name.clear();
                self.new_system_description.clear();
                self.new_system_parent_id = None;
                self.new_system_type = "service".to_owned();
                self.new_system_route_methods.clear();
                self.new_system_tech_id_for_assignment = None;
                self.new_system_assigned_tech_ids.clear();
                self.show_add_system_modal = false;
                self.show_add_tech_modal = false;
                self.status_message = "System created".to_owned();
            }
            Err(error) => {
                self.status_message = format!("Failed to create system: {error}");
            }
        }
    }

    pub(super) fn create_systems_bulk_from_list(&mut self) {
        let names = self
            .bulk_new_system_names
            .split(|character| character == ',' || character == '\n' || character == '\r')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| name.to_owned())
            .collect::<Vec<_>>();

        if names.is_empty() {
            self.status_message = "Enter at least one system name".to_owned();
            return;
        }

        let parent_id = self.bulk_new_system_parent_id;
        let parent_tech_ids = parent_id
            .and_then(|id| self.system_tech_ids_by_system.get(&id).cloned())
            .unwrap_or_default();

        let result = (|| -> anyhow::Result<()> {
            let mut created_ids = Vec::new();
            for name in &names {
                let new_id = self
                    .repo
                    .create_system(name, "", parent_id, "service", None)?;
                self.mark_system_as_new(new_id);

                for tech_id in &parent_tech_ids {
                    let _ = self.repo.add_tech_to_system(new_id, *tech_id);
                }

                created_ids.push(new_id);
            }

            self.refresh_systems()?;

            for created_id in created_ids {
                if let Some((position, zone_binding)) = self.spawn_position_for_new_system(parent_id)
                {
                    self.map_positions.insert(created_id, position);
                    self.persist_map_position(created_id, position);

                    if let Some((zone_id, offset)) = zone_binding {
                        self.assign_system_to_zone_offset(created_id, zone_id, offset);
                        self.persist_system_zone_offset(created_id, zone_id, offset);
                    }
                }
            }

            Ok(())
        })();

        match result {
            Ok(_) => {
                self.bulk_new_system_names.clear();
                self.show_bulk_add_systems_modal = false;
                self.status_message = format!("Created {} systems", names.len());
            }
            Err(error) => {
                self.status_message = format!("Failed to bulk-create systems: {error}");
            }
        }
    }

    pub(super) fn fast_add_selected_catalog_tech_to_system(&mut self, system_id: i64) {
        let Some(tech_id) = self.selected_catalog_tech_id_for_edit else {
            self.status_message = "Select a technology in Tech Catalog first".to_owned();
            return;
        };

        let already_assigned = self
            .system_tech_ids_by_system
            .get(&system_id)
            .map(|tech_ids| tech_ids.contains(&tech_id))
            .unwrap_or(false);

        if already_assigned {
            return;
        }

        let tech_name = self.tech_name_by_id(tech_id);
        let system_name = self.system_name_by_id(system_id);

        let result = self
            .repo
            .add_tech_to_system(system_id, tech_id)
            .and_then(|_| self.refresh_systems())
            .and_then(|_| {
                if self.selected_system_id == Some(system_id) {
                    self.load_selected_data(system_id)?;
                }
                Ok(())
            });

        match result {
            Ok(_) => {
                self.mark_system_as_dirty(system_id);
                self.status_message =
                    format!("Added technology '{tech_name}' to '{system_name}'");
            }
            Err(error) => {
                self.status_message = format!("Failed to fast-assign technology: {error}");
            }
        }
    }

    pub(super) fn fast_add_selected_catalog_tech_to_subtree(&mut self, parent_system_id: i64) {
        let Some(tech_id) = self.selected_catalog_tech_id_for_edit else {
            return;
        };

        let subtree_ids = self.system_and_descendant_ids(parent_system_id);
        if subtree_ids.is_empty() {
            return;
        }

        let mut added_count = 0usize;
        let result = (|| -> anyhow::Result<()> {
            for system_id in &subtree_ids {
                let already_assigned = self
                    .system_tech_ids_by_system
                    .get(system_id)
                    .map(|tech_ids| tech_ids.contains(&tech_id))
                    .unwrap_or(false);

                if already_assigned {
                    continue;
                }

                self.repo.add_tech_to_system(*system_id, tech_id)?;
                added_count += 1;
            }

            self.refresh_systems()?;
            if let Some(selected_system_id) = self.selected_system_id {
                self.load_selected_data(selected_system_id)?;
            }

            Ok(())
        })();

        match result {
            Ok(_) => {
                for system_id in &subtree_ids {
                    self.mark_system_as_dirty(*system_id);
                }
                if added_count > 0 {
                    self.status_message = format!(
                        "Added '{}' to {} system(s) in subtree",
                        self.tech_name_by_id(tech_id),
                        added_count
                    );
                }
            }
            Err(error) => {
                self.status_message = format!("Failed to apply tech to subtree: {error}");
            }
        }
    }
    pub(super) fn create_link(&mut self) {
        let Some(source_id) = self.selected_system_id else {
            self.status_message = "Select a system to create an interaction".to_owned();
            return;
        };

        let Some(target_id) = self.new_link_target_id else {
            self.status_message = "Select a target system".to_owned();
            return;
        };

        let label = self.new_link_label.trim().to_string();
        self.create_link_between_kind(source_id, target_id, &label, InteractionKind::Standard);
    }

    pub(super) fn create_link_between(&mut self, source_id: i64, target_id: i64, label: &str) {
        self.create_link_between_kind(source_id, target_id, label, InteractionKind::Standard);
    }

    pub(super) fn create_link_between_kind(
        &mut self,
        source_id: i64,
        target_id: i64,
        label: &str,
        kind: InteractionKind,
    ) {
        if source_id == target_id {
            self.status_message = "A system cannot link to itself".to_owned();
            return;
        }

        let result = self
            .repo
            .create_link(
                source_id,
                target_id,
                label,
                Self::interaction_kind_to_setting_value(kind),
                None,
                None,
            )
            .and_then(|_| self.refresh_systems())
            .and_then(|_| self.load_selected_data(source_id));

        match result {
            Ok(_) => {
                self.mark_project_as_dirty();
                self.new_link_label.clear();
                self.new_link_target_id = None;
                self.selected_system_id = Some(source_id);
                self.status_message = "Interaction saved".to_owned();
            }
            Err(error) => {
                self.status_message = format!("Failed to create interaction: {error}");
            }
        }
    }

    pub(super) fn assign_parent_between(&mut self, child_id: i64, parent_id: i64) {
        if child_id == parent_id {
            self.status_message = "A system cannot be its own parent".to_owned();
            return;
        }

        if self.would_create_parent_cycle(child_id, parent_id) {
            self.status_message = "Invalid parent: this would create a cycle".to_owned();
            return;
        }

        let current_parent_id = self
            .systems
            .iter()
            .find(|system| system.id == child_id)
            .and_then(|system| system.parent_id);
        if current_parent_id == Some(parent_id) {
            self.status_message = "Parent unchanged".to_owned();
            return;
        }

        let child_name = self.system_name_by_id(child_id);
        let parent_name = self.system_name_by_id(parent_id);

        let result = (|| -> anyhow::Result<()> {
            self.repo.update_system_parent(child_id, Some(parent_id))?;
            self.mark_system_as_dirty(child_id);
            self.refresh_systems()?;
            if self.selected_system_id == Some(child_id) {
                self.load_selected_data(child_id)?;
            }
            Ok(())
        })();

        match result {
            Ok(_) => {
                self.status_message =
                    format!("Assigned parent: '{parent_name}' <- '{child_name}'");
            }
            Err(error) => {
                self.status_message = format!("Failed to assign parent: {error}");
            }
        }
    }
}
