use std::path::Path;

use eframe::egui::{self, Align, Layout, RichText, Vec2};
use rfd::FileDialog;

use crate::app::{
    AppModal, SidebarTab, SystemsCatalogApp, UpdateCheckState, MAP_MAX_ZOOM, MAP_MIN_ZOOM,
    MAP_WORLD_MAX_SIZE, MAP_WORLD_MIN_SIZE,
};

impl SystemsCatalogApp {
    pub(super) fn render_top_toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Project").clicked() {
                        if self.new_catalog_name.trim().is_empty() {
                            self.new_catalog_name = "new_project".to_owned();
                        }
                        if self.new_catalog_directory.trim().is_empty() {
                            self.new_catalog_directory = self
                                .recent_catalog_paths
                                .first()
                                .and_then(|path| Path::new(path).parent())
                                .map(|path| path.to_string_lossy().to_string())
                                .unwrap_or_else(|| ".".to_owned());
                        }
                        self.open_modal(AppModal::NewCatalogConfirm);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Save Project").clicked() {
                        self.open_modal(AppModal::SaveCatalog);
                        ui.close_menu();
                    }
                    if ui.button("Load Project").clicked() {
                        self.open_modal(AppModal::LoadCatalog);
                        ui.close_menu();
                    }
                    if ui.button("Import DDL File").clicked() {
                        let mut dialog =
                            FileDialog::new().add_filter("DDL", &["sql", "ddl", "txt"]);
                        if !self.current_catalog_path.trim().is_empty() {
                            dialog = dialog.set_directory(self.current_catalog_path.as_str());
                        }
                        if let Some(path) = dialog.pick_file() {
                            self.import_database_tables_from_ddl_path(path.as_path());
                        }
                        ui.close_menu();
                    }
                    if ui.button("Import OpenAPI File").clicked() {
                        let mut dialog =
                            FileDialog::new().add_filter("OpenAPI", &["yaml", "yml", "json"]);
                        if !self.current_catalog_path.trim().is_empty() {
                            dialog = dialog.set_directory(self.current_catalog_path.as_str());
                        }
                        if let Some(path) = dialog.pick_file() {
                            self.import_api_routes_from_openapi_path(path.as_path());
                        }
                        ui.close_menu();
                    }
                    if ui.button("Import LLM Systems File").clicked() {
                        let mut dialog = FileDialog::new().add_filter("LLM Systems", &["json"]);
                        if !self.current_catalog_path.trim().is_empty() {
                            dialog = dialog.set_directory(self.current_catalog_path.as_str());
                        }
                        if let Some(path) = dialog.pick_file() {
                            self.import_llm_systems_from_path(path.as_path());
                        }
                        ui.close_menu();
                    }
                    if ui.button("Import LLM Detailed Map").clicked() {
                        let mut dialog = FileDialog::new().add_filter("LLM Detailed", &["json"]);
                        if !self.current_catalog_path.trim().is_empty() {
                            dialog = dialog.set_directory(self.current_catalog_path.as_str());
                        }
                        if let Some(path) = dialog.pick_file() {
                            self.import_llm_detailed_map_from_path(path.as_path());
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui
                        .checkbox(&mut self.project_autosave_enabled, "Autosave Project")
                        .changed()
                    {
                        self.project_last_autosave_at_secs = None;
                    }
                    ui.checkbox(
                        &mut self.manage_system_json_hierarchy,
                        "Manage System JSON Hierarchy",
                    );

                    if !self.current_catalog_path.trim().is_empty() {
                        let current_path = self.current_catalog_path.trim().to_owned();
                        if self.git_repo_detect_path != current_path {
                            self.git_repo_detect_path = current_path;
                        }

                        ui.separator();
                        ui.label("Version control");
                        match self.git_repo_detected_for_path {
                            Some(true) => {
                                if ui.button("Commit").clicked() {
                                    self.commit_project_changes();
                                    ui.close_menu();
                                }
                                if ui.button("Rollback").clicked() {
                                    self.rollback_project_changes();
                                    ui.close_menu();
                                }
                                if ui.button("Re-check Version Control").clicked() {
                                    self.detect_version_control_status();
                                }
                            }
                            Some(false) => {
                                if ui.button("Enable Version Control").clicked() {
                                    self.enable_version_control();
                                    ui.close_menu();
                                }
                                if ui.button("Re-check Version Control").clicked() {
                                    self.detect_version_control_status();
                                }
                            }
                            None => {
                                if ui.button("Check Version Control").clicked() {
                                    self.detect_version_control_status();
                                }
                            }
                        }
                    }

                    if !self.recent_catalog_paths.is_empty() {
                        ui.separator();
                        ui.label("Recent projects");
                        let recent_paths = self.recent_catalog_paths.clone();
                        for path in recent_paths {
                            let project_name =
                                SystemsCatalogApp::catalog_name_from_path(path.as_str());
                            let label = format!("{} ({})", project_name, path);
                            if ui.button(label).clicked() {
                                self.pending_catalog_switch_path = Some(path.clone());
                                ui.close_menu();
                            }
                        }
                    }
                });

                ui.menu_button("Edit", |ui| {
                    if ui.button("Command Palette    Ctrl+P").clicked() {
                        self.command_palette_query.clear();
                        self.focus_command_palette_query = true;
                        self.open_modal(AppModal::CommandPalette);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Add System          Ctrl+N").clicked() {
                        self.open_add_system_modal_with_prefill(self.selected_system_id);
                        ui.close_menu();
                    }
                    if ui.button("Bulk Add Systems    Ctrl+Shift+N").clicked() {
                        self.open_bulk_add_systems_modal_with_prefill(self.selected_system_id);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Copy Selected       Alt+C").clicked() {
                        self.copy_selected_systems_to_clipboard();
                        ui.close_menu();
                    }
                    if ui.button("Paste Cards         Alt+V").clicked() {
                        self.load_copied_systems_from_clipboard();
                        self.paste_copied_systems();
                        ui.close_menu();
                    }
                });

                ui.menu_button("View", |ui| {
                    if ui.button("Zoom In").clicked() {
                        let target = (self.map_zoom + 0.1).min(MAP_MAX_ZOOM);
                        self.map_zoom = target;
                        self.settings_dirty = true;
                        ui.close_menu();
                    }
                    if ui.button("Zoom Out").clicked() {
                        let target = (self.map_zoom - 0.1).max(MAP_MIN_ZOOM);
                        self.map_zoom = target;
                        self.settings_dirty = true;
                        ui.close_menu();
                    }
                    if ui.button("Reset Zoom").clicked() {
                        self.map_zoom = 1.0;
                        self.settings_dirty = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Reset Pan").clicked() {
                        self.map_pan = Vec2::ZERO;
                        self.settings_dirty = true;
                        ui.close_menu();
                    }
                    if ui.button("Reset Layout").clicked() {
                        self.reset_map_layout();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Show All").clicked() {
                        self.clear_subset_visibility();
                        ui.close_menu();
                    }
                    if ui.button("Clear Selection Set").clicked() {
                        self.selected_map_system_ids.clear();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui
                        .checkbox(&mut self.snap_to_grid, "Snap to Grid")
                        .changed()
                    {
                        self.map_node_size_cache.clear();
                        self.settings_dirty = true;
                    }
                    if ui
                        .checkbox(
                            &mut self.map_zoom_anchor_to_pointer,
                            "Zoom Toward Mouse Pointer",
                        )
                        .changed()
                    {
                        self.settings_dirty = true;
                    }
                    ui.separator();
                    ui.label("Canvas Size");
                    ui.horizontal(|ui| {
                        let mut width = self.map_world_size.x;
                        if ui
                            .add(
                                egui::DragValue::new(&mut width)
                                    .clamp_range(MAP_WORLD_MIN_SIZE.x..=MAP_WORLD_MAX_SIZE.x)
                                    .speed(50.0)
                                    .prefix("W "),
                            )
                            .changed()
                        {
                            self.map_world_size.x =
                                width.clamp(MAP_WORLD_MIN_SIZE.x, MAP_WORLD_MAX_SIZE.x);
                            self.settings_dirty = true;
                        }
                        let mut height = self.map_world_size.y;
                        if ui
                            .add(
                                egui::DragValue::new(&mut height)
                                    .clamp_range(MAP_WORLD_MIN_SIZE.y..=MAP_WORLD_MAX_SIZE.y)
                                    .speed(50.0)
                                    .prefix("H "),
                            )
                            .changed()
                        {
                            self.map_world_size.y =
                                height.clamp(MAP_WORLD_MIN_SIZE.y, MAP_WORLD_MAX_SIZE.y);
                            self.settings_dirty = true;
                        }
                    });
                });

                ui.menu_button("Tools", |ui| {
                    if ui.button("Add Technology       Alt+N").clicked() {
                        self.open_modal(AppModal::AddTech);
                        self.focus_add_tech_name_on_open = true;
                        ui.close_menu();
                    }
                    if ui.button("Flow Inspector").clicked() {
                        self.open_modal(AppModal::FlowInspector);
                        self.flow_inspector_pick_target = None;
                        self.flow_inspector_last_seen_selected_system_id = self.selected_system_id;
                        ui.close_menu();
                    }
                });

                ui.menu_button("Debug", |ui| {
                    ui.checkbox(&mut self.show_debug_inspection_window, "🔍 Inspection");
                    ui.checkbox(&mut self.show_debug_memory_window, "📝 Memory");
                });

                self.render_connection_style_menu(ui);
                self.render_stats_menu(ui);

                ui.menu_button("Help", |ui| {
                    if ui.button("Getting Started").clicked() {
                        self.open_modal(AppModal::HelpGettingStarted);
                        ui.close_menu();
                    }
                    if ui.button("Creating Interactions").clicked() {
                        self.open_modal(AppModal::HelpCreatingInteractions);
                        ui.close_menu();
                    }
                    if ui.button("Managing Technology").clicked() {
                        self.open_modal(AppModal::HelpManagingTechnology);
                        ui.close_menu();
                    }
                    if ui.button("Understanding the Map").clicked() {
                        self.open_modal(AppModal::HelpUnderstandingMap);
                        ui.close_menu();
                    }
                    if ui.button("Zones & Organization").clicked() {
                        self.open_modal(AppModal::HelpZones);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Keyboard Shortcuts").clicked() {
                        self.open_modal(AppModal::HelpKeyboardShortcuts);
                        ui.close_menu();
                    }
                    if ui.button("Hotkeys              F1").clicked() {
                        self.open_modal(AppModal::Hotkeys);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Troubleshooting & FAQ").clicked() {
                        self.open_modal(AppModal::HelpTroubleshooting);
                        ui.close_menu();
                    }
                });

                ui.separator();
                let systems_selected =
                    self.show_left_sidebar && self.active_sidebar_tab == SidebarTab::Systems;
                if ui.selectable_label(systems_selected, "Systems").clicked() {
                    self.toggle_left_sidebar_tab(SidebarTab::Systems);
                }
                let tech_selected =
                    self.show_left_sidebar && self.active_sidebar_tab == SidebarTab::TechCatalog;
                if ui.selectable_label(tech_selected, "Tech").clicked() {
                    self.toggle_left_sidebar_tab(SidebarTab::TechCatalog);
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let UpdateCheckState::UpdateAvailable(update) =
                        self.update_check_state.clone()
                    {
                        let label = format!("Update {}", update.version);
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(label)
                                        .small()
                                        .color(egui::Color32::from_rgb(255, 245, 208)),
                                )
                                .fill(egui::Color32::from_rgb(186, 97, 32))
                                .min_size(Vec2::new(88.0, 22.0)),
                            )
                            .on_hover_text(format!("Install release {}", update.tag_name))
                            .clicked()
                        {
                            self.confirm_and_start_update_install();
                        }

                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Release")
                                        .small()
                                        .color(egui::Color32::from_rgb(214, 226, 255)),
                                )
                                .fill(egui::Color32::from_rgb(52, 78, 130))
                                .min_size(Vec2::new(66.0, 22.0)),
                            )
                            .clicked()
                        {
                            self.open_update_release_page();
                        }

                        ui.separator();
                    }

                    if matches!(self.update_check_state, UpdateCheckState::Checking) {
                        ui.label(
                            RichText::new("Checking updates...")
                                .small()
                                .color(egui::Color32::from_rgb(194, 204, 224)),
                        );
                        ui.separator();
                    }

                    if matches!(self.update_check_state, UpdateCheckState::Applying) {
                        ui.label(
                            RichText::new("Applying update...")
                                .small()
                                .color(egui::Color32::from_rgb(255, 230, 194)),
                        );
                        ui.separator();
                    }

                    if let UpdateCheckState::Error(message) = &self.update_check_state {
                        ui.label(
                            RichText::new(format!("Update check failed: {message}"))
                                .small()
                                .color(egui::Color32::from_rgb(240, 162, 162)),
                        );
                        ui.separator();
                    }

                    let selected_recent_path = self
                        .recent_catalog_paths
                        .iter()
                        .find(|path| path.as_str() == self.current_catalog_path.as_str())
                        .cloned()
                        .or_else(|| self.recent_catalog_paths.first().cloned());

                    if let Some(selected_path) = selected_recent_path {
                        egui::ComboBox::from_id_source("header_recent_projects")
                            .selected_text(format!(
                                "Project: {}",
                                Self::catalog_name_from_path(selected_path.as_str())
                            ))
                            .show_ui(ui, |ui| {
                                let recent_paths = self.recent_catalog_paths.clone();
                                for path in recent_paths {
                                    let project_name = Self::catalog_name_from_path(path.as_str());
                                    let selected = path == self.current_catalog_path;
                                    let label = format!("{} ({})", project_name, path);
                                    if ui.selectable_label(selected, label).clicked() {
                                        self.pending_catalog_switch_path = Some(path.clone());
                                    }
                                }
                            });
                        ui.separator();
                    }

                    ui.label(
                        RichText::new(format!("Project: {}", self.current_catalog_name))
                            .small()
                            .strong(),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(format!("{:.0}%", self.map_zoom * 100.0))
                            .weak()
                            .small(),
                    );
                    ui.separator();
                    let status_text = RichText::new(&self.status_message)
                        .small()
                        .color(egui::Color32::from_rgb(196, 204, 222));
                    egui::Frame::none()
                        .fill(egui::Color32::from_rgba_unmultiplied(54, 72, 108, 84))
                        .rounding(egui::Rounding::same(6.0))
                        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                        .show(ui, |ui| {
                            ui.label(status_text);
                        });
                });
            });
        });
    }
}
