use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::project_store::{
    load_filesystem_project_manifest, InteractionFile, LightweightEntityRef,
    LightweightProjectFile, LoadedFilesystemProjectManifest, ProjectFile, SystemFile,
    LIGHTWEIGHT_PROJECT_FILE_NAME, LIGHTWEIGHT_PROJECT_SCHEMA_VERSION,
};

/// File-native project store with atomic writes and lazy entity loading.
///
/// Manages a project directory containing:
/// - `project.json` - Lightweight manifest with entity references and positions
/// - `systems/*.json` - Individual entity files (loaded on-demand)
/// - `interactions/*.json` - Interaction/relationship files
#[derive(Clone)]
#[allow(dead_code)]
pub struct FileStore {
    root: PathBuf,
    manifest: LightweightProjectFile,
    loaded_entities: HashMap<String, SystemFile>,
    loaded_interactions: HashMap<String, InteractionFile>,
    dirty_entities: HashSet<String>,
    dirty_interactions: HashSet<String>,
    manifest_dirty: bool,
}

#[allow(dead_code)]
impl FileStore {
    fn slugify_file_stem(value: &str) -> String {
        let mut slug = value
            .trim()
            .to_ascii_lowercase()
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character
                } else {
                    '_'
                }
            })
            .collect::<String>();

        while slug.contains("__") {
            slug = slug.replace("__", "_");
        }

        let slug = slug.trim_matches('_');
        if slug.is_empty() {
            "system".to_owned()
        } else {
            slug.to_owned()
        }
    }

    fn normalized_optional_text(value: Option<&str>) -> Option<String> {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    }

    fn next_system_id(&mut self) -> Result<i64> {
        let mut max_id = 0i64;
        for entity_ref in self.manifest.entities.clone() {
            if let Some(system_id) = entity_ref.system_id {
                max_id = max_id.max(system_id);
            } else {
                let entity = self.load_entity(entity_ref.file_path.as_str())?;
                max_id = max_id.max(entity.id);
            }
        }

        Ok(max_id + 1)
    }

    fn entity_file_path_for_system_id(&mut self, system_id: i64) -> Result<String> {
        for entity_ref in self.manifest.entities.clone() {
            let file_path = entity_ref.file_path;
            if entity_ref.system_id == Some(system_id)
                || self.load_entity(file_path.as_str())?.id == system_id
            {
                return Ok(file_path);
            }
        }

        Err(anyhow::anyhow!("System not found: {system_id}"))
    }

    fn next_link_id(&mut self) -> Result<i64> {
        let mut max_id = 0i64;
        for interaction in self.load_all_interactions()? {
            max_id = max_id.max(interaction.id);
        }

        Ok(max_id + 1)
    }

    fn next_note_id(&mut self) -> Result<i64> {
        let mut max_id = 0i64;
        for entity_ref in self.manifest.entities.clone() {
            let entity = self.load_entity(entity_ref.file_path.as_str())?;
            for note in &entity.notes {
                max_id = max_id.max(note.id);
            }
        }

        Ok(max_id + 1)
    }

    fn interaction_file_path_for_id(&mut self, link_id: i64) -> Result<String> {
        self.load_all_interactions()?;
        self.loaded_interactions
            .iter()
            .find_map(|(file_path, interaction)| {
                (interaction.id == link_id).then(|| file_path.clone())
            })
            .ok_or_else(|| anyhow::anyhow!("Interaction not found: {link_id}"))
    }

    fn note_location_for_id(&mut self, note_id: i64) -> Result<(String, usize)> {
        for entity_ref in self.manifest.entities.clone() {
            let file_path = entity_ref.file_path;
            let entity = self.load_entity(file_path.as_str())?;
            if let Some(index) = entity.notes.iter().position(|note| note.id == note_id) {
                return Ok((file_path, index));
            }
        }

        Err(anyhow::anyhow!("Note not found: {note_id}"))
    }

    fn current_timestamp_string() -> String {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_owned())
    }

    fn sync_entity_ref_summary(&mut self, file_path: &str) -> Result<()> {
        let summary = {
            let manifest_position = self
                .manifest
                .entities
                .iter()
                .find(|candidate| candidate.file_path == file_path)
                .map(|entity_ref| (entity_ref.pos_x, entity_ref.pos_y));
            let entity = self.load_entity(file_path)?;
            let (pos_x, pos_y) = manifest_position
                .unwrap_or((entity.map_x.unwrap_or(0.0), entity.map_y.unwrap_or(0.0)));
            crate::project_store::LightweightEntityRef::from_system_file(
                file_path.to_owned(),
                pos_x,
                pos_y,
                entity,
            )
        };

        self.upsert_entity_ref(summary);
        Ok(())
    }

    pub fn entity_ref_for_system_id(
        &mut self,
        system_id: i64,
    ) -> Result<crate::project_store::LightweightEntityRef> {
        for entity_ref in self.manifest.entities.clone() {
            if entity_ref.system_id == Some(system_id) {
                return Ok(entity_ref);
            }

            let file_path = entity_ref.file_path.clone();
            if self.load_entity(file_path.as_str())?.id == system_id {
                self.sync_entity_ref_summary(file_path.as_str())?;
                if let Some(updated) = self
                    .manifest
                    .entities
                    .iter()
                    .find(|candidate| candidate.file_path == file_path)
                    .cloned()
                {
                    return Ok(updated);
                }
            }
        }

        Err(anyhow::anyhow!("System not found: {system_id}"))
    }

    /// Open an existing file-based project from the given root directory.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let manifest = Self::load_manifest(&root)?;

        Ok(Self {
            root,
            manifest,
            loaded_entities: HashMap::new(),
            loaded_interactions: HashMap::new(),
            dirty_entities: HashSet::new(),
            dirty_interactions: HashSet::new(),
            manifest_dirty: false,
        })
    }

    /// Create a new file-based project at the given root directory.
    pub fn create(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)
            .with_context(|| format!("Failed to create project directory: {}", root.display()))?;

        let systems_dir = root.join("systems");
        let interactions_dir = root.join("interactions");
        fs::create_dir_all(&systems_dir).with_context(|| {
            format!(
                "Failed to create systems directory: {}",
                systems_dir.display()
            )
        })?;
        fs::create_dir_all(&interactions_dir).with_context(|| {
            format!(
                "Failed to create interactions directory: {}",
                interactions_dir.display()
            )
        })?;

        let manifest = LightweightProjectFile {
            schema_version: LIGHTWEIGHT_PROJECT_SCHEMA_VERSION,
            entities: Vec::new(),
        };

        let mut store = Self {
            root,
            manifest,
            loaded_entities: HashMap::new(),
            loaded_interactions: HashMap::new(),
            dirty_entities: HashSet::new(),
            dirty_interactions: HashSet::new(),
            manifest_dirty: true,
        };

        store.save()?;
        Ok(store)
    }

    /// Get the project root directory path.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Get entity references from the manifest (positions, types, file paths).
    pub fn entity_refs(&self) -> &[LightweightEntityRef] {
        &self.manifest.entities
    }

    /// Load an entity file by its relative file path.
    ///
    /// Uses lazy loading: loads from disk only if not already in memory.
    pub fn load_entity(&mut self, file_path: &str) -> Result<&SystemFile> {
        if !self.loaded_entities.contains_key(file_path) {
            let absolute_path = self
                .root
                .join(file_path.replace('/', std::path::MAIN_SEPARATOR_STR));
            let content = fs::read_to_string(&absolute_path).with_context(|| {
                format!("Failed to read entity file: {}", absolute_path.display())
            })?;
            let entity: SystemFile = serde_json::from_str(&content).with_context(|| {
                format!("Failed to parse entity file: {}", absolute_path.display())
            })?;
            self.loaded_entities.insert(file_path.to_owned(), entity);
        }

        Ok(self
            .loaded_entities
            .get(file_path)
            .expect("entity should be loaded"))
    }

    /// Load an entity file mutably for editing.
    pub fn load_entity_mut(&mut self, file_path: &str) -> Result<&mut SystemFile> {
        if !self.loaded_entities.contains_key(file_path) {
            let absolute_path = self
                .root
                .join(file_path.replace('/', std::path::MAIN_SEPARATOR_STR));
            let content = fs::read_to_string(&absolute_path).with_context(|| {
                format!("Failed to read entity file: {}", absolute_path.display())
            })?;
            let entity: SystemFile = serde_json::from_str(&content).with_context(|| {
                format!("Failed to parse entity file: {}", absolute_path.display())
            })?;
            self.loaded_entities.insert(file_path.to_owned(), entity);
        }

        self.dirty_entities.insert(file_path.to_owned());
        Ok(self
            .loaded_entities
            .get_mut(file_path)
            .expect("entity should be loaded"))
    }

    /// Add or update an entity reference in the manifest.
    pub fn upsert_entity_ref(&mut self, entity_ref: LightweightEntityRef) {
        let file_path = entity_ref.file_path.clone();

        if let Some(existing) = self
            .manifest
            .entities
            .iter_mut()
            .find(|e| e.file_path == file_path)
        {
            *existing = entity_ref;
        } else {
            self.manifest.entities.push(entity_ref);
        }

        self.manifest_dirty = true;
    }

    /// Update entity position in the manifest (position-only edit).
    pub fn update_entity_position(
        &mut self,
        file_path: &str,
        pos_x: f32,
        pos_y: f32,
    ) -> Result<()> {
        if let Some(entity) = self.loaded_entities.get_mut(file_path) {
            entity.map_x = Some(pos_x);
            entity.map_y = Some(pos_y);
            self.dirty_entities.insert(file_path.to_owned());
        }

        let entity_ref = self
            .manifest
            .entities
            .iter_mut()
            .find(|e| e.file_path == file_path)
            .with_context(|| format!("Entity not found in manifest: {file_path}"))?;

        entity_ref.pos_x = pos_x;
        entity_ref.pos_y = pos_y;
        self.manifest_dirty = true;
        Ok(())
    }

    /// Remove an entity reference from the manifest and delete its file.
    pub fn remove_entity(&mut self, file_path: &str) -> Result<()> {
        self.manifest.entities.retain(|e| e.file_path != file_path);
        self.loaded_entities.remove(file_path);
        self.dirty_entities.remove(file_path);

        let absolute_path = self
            .root
            .join(file_path.replace('/', std::path::MAIN_SEPARATOR_STR));
        if absolute_path.exists() {
            fs::remove_file(&absolute_path).with_context(|| {
                format!("Failed to delete entity file: {}", absolute_path.display())
            })?;
        }

        self.manifest_dirty = true;
        Ok(())
    }

    /// Load all interactions from the interactions directory.
    pub fn load_all_interactions(&mut self) -> Result<Vec<&InteractionFile>> {
        let interactions_dir = self.root.join("interactions");
        if !interactions_dir.exists() {
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(&interactions_dir).with_context(|| {
            format!(
                "Failed to read interactions directory: {}",
                interactions_dir.display()
            )
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            let is_json = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("json"))
                .unwrap_or(false);

            if !is_json {
                continue;
            }

            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                let relative_path = format!("interactions/{file_name}");

                use std::collections::hash_map::Entry;
                if let Entry::Vacant(entry) = self.loaded_interactions.entry(relative_path.clone())
                {
                    let content = fs::read_to_string(&path).with_context(|| {
                        format!("Failed to read interaction file: {}", path.display())
                    })?;
                    let interaction: InteractionFile = serde_json::from_str(&content)
                        .with_context(|| {
                            format!("Failed to parse interaction file: {}", path.display())
                        })?;
                    entry.insert(interaction);
                }
            }
        }

        Ok(self.loaded_interactions.values().collect())
    }

    /// Add or update an interaction file.
    pub fn upsert_interaction(
        &mut self,
        file_path: impl Into<String>,
        interaction: InteractionFile,
    ) {
        let file_path = file_path.into();
        self.loaded_interactions
            .insert(file_path.clone(), interaction);
        self.dirty_interactions.insert(file_path);
    }

    /// Remove an interaction file.
    pub fn remove_interaction(&mut self, file_path: &str) -> Result<()> {
        self.loaded_interactions.remove(file_path);
        self.dirty_interactions.remove(file_path);

        let absolute_path = self
            .root
            .join(file_path.replace('/', std::path::MAIN_SEPARATOR_STR));
        if absolute_path.exists() {
            fs::remove_file(&absolute_path).with_context(|| {
                format!(
                    "Failed to delete interaction file: {}",
                    absolute_path.display()
                )
            })?;
        }

        Ok(())
    }

    /// Check if there are any unsaved changes.
    pub fn has_unsaved_changes(&self) -> bool {
        self.manifest_dirty
            || !self.dirty_entities.is_empty()
            || !self.dirty_interactions.is_empty()
    }

    /// Save all dirty entities and manifest to disk with atomic writes.
    pub fn save(&mut self) -> Result<()> {
        // Save dirty entity files
        for file_path in &self.dirty_entities {
            if let Some(entity) = self.loaded_entities.get(file_path) {
                Self::write_entity_atomically(&self.root, file_path, entity)?;
            }
        }

        // Save dirty interaction files
        for file_path in &self.dirty_interactions {
            if let Some(interaction) = self.loaded_interactions.get(file_path) {
                Self::write_interaction_atomically(&self.root, file_path, interaction)?;
            }
        }

        // Save manifest if dirty
        if self.manifest_dirty {
            Self::write_manifest_atomically(&self.root, &self.manifest)?;
        }

        // Clear dirty flags
        self.dirty_entities.clear();
        self.dirty_interactions.clear();
        self.manifest_dirty = false;

        Ok(())
    }

    /// Load the lightweight project manifest from disk.
    fn load_manifest(root: &Path) -> Result<LightweightProjectFile> {
        let loaded_manifest = load_filesystem_project_manifest(root)?;
        Self::lightweight_manifest_from_project(root, &loaded_manifest)
    }

    fn lightweight_manifest_from_project(
        root: &Path,
        loaded_manifest: &LoadedFilesystemProjectManifest,
    ) -> Result<LightweightProjectFile> {
        let project = &loaded_manifest.project;
        let mut entities = Vec::with_capacity(project.systems_paths.len());
        let mut seen_paths = HashSet::new();

        for system_path in &project.systems_paths {
            if !seen_paths.insert(system_path.clone()) {
                continue;
            }

            let summary_position = loaded_manifest
                .lightweight_positions_by_file_path
                .get(system_path)
                .copied();

            let absolute_path = root.join(system_path.replace('/', std::path::MAIN_SEPARATOR_STR));
            let summary = match fs::read_to_string(&absolute_path)
                .ok()
                .and_then(|text| serde_json::from_str::<SystemFile>(&text).ok())
            {
                Some(system) => LightweightEntityRef::from_system_file(
                    system_path.clone(),
                    summary_position
                        .map(|(pos_x, _)| pos_x)
                        .unwrap_or_else(|| system.map_x.unwrap_or(0.0)),
                    summary_position
                        .map(|(_, pos_y)| pos_y)
                        .unwrap_or_else(|| system.map_y.unwrap_or(0.0)),
                    &system,
                ),
                None => {
                    let (pos_x, pos_y) = summary_position.unwrap_or((0.0, 0.0));
                    LightweightEntityRef::new("service", system_path.clone(), pos_x, pos_y)
                }
            };

            entities.push(summary);
        }

        Ok(LightweightProjectFile {
            schema_version: project
                .schema_version
                .max(LIGHTWEIGHT_PROJECT_SCHEMA_VERSION),
            entities,
        })
    }

    /// Atomically write entity file using temp file + rename strategy.
    fn write_entity_atomically(root: &Path, file_path: &str, entity: &SystemFile) -> Result<()> {
        let absolute_path = root.join(file_path.replace('/', std::path::MAIN_SEPARATOR_STR));

        if let Some(parent) = absolute_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create parent directory: {}", parent.display())
            })?;
        }

        let bytes = serde_json::to_vec_pretty(entity).context("Failed to serialize entity")?;

        Self::atomic_write(&absolute_path, &bytes)
    }

    /// Atomically write interaction file using temp file + rename strategy.
    fn write_interaction_atomically(
        root: &Path,
        file_path: &str,
        interaction: &InteractionFile,
    ) -> Result<()> {
        let absolute_path = root.join(file_path.replace('/', std::path::MAIN_SEPARATOR_STR));

        if let Some(parent) = absolute_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create parent directory: {}", parent.display())
            })?;
        }

        let bytes =
            serde_json::to_vec_pretty(interaction).context("Failed to serialize interaction")?;

        Self::atomic_write(&absolute_path, &bytes)
    }

    /// Atomically write manifest file using temp file + rename strategy.
    fn write_manifest_atomically(root: &Path, manifest: &LightweightProjectFile) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            let legacy_manifest_path = root.join("Project.json");
            if legacy_manifest_path.exists() {
                let is_full_project_manifest = fs::read_to_string(&legacy_manifest_path)
                    .ok()
                    .and_then(|text| serde_json::from_str::<ProjectFile>(&text).ok())
                    .is_some();

                if is_full_project_manifest {
                    // Avoid clobbering full project metadata on case-insensitive filesystems.
                    return Ok(());
                }
            }
        }

        let manifest_path = root.join(LIGHTWEIGHT_PROJECT_FILE_NAME);
        let bytes = serde_json::to_vec_pretty(manifest).context("Failed to serialize manifest")?;

        Self::atomic_write(&manifest_path, &bytes)
    }

    /// Perform atomic write: write to temp file, then rename to final path.
    ///
    /// This ensures that the file is never in a partially-written state,
    /// providing crash-safety guarantees.
    fn atomic_write(target_path: &Path, bytes: &[u8]) -> Result<()> {
        let temp_path = target_path.with_extension("tmp");

        fs::write(&temp_path, bytes)
            .with_context(|| format!("Failed to write temp file: {}", temp_path.display()))?;

        fs::rename(&temp_path, target_path).with_context(|| {
            format!(
                "Failed to rename {} to {}",
                temp_path.display(),
                target_path.display()
            )
        })?;

        Ok(())
    }

    // === File-Native CRUD API ===
    // These methods back the app's persistence workflows directly against the
    // on-disk project format.

    #[allow(dead_code)]
    pub fn create_system(
        &mut self,
        name: &str,
        description: &str,
        parent_id: Option<i64>,
        system_type: &str,
        route_methods: Option<&str>,
    ) -> anyhow::Result<i64> {
        let trimmed_name = name.trim();
        if trimmed_name.is_empty() {
            return Err(anyhow::anyhow!("System name is required"));
        }

        let new_id = self.next_system_id()?;
        let file_stem = Self::slugify_file_stem(trimmed_name);
        let file_path = format!("systems/{file_stem}__{new_id}.json");
        let normalized_type = {
            let trimmed = system_type.trim();
            if trimmed.is_empty() {
                "service".to_owned()
            } else {
                trimmed.to_owned()
            }
        };

        let entity = SystemFile {
            id: new_id,
            name: trimmed_name.to_owned(),
            description: description.trim().to_owned(),
            parent_id,
            calculated_name: trimmed_name.to_owned(),
            map_x: Some(0.0),
            map_y: Some(0.0),
            line_color_override: None,
            naming_root: false,
            naming_delimiter: "/".to_owned(),
            system_type: normalized_type.clone(),
            route_methods: Self::normalized_optional_text(route_methods),
            tech_ids: Vec::new(),
            notes: Vec::new(),
            database_columns: Vec::new(),
        };

        self.loaded_entities.insert(file_path.clone(), entity);
        self.dirty_entities.insert(file_path.clone());
        let summary = self
            .loaded_entities
            .get(file_path.as_str())
            .map(|entity| {
                LightweightEntityRef::from_system_file(file_path.clone(), 0.0, 0.0, entity)
            })
            .expect("inserted entity should exist");
        self.upsert_entity_ref(summary);
        self.save()?;

        Ok(new_id)
    }

    #[allow(dead_code)]
    pub fn delete_system(&mut self, system_id: i64) -> anyhow::Result<()> {
        let file_path = self.entity_file_path_for_system_id(system_id)?;
        self.remove_entity(file_path.as_str())?;

        self.load_all_interactions()?;
        let interaction_paths = self
            .loaded_interactions
            .iter()
            .filter_map(|(path, interaction)| {
                (interaction.source_system_id == system_id
                    || interaction.target_system_id == system_id)
                    .then(|| path.clone())
            })
            .collect::<Vec<_>>();
        for interaction_path in interaction_paths {
            self.remove_interaction(interaction_path.as_str())?;
        }

        self.save()
    }

    #[allow(dead_code)]
    pub fn list_systems(&mut self) -> anyhow::Result<Vec<crate::models::SystemRecord>> {
        let mut systems = Vec::new();
        for entity_ref in self.manifest.entities.clone() {
            if entity_ref.has_cached_summary() {
                systems.push(crate::models::SystemRecord {
                    id: entity_ref
                        .system_id
                        .expect("cached summary should include id"),
                    name: entity_ref.name.clone().unwrap_or_default(),
                    description: entity_ref.description.clone().unwrap_or_default(),
                    parent_id: entity_ref.parent_id,
                    map_x: Some(entity_ref.pos_x),
                    map_y: Some(entity_ref.pos_y),
                    line_color_override: entity_ref.line_color_override.clone(),
                    naming_root: entity_ref.naming_root,
                    naming_delimiter: entity_ref.naming_delimiter.clone(),
                    system_type: entity_ref.entity_type_id.clone(),
                    route_methods: entity_ref.route_methods.clone(),
                });
                continue;
            }

            let entity = self.load_entity(entity_ref.file_path.as_str())?;
            systems.push(crate::models::SystemRecord {
                id: entity.id,
                name: entity.name.clone(),
                description: entity.description.clone(),
                parent_id: entity.parent_id,
                map_x: Some(entity_ref.pos_x),
                map_y: Some(entity_ref.pos_y),
                line_color_override: entity.line_color_override.clone(),
                naming_root: entity.naming_root,
                naming_delimiter: entity.naming_delimiter.clone(),
                system_type: entity.system_type.clone(),
                route_methods: entity.route_methods.clone(),
            });
        }

        Ok(systems)
    }

    #[allow(dead_code)]
    pub fn update_system_details(
        &mut self,
        system_id: i64,
        name: &str,
        description: &str,
        naming_root: bool,
        naming_delim: &str,
        system_type: &str,
        route_methods: Option<&str>,
    ) -> anyhow::Result<()> {
        let file_path = self.entity_file_path_for_system_id(system_id)?;
        {
            let entity = self.load_entity_mut(file_path.as_str())?;
            entity.name = name.trim().to_owned();
            entity.description = description.trim().to_owned();
            entity.calculated_name = entity.name.clone();
            entity.naming_root = naming_root;
            entity.naming_delimiter = naming_delim.to_owned();
            entity.system_type = if system_type.trim().is_empty() {
                "service".to_owned()
            } else {
                system_type.trim().to_owned()
            };
            entity.route_methods = Self::normalized_optional_text(route_methods);
        }
        self.sync_entity_ref_summary(file_path.as_str())?;
        self.save()
    }

    #[allow(dead_code)]
    pub fn update_system_position_optional(
        &mut self,
        system_id: i64,
        x: Option<f32>,
        y: Option<f32>,
    ) -> anyhow::Result<()> {
        let file_path = self.entity_file_path_for_system_id(system_id)?;
        {
            let entity = self.load_entity_mut(file_path.as_str())?;
            entity.map_x = x;
            entity.map_y = y;
        }

        if let Some(entity_ref) = self
            .manifest
            .entities
            .iter_mut()
            .find(|entity| entity.file_path == file_path)
        {
            entity_ref.pos_x = x.unwrap_or(0.0);
            entity_ref.pos_y = y.unwrap_or(0.0);
            self.manifest_dirty = true;
        }

        self.sync_entity_ref_summary(file_path.as_str())?;

        self.save()
    }

    #[allow(dead_code)]
    pub fn update_system_line_color_override(
        &mut self,
        system_id: i64,
        color: Option<&str>,
    ) -> anyhow::Result<()> {
        let file_path = self.entity_file_path_for_system_id(system_id)?;
        {
            let entity = self.load_entity_mut(file_path.as_str())?;
            entity.line_color_override = Self::normalized_optional_text(color);
        }
        self.sync_entity_ref_summary(file_path.as_str())?;
        self.save()
    }

    #[allow(dead_code)]
    pub fn update_system_parent(
        &mut self,
        child_id: i64,
        parent_id: Option<i64>,
    ) -> anyhow::Result<()> {
        let file_path = self.entity_file_path_for_system_id(child_id)?;
        {
            let entity = self.load_entity_mut(file_path.as_str())?;
            entity.parent_id = parent_id;
        }
        self.sync_entity_ref_summary(file_path.as_str())?;
        self.save()
    }

    #[allow(dead_code)]
    pub fn insert_system_with_id(
        &mut self,
        id: i64,
        name: &str,
        description: &str,
        parent_id: Option<i64>,
        map_x: Option<f32>,
        map_y: Option<f32>,
        line_color_override: Option<&str>,
        naming_root: bool,
        naming_delimiter: &str,
        system_type: &str,
        route_methods: Option<&str>,
    ) -> anyhow::Result<()> {
        let file_stem = Self::slugify_file_stem(name);
        let file_path = format!("systems/{file_stem}__{id}.json");
        let entity = SystemFile {
            id,
            name: name.trim().to_owned(),
            description: description.trim().to_owned(),
            parent_id,
            calculated_name: name.trim().to_owned(),
            map_x,
            map_y,
            line_color_override: Self::normalized_optional_text(line_color_override),
            naming_root,
            naming_delimiter: naming_delimiter.to_owned(),
            system_type: if system_type.trim().is_empty() {
                "service".to_owned()
            } else {
                system_type.trim().to_owned()
            },
            route_methods: Self::normalized_optional_text(route_methods),
            tech_ids: Vec::new(),
            notes: Vec::new(),
            database_columns: Vec::new(),
        };

        self.loaded_entities.insert(file_path.clone(), entity);
        self.dirty_entities.insert(file_path.clone());
        let summary = self
            .loaded_entities
            .get(file_path.as_str())
            .map(|entity| {
                LightweightEntityRef::from_system_file(
                    file_path.clone(),
                    map_x.unwrap_or(0.0),
                    map_y.unwrap_or(0.0),
                    entity,
                )
            })
            .expect("inserted entity should exist");
        self.upsert_entity_ref(summary);
        self.save()
    }

    #[allow(dead_code)]
    pub fn replace_database_columns_for_system(
        &mut self,
        system_id: i64,
        columns: &[crate::models::DatabaseColumnInput],
    ) -> anyhow::Result<()> {
        let file_path = self.entity_file_path_for_system_id(system_id)?;
        {
            let entity = self.load_entity_mut(file_path.as_str())?;
            entity.database_columns = columns
                .iter()
                .map(|column| crate::project_store::DatabaseColumnFile {
                    position: column.position,
                    column_name: column.column_name.clone(),
                    column_type: column.column_type.clone(),
                    constraints: column.constraints.clone(),
                })
                .collect();
        }
        self.sync_entity_ref_summary(file_path.as_str())?;
        self.save()
    }

    #[allow(dead_code)]
    pub fn create_link(
        &mut self,
        source_system_id: i64,
        target_system_id: i64,
        label: &str,
        kind: &str,
        source_column_name: Option<&str>,
        target_column_name: Option<&str>,
    ) -> anyhow::Result<()> {
        let link_id = self.next_link_id()?;
        self.insert_link_with_id(
            link_id,
            source_system_id,
            target_system_id,
            label,
            "",
            kind,
            source_column_name,
            target_column_name,
        )
    }

    #[allow(dead_code)]
    pub fn delete_link(&mut self, link_id: i64) -> anyhow::Result<()> {
        let file_path = self.interaction_file_path_for_id(link_id)?;
        self.remove_interaction(file_path.as_str())?;
        self.save()
    }

    #[allow(dead_code)]
    pub fn list_links(&mut self) -> anyhow::Result<Vec<crate::models::SystemLink>> {
        let mut links = self
            .load_all_interactions()?
            .into_iter()
            .map(|interaction| crate::models::SystemLink {
                id: interaction.id,
                source_system_id: interaction.source_system_id,
                target_system_id: interaction.target_system_id,
                label: interaction.label.clone(),
                note: interaction.note.clone(),
                kind: interaction.kind.clone(),
                source_column_name: interaction.source_column_name.clone(),
                target_column_name: interaction.target_column_name.clone(),
            })
            .collect::<Vec<_>>();
        links.sort_by_key(|link| link.id);
        Ok(links)
    }

    #[allow(dead_code)]
    pub fn update_link_details(
        &mut self,
        link_id: i64,
        label: &str,
        note: &str,
        kind: &str,
        source_col: Option<&str>,
        target_col: Option<&str>,
    ) -> anyhow::Result<()> {
        let file_path = self.interaction_file_path_for_id(link_id)?;
        if let Some(interaction) = self.loaded_interactions.get_mut(file_path.as_str()) {
            interaction.label = label.to_owned();
            interaction.note = note.to_owned();
            interaction.kind = kind.to_owned();
            interaction.source_column_name = Self::normalized_optional_text(source_col);
            interaction.target_column_name = Self::normalized_optional_text(target_col);
            self.dirty_interactions.insert(file_path);
        }
        self.save()
    }

    #[allow(dead_code)]
    pub fn update_link_endpoints(
        &mut self,
        link_id: i64,
        source_id: i64,
        target_id: i64,
    ) -> anyhow::Result<()> {
        let file_path = self.interaction_file_path_for_id(link_id)?;
        let mut interaction = self
            .loaded_interactions
            .get(file_path.as_str())
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Interaction not loaded: {link_id}"))?;
        self.remove_interaction(file_path.as_str())?;
        interaction.source_system_id = source_id;
        interaction.target_system_id = target_id;
        let next_path = format!("interactions/{source_id}__to__{target_id}__{link_id}.json");
        self.upsert_interaction(next_path, interaction);
        self.save()
    }

    #[allow(dead_code)]
    pub fn create_note(&mut self, system_id: i64, body: &str) -> anyhow::Result<()> {
        let note_id = self.next_note_id()?;
        self.insert_note_with_id(
            note_id,
            system_id,
            body,
            Self::current_timestamp_string().as_str(),
        )
    }

    #[allow(dead_code)]
    pub fn list_notes_for_system(
        &mut self,
        system_id: i64,
    ) -> anyhow::Result<Vec<crate::models::SystemNote>> {
        let file_path = self.entity_file_path_for_system_id(system_id)?;
        let entity = self.load_entity(file_path.as_str())?;

        Ok(entity
            .notes
            .iter()
            .cloned()
            .map(|note| crate::models::SystemNote {
                id: note.id,
                body: note.body,
                updated_at: note.updated_at,
            })
            .collect())
    }

    #[allow(dead_code)]
    pub fn delete_note(&mut self, note_id: i64) -> anyhow::Result<()> {
        let (file_path, index) = self.note_location_for_id(note_id)?;
        {
            let entity = self.load_entity_mut(file_path.as_str())?;
            entity.notes.remove(index);
        }
        self.save()
    }

    #[allow(dead_code)]
    pub fn update_note(&mut self, note_id: i64, body: &str) -> anyhow::Result<()> {
        let (file_path, index) = self.note_location_for_id(note_id)?;
        {
            let entity = self.load_entity_mut(file_path.as_str())?;
            entity.notes[index].body = body.to_owned();
            entity.notes[index].updated_at = Self::current_timestamp_string();
        }
        self.save()
    }

    #[allow(dead_code)]
    pub fn delete_notes_for_system(&mut self, system_id: i64) -> anyhow::Result<()> {
        let file_path = self.entity_file_path_for_system_id(system_id)?;
        {
            let entity = self.load_entity_mut(file_path.as_str())?;
            entity.notes.clear();
        }
        self.save()
    }

    #[allow(dead_code)]
    pub fn add_tech_to_system(&mut self, _system_id: i64, _tech_id: i64) -> anyhow::Result<()> {
        let file_path = self.entity_file_path_for_system_id(_system_id)?;
        {
            let entity = self.load_entity_mut(file_path.as_str())?;
            if !entity.tech_ids.contains(&_tech_id) {
                entity.tech_ids.push(_tech_id);
            }
        }
        self.sync_entity_ref_summary(file_path.as_str())?;
        self.save()?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn insert_note_with_id(
        &mut self,
        id: i64,
        system_id: i64,
        body: &str,
        updated_at: &str,
    ) -> anyhow::Result<()> {
        let file_path = self.entity_file_path_for_system_id(system_id)?;
        {
            let entity = self.load_entity_mut(file_path.as_str())?;
            if let Some(existing) = entity.notes.iter_mut().find(|note| note.id == id) {
                existing.body = body.to_owned();
                existing.updated_at = updated_at.to_owned();
            } else {
                entity.notes.push(crate::project_store::SystemNoteFile {
                    id,
                    body: body.to_owned(),
                    updated_at: updated_at.to_owned(),
                });
            }
        }
        self.save()
    }

    #[allow(dead_code)]
    pub fn insert_link_with_id(
        &mut self,
        id: i64,
        source_system_id: i64,
        target_system_id: i64,
        label: &str,
        note: &str,
        kind: &str,
        source_column_name: Option<&str>,
        target_column_name: Option<&str>,
    ) -> anyhow::Result<()> {
        let file_path =
            format!("interactions/{source_system_id}__to__{target_system_id}__{id}.json");
        self.upsert_interaction(
            file_path,
            InteractionFile {
                id,
                source_system_id,
                target_system_id,
                label: label.to_owned(),
                note: note.to_owned(),
                kind: kind.to_owned(),
                source_column_name: Self::normalized_optional_text(source_column_name),
                target_column_name: Self::normalized_optional_text(target_column_name),
            },
        );
        self.save()
    }

    #[allow(dead_code)]
    pub fn remove_tech_from_system(&mut self, system_id: i64, tech_id: i64) -> anyhow::Result<()> {
        let file_path = self.entity_file_path_for_system_id(system_id)?;
        {
            let entity = self.load_entity_mut(file_path.as_str())?;
            entity.tech_ids.retain(|existing| *existing != tech_id);
        }
        self.sync_entity_ref_summary(file_path.as_str())?;
        self.save()
    }

    #[allow(dead_code)]
    pub fn clear_catalog_data(&mut self) -> anyhow::Result<()> {
        let systems_dir = self.root.join("systems");
        let interactions_dir = self.root.join("interactions");

        if systems_dir.exists() {
            fs::remove_dir_all(&systems_dir).with_context(|| {
                format!(
                    "Failed to clear systems directory: {}",
                    systems_dir.display()
                )
            })?;
        }
        if interactions_dir.exists() {
            fs::remove_dir_all(&interactions_dir).with_context(|| {
                format!(
                    "Failed to clear interactions directory: {}",
                    interactions_dir.display()
                )
            })?;
        }

        fs::create_dir_all(&systems_dir).with_context(|| {
            format!(
                "Failed to recreate systems directory: {}",
                systems_dir.display()
            )
        })?;
        fs::create_dir_all(&interactions_dir).with_context(|| {
            format!(
                "Failed to recreate interactions directory: {}",
                interactions_dir.display()
            )
        })?;

        self.manifest.entities.clear();
        self.loaded_entities.clear();
        self.loaded_interactions.clear();
        self.dirty_entities.clear();
        self.dirty_interactions.clear();
        self.manifest_dirty = true;
        self.save()
    }

    #[allow(dead_code)]
    pub fn clear_non_system_catalog_data(&mut self) -> anyhow::Result<()> {
        let interaction_paths = self
            .load_all_interactions()?
            .iter()
            .map(|interaction| {
                format!(
                    "interactions/{}__to__{}__{}.json",
                    interaction.source_system_id, interaction.target_system_id, interaction.id
                )
            })
            .collect::<Vec<_>>();
        for path in interaction_paths {
            self.remove_interaction(path.as_str())?;
        }

        for entity_ref in self.manifest.entities.clone() {
            let entity = self.load_entity_mut(entity_ref.file_path.as_str())?;
            entity.tech_ids.clear();
            entity.notes.clear();
            entity.database_columns.clear();
        }

        self.save()
    }

    #[allow(dead_code)]
    pub fn replace_system_tech_assignments(
        &mut self,
        system_id: i64,
        tech_ids: &[i64],
    ) -> anyhow::Result<()> {
        let file_path = self.entity_file_path_for_system_id(system_id)?;
        {
            let entity = self.load_entity_mut(file_path.as_str())?;
            entity.tech_ids = tech_ids.to_vec();
        }
        self.sync_entity_ref_summary(file_path.as_str())?;
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_store::LightweightEntityRef;

    fn temp_test_dir(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be valid")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("file_store_{name}_{unique}"));
        std::fs::create_dir_all(&directory).expect("temp test directory should be created");
        directory
    }

    #[test]
    fn file_store_create_initializes_empty_project() {
        let root = temp_test_dir("create");

        let store = FileStore::create(&root).expect("store should be created");

        assert_eq!(store.root(), root.as_path());
        assert_eq!(store.entity_refs().len(), 0);
        assert!(root.join("project.json").exists());
        assert!(root.join("systems").exists());
        assert!(root.join("interactions").exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_store_open_loads_existing_manifest() {
        let root = temp_test_dir("open");

        let manifest = LightweightProjectFile {
            schema_version: LIGHTWEIGHT_PROJECT_SCHEMA_VERSION,
            entities: vec![
                LightweightEntityRef::new("api", "systems/orders.json", 10.0, 20.0),
                LightweightEntityRef::new("service", "systems/users.json", 30.0, 40.0),
            ],
        };

        fs::create_dir_all(&root).expect("root should exist");
        let manifest_bytes =
            serde_json::to_vec_pretty(&manifest).expect("manifest should serialize");
        fs::write(root.join("project.json"), &manifest_bytes).expect("manifest should be written");

        let store = FileStore::open(&root).expect("store should open");

        assert_eq!(store.entity_refs().len(), 2);
        assert_eq!(store.entity_refs()[0].file_path, "systems/orders.json");
        assert_eq!(store.entity_refs()[1].file_path, "systems/users.json");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_store_open_prefers_lightweight_manifest_positions_over_entity_file_positions() {
        let root = temp_test_dir("open_lightweight_positions");
        fs::create_dir_all(root.join("systems")).expect("systems dir should exist");

        let entity = crate::project_store::SystemFile {
            id: 42,
            name: "Orders".to_owned(),
            description: String::new(),
            parent_id: None,
            calculated_name: "Orders".to_owned(),
            map_x: Some(0.0),
            map_y: Some(0.0),
            line_color_override: None,
            naming_root: false,
            naming_delimiter: "/".to_owned(),
            system_type: "service".to_owned(),
            route_methods: None,
            tech_ids: Vec::new(),
            notes: Vec::new(),
            database_columns: Vec::new(),
        };

        fs::write(
            root.join("systems/orders__42.json"),
            serde_json::to_vec_pretty(&entity).expect("entity should serialize"),
        )
        .expect("entity file should be written");

        let manifest = LightweightProjectFile {
            schema_version: LIGHTWEIGHT_PROJECT_SCHEMA_VERSION,
            entities: vec![LightweightEntityRef::new(
                "service",
                "systems/orders__42.json",
                512.0,
                256.0,
            )],
        };

        fs::write(
            root.join("project.json"),
            serde_json::to_vec_pretty(&manifest).expect("manifest should serialize"),
        )
        .expect("manifest should be written");

        let store = FileStore::open(&root).expect("store should open");
        assert_eq!(store.entity_refs()[0].pos_x, 512.0);
        assert_eq!(store.entity_refs()[0].pos_y, 256.0);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_store_upsert_entity_ref_adds_new_reference() {
        let root = temp_test_dir("upsert_add");
        let mut store = FileStore::create(&root).expect("store should be created");

        store.upsert_entity_ref(LightweightEntityRef::new(
            "api",
            "systems/orders.json",
            10.0,
            20.0,
        ));

        assert_eq!(store.entity_refs().len(), 1);
        assert_eq!(store.entity_refs()[0].entity_type_id, "api");
        assert!(store.manifest_dirty);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_store_upsert_entity_ref_updates_existing_reference() {
        let root = temp_test_dir("upsert_update");
        let mut store = FileStore::create(&root).expect("store should be created");

        store.upsert_entity_ref(LightweightEntityRef::new(
            "api",
            "systems/orders.json",
            10.0,
            20.0,
        ));

        store.upsert_entity_ref(LightweightEntityRef::new(
            "service",
            "systems/orders.json",
            30.0,
            40.0,
        ));

        assert_eq!(store.entity_refs().len(), 1);
        assert_eq!(store.entity_refs()[0].entity_type_id, "service");
        assert_eq!(store.entity_refs()[0].pos_x, 30.0);
        assert_eq!(store.entity_refs()[0].pos_y, 40.0);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_store_update_entity_position_modifies_only_position() {
        let root = temp_test_dir("update_position");
        let mut store = FileStore::create(&root).expect("store should be created");

        store.upsert_entity_ref(LightweightEntityRef::new(
            "api",
            "systems/orders.json",
            10.0,
            20.0,
        ));

        store
            .update_entity_position("systems/orders.json", 100.0, 200.0)
            .expect("position should update");

        assert_eq!(store.entity_refs()[0].entity_type_id, "api");
        assert_eq!(store.entity_refs()[0].pos_x, 100.0);
        assert_eq!(store.entity_refs()[0].pos_y, 200.0);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_store_sync_entity_ref_summary_preserves_manifest_position_after_details_edit() {
        let root = temp_test_dir("preserve_position_on_details_edit");
        fs::create_dir_all(root.join("systems")).expect("systems dir should exist");

        let entity = crate::project_store::SystemFile {
            id: 7,
            name: "Payments".to_owned(),
            description: String::new(),
            parent_id: None,
            calculated_name: "Payments".to_owned(),
            map_x: Some(0.0),
            map_y: Some(0.0),
            line_color_override: None,
            naming_root: false,
            naming_delimiter: "/".to_owned(),
            system_type: "service".to_owned(),
            route_methods: None,
            tech_ids: Vec::new(),
            notes: Vec::new(),
            database_columns: Vec::new(),
        };

        fs::write(
            root.join("systems/payments__7.json"),
            serde_json::to_vec_pretty(&entity).expect("entity should serialize"),
        )
        .expect("entity file should be written");

        let manifest = LightweightProjectFile {
            schema_version: LIGHTWEIGHT_PROJECT_SCHEMA_VERSION,
            entities: vec![LightweightEntityRef::new(
                "service",
                "systems/payments__7.json",
                640.0,
                320.0,
            )],
        };

        fs::write(
            root.join("project.json"),
            serde_json::to_vec_pretty(&manifest).expect("manifest should serialize"),
        )
        .expect("manifest should be written");

        let mut store = FileStore::open(&root).expect("store should open");

        store
            .update_system_details(
                7,
                "Payments Service",
                "Updated",
                false,
                "/",
                "service",
                None,
            )
            .expect("details update should succeed");

        let updated_ref = store
            .entity_ref_for_system_id(7)
            .expect("entity ref should still exist");
        assert_eq!(updated_ref.pos_x, 640.0);
        assert_eq!(updated_ref.pos_y, 320.0);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_store_save_persists_manifest_to_disk() {
        let root = temp_test_dir("save_manifest");
        let mut store = FileStore::create(&root).expect("store should be created");

        store.upsert_entity_ref(LightweightEntityRef::new(
            "database",
            "systems/inventory.json",
            50.0,
            60.0,
        ));

        store.save().expect("save should succeed");

        let reloaded = FileStore::open(&root).expect("store should reload");
        assert_eq!(reloaded.entity_refs().len(), 1);
        assert_eq!(
            reloaded.entity_refs()[0].file_path,
            "systems/inventory.json"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_store_atomic_write_creates_temp_file_first() {
        let root = temp_test_dir("atomic_write");
        let target = root.join("test.json");
        let content = b"{\"test\": true}";

        FileStore::atomic_write(&target, content).expect("atomic write should succeed");

        assert!(target.exists());
        let read_content = fs::read(&target).expect("file should be readable");
        assert_eq!(read_content, content);

        let temp_file = target.with_extension("tmp");
        assert!(!temp_file.exists(), "temp file should be cleaned up");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_store_remove_entity_deletes_reference_and_sets_dirty() {
        let root = temp_test_dir("remove_entity");
        let mut store = FileStore::create(&root).expect("store should be created");

        store.upsert_entity_ref(LightweightEntityRef::new(
            "api",
            "systems/orders.json",
            10.0,
            20.0,
        ));
        store.upsert_entity_ref(LightweightEntityRef::new(
            "service",
            "systems/users.json",
            30.0,
            40.0,
        ));

        store
            .remove_entity("systems/orders.json")
            .expect("remove should succeed");

        assert_eq!(store.entity_refs().len(), 1);
        assert_eq!(store.entity_refs()[0].file_path, "systems/users.json");
        assert!(store.manifest_dirty);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_store_lazy_loads_entity_only_once() {
        let root = temp_test_dir("lazy_load");
        fs::create_dir_all(root.join("systems")).expect("systems dir should exist");

        let entity = crate::project_store::SystemFile {
            id: 42,
            name: "TestEntity".to_owned(),
            description: "Test description".to_owned(),
            parent_id: None,
            calculated_name: "TestEntity".to_owned(),
            map_x: Some(10.0),
            map_y: Some(20.0),
            line_color_override: None,
            naming_root: false,
            naming_delimiter: "/".to_owned(),
            system_type: "service".to_owned(),
            route_methods: None,
            tech_ids: Vec::new(),
            notes: Vec::new(),
            database_columns: Vec::new(),
        };

        let entity_path = root.join("systems/test.json");
        let entity_bytes = serde_json::to_vec_pretty(&entity).expect("entity should serialize");
        fs::write(&entity_path, &entity_bytes).expect("entity file should be written");

        let manifest = LightweightProjectFile {
            schema_version: LIGHTWEIGHT_PROJECT_SCHEMA_VERSION,
            entities: vec![LightweightEntityRef::new(
                "service",
                "systems/test.json",
                10.0,
                20.0,
            )],
        };
        let manifest_bytes =
            serde_json::to_vec_pretty(&manifest).expect("manifest should serialize");
        fs::write(root.join("project.json"), &manifest_bytes).expect("manifest should be written");

        let mut store = FileStore::open(&root).expect("store should open");

        // First load
        let loaded1 = store
            .load_entity("systems/test.json")
            .expect("entity should load");
        assert_eq!(loaded1.id, 42);
        assert_eq!(loaded1.name, "TestEntity");

        // Modify the file on disk
        let mut modified_entity = entity.clone();
        modified_entity.name = "ModifiedEntity".to_owned();
        let modified_bytes =
            serde_json::to_vec_pretty(&modified_entity).expect("modified entity should serialize");
        fs::write(&entity_path, &modified_bytes).expect("modified entity should be written");

        // Second load should return cached version (not reload from disk)
        let loaded2 = store
            .load_entity("systems/test.json")
            .expect("entity should load");
        assert_eq!(
            loaded2.name, "TestEntity",
            "entity should be cached, not reloaded"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_store_load_entity_mut_marks_entity_dirty() {
        let root = temp_test_dir("load_mut");
        fs::create_dir_all(root.join("systems")).expect("systems dir should exist");

        let entity = crate::project_store::SystemFile {
            id: 1,
            name: "Original".to_owned(),
            description: String::new(),
            parent_id: None,
            calculated_name: "Original".to_owned(),
            map_x: None,
            map_y: None,
            line_color_override: None,
            naming_root: false,
            naming_delimiter: "/".to_owned(),
            system_type: "service".to_owned(),
            route_methods: None,
            tech_ids: Vec::new(),
            notes: Vec::new(),
            database_columns: Vec::new(),
        };

        let entity_path = root.join("systems/entity.json");
        fs::write(
            &entity_path,
            serde_json::to_vec_pretty(&entity).expect("entity should serialize"),
        )
        .expect("entity file should be written");

        let manifest = LightweightProjectFile {
            schema_version: LIGHTWEIGHT_PROJECT_SCHEMA_VERSION,
            entities: vec![LightweightEntityRef::new(
                "service",
                "systems/entity.json",
                0.0,
                0.0,
            )],
        };
        fs::write(
            root.join("project.json"),
            serde_json::to_vec_pretty(&manifest).expect("manifest should serialize"),
        )
        .expect("manifest should be written");

        let mut store = FileStore::open(&root).expect("store should open");
        assert!(!store.has_unsaved_changes());

        {
            let entity_mut = store
                .load_entity_mut("systems/entity.json")
                .expect("entity should load");
            entity_mut.name = "Modified".to_owned();
        }

        assert!(store.has_unsaved_changes());
        assert!(store.dirty_entities.contains("systems/entity.json"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_store_save_writes_dirty_entity_to_disk() {
        let root = temp_test_dir("save_dirty_entity");
        fs::create_dir_all(root.join("systems")).expect("systems dir should exist");

        let entity = crate::project_store::SystemFile {
            id: 99,
            name: "Original".to_owned(),
            description: String::new(),
            parent_id: None,
            calculated_name: "Original".to_owned(),
            map_x: None,
            map_y: None,
            line_color_override: None,
            naming_root: false,
            naming_delimiter: "/".to_owned(),
            system_type: "api".to_owned(),
            route_methods: Some("GET, POST".to_owned()),
            tech_ids: Vec::new(),
            notes: Vec::new(),
            database_columns: Vec::new(),
        };

        let entity_path = root.join("systems/api.json");
        fs::write(
            &entity_path,
            serde_json::to_vec_pretty(&entity).expect("entity should serialize"),
        )
        .expect("entity file should be written");

        let manifest = LightweightProjectFile {
            schema_version: LIGHTWEIGHT_PROJECT_SCHEMA_VERSION,
            entities: vec![LightweightEntityRef::new(
                "api",
                "systems/api.json",
                5.0,
                10.0,
            )],
        };
        fs::write(
            root.join("project.json"),
            serde_json::to_vec_pretty(&manifest).expect("manifest should serialize"),
        )
        .expect("manifest should be written");

        let mut store = FileStore::open(&root).expect("store should open");

        {
            let entity_mut = store
                .load_entity_mut("systems/api.json")
                .expect("entity should load");
            entity_mut.name = "Updated API".to_owned();
        }

        store.save().expect("save should succeed");
        assert!(!store.has_unsaved_changes());

        // Verify the file was updated on disk
        let saved_content =
            fs::read_to_string(&entity_path).expect("saved file should be readable");
        let saved_entity: crate::project_store::SystemFile =
            serde_json::from_str(&saved_content).expect("saved file should parse");
        assert_eq!(saved_entity.name, "Updated API");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_store_load_all_interactions_discovers_files() {
        let root = temp_test_dir("load_interactions");
        fs::create_dir_all(root.join("interactions")).expect("interactions dir should exist");

        let interaction1 = InteractionFile {
            id: 1,
            source_system_id: 10,
            target_system_id: 20,
            label: "calls".to_owned(),
            note: String::new(),
            kind: "http".to_owned(),
            source_column_name: None,
            target_column_name: None,
        };

        let interaction2 = InteractionFile {
            id: 2,
            source_system_id: 20,
            target_system_id: 30,
            label: "queries".to_owned(),
            note: String::new(),
            kind: "database".to_owned(),
            source_column_name: None,
            target_column_name: None,
        };

        fs::write(
            root.join("interactions/10__to__20__1.json"),
            serde_json::to_vec_pretty(&interaction1).expect("interaction should serialize"),
        )
        .expect("interaction file should be written");

        fs::write(
            root.join("interactions/20__to__30__2.json"),
            serde_json::to_vec_pretty(&interaction2).expect("interaction should serialize"),
        )
        .expect("interaction file should be written");

        let manifest = LightweightProjectFile {
            schema_version: LIGHTWEIGHT_PROJECT_SCHEMA_VERSION,
            entities: Vec::new(),
        };
        fs::write(
            root.join("project.json"),
            serde_json::to_vec_pretty(&manifest).expect("manifest should serialize"),
        )
        .expect("manifest should be written");

        let mut store = FileStore::open(&root).expect("store should open");
        let interactions = store
            .load_all_interactions()
            .expect("interactions should load");

        assert_eq!(interactions.len(), 2);
        let labels: Vec<&str> = interactions.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"calls"));
        assert!(labels.contains(&"queries"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_store_upsert_interaction_marks_dirty() {
        let root = temp_test_dir("upsert_interaction");
        let mut store = FileStore::create(&root).expect("store should be created");

        let interaction = InteractionFile {
            id: 100,
            source_system_id: 1,
            target_system_id: 2,
            label: "new interaction".to_owned(),
            note: String::new(),
            kind: "http".to_owned(),
            source_column_name: None,
            target_column_name: None,
        };

        store.upsert_interaction("interactions/1__to__2__100.json", interaction);

        assert!(store.has_unsaved_changes());
        assert!(store
            .dirty_interactions
            .contains("interactions/1__to__2__100.json"));

        let _ = std::fs::remove_dir_all(root);
    }
}
