use eframe::egui;

use crate::app::entities::{map_icon_for_system_type, EntitySelectableInputs, SystemRenderEntity};
use crate::app::SystemsCatalogApp;
use crate::models::SystemRecord;
use crate::project_store::SystemFile;

pub(crate) struct ServiceRenderEntity;

impl SystemRenderEntity for ServiceRenderEntity {
    fn entity_key(&self) -> &'static str {
        "service"
    }

    fn selectable_inputs(&self) -> EntitySelectableInputs {
        EntitySelectableInputs {
            can_select_parent: true,
            can_select_route_methods: false,
            can_select_database_columns: false,
        }
    }

    fn render_map_label(&self, _app: &SystemsCatalogApp, system: &SystemRecord) -> String {
        let prefix = map_icon_for_system_type(system.system_type.as_str());
        if prefix.is_empty() {
            system.name.clone()
        } else {
            format!("{prefix} {}", system.name)
        }
    }

    fn render_details_panel(
        &self,
        _app: &mut SystemsCatalogApp,
        _ui: &mut egui::Ui,
        _system: &SystemRecord,
    ) {
    }

    fn apply_system_file_schema(
        &self,
        _app: &SystemsCatalogApp,
        _system: &SystemRecord,
        system_file: &mut SystemFile,
    ) {
        system_file.route_methods = None;
        system_file.database_columns.clear();
    }

    fn normalize_loaded_system_file(&self, system_file: &mut SystemFile) {
        system_file.route_methods = None;
        system_file.database_columns.clear();
    }
}
