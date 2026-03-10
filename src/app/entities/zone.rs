use eframe::egui::{self, Color32, RichText};

use crate::app::SystemsCatalogApp;
use crate::models::ZoneRecord;

pub(crate) trait ZoneRenderEntity {
    fn entity_key(&self) -> &'static str;
    fn render_name_for_map(
        &self,
        app: &SystemsCatalogApp,
        zone: &ZoneRecord,
        zone_overview_mode: bool,
        representative_system_id: Option<i64>,
    ) -> String;
    fn fill_color_for_map(&self, app: &SystemsCatalogApp, zone: &ZoneRecord) -> Color32;
    fn render_details_panel(&self, app: &mut SystemsCatalogApp, ui: &mut egui::Ui);
}

pub(crate) struct DefaultZoneRenderEntity;

impl ZoneRenderEntity for DefaultZoneRenderEntity {
    fn entity_key(&self) -> &'static str {
        "zone"
    }

    fn render_name_for_map(
        &self,
        app: &SystemsCatalogApp,
        zone: &ZoneRecord,
        zone_overview_mode: bool,
        representative_system_id: Option<i64>,
    ) -> String {
        if zone.minimized && representative_system_id.is_some() && !zone_overview_mode {
            representative_system_id
                .map(|id| app.system_name_by_id(id))
                .unwrap_or_else(|| zone.name.clone())
        } else {
            zone.name.clone()
        }
    }

    fn fill_color_for_map(&self, app: &SystemsCatalogApp, zone: &ZoneRecord) -> Color32 {
        zone.color
            .as_deref()
            .and_then(SystemsCatalogApp::color_from_setting_value)
            .unwrap_or_else(|| {
                let _ = app;
                Color32::from_rgba_unmultiplied(96, 140, 255, 40)
            })
    }

    fn render_details_panel(&self, app: &mut SystemsCatalogApp, ui: &mut egui::Ui) {
        let Some(zone_id) = app.selected_zone_id else {
            return;
        };

        ui.set_max_width(ui.available_width());
        ui.heading("Zone Details");

        if let Some(zone) = app.zones.iter().find(|z| z.id == zone_id) {
            ui.label(RichText::new(zone.name.clone()).strong());
        }

        ui.separator();

        ui.label("Name");
        let name_response = ui.text_edit_singleline(&mut app.selected_zone_name);
        if name_response.lost_focus() {
            app.update_selected_zone_properties();
        }

        ui.horizontal(|ui| {
            ui.label("Color");
            if ui
                .color_edit_button_srgba(&mut app.selected_zone_color)
                .changed()
            {
                app.update_selected_zone_properties();
            }
        });

        let mut render_priority = app.selected_zone_render_priority;
        if ui
            .add(
                egui::DragValue::new(&mut render_priority)
                    .speed(1.0)
                    .prefix("Render priority "),
            )
            .changed()
        {
            app.selected_zone_render_priority = render_priority;
            app.update_selected_zone_properties();
        }

        ui.separator();

        if let Some(zone_id) = app.selected_zone_id {
            let parent_label = app
                .selected_zone_parent_zone_id
                .and_then(|id| {
                    app.zones
                        .iter()
                        .find(|zone| zone.id == id)
                        .map(|zone| zone.name.clone())
                })
                .unwrap_or_else(|| "No parent zone".to_owned());

            let previous_parent = app.selected_zone_parent_zone_id;
            let parent_candidates = app.zone_parent_candidates(zone_id);

            egui::ComboBox::from_id_source(("zone_parent_zone_sidebar", zone_id))
                .selected_text(parent_label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut app.selected_zone_parent_zone_id,
                        None,
                        "No parent zone",
                    );
                    for (candidate_id, candidate_name) in parent_candidates {
                        ui.selectable_value(
                            &mut app.selected_zone_parent_zone_id,
                            Some(candidate_id),
                            candidate_name,
                        );
                    }
                });

            if app.selected_zone_parent_zone_id != previous_parent {
                app.update_selected_zone_properties();
            }
        }

        if let Some(zone_id) = app.selected_zone_id {
            let representative_candidates = app.zone_representative_candidates(zone_id);
            let unique_common = app.zone_unique_common_ancestor_system_id(zone_id);
            let representative_locked = unique_common.is_some();

            let representative_label = unique_common
                .or(app.selected_zone_representative_system_id)
                .map(|id| app.system_dropdown_label(id))
                .unwrap_or_else(|| "Choose representative".to_owned());
            let representative_label =
                SystemsCatalogApp::clamp_text_to_width(&representative_label, ui.available_width());

            egui::ComboBox::from_id_source(("zone_representative_sidebar", zone_id))
                .selected_text(representative_label)
                .show_ui(ui, |ui| {
                    if representative_locked {
                        if let Some(ancestor_id) = unique_common {
                            let ancestor_name = app.system_dropdown_label(ancestor_id);
                            ui.selectable_value(
                                &mut app.selected_zone_representative_system_id,
                                Some(ancestor_id),
                                ancestor_name,
                            );
                        }
                    } else {
                        ui.selectable_value(
                            &mut app.selected_zone_representative_system_id,
                            None,
                            "Choose representative",
                        );
                        for candidate_id in representative_candidates {
                            let candidate_name = app.system_dropdown_label(candidate_id);
                            ui.selectable_value(
                                &mut app.selected_zone_representative_system_id,
                                Some(candidate_id),
                                candidate_name,
                            );
                        }
                    }
                });

            if representative_locked && app.selected_zone_representative_system_id != unique_common
            {
                app.selected_zone_representative_system_id = unique_common;
                app.update_selected_zone_properties();
            }

            if !representative_locked
                && app
                    .zones
                    .iter()
                    .find(|zone| zone.id == zone_id)
                    .map(|zone| zone.representative_system_id)
                    != Some(app.selected_zone_representative_system_id)
            {
                app.update_selected_zone_properties();
            }
        }

        ui.separator();

        ui.horizontal(|ui| {
            if let Some(zone_id) = app.selected_zone_id {
                let minimize_label = if app.selected_zone_minimized {
                    "Maximize"
                } else {
                    "Minimize"
                };
                if ui.button(minimize_label).clicked() {
                    app.toggle_zone_minimized(zone_id);
                }
            }

            if ui.button("Delete Zone").clicked() {
                app.delete_selected_zone();
            }

            if ui.button("Deselect Zone").clicked() {
                app.selected_zone_id = None;
                app.selected_zone_name.clear();
                app.selected_zone_render_priority = 1;
                app.selected_zone_parent_zone_id = None;
                app.selected_zone_minimized = false;
                app.selected_zone_representative_system_id = None;
            }
        });

        ui.separator();
    }
}
