use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub const LIGHTWEIGHT_PROJECT_SCHEMA_VERSION: u32 = 2;

pub const LEGACY_PROJECT_FILE_NAME: &str = "Project.json";
pub const LIGHTWEIGHT_PROJECT_FILE_NAME: &str = "project.json";

fn default_entity_type_id() -> String {
    "service".to_owned()
}

fn normalize_entity_type_id(entity_type_id: Option<String>) -> String {
    entity_type_id
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(default_entity_type_id)
}

fn normalize_manifest_relative_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut normalized = String::with_capacity(trimmed.len());
    let mut previous_was_separator = false;
    for character in trimmed.chars() {
        let mapped = if character == '\\' { '/' } else { character };
        if mapped == '/' {
            if previous_was_separator {
                continue;
            }
            previous_was_separator = true;
            normalized.push('/');
        } else {
            previous_was_separator = false;
            normalized.push(mapped);
        }
    }

    normalized.trim_start_matches('/').to_owned()
}

fn default_lightweight_project_schema_version() -> u32 {
    LIGHTWEIGHT_PROJECT_SCHEMA_VERSION
}

fn default_manage_system_json_hierarchy() -> bool {
    false
}

fn default_has_git() -> bool {
    false
}

pub fn default_project_settings_for_import() -> ProjectSettings {
    ProjectSettings {
        autosave_enabled: true,
        manage_system_json_hierarchy: false,
        has_git: false,
        map_zoom: 1.0,
        map_pan_x: 0.0,
        map_pan_y: 0.0,
        map_world_width: crate::app::MAP_WORLD_SIZE.x,
        map_world_height: crate::app::MAP_WORLD_SIZE.y,
        snap_to_grid: false,
    }
}

pub fn collect_interaction_paths_from_root(root: &Path) -> Vec<String> {
    let interactions_root = root.join("interactions");
    let Ok(entries) = std::fs::read_dir(interactions_root) else {
        return Vec::new();
    };

    let mut relative_paths = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let is_json = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("json"))
                .unwrap_or(false);
            if !is_json {
                return None;
            }

            let file_name = path.file_name()?.to_str()?.to_owned();
            Some(format!("interactions/{file_name}"))
        })
        .collect::<Vec<_>>();

    relative_paths.sort();
    relative_paths
}

#[derive(Debug, Clone)]
pub struct LoadedFilesystemProjectManifest {
    pub project: ProjectFile,
    pub has_explicit_project_metadata: bool,
    #[cfg_attr(not(test), allow(dead_code))]
    pub lightweight_positions_by_file_path: HashMap<String, (f32, f32)>,
}

pub fn load_filesystem_project_manifest(root: &Path) -> Result<LoadedFilesystemProjectManifest> {
    let legacy_project_path = root.join(LEGACY_PROJECT_FILE_NAME);
    if legacy_project_path.is_file() {
        let project_text = std::fs::read_to_string(&legacy_project_path)?;
        if let Ok(project) = serde_json::from_str::<ProjectFile>(project_text.as_str()) {
            return Ok(LoadedFilesystemProjectManifest {
                project,
                has_explicit_project_metadata: true,
                lightweight_positions_by_file_path: HashMap::new(),
            });
        }
    }

    let lightweight_project_path = root.join(LIGHTWEIGHT_PROJECT_FILE_NAME);
    let project_text = std::fs::read_to_string(&lightweight_project_path)?;
    let lightweight_project: LightweightProjectFile = serde_json::from_str(project_text.as_str())?;

    let mut systems_paths = Vec::new();
    let mut lightweight_positions_by_file_path = HashMap::new();
    let mut seen_paths = HashSet::new();

    for entity in &lightweight_project.entities {
        let file_path = normalize_manifest_relative_path(entity.file_path.as_str());
        if file_path.is_empty() {
            continue;
        }

        if seen_paths.insert(file_path.clone()) {
            systems_paths.push(file_path.clone());
        }

        lightweight_positions_by_file_path.insert(file_path, (entity.pos_x, entity.pos_y));
    }

    let project = ProjectFile {
        schema_version: lightweight_project.schema_version,
        systems_paths,
        interactions_paths: collect_interaction_paths_from_root(root),
        tech_catalog: Vec::new(),
        zones: Vec::new(),
        zone_offsets: Vec::new(),
        settings: default_project_settings_for_import(),
    };

    Ok(LoadedFilesystemProjectManifest {
        project,
        has_explicit_project_metadata: false,
        lightweight_positions_by_file_path,
    })
}

