mod api;
mod database;
mod service;
mod step_processor;
mod zone;

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

use eframe::egui;
use serde::Deserialize;

use crate::app::SystemsCatalogApp;
use crate::models::SystemRecord;
use crate::project_store::SystemFile;

pub(crate) use zone::ZoneRenderEntity;

pub(crate) struct EntitySelectableInputs {
    pub can_select_parent: bool,
    pub can_select_route_methods: bool,
    pub can_select_database_columns: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct EntityTypeOption {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone)]
struct EntityTypePluginDefinition {
    key: String,
    label: String,
    base_type: String,
    eager_map_content: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EntityTypeManifestFile {
    key: String,
    label: Option<String>,
    base_type: Option<String>,
    #[serde(default)]
    eager_map_content: bool,
}

static ENTITY_TYPE_PLUGINS: OnceLock<HashMap<String, EntityTypePluginDefinition>> = OnceLock::new();

fn normalize_type_key(value: &str) -> String {
    let normalized = value
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
        .replace(' ', "_");
    if normalized.is_empty() {
        "service".to_owned()
    } else if normalized == "route" {
        "api".to_owned()
    } else {
        normalized
    }
}

fn built_in_entity_type_options() -> Vec<EntityTypeOption> {
    vec![
        EntityTypeOption {
            key: "service".to_owned(),
            label: "Service".to_owned(),
        },
        EntityTypeOption {
            key: "api".to_owned(),
            label: "API".to_owned(),
        },
        EntityTypeOption {
            key: "database".to_owned(),
            label: "Database".to_owned(),
        },
        EntityTypeOption {
            key: "step_processor".to_owned(),
            label: "Step Processor".to_owned(),
        },
    ]
}

fn load_entity_type_plugins() -> HashMap<String, EntityTypePluginDefinition> {
    let mut plugins = HashMap::new();
    let root = Path::new("assets").join("entity_types");
    let Ok(entries) = std::fs::read_dir(root) else {
        return plugins;
    };

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

        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_str::<EntityTypeManifestFile>(&contents) else {
            continue;
        };

        let key = normalize_type_key(manifest.key.as_str());
        let base_type = normalize_type_key(manifest.base_type.as_deref().unwrap_or("service"));
        let label = manifest
            .label
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| key.replace('_', " "));

        if key == "service" || key == "api" || key == "database" || key == "step_processor" {
            continue;
        }

        plugins.insert(
            key.clone(),
            EntityTypePluginDefinition {
                key,
                label,
                base_type,
                eager_map_content: manifest.eager_map_content,
            },
        );
    }

    plugins
}

fn entity_type_plugins() -> &'static HashMap<String, EntityTypePluginDefinition> {
    ENTITY_TYPE_PLUGINS.get_or_init(load_entity_type_plugins)
}

fn resolved_base_entity_key(system_type: &str) -> String {
    let key = normalize_type_key(system_type);
    if let Some(plugin) = entity_type_plugins().get(&key) {
        return plugin.base_type.clone();
    }
    key
}

pub(crate) trait SystemRenderEntity {
    fn entity_key(&self) -> &'static str;
    fn selectable_inputs(&self) -> EntitySelectableInputs;
    fn requires_eager_map_content(&self) -> bool {
        false
    }
    fn render_map_label(&self, app: &SystemsCatalogApp, system: &SystemRecord) -> String;
    fn render_details_panel(
        &self,
        app: &mut SystemsCatalogApp,
        ui: &mut egui::Ui,
        system: &SystemRecord,
    );
    fn apply_system_file_schema(
        &self,
        app: &SystemsCatalogApp,
        system: &SystemRecord,
        system_file: &mut SystemFile,
    );
    fn normalize_loaded_system_file(&self, system_file: &mut SystemFile);
}

static SERVICE_ENTITY: service::ServiceRenderEntity = service::ServiceRenderEntity;
static API_ENTITY: api::ApiRenderEntity = api::ApiRenderEntity;
static DATABASE_ENTITY: database::DatabaseRenderEntity = database::DatabaseRenderEntity;
static STEP_PROCESSOR_ENTITY: step_processor::StepProcessorRenderEntity =
    step_processor::StepProcessorRenderEntity;
static ZONE_ENTITY: zone::DefaultZoneRenderEntity = zone::DefaultZoneRenderEntity;

pub(crate) fn map_icon_for_system_type(system_type: &str) -> &'static str {
    match resolved_base_entity_key(system_type).as_str() {
        "route" | "api" => egui_material_icons::icons::ICON_ROUTE,
        "database" => egui_material_icons::icons::ICON_DATABASE,
        "step_processor" => egui_material_icons::icons::ICON_ROUTE,
        _ => egui_material_icons::icons::ICON_ROUTE,
    }
}

impl SystemsCatalogApp {
    pub(crate) fn supported_system_types(&self) -> Vec<EntityTypeOption> {
        let mut options = built_in_entity_type_options();
        let mut seen = options
            .iter()
            .map(|option| option.key.clone())
            .collect::<HashSet<_>>();

        let mut plugin_options = entity_type_plugins()
            .values()
            .filter(|plugin| !seen.contains(&plugin.key))
            .map(|plugin| EntityTypeOption {
                key: plugin.key.clone(),
                label: plugin.label.clone(),
            })
            .collect::<Vec<_>>();
        plugin_options.sort_by(|left, right| left.label.cmp(&right.label));

        for option in plugin_options {
            seen.insert(option.key.clone());
            options.push(option);
        }

        options
    }

    pub(crate) fn system_type_display_label(&self, system_type: &str) -> String {
        let key = normalize_type_key(system_type);
        self.supported_system_types()
            .into_iter()
            .find(|option| option.key == key)
            .map(|option| option.label)
            .unwrap_or_else(|| key)
    }

    pub(crate) fn entity_supports_row_references_for_type(&self, system_type: &str) -> bool {
        self.system_entity_for_type(system_type)
            .selectable_inputs()
            .can_select_database_columns
    }

    pub(crate) fn entity_requires_eager_map_content_for_type(&self, system_type: &str) -> bool {
        let normalized = normalize_type_key(system_type);
        if let Some(plugin) = entity_type_plugins().get(&normalized) {
            if plugin.eager_map_content {
                return true;
            }
        }

        self.system_entity_for_type(system_type)
            .requires_eager_map_content()
    }

    pub(crate) fn system_entity_for_type(
        &self,
        system_type: &str,
    ) -> &'static dyn SystemRenderEntity {
        match resolved_base_entity_key(system_type).as_str() {
            "api" => &API_ENTITY,
            "database" => &DATABASE_ENTITY,
            "step_processor" => &STEP_PROCESSOR_ENTITY,
            _ => &SERVICE_ENTITY,
        }
    }

    pub(crate) fn system_entity_for(
        &self,
        system: &SystemRecord,
    ) -> &'static dyn SystemRenderEntity {
        self.system_entity_for_type(system.system_type.as_str())
    }

    pub(crate) fn zone_render_entity(&self) -> &'static dyn ZoneRenderEntity {
        &ZONE_ENTITY
    }

    pub(crate) fn apply_system_file_entity_schema(
        &self,
        system: &SystemRecord,
        system_file: &mut SystemFile,
    ) {
        self.system_entity_for(system)
            .apply_system_file_schema(self, system, system_file);
    }

    pub(crate) fn normalize_loaded_system_file_for_entity(&self, system_file: &mut SystemFile) {
        self.system_entity_for_type(system_file.system_type.as_str())
            .normalize_loaded_system_file(system_file);
    }
}
