use eframe::egui;

use crate::app::entities::{map_icon_for_system_type, EntitySelectableInputs, SystemRenderEntity};
use crate::app::SystemsCatalogApp;
use crate::models::{DatabaseColumnInput, SystemRecord};
use crate::project_store::{DatabaseColumnFile, SystemFile};

pub(crate) struct StepProcessorRenderEntity;

impl SystemRenderEntity for StepProcessorRenderEntity {
    fn entity_key(&self) -> &'static str {
        "step_processor"
    }

    fn selectable_inputs(&self) -> EntitySelectableInputs {
        EntitySelectableInputs {
            can_select_parent: true,
            can_select_route_methods: false,
            can_select_database_columns: true,
        }
    }

    fn render_map_label(&self, app: &SystemsCatalogApp, system: &SystemRecord) -> String {
        let prefix = map_icon_for_system_type(system.system_type.as_str());
        let title = if prefix.is_empty() {
            system.name.clone()
        } else {
            format!("{prefix} {}", system.name)
        };

        let Some(steps) = app.database_columns_by_system.get(&system.id) else {
            return title;
        };

        if steps.is_empty() {
            return title;
        }

        let mut rows = Vec::with_capacity(steps.len() + 1);
        rows.push(title);
        for (index, step) in steps.iter().enumerate() {
            let step_name = step.column_name.trim();
            if step_name.is_empty() {
                continue;
            }
            rows.push(format!("{} ) {}", index + 1, step_name));
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
        ui.label("Processing steps");
        ui.small("Click a step on the map card to wire step-level interactions");

        let mut row_to_remove: Option<usize> = None;
        let mut row_to_move: Option<(usize, usize)> = None;
        let total_steps = app.selected_database_columns.len();
        for (index, step) in app.selected_database_columns.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{}.", index + 1));
                ui.add(
                    egui::TextEdit::singleline(&mut step.column_name)
                        .hint_text("step name")
                        .desired_width(280.0),
                );

                if ui
                    .add_enabled(index > 0, egui::Button::new("Up"))
                    .clicked()
                {
                    row_to_move = Some((index, index - 1));
                }
                if ui
                    .add_enabled(
                        index + 1 < total_steps,
                        egui::Button::new("Down"),
                    )
                    .clicked()
                {
                    row_to_move = Some((index, index + 1));
                }

                if ui.button("Remove").clicked() {
                    row_to_remove = Some(index);
                }
            });
        }

        if let Some((from, to)) = row_to_move {
            app.selected_database_columns.swap(from, to);
        }

        if let Some(index) = row_to_remove {
            app.selected_database_columns.remove(index);
        }

        for (position, step) in app.selected_database_columns.iter_mut().enumerate() {
            step.position = position as i64;
            if step.column_type.trim().is_empty() {
                step.column_type = "step".to_owned();
            }
            step.constraints = None;
        }

        ui.horizontal(|ui| {
            if ui.button("Add step").clicked() {
                app.selected_database_columns.push(DatabaseColumnInput {
                    position: app.selected_database_columns.len() as i64,
                    column_name: String::new(),
                    column_type: "step".to_owned(),
                    constraints: None,
                });
            }

            if app.selected_database_columns.is_empty() && ui.button("Add sample steps").clicked() {
                app.selected_database_columns = vec![
                    DatabaseColumnInput {
                        position: 0,
                        column_name: "get users".to_owned(),
                        column_type: "step".to_owned(),
                        constraints: None,
                    },
                    DatabaseColumnInput {
                        position: 1,
                        column_name: "updateDateLastContacted".to_owned(),
                        column_type: "step".to_owned(),
                        constraints: None,
                    },
                    DatabaseColumnInput {
                        position: 2,
                        column_name: "Check for outstanding balance".to_owned(),
                        column_type: "step".to_owned(),
                        constraints: None,
                    },
                    DatabaseColumnInput {
                        position: 3,
                        column_name: "Send Confetti".to_owned(),
                        column_type: "step".to_owned(),
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
            .map(|step| DatabaseColumnFile {
                position: step.position,
                column_name: step.column_name,
                column_type: "step".to_owned(),
                constraints: None,
            })
            .collect();
    }

    fn normalize_loaded_system_file(&self, system_file: &mut SystemFile) {
        system_file.route_methods = None;
        for step in &mut system_file.database_columns {
            if step.column_type.trim().is_empty() {
                step.column_type = "step".to_owned();
            }
            step.constraints = None;
        }
    }
}