pub fn filesystem_project_has_manifest(root: &Path) -> bool {
    root.join(LEGACY_PROJECT_FILE_NAME).is_file() || root.join(LIGHTWEIGHT_PROJECT_FILE_NAME).is_file()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFile {
    pub schema_version: u32,
    pub systems_paths: Vec<String>,
    pub interactions_paths: Vec<String>,
    pub tech_catalog: Vec<ProjectTechItem>,
    pub zones: Vec<ProjectZone>,
    pub zone_offsets: Vec<ProjectZoneOffset>,
    pub settings: ProjectSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTechItem {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub documentation_link: Option<String>,
    pub color: Option<String>,
    pub display_priority: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectZone {
    pub id: i64,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: Option<String>,
    pub render_priority: i64,
    pub parent_zone_id: Option<i64>,
    pub minimized: bool,
    pub representative_system_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectZoneOffset {
    pub zone_id: i64,
    pub system_id: i64,
    pub offset_x: f32,
    pub offset_y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettings {
    pub autosave_enabled: bool,
    #[serde(default = "default_manage_system_json_hierarchy")]
    pub manage_system_json_hierarchy: bool,
    #[serde(default = "default_has_git")]
    pub has_git: bool,
    pub map_zoom: f32,
    pub map_pan_x: f32,
    pub map_pan_y: f32,
    pub map_world_width: f32,
    pub map_world_height: f32,
    pub snap_to_grid: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum LightweightEntityRefRepr {
    Tuple4((Option<String>, String, f32, f32)),
    Tuple3((String, f32, f32)),
    Object {
        #[serde(default)]
        entity_type_id: Option<String>,
        #[serde(default)]
        system_id: Option<i64>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        parent_id: Option<i64>,
        #[serde(alias = "filePath")]
        file_path: String,
        #[serde(alias = "posX")]
        pos_x: f32,
        #[serde(alias = "posY")]
        pos_y: f32,
        #[serde(default)]
        line_color_override: Option<String>,
        #[serde(default)]
        naming_root: bool,
        #[serde(default = "default_naming_delimiter")]
        naming_delimiter: String,
        #[serde(default)]
        route_methods: Option<String>,
        #[serde(default)]
        tech_ids: Vec<i64>,
        #[serde(default)]
        database_columns: Vec<DatabaseColumnFile>,
    },
}

fn default_naming_delimiter() -> String {
    "/".to_owned()
}

/// Lightweight map node reference stored in `project.json`.
///
/// JSON representation is a compact tuple array:
/// `[entityTypeId, filePath, posX, posY]`.
#[derive(Debug, Clone, PartialEq)]
pub struct LightweightEntityRef {
    pub entity_type_id: String,
    pub system_id: Option<i64>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub parent_id: Option<i64>,
    pub file_path: String,
    pub pos_x: f32,
    pub pos_y: f32,
    pub line_color_override: Option<String>,
    pub naming_root: bool,
    pub naming_delimiter: String,
    pub route_methods: Option<String>,
    pub tech_ids: Vec<i64>,
    pub database_columns: Vec<DatabaseColumnFile>,
}

impl LightweightEntityRef {
    pub fn new(
        entity_type_id: impl Into<String>,
        file_path: impl Into<String>,
        pos_x: f32,
        pos_y: f32,
    ) -> Self {
        Self {
            entity_type_id: normalize_entity_type_id(Some(entity_type_id.into())),
            system_id: None,
            name: None,
            description: None,
            parent_id: None,
            file_path: normalize_manifest_relative_path(file_path.into().as_str()),
            pos_x,
            pos_y,
            line_color_override: None,
            naming_root: false,
            naming_delimiter: default_naming_delimiter(),
            route_methods: None,
            tech_ids: Vec::new(),
            database_columns: Vec::new(),
        }
    }

    pub fn has_cached_summary(&self) -> bool {
        self.system_id.is_some() && self.name.is_some()
    }
}

impl Serialize for LightweightEntityRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let has_extended_summary = self.system_id.is_some()
            || self.name.is_some()
            || self.description.is_some()
            || self.parent_id.is_some()
            || self.line_color_override.is_some()
            || self.naming_root
            || self.naming_delimiter != "/"
            || self.route_methods.is_some()
            || !self.tech_ids.is_empty()
            || !self.database_columns.is_empty();

        if has_extended_summary {
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct LightweightEntityRefObject<'a> {
                entity_type_id: &'a str,
                #[serde(skip_serializing_if = "Option::is_none")]
                system_id: Option<i64>,
                #[serde(skip_serializing_if = "Option::is_none")]
                name: Option<&'a String>,
                #[serde(skip_serializing_if = "Option::is_none")]
                description: Option<&'a String>,
                #[serde(skip_serializing_if = "Option::is_none")]
                parent_id: Option<i64>,
                file_path: &'a str,
                pos_x: f32,
                pos_y: f32,
                #[serde(skip_serializing_if = "Option::is_none")]
                line_color_override: Option<&'a String>,
                #[serde(skip_serializing_if = "std::ops::Not::not")]
                naming_root: bool,
                #[serde(skip_serializing_if = "is_default_naming_delimiter")]
                naming_delimiter: &'a str,
                #[serde(skip_serializing_if = "Option::is_none")]
                route_methods: Option<&'a String>,
                #[serde(skip_serializing_if = "Vec::is_empty")]
                tech_ids: &'a Vec<i64>,
                #[serde(skip_serializing_if = "Vec::is_empty")]
                database_columns: &'a Vec<DatabaseColumnFile>,
            }

            fn is_default_naming_delimiter(value: &&str) -> bool {
                *value == "/"
            }

            LightweightEntityRefObject {
                entity_type_id: self.entity_type_id.as_str(),
                system_id: self.system_id,
                name: self.name.as_ref(),
                description: self.description.as_ref(),
                parent_id: self.parent_id,
                file_path: self.file_path.as_str(),
                pos_x: self.pos_x,
                pos_y: self.pos_y,
                line_color_override: self.line_color_override.as_ref(),
                naming_root: self.naming_root,
                naming_delimiter: self.naming_delimiter.as_str(),
                route_methods: self.route_methods.as_ref(),
                tech_ids: &self.tech_ids,
                database_columns: &self.database_columns,
            }
            .serialize(serializer)
        } else {
            (
                self.entity_type_id.as_str(),
                self.file_path.as_str(),
                self.pos_x,
                self.pos_y,
            )
                .serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for LightweightEntityRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = LightweightEntityRefRepr::deserialize(deserializer)?;
        let entity = match value {
            LightweightEntityRefRepr::Tuple4((entity_type_id, file_path, pos_x, pos_y)) => Self {
                entity_type_id: normalize_entity_type_id(entity_type_id),
                system_id: None,
                name: None,
                description: None,
                parent_id: None,
                file_path: normalize_manifest_relative_path(file_path.as_str()),
                pos_x,
                pos_y,
                line_color_override: None,
                naming_root: false,
                naming_delimiter: default_naming_delimiter(),
                route_methods: None,
                tech_ids: Vec::new(),
                database_columns: Vec::new(),
            },
            LightweightEntityRefRepr::Tuple3((file_path, pos_x, pos_y)) => Self {
                entity_type_id: default_entity_type_id(),
                system_id: None,
                name: None,
                description: None,
                parent_id: None,
                file_path: normalize_manifest_relative_path(file_path.as_str()),
                pos_x,
                pos_y,
                line_color_override: None,
                naming_root: false,
                naming_delimiter: default_naming_delimiter(),
                route_methods: None,
                tech_ids: Vec::new(),
                database_columns: Vec::new(),
            },
            LightweightEntityRefRepr::Object {
                entity_type_id,
                system_id,
                name,
                description,
                parent_id,
                file_path,
                pos_x,
                pos_y,
                line_color_override,
                naming_root,
                naming_delimiter,
                route_methods,
                tech_ids,
                database_columns,
            } => Self {
                entity_type_id: normalize_entity_type_id(entity_type_id),
                system_id,
                name: name.map(|value| value.trim().to_owned()).filter(|value| !value.is_empty()),
                description,
                parent_id,
                file_path: normalize_manifest_relative_path(file_path.as_str()),
                pos_x,
                pos_y,
                line_color_override,
                naming_root,
                naming_delimiter: if naming_delimiter.trim().is_empty() {
                    default_naming_delimiter()
                } else {
                    naming_delimiter
                },
                route_methods: route_methods
                    .map(|value| value.trim().to_owned())
                    .filter(|value| !value.is_empty()),
                tech_ids,
                database_columns,
            },
        };

        Ok(entity)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LightweightProjectFile {
    #[serde(default = "default_lightweight_project_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub entities: Vec<LightweightEntityRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemFile {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub parent_id: Option<i64>,
    pub calculated_name: String,
    pub map_x: Option<f32>,
    pub map_y: Option<f32>,
    pub line_color_override: Option<String>,
    pub naming_root: bool,
    pub naming_delimiter: String,
    pub system_type: String,
    pub route_methods: Option<String>,
    pub tech_ids: Vec<i64>,
    pub notes: Vec<SystemNoteFile>,
    pub database_columns: Vec<DatabaseColumnFile>,
}

impl LightweightEntityRef {
    pub fn from_system_file(file_path: impl Into<String>, pos_x: f32, pos_y: f32, system: &SystemFile) -> Self {
        Self {
            entity_type_id: normalize_entity_type_id(Some(system.system_type.clone())),
            system_id: Some(system.id),
            name: Some(system.name.clone()),
            description: Some(system.description.clone()),
            parent_id: system.parent_id,
            file_path: normalize_manifest_relative_path(file_path.into().as_str()),
            pos_x,
            pos_y,
            line_color_override: system.line_color_override.clone(),
            naming_root: system.naming_root,
            naming_delimiter: if system.naming_delimiter.trim().is_empty() {
                default_naming_delimiter()
            } else {
                system.naming_delimiter.clone()
            },
            route_methods: system.route_methods.clone(),
            tech_ids: system.tech_ids.clone(),
            database_columns: system.database_columns.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemNoteFile {
    pub id: i64,
    pub body: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseColumnFile {
    pub position: i64,
    pub column_name: String,
    pub column_type: String,
    pub constraints: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InteractionFile {
    pub id: i64,
    pub source_system_id: i64,
    pub target_system_id: i64,
    pub label: String,
    pub note: String,
    pub kind: String,
    pub source_column_name: Option<String>,
    pub target_column_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        collect_interaction_paths_from_root, load_filesystem_project_manifest,
        LightweightEntityRef, LightweightProjectFile, ProjectFile, ProjectSettings,
        LIGHTWEIGHT_PROJECT_SCHEMA_VERSION,
    };

    fn temp_test_dir(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be valid")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("systems_catalog_{name}_{unique}"));
        std::fs::create_dir_all(&directory).expect("temp test directory should be created");
        directory
    }

    #[test]
    fn lightweight_entity_ref_serializes_to_tuple_array() {
        let entity = LightweightEntityRef::new("api", "systems/orders.json", 12.5, 33.0);
        let value = serde_json::to_value(entity).expect("entity should serialize");

        assert_eq!(
            value,
            serde_json::json!(["api", "systems/orders.json", 12.5, 33.0])
        );
    }

    #[test]
    fn lightweight_entity_ref_defaults_type_for_three_tuple_format() {
        let value = serde_json::json!(["systems/orders.json", 5.0, 8.0]);
        let entity: LightweightEntityRef =
            serde_json::from_value(value).expect("3-tuple should deserialize");

        assert_eq!(entity.entity_type_id, "service");
        assert_eq!(entity.file_path, "systems/orders.json");
        assert_eq!(entity.pos_x, 5.0);
        assert_eq!(entity.pos_y, 8.0);
    }

    #[test]
    fn lightweight_entity_ref_defaults_type_when_missing_or_blank() {
        let missing_type_value = serde_json::json!({
            "filePath": "systems/users.json",
            "posX": 1.0,
            "posY": 2.0
        });
        let missing_type: LightweightEntityRef =
            serde_json::from_value(missing_type_value).expect("object should deserialize");

        assert_eq!(missing_type.entity_type_id, "service");

        let blank_type_value = serde_json::json!(["   ", "systems/users.json", 1.0, 2.0]);
        let blank_type: LightweightEntityRef =
            serde_json::from_value(blank_type_value).expect("tuple should deserialize");

        assert_eq!(blank_type.entity_type_id, "service");
    }

    #[test]
    fn lightweight_entity_ref_normalizes_windows_path_separators() {
        let entity = LightweightEntityRef::new("api", "systems\\orders\\root.json", 1.0, 2.0);
        assert_eq!(entity.file_path, "systems/orders/root.json");

        let tuple_value = serde_json::json!(["api", "systems\\orders\\leaf.json", 3.0, 4.0]);
        let tuple_entity: LightweightEntityRef =
            serde_json::from_value(tuple_value).expect("tuple should deserialize");
        assert_eq!(tuple_entity.file_path, "systems/orders/leaf.json");
    }

    #[test]
    fn lightweight_project_file_defaults_schema_version() {
        let project: LightweightProjectFile =
            serde_json::from_value(serde_json::json!({ "entities": [] }))
                .expect("project should deserialize");

        assert_eq!(project.schema_version, LIGHTWEIGHT_PROJECT_SCHEMA_VERSION);
    }

    #[test]
    fn collect_interaction_paths_returns_sorted_json_files_only() {
        let root = temp_test_dir("interactions_sorted");
        let interactions_root = root.join("interactions");
        std::fs::create_dir_all(&interactions_root).expect("interactions directory should exist");
        std::fs::write(interactions_root.join("b.json"), b"{}")
            .expect("json file should be written");
        std::fs::write(interactions_root.join("a.json"), b"{}")
            .expect("json file should be written");
        std::fs::write(interactions_root.join("skip.txt"), b"not json")
            .expect("text file should be written");

        let paths = collect_interaction_paths_from_root(&root);
        assert_eq!(paths, vec!["interactions/a.json", "interactions/b.json"]);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn load_filesystem_project_manifest_prefers_legacy_project_file() {
        let root = temp_test_dir("legacy_manifest");
        let legacy_project = ProjectFile {
            schema_version: 1,
            systems_paths: vec!["systems/root_api__1.json".to_owned()],
            interactions_paths: vec!["interactions/1__to__2__99.json".to_owned()],
            tech_catalog: Vec::new(),
            zones: Vec::new(),
            zone_offsets: Vec::new(),
            settings: ProjectSettings {
                autosave_enabled: false,
                manage_system_json_hierarchy: true,
                has_git: true,
                map_zoom: 0.8,
                map_pan_x: 2.0,
                map_pan_y: -2.0,
                map_world_width: 9000.0,
                map_world_height: 9000.0,
                snap_to_grid: true,
            },
        };

        std::fs::write(
            root.join("Project.json"),
            serde_json::to_vec_pretty(&legacy_project).expect("legacy project should serialize"),
        )
        .expect("legacy project should be written");

        std::fs::write(
            root.join("project.json"),
            serde_json::to_vec_pretty(&LightweightProjectFile {
                schema_version: 2,
                entities: vec![LightweightEntityRef::new(
                    "api",
                    "systems/ignored__1.json",
                    1.0,
                    2.0,
                )],
            })
            .expect("lightweight project should serialize"),
        )
        .expect("lightweight project should be written");

        let loaded = load_filesystem_project_manifest(&root).expect("manifest should load");
        #[cfg(windows)]
        {
            // On case-insensitive filesystems, Project.json and project.json refer to the same
            // path, so the most recent write wins and lightweight content can be loaded.
            assert_eq!(loaded.project.schema_version, 2);
            assert_eq!(
                loaded.project.systems_paths,
                vec!["systems/ignored__1.json".to_owned()]
            );
            assert_eq!(
                loaded
                    .lightweight_positions_by_file_path
                    .get("systems/ignored__1.json"),
                Some(&(1.0, 2.0))
            );
        }

        #[cfg(not(windows))]
        {
            assert_eq!(loaded.project.schema_version, 1);
            assert_eq!(loaded.project.systems_paths, legacy_project.systems_paths);
            assert!(loaded.lightweight_positions_by_file_path.is_empty());
        }

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn load_filesystem_project_manifest_uses_lightweight_when_legacy_missing() {
        let root = temp_test_dir("lightweight_manifest");
        let interactions_root = root.join("interactions");
        std::fs::create_dir_all(&interactions_root).expect("interactions directory should exist");
        std::fs::write(interactions_root.join("first.json"), b"{}")
            .expect("interaction file should be written");
        std::fs::write(interactions_root.join("second.json"), b"{}")
            .expect("interaction file should be written");

        let lightweight_project = LightweightProjectFile {
            schema_version: 2,
            entities: vec![
                LightweightEntityRef::new("api", "systems/root_orders__1.json", 10.0, 20.0),
                LightweightEntityRef::new("service", "systems/root_orders__1.json", 30.0, 40.0),
                LightweightEntityRef::new("database", "systems/root_db__2.json", 50.0, 60.0),
            ],
        };

        std::fs::write(
            root.join("project.json"),
            serde_json::to_vec_pretty(&lightweight_project)
                .expect("lightweight project should serialize"),
        )
        .expect("lightweight project should be written");

        let loaded = load_filesystem_project_manifest(&root).expect("manifest should load");
        assert_eq!(loaded.project.schema_version, 2);
        assert_eq!(
            loaded.project.systems_paths,
            vec![
                "systems/root_orders__1.json".to_owned(),
                "systems/root_db__2.json".to_owned()
            ]
        );
        assert_eq!(loaded.project.tech_catalog.len(), 0);
        assert_eq!(loaded.project.zones.len(), 0);
        assert_eq!(loaded.project.zone_offsets.len(), 0);
        assert_eq!(loaded.project.settings.autosave_enabled, true);
        assert_eq!(
            loaded
                .lightweight_positions_by_file_path
                .get("systems/root_orders__1.json"),
            Some(&(30.0, 40.0))
        );
        assert_eq!(
            loaded
                .lightweight_positions_by_file_path
                .get("systems/root_db__2.json"),
            Some(&(50.0, 60.0))
        );
        assert_eq!(loaded.project.interactions_paths.len(), 2);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn load_filesystem_project_manifest_normalizes_lightweight_entity_paths() {
        let root = temp_test_dir("lightweight_paths_normalized");

        let lightweight_project = LightweightProjectFile {
            schema_version: 2,
            entities: vec![
                LightweightEntityRef::new("api", "systems\\root_orders__1.json", 10.0, 20.0),
                LightweightEntityRef::new("service", "//systems/root_orders__1.json", 30.0, 40.0),
                LightweightEntityRef::new("database", "systems//root_db__2.json", 50.0, 60.0),
            ],
        };

        std::fs::write(
            root.join("project.json"),
            serde_json::to_vec_pretty(&lightweight_project)
                .expect("lightweight project should serialize"),
        )
        .expect("lightweight project should be written");

        let loaded = load_filesystem_project_manifest(&root).expect("manifest should load");
        assert_eq!(
            loaded.project.systems_paths,
            vec![
                "systems/root_orders__1.json".to_owned(),
                "systems/root_db__2.json".to_owned()
            ]
        );
        assert_eq!(
            loaded
                .lightweight_positions_by_file_path
                .get("systems/root_orders__1.json"),
            Some(&(30.0, 40.0))
        );
        assert_eq!(
            loaded
                .lightweight_positions_by_file_path
                .get("systems/root_db__2.json"),
            Some(&(50.0, 60.0))
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn lightweight_project_roundtrip_preserves_all_entity_data() {
        let root = temp_test_dir("roundtrip_entity_data");

        let original_entities = vec![
            LightweightEntityRef::new("api", "systems/orders.json", 10.0, 20.0),
            LightweightEntityRef::new("service", "systems/users.json", 30.0, 40.0),
            LightweightEntityRef::new("database", "systems/inventory.json", 50.0, 60.0),
            LightweightEntityRef::new("zone", "systems/checkout.json", 70.0, 80.0),
        ];

        let original_project = LightweightProjectFile {
            schema_version: LIGHTWEIGHT_PROJECT_SCHEMA_VERSION,
            entities: original_entities.clone(),
        };

        let project_bytes =
            serde_json::to_vec_pretty(&original_project).expect("project should serialize");
        std::fs::write(root.join("project.json"), &project_bytes)
            .expect("project file should be written");

        let loaded = load_filesystem_project_manifest(&root).expect("manifest should load");

        assert_eq!(loaded.project.schema_version, LIGHTWEIGHT_PROJECT_SCHEMA_VERSION);
        assert_eq!(loaded.project.systems_paths.len(), 4);
        assert!(loaded.project.systems_paths.contains(&"systems/orders.json".to_owned()));
        assert!(loaded.project.systems_paths.contains(&"systems/users.json".to_owned()));
        assert!(loaded.project.systems_paths.contains(&"systems/inventory.json".to_owned()));
        assert!(loaded.project.systems_paths.contains(&"systems/checkout.json".to_owned()));

        assert_eq!(
            loaded.lightweight_positions_by_file_path.get("systems/orders.json"),
            Some(&(10.0, 20.0))
        );
        assert_eq!(
            loaded.lightweight_positions_by_file_path.get("systems/users.json"),
            Some(&(30.0, 40.0))
        );
        assert_eq!(
            loaded.lightweight_positions_by_file_path.get("systems/inventory.json"),
            Some(&(50.0, 60.0))
        );
        assert_eq!(
            loaded.lightweight_positions_by_file_path.get("systems/checkout.json"),
            Some(&(70.0, 80.0))
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn lightweight_project_roundtrip_deduplicates_paths_and_preserves_last_position() {
        let root = temp_test_dir("roundtrip_dedupe");

        let original_project = LightweightProjectFile {
            schema_version: LIGHTWEIGHT_PROJECT_SCHEMA_VERSION,
            entities: vec![
                LightweightEntityRef::new("api", "systems/orders.json", 10.0, 20.0),
                LightweightEntityRef::new("service", "systems/orders.json", 30.0, 40.0),
                LightweightEntityRef::new("database", "systems/orders.json", 50.0, 60.0),
            ],
        };

        let project_bytes =
            serde_json::to_vec_pretty(&original_project).expect("project should serialize");
        std::fs::write(root.join("project.json"), &project_bytes)
            .expect("project file should be written");

        let loaded = load_filesystem_project_manifest(&root).expect("manifest should load");

        assert_eq!(loaded.project.systems_paths.len(), 1);
        assert_eq!(loaded.project.systems_paths[0], "systems/orders.json");

        assert_eq!(
            loaded.lightweight_positions_by_file_path.get("systems/orders.json"),
            Some(&(50.0, 60.0)),
            "should preserve the last position when multiple entities reference the same file"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn lightweight_project_roundtrip_handles_empty_entity_list() {
        let root = temp_test_dir("roundtrip_empty");

        let original_project = LightweightProjectFile {
            schema_version: LIGHTWEIGHT_PROJECT_SCHEMA_VERSION,
            entities: Vec::new(),
        };

        let project_bytes =
            serde_json::to_vec_pretty(&original_project).expect("project should serialize");
        std::fs::write(root.join("project.json"), &project_bytes)
            .expect("project file should be written");

        let loaded = load_filesystem_project_manifest(&root).expect("manifest should load");

        assert_eq!(loaded.project.schema_version, LIGHTWEIGHT_PROJECT_SCHEMA_VERSION);
        assert_eq!(loaded.project.systems_paths.len(), 0);
        assert_eq!(loaded.lightweight_positions_by_file_path.len(), 0);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn lightweight_project_roundtrip_filters_empty_file_paths() {
        let root = temp_test_dir("roundtrip_empty_paths");

        let json_text = r#"{
            "schemaVersion": 2,
            "entities": [
                ["api", "systems/valid.json", 10.0, 20.0],
                ["service", "", 30.0, 40.0],
                ["database", "   ", 50.0, 60.0],
                ["api", "systems/another.json", 70.0, 80.0]
            ]
        }"#;

        std::fs::write(root.join("project.json"), json_text)
            .expect("project file should be written");

        let loaded = load_filesystem_project_manifest(&root).expect("manifest should load");

        assert_eq!(loaded.project.systems_paths.len(), 2);
        assert!(loaded.project.systems_paths.contains(&"systems/valid.json".to_owned()));
        assert!(loaded.project.systems_paths.contains(&"systems/another.json".to_owned()));
        assert_eq!(loaded.lightweight_positions_by_file_path.len(), 2);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn lightweight_project_roundtrip_preserves_entity_types_through_serialization() {
        let root = temp_test_dir("roundtrip_entity_types");

        let entity_types = ["api", "service", "database", "zone", "step_processor"];
        let entities = entity_types
            .iter()
            .enumerate()
            .map(|(index, entity_type)| {
                LightweightEntityRef::new(
                    *entity_type,
                    format!("systems/entity_{index}.json"),
                    (index * 10) as f32,
                    (index * 20) as f32,
                )
            })
            .collect();

        let original_project = LightweightProjectFile {
            schema_version: LIGHTWEIGHT_PROJECT_SCHEMA_VERSION,
            entities,
        };

        let project_bytes =
            serde_json::to_vec_pretty(&original_project).expect("project should serialize");
        std::fs::write(root.join("project.json"), &project_bytes)
            .expect("project file should be written");

        let reloaded_text = std::fs::read_to_string(root.join("project.json"))
            .expect("project file should be readable");
        let reloaded_project: LightweightProjectFile =
            serde_json::from_str(&reloaded_text).expect("project should deserialize");

        assert_eq!(reloaded_project.entities.len(), entity_types.len());
        for (index, entity_type) in entity_types.iter().enumerate() {
            assert_eq!(reloaded_project.entities[index].entity_type_id, *entity_type);
            assert_eq!(
                reloaded_project.entities[index].file_path,
                format!("systems/entity_{index}.json")
            );
            assert_eq!(reloaded_project.entities[index].pos_x, (index * 10) as f32);
            assert_eq!(reloaded_project.entities[index].pos_y, (index * 20) as f32);
        }

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn lightweight_project_roundtrip_handles_special_characters_in_paths() {
        let root = temp_test_dir("roundtrip_special_chars");

        let entities = vec![
            LightweightEntityRef::new("api", "systems/order-service.json", 10.0, 20.0),
            LightweightEntityRef::new("service", "systems/user_auth.json", 30.0, 40.0),
            LightweightEntityRef::new("database", "systems/inv.db.json", 50.0, 60.0),
        ];

        let original_project = LightweightProjectFile {
            schema_version: LIGHTWEIGHT_PROJECT_SCHEMA_VERSION,
            entities,
        };

        let project_bytes =
            serde_json::to_vec_pretty(&original_project).expect("project should serialize");
        std::fs::write(root.join("project.json"), &project_bytes)
            .expect("project file should be written");

        let loaded = load_filesystem_project_manifest(&root).expect("manifest should load");

        assert_eq!(loaded.project.systems_paths.len(), 3);
        assert!(loaded.project.systems_paths.contains(&"systems/order-service.json".to_owned()));
        assert!(loaded.project.systems_paths.contains(&"systems/user_auth.json".to_owned()));
        assert!(loaded.project.systems_paths.contains(&"systems/inv.db.json".to_owned()));

        let _ = std::fs::remove_dir_all(root);
    }
}
