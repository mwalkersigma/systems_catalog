mod api;
mod database;
mod service;
mod zone;

use eframe::egui;

use crate::app::SystemsCatalogApp;
use crate::models::SystemRecord;
use crate::project_store::SystemFile;

pub(crate) use zone::ZoneRenderEntity;

pub(crate) struct EntitySelectableInputs {
    pub can_select_parent: bool,
    pub can_select_route_methods: bool,
    pub can_select_database_columns: bool,
}

pub(crate) trait SystemRenderEntity {
    fn entity_key(&self) -> &'static str;
    fn selectable_inputs(&self) -> EntitySelectableInputs;
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
static ZONE_ENTITY: zone::DefaultZoneRenderEntity = zone::DefaultZoneRenderEntity;

pub(crate) fn map_icon_for_system_type(system_type: &str) -> &'static str {
    match system_type {
        "route" | "api" => egui_material_icons::icons::ICON_ROUTE,
        "database" => egui_material_icons::icons::ICON_DATABASE,
        _ => egui_material_icons::icons::ICON_ROUTE,
    }
}

impl SystemsCatalogApp {
    pub(crate) fn system_entity_for_type(&self, system_type: &str) -> &'static dyn SystemRenderEntity {
        match Self::normalize_system_type(system_type).as_str() {
            "api" => &API_ENTITY,
            "database" => &DATABASE_ENTITY,
            _ => &SERVICE_ENTITY,
        }
    }

    pub(crate) fn system_entity_for(&self, system: &SystemRecord) -> &'static dyn SystemRenderEntity {
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
