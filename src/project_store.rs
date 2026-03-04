use serde::{Deserialize, Serialize};

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
    pub map_zoom: f32,
    pub map_pan_x: f32,
    pub map_pan_y: f32,
    pub map_world_width: f32,
    pub map_world_height: f32,
    pub snap_to_grid: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemNoteFile {
    pub id: i64,
    pub body: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
