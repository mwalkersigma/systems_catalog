use std::collections::HashMap;

use eframe::egui::{
    self, Align, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Shape, Stroke, Vec2,
};

use crate::app::{LineStyle, LineTerminator, SystemsCatalogApp, MAP_NODE_SIZE};

impl SystemsCatalogApp {
    fn disclosure_icon(is_collapsed: bool) -> &'static str {
        if is_collapsed {
            "+"
        } else {
            "-"
        }
    }

    fn map_node_size_for(&self, label: &str) -> Vec2 {
        let estimated_width = (label.chars().count() as f32 * 8.0) + 46.0;
        Vec2::new(
            estimated_width.clamp(MAP_NODE_SIZE.x, 360.0),
            MAP_NODE_SIZE.y,
        )
    }

    fn brighten_color(&self, color: Color32, percent: f32) -> Color32 {
        if percent <= 100.0 {
            return color;
        }

        let factor = ((percent - 100.0) / 100.0).clamp(0.0, 1.0);
        let brighten = |channel: u8| -> u8 {
            let value = channel as f32 + ((255.0 - channel as f32) * factor);
            value.round().clamp(0.0, 255.0) as u8
        };

        Color32::from_rgba_unmultiplied(
            brighten(color.r()),
            brighten(color.g()),
            brighten(color.b()),
            color.a(),
        )
    }

    fn system_line_override_color(&self, system_id: i64) -> Option<Color32> {
        self.systems
            .iter()
            .find(|system| system.id == system_id)
            .and_then(|system| {
                system
                    .line_color_override
                    .as_deref()
                    .and_then(Self::color_from_setting_value)
            })
    }

    fn parent_line_style_for(&self, parent_system_id: i64) -> LineStyle {
        let mut style = self.parent_line_style;
        if let Some(override_color) = self.system_line_override_color(parent_system_id) {
            style.color = override_color;
        }
        style
    }

    fn interaction_line_style_for(
        &self,
        source_system_id: i64,
        target_system_id: i64,
    ) -> LineStyle {
        let mut style = self.interaction_line_style;
        if let Some(override_color) = self.system_line_override_color(source_system_id) {
            style.color = override_color;
        } else if let Some(override_color) = self.system_line_override_color(target_system_id) {
            style.color = override_color;
        }
        style
    }

    fn rect_edge_point(rect: Rect, direction_from_center: Vec2) -> Pos2 {
        let center = rect.center();
        let half_width = (rect.width() * 0.5 - 2.0).max(1.0);
        let half_height = (rect.height() * 0.5 - 2.0).max(1.0);

        if direction_from_center.length_sq() <= f32::EPSILON {
            return center;
        }

        let scale_x = if direction_from_center.x.abs() > f32::EPSILON {
            half_width / direction_from_center.x.abs()
        } else {
            f32::INFINITY
        };

        let scale_y = if direction_from_center.y.abs() > f32::EPSILON {
            half_height / direction_from_center.y.abs()
        } else {
            f32::INFINITY
        };

        let scale = scale_x.min(scale_y);
        center + (direction_from_center * scale)
    }

    fn card_to_card_endpoints(&self, from_rect: Rect, to_rect: Rect) -> (Pos2, Pos2) {
        let from_center = from_rect.center();
        let to_center = to_rect.center();
        let direction = to_center - from_center;

        if direction.length_sq() <= f32::EPSILON {
            return (from_center, to_center);
        }

        let start = Self::rect_edge_point(from_rect, direction);
        let end = Self::rect_edge_point(to_rect, -direction);
        (start, end)
    }

    fn rect_to_point_endpoint(&self, from_rect: Rect, to_point: Pos2) -> Pos2 {
        let direction = to_point - from_rect.center();
        Self::rect_edge_point(from_rect, direction)
    }

    fn line_color(&self, style: LineStyle, dimmed: bool, boosted: bool) -> Color32 {
        let mut color = style.color;

        if boosted {
            color = self.brighten_color(color, self.selected_line_brightness_percent);
        }

        if !dimmed {
            return color;
        }

        let dimmed_alpha = ((color.a() as f32) * (self.dimmed_line_opacity_percent / 100.0))
            .round()
            .clamp(0.0, 255.0) as u8;

        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), dimmed_alpha)
    }

    fn draw_directed_connection(
        &self,
        painter: &egui::Painter,
        from: Pos2,
        to: Pos2,
        style: LineStyle,
        dimmed: bool,
        boosted: bool,
    ) {
        let direction = to - from;
        let distance = direction.length();
        if distance < 2.0 {
            return;
        }

        let color = self.line_color(style, dimmed, boosted);
        let stroke = Stroke::new(style.width * self.map_zoom.clamp(0.8, 1.8), color);
        let unit = direction / distance;
        let normal = Vec2::new(-unit.y, unit.x);
        let arrow_size = (9.0 * self.map_zoom).clamp(7.0, 18.0);

        match style.terminator {
            LineTerminator::None => {
                painter.line_segment([from, to], stroke);
            }
            LineTerminator::Arrow => {
                let line_end = to - (unit * (arrow_size + 2.0));
                painter.line_segment([from, line_end], stroke);

                let arrow_left = to - (unit * arrow_size) + (normal * (arrow_size * 0.45));
                let arrow_right = to - (unit * arrow_size) - (normal * (arrow_size * 0.45));

                painter.line_segment([to, arrow_left], stroke);
                painter.line_segment([to, arrow_right], stroke);
            }
            LineTerminator::FilledArrow => {
                let tip = to;
                let base = to - (unit * arrow_size);
                let arrow_left = base + (normal * (arrow_size * 0.5));
                let arrow_right = base - (normal * (arrow_size * 0.5));
                let line_end = base - (unit * 1.0);

                painter.line_segment([from, line_end], stroke);
                painter.add(Shape::convex_polygon(
                    vec![tip, arrow_left, arrow_right],
                    color,
                    Stroke::NONE,
                ));
            }
        }
    }

    fn terminator_label(terminator: LineTerminator) -> &'static str {
        match terminator {
            LineTerminator::None => "None",
            LineTerminator::Arrow => "Arrow",
            LineTerminator::FilledArrow => "Filled arrow",
        }
    }

    fn render_terminator_combo(
        ui: &mut egui::Ui,
        id: &str,
        label: &str,
        terminator: &mut LineTerminator,
    ) {
        egui::ComboBox::from_id_source(id)
            .selected_text(Self::terminator_label(*terminator))
            .show_ui(ui, |ui| {
                ui.selectable_value(terminator, LineTerminator::None, "None");
                ui.selectable_value(terminator, LineTerminator::Arrow, "Arrow");
                ui.selectable_value(terminator, LineTerminator::FilledArrow, "Filled arrow");
            });
        ui.label(label);
    }

    fn render_add_system_modal(&mut self, ctx: &egui::Context) {
        if !self.show_add_system_modal {
            return;
        }

        let mut open = self.show_add_system_modal;
        egui::Window::new("Add System")
            .collapsible(false)
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut self.new_system_name);

                ui.label("Description");
                ui.add(egui::TextEdit::multiline(&mut self.new_system_description).desired_rows(4));

                let selected_parent_label = self
                    .new_system_parent_id
                    .map(|id| self.system_name_by_id(id))
                    .unwrap_or_else(|| "No parent (root system)".to_owned());

                egui::ComboBox::from_label("Parent")
                    .selected_text(selected_parent_label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.new_system_parent_id,
                            None,
                            "No parent (root system)",
                        );
                        for system in &self.systems {
                            ui.selectable_value(
                                &mut self.new_system_parent_id,
                                Some(system.id),
                                system.name.as_str(),
                            );
                        }
                    });

                ui.horizontal(|ui| {
                    if ui.button("Create").clicked() {
                        self.create_system();
                        self.show_add_system_modal = false;
                    }

                    if ui.button("Cancel").clicked() {
                        self.show_add_system_modal = false;
                    }
                });
            });

        self.show_add_system_modal = open;
    }

    fn render_add_tech_modal(&mut self, ctx: &egui::Context) {
        if !self.show_add_tech_modal {
            return;
        }

        let mut open = self.show_add_tech_modal;
        egui::Window::new("Add Technology")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Technology name");
                ui.text_edit_singleline(&mut self.new_tech_name);

                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        self.create_tech_item();
                        self.show_add_tech_modal = false;
                    }

                    if ui.button("Cancel").clicked() {
                        self.show_add_tech_modal = false;
                    }
                });
            });

        self.show_add_tech_modal = open;
    }

    fn render_line_style_modal(&mut self, ctx: &egui::Context) {
        if !self.show_line_style_modal {
            return;
        }

        let mut open = self.show_line_style_modal;
        egui::Window::new("Connection Style")
            .collapsible(false)
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                let mut changed = false;

                ui.label("Parent connections");
                changed |= ui
                    .checkbox(&mut self.show_parent_lines, "Show parent lines")
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.parent_line_style.width, 0.5..=6.0)
                            .text("Width"),
                    )
                    .changed();
                ui.horizontal(|ui| {
                    ui.label("Color");
                    changed |= ui
                        .color_edit_button_srgba(&mut self.parent_line_style.color)
                        .changed();
                });

                let old_parent_terminator = self.parent_line_style.terminator;
                Self::render_terminator_combo(
                    ui,
                    "parent_terminator",
                    "Terminator",
                    &mut self.parent_line_style.terminator,
                );
                if old_parent_terminator != self.parent_line_style.terminator {
                    changed = true;
                }

                ui.separator();
                ui.label("Interaction connections");
                changed |= ui
                    .checkbox(&mut self.show_interaction_lines, "Show interaction lines")
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.interaction_line_style.width, 0.5..=6.0)
                            .text("Width"),
                    )
                    .changed();
                ui.horizontal(|ui| {
                    ui.label("Color");
                    changed |= ui
                        .color_edit_button_srgba(&mut self.interaction_line_style.color)
                        .changed();
                });

                let old_interaction_terminator = self.interaction_line_style.terminator;
                Self::render_terminator_combo(
                    ui,
                    "interaction_terminator",
                    "Terminator",
                    &mut self.interaction_line_style.terminator,
                );
                if old_interaction_terminator != self.interaction_line_style.terminator {
                    changed = true;
                }

                ui.separator();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.dimmed_line_opacity_percent, 0.0..=100.0)
                            .text("Dimmed opacity %"),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(
                            &mut self.selected_line_brightness_percent,
                            100.0..=220.0,
                        )
                        .text("Selected line brightness %"),
                    )
                    .changed();

                if changed {
                    self.settings_dirty = true;
                }

                if ui.button("Close").clicked() {
                    self.show_line_style_modal = false;
                }
            });

        self.show_line_style_modal = open;
    }

    fn select_system(&mut self, system_id: i64) {
        self.selected_system_id = Some(system_id);
        if let Err(error) = self.load_selected_data(system_id) {
            self.status_message = format!("Failed to load selection: {error}");
        }
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.heading("Systems List");

        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                if let Err(error) = self.refresh_systems() {
                    self.status_message = format!("Refresh failed: {error}");
                }
            }
            ui.label(RichText::new("Hierarchy").weak());

            if ui.small_button("Show all").clicked() {
                self.clear_subset_visibility();
            }
        });

        ui.separator();

        let rows = self.visible_hierarchy_rows();
        if rows.is_empty() {
            ui.label("No systems yet.");
        } else {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (depth, system_id, name, has_children, is_collapsed) in rows {
                    let indent = "  ".repeat(depth);
                    let row_text = format!("{indent}• {name}");
                    let selected = self.selected_system_id == Some(system_id);

                    ui.horizontal(|ui| {
                        if has_children {
                            let icon = Self::disclosure_icon(is_collapsed);
                            if ui.small_button(icon).clicked() {
                                self.on_disclosure_click(system_id);
                            }
                        }

                        if ui.selectable_label(selected, row_text).clicked() {
                            self.select_system(system_id);
                        }
                    });
                }
            });
        }

        ui.separator();
        ui.label("Tech Catalog");

        let selected_catalog_tech_label = self
            .selected_catalog_tech_id_for_edit
            .map(|id| self.tech_name_by_id(id))
            .unwrap_or_else(|| "Select technology to edit".to_owned());

        egui::ComboBox::from_label("Catalog item")
            .selected_text(selected_catalog_tech_label)
            .show_ui(ui, |ui| {
                for tech in &self.tech_catalog {
                    let was_selected = self.selected_catalog_tech_id_for_edit == Some(tech.id);
                    if ui
                        .selectable_label(was_selected, tech.name.as_str())
                        .clicked()
                    {
                        self.selected_catalog_tech_id_for_edit = Some(tech.id);
                        self.edited_tech_name = tech.name.clone();
                    }
                }
            });

        ui.text_edit_singleline(&mut self.edited_tech_name);
        ui.horizontal(|ui| {
            if ui.button("Update").clicked() {
                self.update_selected_catalog_tech();
            }
            if ui.button("Delete").clicked() {
                self.delete_selected_catalog_tech();
            }
        });
    }

    fn render_details(&mut self, ui: &mut egui::Ui) {
        ui.heading("Selected System Details");

        let Some(system) = self.selected_system().cloned() else {
            ui.label("Select a system from the list or map.");
            return;
        };

        ui.label(RichText::new(system.name.clone()).strong());
        if let Some(parent_id) = system.parent_id {
            ui.label(format!("Parent: {}", self.system_name_by_id(parent_id)));
        } else {
            ui.label("Parent: none (root)");
        }

        ui.separator();
        ui.label("Parent assignment");

        let selected_parent_label = self
            .selected_system_parent_id
            .map(|id| self.system_name_by_id(id))
            .unwrap_or_else(|| "No parent (root system)".to_owned());

        let valid_parent_candidates = self
            .systems
            .iter()
            .filter(|candidate| {
                candidate.id != system.id
                    && !self.would_create_parent_cycle(system.id, candidate.id)
            })
            .map(|candidate| (candidate.id, candidate.name.clone()))
            .collect::<Vec<_>>();

        egui::ComboBox::from_label("Set parent")
            .selected_text(selected_parent_label)
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.selected_system_parent_id,
                    None,
                    "No parent (root system)",
                );

                for (candidate_id, candidate_name) in &valid_parent_candidates {
                    ui.selectable_value(
                        &mut self.selected_system_parent_id,
                        Some(*candidate_id),
                        candidate_name.as_str(),
                    );
                }
            });

        ui.horizontal(|ui| {
            if ui.button("Save parent").clicked() {
                self.update_selected_system_parent();
            }

            if ui.button("Delete system").clicked() {
                self.delete_selected_system();
                return;
            }
        });

        ui.separator();
        ui.label("Per-system line color override");
        ui.horizontal(|ui| {
            let mut use_override = self.selected_system_line_color_override.is_some();
            if ui.checkbox(&mut use_override, "Enable override").changed() {
                if use_override {
                    self.selected_system_line_color_override =
                        Some(self.interaction_line_style.color);
                } else {
                    self.selected_system_line_color_override = None;
                }
            }

            if let Some(mut color) = self.selected_system_line_color_override {
                if ui.color_edit_button_srgba(&mut color).changed() {
                    self.selected_system_line_color_override = Some(color);
                }
            }
        });

        ui.horizontal(|ui| {
            if ui.button("Save line override").clicked() {
                self.update_selected_system_line_color_override();
            }
            if ui.button("Clear override").clicked() {
                self.selected_system_line_color_override = None;
                self.update_selected_system_line_color_override();
            }
        });

        ui.separator();
        ui.label("Description");
        ui.label(system.description.clone());

        ui.separator();
        ui.label("Interactions");

        let selected_target_label = self
            .new_link_target_id
            .map(|id| self.system_name_by_id(id))
            .unwrap_or_else(|| "Select target system".to_owned());

        egui::ComboBox::from_label("Target")
            .selected_text(selected_target_label)
            .show_ui(ui, |ui| {
                for candidate in self
                    .systems
                    .iter()
                    .filter(|candidate| candidate.id != system.id)
                {
                    ui.selectable_value(
                        &mut self.new_link_target_id,
                        Some(candidate.id),
                        candidate.name.as_str(),
                    );
                }
            });

        ui.text_edit_singleline(&mut self.new_link_label);
        if ui.button("Add interaction").clicked() {
            self.create_link();
        }

        if self.selected_links.is_empty() {
            ui.label("No interactions recorded.");
        } else {
            let selected_link_label = self
                .selected_link_id_for_edit
                .and_then(|link_id| {
                    self.selected_links
                        .iter()
                        .find(|link| link.id == link_id)
                        .map(|link| {
                            format!(
                                "#{} {} → {}",
                                link.id,
                                self.system_name_by_id(link.source_system_id),
                                self.system_name_by_id(link.target_system_id)
                            )
                        })
                })
                .unwrap_or_else(|| "Select interaction".to_owned());

            egui::ComboBox::from_label("Edit interaction")
                .selected_text(selected_link_label)
                .show_ui(ui, |ui| {
                    for link in &self.selected_links {
                        let label = format!(
                            "#{} {} → {}",
                            link.id,
                            self.system_name_by_id(link.source_system_id),
                            self.system_name_by_id(link.target_system_id)
                        );

                        let was_selected = self.selected_link_id_for_edit == Some(link.id);
                        if ui.selectable_label(was_selected, label).clicked() {
                            self.selected_link_id_for_edit = Some(link.id);
                            self.edited_link_label = link.label.clone();
                        }
                    }
                });

            ui.label("Interaction label");
            ui.text_edit_singleline(&mut self.edited_link_label);
            ui.horizontal(|ui| {
                if ui.button("Update interaction").clicked() {
                    self.update_selected_link();
                }
                if ui.button("Delete interaction").clicked() {
                    self.delete_selected_link();
                }
            });
        }

        ui.separator();
        ui.label("System tech stack");

        let selected_tech_label = self
            .selected_tech_id_for_assignment
            .map(|id| self.tech_name_by_id(id))
            .unwrap_or_else(|| "Select technology".to_owned());

        egui::ComboBox::from_label("Technology")
            .selected_text(selected_tech_label)
            .show_ui(ui, |ui| {
                for tech in &self.tech_catalog {
                    ui.selectable_value(
                        &mut self.selected_tech_id_for_assignment,
                        Some(tech.id),
                        tech.name.as_str(),
                    );
                }
            });

        if ui.button("Assign technology to system").clicked() {
            self.add_selected_tech_to_system();
        }

        if self.selected_system_tech.is_empty() {
            ui.label("No technologies assigned to this system.");
        } else {
            let selected_system_tech = self.selected_system_tech.clone();
            ui.vertical(|ui| {
                for tech in selected_system_tech {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(tech.name.to_string());
                            if ui.small_button("Remove").clicked() {
                                self.remove_tech_from_selected_system(tech.id);
                            }
                        });
                    });
                }
            });
        }

        ui.separator();
        ui.label("Cumulative child tech stack (deduped)");
        if self.selected_cumulative_child_tech.is_empty() {
            ui.label("No child-system technologies found.");
        } else {
            ui.vertical(|ui| {
                for tech_name in &self.selected_cumulative_child_tech {
                    ui.label(format!("• {tech_name}"));
                }
            });
        }

        ui.separator();
        ui.label("Notes");
        ui.add(egui::TextEdit::multiline(&mut self.note_text).desired_rows(8));
        if ui.button("Save notes").clicked() {
            self.save_note();
        }
    }

    fn render_map_canvas(&mut self, ui: &mut egui::Ui) {
        ui.heading("Mind Map");
        ui.label("Hold Space and drag to pan. Scroll to zoom. Shift+drag from a node to create an interaction.");

        ui.horizontal(|ui| {
            ui.label("Zoom");

            if ui.small_button("-").clicked() {
                self.map_zoom = (self.map_zoom - 0.1).max(0.5);
                self.settings_dirty = true;
            }

            if ui.small_button("+").clicked() {
                self.map_zoom = (self.map_zoom + 0.1).min(2.5);
                self.settings_dirty = true;
            }

            if ui.small_button("Reset zoom").clicked() {
                self.map_zoom = 1.0;
                self.settings_dirty = true;
            }

            if ui.small_button("Reset layout").clicked() {
                self.reset_map_layout();
            }

            if ui.small_button("Reset pan").clicked() {
                self.map_pan = Vec2::ZERO;
                self.settings_dirty = true;
            }

            if ui.small_button("Show all").clicked() {
                self.clear_subset_visibility();
            }

            ui.label(format!("{:.0}%", self.map_zoom * 100.0));
        });

        let mut desired_size = ui.available_size();
        desired_size.y = desired_size.y.max(420.0);

        let (map_rect, map_response) =
            ui.allocate_exact_size(desired_size, Sense::click_and_drag());
        let painter = ui.painter_at(map_rect);

        let space_down = ui.input(|input| input.key_down(egui::Key::Space));
        let pan_mode_active = space_down && map_response.dragged();
        if pan_mode_active {
            let pointer_delta = ui.input(|input| input.pointer.delta());
            self.map_pan += pointer_delta;
            self.settings_dirty = true;
        }

        let wheel_delta_y = ui.input(|input| input.smooth_scroll_delta.y);
        if wheel_delta_y.abs() > f32::EPSILON {
            let zoom_step = (wheel_delta_y / 400.0).clamp(-0.15, 0.15);
            self.map_zoom = (self.map_zoom + zoom_step).clamp(0.5, 2.5);
            self.settings_dirty = true;
        }

        painter.rect_filled(map_rect, 6.0, Color32::from_gray(24));
        painter.rect_stroke(map_rect, 6.0, Stroke::new(1.0, Color32::from_gray(60)));

        self.ensure_map_positions();

        let zoom = self.map_zoom;
        let pan = self.map_pan;
        let to_screen = |local: Pos2| -> Pos2 {
            Pos2::new(
                map_rect.left() + pan.x + (local.x * zoom),
                map_rect.top() + pan.y + (local.y * zoom),
            )
        };

        let visible_ids = self.visible_system_ids();
        let visible_systems = self
            .systems
            .iter()
            .filter(|system| visible_ids.contains(&system.id))
            .cloned()
            .collect::<Vec<_>>();

        let mut node_rects: HashMap<i64, Rect> = HashMap::new();
        for system in &visible_systems {
            if let Some(local_position) = self.map_positions.get(&system.id) {
                let node_size_screen = self.map_node_size_for(system.name.as_str()) * zoom;
                let rect = Rect::from_min_size(to_screen(*local_position), node_size_screen);
                node_rects.insert(system.id, rect);
            }
        }

        let selected_id = self.selected_system_id;

        if self.show_parent_lines {
            for system in &visible_systems {
                let Some(parent_id) = system.parent_id else {
                    continue;
                };

                let Some(parent_rect) = node_rects.get(&parent_id) else {
                    continue;
                };
                let Some(child_rect) = node_rects.get(&system.id) else {
                    continue;
                };

                let dimmed = selected_id
                    .map(|id| id != parent_id && id != system.id)
                    .unwrap_or(false);
                let boosted = selected_id
                    .map(|id| id == parent_id || id == system.id)
                    .unwrap_or(false);

                let parent_style = self.parent_line_style_for(parent_id);

                let (from, to) = self.card_to_card_endpoints(*parent_rect, *child_rect);

                self.draw_directed_connection(
                    &painter,
                    from,
                    to,
                    parent_style,
                    dimmed,
                    selected_id.is_some() && boosted,
                );
            }
        }

        if self.show_interaction_lines {
            for link in &self.all_links {
                let Some(source_rect) = node_rects.get(&link.source_system_id) else {
                    continue;
                };
                let Some(target_rect) = node_rects.get(&link.target_system_id) else {
                    continue;
                };

                let dimmed = selected_id
                    .map(|id| id != link.source_system_id && id != link.target_system_id)
                    .unwrap_or(false);
                let boosted = selected_id
                    .map(|id| id == link.source_system_id || id == link.target_system_id)
                    .unwrap_or(false);

                let interaction_style =
                    self.interaction_line_style_for(link.source_system_id, link.target_system_id);

                let (from, to) = self.card_to_card_endpoints(*source_rect, *target_rect);

                self.draw_directed_connection(
                    &painter,
                    from,
                    to,
                    interaction_style,
                    dimmed,
                    selected_id.is_some() && boosted,
                );
            }
        }

        let systems_snapshot = visible_systems.clone();
        for system in systems_snapshot {
            let Some(current_local_position) = self.map_positions.get(&system.id).copied() else {
                continue;
            };

            let node_size = self.map_node_size_for(system.name.as_str());
            let node_size_screen = node_size * zoom;

            let node_rect =
                Rect::from_min_size(to_screen(current_local_position), node_size_screen);
            let interaction_sense = if space_down {
                Sense::hover()
            } else {
                Sense::click_and_drag()
            };

            let response = ui.interact(
                node_rect,
                ui.id().with(("map_node", system.id)),
                interaction_sense,
            );

            if response.clicked() {
                if let Some(source_id) = self.map_link_click_source {
                    if source_id != system.id {
                        self.create_link_between(source_id, system.id, "");
                        self.map_link_click_source = None;
                    }
                } else {
                    self.select_system(system.id);
                }
            }

            if response.drag_started() {
                let shift_held = ui.input(|input| input.modifiers.shift);
                if shift_held {
                    self.map_link_drag_from = Some(system.id);
                }
            }

            if response.dragged() {
                let shift_held = ui.input(|input| input.modifiers.shift);
                if self.map_link_drag_from == Some(system.id) || shift_held {
                    self.map_link_drag_from = Some(system.id);
                } else {
                    let pointer_delta = ui.input(|input| input.pointer.delta());
                    if pointer_delta != Vec2::ZERO {
                        let local_delta = pointer_delta / self.map_zoom;
                        let next_position = Pos2::new(
                            current_local_position.x + local_delta.x,
                            current_local_position.y + local_delta.y,
                        );
                        let clamped = self.clamp_node_position(map_rect, next_position, node_size);
                        self.map_positions.insert(system.id, clamped);
                    }
                }
            }

            if response.drag_stopped() && self.map_link_drag_from != Some(system.id) {
                if let Some(position) = self.map_positions.get(&system.id).copied() {
                    self.persist_map_position(system.id, position);
                }
            }

            let is_selected = self.selected_system_id == Some(system.id);
            let fill = if is_selected {
                Color32::from_gray(74)
            } else {
                Color32::from_gray(46)
            };

            painter.rect_filled(node_rect, 6.0, fill);
            painter.rect_stroke(node_rect, 6.0, Stroke::new(1.0, Color32::from_gray(120)));
            painter.text(
                node_rect.center(),
                egui::Align2::CENTER_CENTER,
                system.name,
                FontId::proportional((15.0 * self.map_zoom).clamp(12.0, 22.0)),
                Color32::from_gray(230),
            );

            let has_children = self
                .systems
                .iter()
                .any(|candidate| candidate.parent_id == Some(system.id));
            if has_children {
                let disclosure_radius = (9.0 * self.map_zoom).clamp(7.0, 15.0);
                let disclosure_center = Pos2::new(
                    node_rect.left() + (disclosure_radius + 2.0),
                    node_rect.top() + (disclosure_radius + 2.0),
                );
                let disclosure_rect =
                    Rect::from_center_size(disclosure_center, Vec2::splat(disclosure_radius * 2.0));
                let disclosure_response = ui.interact(
                    disclosure_rect,
                    ui.id().with(("map_disclosure", system.id)),
                    Sense::click(),
                );

                let collapsed = self.collapsed_system_ids.contains(&system.id);
                painter.circle_filled(
                    disclosure_center,
                    disclosure_radius,
                    Color32::from_gray(105),
                );
                painter.text(
                    disclosure_center,
                    egui::Align2::CENTER_CENTER,
                    Self::disclosure_icon(collapsed),
                    FontId::proportional((14.0 * self.map_zoom).clamp(10.0, 18.0)),
                    Color32::from_gray(20),
                );

                if disclosure_response.clicked() {
                    self.on_disclosure_click(system.id);
                }
            }

            if self.selected_system_id == Some(system.id) {
                let plus_radius = (10.0 * self.map_zoom).clamp(8.0, 16.0);
                let plus_center = Pos2::new(
                    node_rect.right() - (plus_radius + 2.0),
                    node_rect.top() + (plus_radius + 2.0),
                );
                let plus_rect = Rect::from_center_size(plus_center, Vec2::splat(plus_radius * 2.0));
                let plus_response = ui.interact(
                    plus_rect,
                    ui.id().with(("map_plus", system.id)),
                    Sense::click(),
                );

                let is_link_source = self.map_link_click_source == Some(system.id);
                let plus_fill = if is_link_source {
                    Color32::from_gray(160)
                } else {
                    Color32::from_gray(95)
                };

                painter.circle_filled(plus_center, plus_radius, plus_fill);
                painter.text(
                    plus_center,
                    egui::Align2::CENTER_CENTER,
                    "+",
                    FontId::proportional((16.0 * self.map_zoom).clamp(12.0, 22.0)),
                    Color32::from_gray(22),
                );

                if plus_response.clicked() {
                    self.map_link_click_source = Some(system.id);
                    self.status_message = format!(
                        "Link mode: selected source '{}'. Click a target system.",
                        self.system_name_by_id(system.id)
                    );
                }
            }
        }

        if let Some(source_id) = self.map_link_drag_from {
            if let Some(source_rect) = node_rects.get(&source_id) {
                if let Some(pointer_pos) = ui.input(|input| input.pointer.interact_pos()) {
                    let from = self.rect_to_point_endpoint(*source_rect, pointer_pos);
                    self.draw_directed_connection(
                        &painter,
                        from,
                        pointer_pos,
                        self.interaction_line_style,
                        false,
                        false,
                    );
                }
            }

            let released = ui.input(|input| input.pointer.any_released());
            if released {
                let pointer_pos = ui.input(|input| input.pointer.interact_pos());
                let target = pointer_pos.and_then(|pos| {
                    node_rects
                        .iter()
                        .find(|(_, rect)| rect.contains(pos))
                        .map(|(system_id, _)| *system_id)
                });

                self.map_link_drag_from = None;

                if let Some(target_id) = target {
                    self.create_link_between(source_id, target_id, "");
                }
            }
        }

        if map_response.clicked() && !space_down {
            let clicked_on_node = ui
                .input(|input| input.pointer.interact_pos())
                .map(|pointer_pos| node_rects.values().any(|rect| rect.contains(pointer_pos)))
                .unwrap_or(false);

            if !clicked_on_node {
                self.clear_selection();
                self.status_message = "Selection cleared".to_owned();
            }
        }
    }
}

impl eframe::App for SystemsCatalogApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Err(error) = self.validate_before_render() {
            self.status_message = format!("State warning: {error}");
        }

        egui::TopBottomPanel::top("header_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Add System").clicked() {
                    self.new_system_parent_id = self.selected_system_id;
                    self.show_add_system_modal = true;
                }
                if ui.button("Add Technology").clicked() {
                    self.show_add_tech_modal = true;
                }
                if ui.button("Connection Style").clicked() {
                    self.show_line_style_modal = true;
                }

                ui.separator();
                ui.label("Systems List | Mind Map | Selected System Details");

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(&self.status_message).italics().weak());
                });
            });
        });

        egui::SidePanel::left("systems_panel")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                self.render_sidebar(ui);
            });

        egui::SidePanel::right("details_panel")
            .resizable(true)
            .default_width(430.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.render_details(ui);
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_map_canvas(ui);
        });

        self.render_add_system_modal(ctx);
        self.render_add_tech_modal(ctx);
        self.render_line_style_modal(ctx);
        self.save_ui_settings_if_dirty();
    }
}
