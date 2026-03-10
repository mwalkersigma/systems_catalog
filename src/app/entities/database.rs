use eframe::egui;

use crate::app::entities::{map_icon_for_system_type, EntitySelectableInputs, SystemRenderEntity};
use crate::app::SystemsCatalogApp;
use crate::models::{DatabaseColumnInput, SystemRecord};
use crate::project_store::{DatabaseColumnFile, SystemFile};

pub(crate) struct DatabaseRenderEntity;

impl SystemRenderEntity for DatabaseRenderEntity {
    fn entity_key(&self) -> &'static str {
        "database"
    }

    fn selectable_inputs(&self) -> EntitySelectableInputs {
        EntitySelectableInputs {
            can_select_parent: true,
            can_select_route_methods: false,
            can_select_database_columns: true,
        }
    }

    fn requires_eager_map_content(&self) -> bool {
        true
    }

    fn render_map_label(&self, app: &SystemsCatalogApp, system: &SystemRecord) -> String {
        let prefix = map_icon_for_system_type(system.system_type.as_str());
        let title = if prefix.is_empty() {
            system.name.clone()
        } else {
            format!("{prefix} {}", system.name)
        };

        let Some(columns) = app.database_columns_by_system.get(&system.id) else {
            return title;
        };

        if columns.is_empty() {
            return title;
        }

        let mut rows = Vec::with_capacity(columns.len() + 2);
        rows.push(title);
        rows.push("column | type | constraints".to_owned());
        for column in columns {
            let mut line = format!("{} | {}", column.column_name, column.column_type);
            if let Some(constraints) = column.constraints.as_deref() {
                let trimmed = constraints.trim();
                if !trimmed.is_empty() {
                    line.push_str(" | ");
                    line.push_str(trimmed);
                }
            }
            rows.push(line);
        }

        rows.join("\n")
    }

    fn render_details_panel(
        &self,
        app: &mut SystemsCatalogApp,
        ui: &mut egui::Ui,
        _system: &SystemRecord,
    ) {
        ui.separator();
        ui.label("Table columns");
        ui.small("Format: name type constraints (optional)");

        let mut row_to_remove: Option<usize> = None;
        for (index, column) in app.selected_database_columns.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut column.column_name)
                        .hint_text("column name")
                        .desired_width(120.0),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut column.column_type)
                        .hint_text("type")
                        .desired_width(90.0),
                );

                let constraints = column.constraints.get_or_insert_with(String::new);
                ui.add(
                    egui::TextEdit::singleline(constraints)
                        .hint_text("constraints")
                        .desired_width(140.0),
                );

                if ui.button("Remove").clicked() {
                    row_to_remove = Some(index);
                }
            });
        }

        if let Some(index) = row_to_remove {
            app.selected_database_columns.remove(index);
        }

        ui.horizontal(|ui| {
            if ui.button("Add column").clicked() {
                app.selected_database_columns.push(DatabaseColumnInput {
                    position: app.selected_database_columns.len() as i64,
                    column_name: String::new(),
                    column_type: "string".to_owned(),
                    constraints: None,
                });
            }

            if app.selected_database_columns.is_empty() && ui.button("Add common starter").clicked()
            {
                app.selected_database_columns = vec![
                    DatabaseColumnInput {
                        position: 0,
                        column_name: "id".to_owned(),
                        column_type: "string".to_owned(),
                        constraints: Some("primary".to_owned()),
                    },
                    DatabaseColumnInput {
                        position: 1,
                        column_name: "name".to_owned(),
                        column_type: "string".to_owned(),
                        constraints: None,
                    },
                    DatabaseColumnInput {
                        position: 2,
                        column_name: "title".to_owned(),
                        column_type: "string".to_owned(),
                        constraints: None,
                    },
                ];
            }
        });
    }

    fn apply_system_file_schema(
        &self,
        app: &SystemsCatalogApp,
        system: &SystemRecord,
        system_file: &mut SystemFile,
    ) {
        system_file.route_methods = None;
        system_file.database_columns = app
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
            .collect();
    }

    fn normalize_loaded_system_file(&self, system_file: &mut SystemFile) {
        system_file.route_methods = None;
    }
}
