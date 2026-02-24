use std::collections::{HashMap, HashSet};
use std::path::Path;

use eframe::egui::{
    self, Align, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Shape, Stroke, Vec2,
};
use rfd::FileDialog;

use crate::app::{
    LineStyle, LineTerminator, SidebarTab, SystemsCatalogApp, MAP_NODE_SIZE, MAP_WORLD_SIZE,
};

const MAP_GRID_SPACING: f32 = 48.0;
const MAP_CARD_MIN_WIDTH: f32 = MAP_NODE_SIZE.x;
const MAP_CARD_MIN_HEIGHT: f32 = MAP_NODE_SIZE.y;
const MAP_CARD_MAX_WIDTH: f32 = 360.0;
const MAP_CARD_MAX_HEIGHT: f32 = 680.0;
const MAP_CARD_CHAR_WIDTH_ESTIMATE: f32 = 8.0;
const MAP_CARD_HORIZONTAL_PADDING: f32 = 46.0;
const MAP_CARD_LINE_HEIGHT: f32 = 20.0;
const MAP_CARD_VERTICAL_PADDING: f32 = 30.0;
const MAP_TEXT_SCALE_THRESHOLD_ZOOM: f32 = 0.5;
const MAP_TEXT_MIN_LOW_ZOOM_MULTIPLIER: f32 = 0.7;

impl SystemsCatalogApp {
    fn disclosure_icon(is_collapsed: bool) -> &'static str {
        if is_collapsed {
            "+"
        } else {
            "-"
        }
    }

    fn map_node_size_for(&self, label: &str) -> Vec2 {
        let char_count = label.chars().count() as f32;
        let minimum_required_width =
            (char_count * MAP_CARD_CHAR_WIDTH_ESTIMATE) + MAP_CARD_HORIZONTAL_PADDING;
        let mut width = minimum_required_width.clamp(MAP_CARD_MIN_WIDTH, MAP_CARD_MAX_WIDTH);

        if self.snap_to_grid {
            let snapped_width = (width / MAP_GRID_SPACING).round() * MAP_GRID_SPACING;
            let minimum_snapped_width = (width / MAP_GRID_SPACING).ceil() * MAP_GRID_SPACING;
            width = if snapped_width >= width {
                snapped_width
            } else {
                minimum_snapped_width
            }
            .clamp(MAP_CARD_MIN_WIDTH, MAP_CARD_MAX_WIDTH);
        }

        let estimated_line_count = self.estimate_wrapped_line_count(label, width) as f32;
        let estimated_text_height = estimated_line_count * MAP_CARD_LINE_HEIGHT;
        let height = (estimated_text_height + MAP_CARD_VERTICAL_PADDING)
            .clamp(MAP_CARD_MIN_HEIGHT, MAP_CARD_MAX_HEIGHT);

        Vec2::new(width, height)
    }

    fn max_chars_per_line_for_width(&self, width: f32) -> usize {
        let usable_width = (width - MAP_CARD_HORIZONTAL_PADDING).max(MAP_CARD_CHAR_WIDTH_ESTIMATE);
        (usable_width / MAP_CARD_CHAR_WIDTH_ESTIMATE).floor().max(1.0) as usize
    }

    fn wrap_label_for_width(&self, label: &str, width: f32) -> String {
        let max_chars_per_line = self.max_chars_per_line_for_width(width);
        if label.chars().count() <= max_chars_per_line {
            return label.to_owned();
        }

        let mut lines: Vec<String> = Vec::new();
        let mut current_line = String::new();

        for word in label.split_whitespace() {
            let word_len = word.chars().count();

            if current_line.is_empty() {
                if word_len <= max_chars_per_line {
                    current_line.push_str(word);
                } else {
                    let mut chunk = String::new();
                    for character in word.chars() {
                        chunk.push(character);
                        if chunk.chars().count() >= max_chars_per_line {
                            lines.push(chunk);
                            chunk = String::new();
                        }
                    }
                    current_line = chunk;
                }
                continue;
            }

            let current_len = current_line.chars().count();
            if current_len + 1 + word_len <= max_chars_per_line {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                lines.push(current_line);
                if word_len <= max_chars_per_line {
                    current_line = word.to_owned();
                } else {
                    let mut chunk = String::new();
                    for character in word.chars() {
                        chunk.push(character);
                        if chunk.chars().count() >= max_chars_per_line {
                            lines.push(chunk);
                            chunk = String::new();
                        }
                    }
                    current_line = chunk;
                }
            }
        }

        if !current_line.is_empty() {
            lines.push(current_line);
        }

        if lines.is_empty() {
            label.to_owned()
        } else {
            lines.join("\n")
        }
    }

    fn estimate_wrapped_line_count(&self, label: &str, width: f32) -> usize {
        self.wrap_label_for_width(label, width).lines().count().max(1)
    }

    fn map_text_scale_multiplier(&self) -> f32 {
        if self.map_zoom >= MAP_TEXT_SCALE_THRESHOLD_ZOOM {
            return 1.0;
        }

        let ratio = (self.map_zoom / MAP_TEXT_SCALE_THRESHOLD_ZOOM).clamp(0.0, 1.0);
        MAP_TEXT_MIN_LOW_ZOOM_MULTIPLIER
            + ((1.0 - MAP_TEXT_MIN_LOW_ZOOM_MULTIPLIER) * ratio)
    }

    fn grid_spot_is_open(
        &self,
        system_id: i64,
        candidate_position: Pos2,
        node_size: Vec2,
        moving_ids: &HashSet<i64>,
    ) -> bool {
        let candidate_rect = Rect::from_min_size(candidate_position, node_size);

        for (other_id, other_position) in &self.map_positions {
            if *other_id == system_id || moving_ids.contains(other_id) {
                continue;
            }

            let other_size = self
                .systems
                .iter()
                .find(|system| system.id == *other_id)
                .map(|system| self.map_node_size_for(system.name.as_str()))
                .unwrap_or(MAP_NODE_SIZE);
            let other_rect = Rect::from_min_size(*other_position, other_size);

            if candidate_rect.intersects(other_rect) {
                return false;
            }
        }

        true
    }

    fn snap_to_open_grid_position(
        &self,
        system_id: i64,
        position: Pos2,
        node_size: Vec2,
        moving_ids: &HashSet<i64>,
    ) -> Pos2 {
        let snapped_origin = Pos2::new(
            (position.x / MAP_GRID_SPACING).round() * MAP_GRID_SPACING,
            (position.y / MAP_GRID_SPACING).round() * MAP_GRID_SPACING,
        );
        let origin = self.clamp_node_position(Rect::NOTHING, snapped_origin, node_size);

        let max_columns = ((MAP_WORLD_SIZE.x / MAP_GRID_SPACING).ceil() as i32).max(1);
        let max_rows = ((MAP_WORLD_SIZE.y / MAP_GRID_SPACING).ceil() as i32).max(1);
        let start_column = (origin.x / MAP_GRID_SPACING).round() as i32;
        let start_row = (origin.y / MAP_GRID_SPACING).round() as i32;

        for row_offset in 0..max_rows {
            let row = (start_row + row_offset).min(max_rows - 1);
            for column_offset in 0..max_columns {
                let column = (start_column + column_offset).min(max_columns - 1);

                let snapped_candidate = Pos2::new(
                    column as f32 * MAP_GRID_SPACING,
                    row as f32 * MAP_GRID_SPACING,
                );
                let candidate =
                    self.clamp_node_position(Rect::NOTHING, snapped_candidate, node_size);

                if self.grid_spot_is_open(system_id, candidate, node_size, moving_ids) {
                    return candidate;
                }
            }
        }

        origin
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
        let mut color = Color32::from_rgb(style.color.r(), style.color.g(), style.color.b());

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

    fn lerp_color(&self, from: Color32, to: Color32, t: f32) -> Color32 {
        let clamped = t.clamp(0.0, 1.0);
        let lerp = |a: u8, b: u8| -> u8 {
            (a as f32 + ((b as f32 - a as f32) * clamped))
                .round()
                .clamp(0.0, 255.0) as u8
        };

        Color32::from_rgba_unmultiplied(
            lerp(from.r(), to.r()),
            lerp(from.g(), to.g()),
            lerp(from.b(), to.b()),
            lerp(from.a(), to.a()),
        )
    }

    fn gradient_color_at(&self, colors: &[Color32], t: f32) -> Color32 {
        if colors.is_empty() {
            return Color32::WHITE;
        }

        if colors.len() == 1 {
            return colors[0];
        }

        let clamped = t.clamp(0.0, 1.0);
        let max_index = colors.len() - 1;
        let scaled = clamped * max_index as f32;
        let start_index = scaled.floor() as usize;
        let end_index = (start_index + 1).min(max_index);
        let local_t = scaled - start_index as f32;

        self.lerp_color(colors[start_index], colors[end_index], local_t)
    }

    fn tech_border_colors_for_system(&self, system_id: i64) -> Vec<Color32> {
        let mut technologies = self
            .system_tech_ids_by_system
            .get(&system_id)
            .into_iter()
            .flat_map(|ids| ids.iter().copied())
            .filter_map(|tech_id| self.tech_catalog.iter().find(|tech| tech.id == tech_id))
            .collect::<Vec<_>>();

        technologies.sort_by(|left, right| {
            right
                .display_priority
                .cmp(&left.display_priority)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });

        technologies
            .into_iter()
            .filter_map(|tech| {
                tech.color
                    .as_deref()
                    .and_then(Self::color_from_setting_value)
            })
            .take(self.tech_border_max_colors)
            .collect()
    }

    fn draw_gradient_card_border(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        border_width: f32,
        colors: &[Color32],
    ) {
        if colors.is_empty() {
            return;
        }

        if colors.len() == 1 {
            painter.rect_stroke(rect, 6.0, Stroke::new(border_width, colors[0]));
            return;
        }

        let samples = 96;
        let min = rect.min;
        let max = rect.max;
        let edge_points = |t: f32| -> Pos2 {
            let quarter = 0.25;
            let half = 0.5;
            let three_quarters = 0.75;

            if t <= quarter {
                let local = t / quarter;
                Pos2::new(min.x + (rect.width() * local), min.y)
            } else if t <= half {
                let local = (t - quarter) / quarter;
                Pos2::new(max.x, min.y + (rect.height() * local))
            } else if t <= three_quarters {
                let local = (t - half) / quarter;
                Pos2::new(max.x - (rect.width() * local), max.y)
            } else {
                let local = (t - three_quarters) / quarter;
                Pos2::new(min.x, max.y - (rect.height() * local))
            }
        };

        for index in 0..samples {
            let start_t = index as f32 / samples as f32;
            let end_t = (index + 1) as f32 / samples as f32;
            let color = self.gradient_color_at(colors, start_t);
            painter.line_segment(
                [edge_points(start_t), edge_points(end_t)],
                Stroke::new(border_width, color),
            );
        }
    }

    fn render_add_system_modal(&mut self, ctx: &egui::Context) {
        if !self.show_add_system_modal {
            return;
        }

        let mut open = self.show_add_system_modal;
        let mut close_requested = false;
        egui::Window::new("Add System")
            .collapsible(false)
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Name");
                let name_response = ui.text_edit_singleline(&mut self.new_system_name);
                if self.focus_add_system_name_on_open {
                    name_response.request_focus();
                    self.focus_add_system_name_on_open = false;
                }

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

                ui.separator();
                ui.label("Assign technologies (optional)");

                let selected_tech_label = self
                    .new_system_tech_id_for_assignment
                    .map(|id| self.tech_name_by_id(id))
                    .unwrap_or_else(|| "Select technology".to_owned());

                let previous_new_system_tech = self.new_system_tech_id_for_assignment;
                egui::ComboBox::from_label("Technology")
                    .selected_text(selected_tech_label)
                    .show_ui(ui, |ui| {
                        for tech in &self.tech_catalog {
                            ui.selectable_value(
                                &mut self.new_system_tech_id_for_assignment,
                                Some(tech.id),
                                tech.name.as_str(),
                            );
                        }
                    });

                if self.new_system_tech_id_for_assignment != previous_new_system_tech {
                    if let Some(tech_id) = self.new_system_tech_id_for_assignment {
                        self.new_system_assigned_tech_ids.insert(tech_id);
                        self.new_system_tech_id_for_assignment = None;
                    }
                }

                if self.new_system_assigned_tech_ids.is_empty() {
                    ui.label("No technologies selected.");
                } else {
                    let mut assigned_tech_snapshot = self
                        .new_system_assigned_tech_ids
                        .iter()
                        .copied()
                        .collect::<Vec<_>>();
                    assigned_tech_snapshot.sort_unstable();

                    for tech_id in assigned_tech_snapshot {
                        let tech_name = self.tech_name_by_id(tech_id);
                        ui.horizontal(|ui| {
                            ui.label(tech_name);
                            if ui.small_button("Remove").clicked() {
                                self.new_system_assigned_tech_ids.remove(&tech_id);
                            }
                        });
                    }
                }

                ui.horizontal(|ui| {
                    if ui.button("Create").clicked() {
                        self.create_system();
                        close_requested = true;
                    }

                    if ui.button("Cancel").clicked() {
                        close_requested = true;
                    }
                });
            });

        if close_requested {
            open = false;
        }

        self.show_add_system_modal = self.show_add_system_modal && open;
    }

    fn render_add_tech_modal(&mut self, ctx: &egui::Context) {
        if !self.show_add_tech_modal {
            return;
        }

        let mut open = self.show_add_tech_modal;
        let mut close_requested = false;
        egui::Window::new("Add Technology")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Technology name");
                let name_response = ui.text_edit_singleline(&mut self.new_tech_name);
                if self.focus_add_tech_name_on_open {
                    name_response.request_focus();
                    self.focus_add_tech_name_on_open = false;
                }

                ui.label("Description (optional)");
                ui.add(egui::TextEdit::multiline(&mut self.new_tech_description).desired_rows(3));

                ui.label("Documentation link (optional)");
                ui.text_edit_singleline(&mut self.new_tech_documentation_link);

                ui.horizontal(|ui| {
                    ui.label("Color (optional)");
                    let mut use_color = self.new_tech_color.is_some();
                    if ui.checkbox(&mut use_color, "Enable").changed() {
                        self.new_tech_color = if use_color {
                            Some(Color32::from_rgb(120, 180, 255))
                        } else {
                            None
                        };
                    }
                    if let Some(mut color) = self.new_tech_color {
                        if ui.color_edit_button_srgba(&mut color).changed() {
                            self.new_tech_color = Some(color);
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Display priority");
                    ui.add(egui::DragValue::new(&mut self.new_tech_display_priority));
                });

                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        self.create_tech_item();
                        close_requested = true;
                    }

                    if ui.button("Cancel").clicked() {
                        close_requested = true;
                    }
                });
            });

        if close_requested {
            open = false;
        }

        self.show_add_tech_modal = self.show_add_tech_modal && open;
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

                ui.separator();
                changed |= ui
                    .checkbox(
                        &mut self.show_tech_border_colors,
                        "Color card borders by technology",
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.tech_border_max_colors, 1..=5)
                            .text("Top tech colors"),
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

    fn render_save_catalog_modal(&mut self, ctx: &egui::Context) {
        if !self.show_save_catalog_modal {
            return;
        }

        let mut open = self.show_save_catalog_modal;
        egui::Window::new("Save Catalog")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("File path");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.save_catalog_path);
                    if ui.button("Browse...").clicked() {
                        let mut dialog = FileDialog::new();

                        if let Some(parent) = Path::new(&self.save_catalog_path).parent() {
                            if !parent.as_os_str().is_empty() {
                                dialog = dialog.set_directory(parent);
                            }
                        }

                        if let Some(file_name) = Path::new(&self.save_catalog_path)
                            .file_name()
                            .and_then(|name| name.to_str())
                            .filter(|name| !name.trim().is_empty())
                        {
                            dialog = dialog.set_file_name(file_name);
                        } else {
                            dialog = dialog.set_file_name("systems_catalog_export.db");
                        }

                        if let Some(path) = dialog
                            .add_filter("Catalog DB", &["db", "sqlite", "sqlite3"])
                            .save_file()
                        {
                            self.save_catalog_path = path.to_string_lossy().to_string();
                        }
                    }
                });

                if !self.recent_catalog_paths.is_empty() {
                    ui.label("Recent paths");
                    egui::ComboBox::from_id_source("save_catalog_recent_paths")
                        .selected_text("Choose recent path")
                        .show_ui(ui, |ui| {
                            for path in &self.recent_catalog_paths {
                                if ui.selectable_label(false, path.as_str()).clicked() {
                                    self.save_catalog_path = path.clone();
                                }
                            }
                        });
                }

                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        self.export_catalog();
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_save_catalog_modal = false;
                    }
                });
            });

        self.show_save_catalog_modal = open;
    }

    fn render_load_catalog_modal(&mut self, ctx: &egui::Context) {
        if !self.show_load_catalog_modal {
            return;
        }

        let mut open = self.show_load_catalog_modal;
        egui::Window::new("Load Catalog")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("File path");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.load_catalog_path);
                    if ui.button("Browse...").clicked() {
                        let mut dialog = FileDialog::new();

                        if let Some(parent) = Path::new(&self.load_catalog_path).parent() {
                            if !parent.as_os_str().is_empty() {
                                dialog = dialog.set_directory(parent);
                            }
                        }

                        if let Some(path) = dialog
                            .add_filter("Catalog DB", &["db", "sqlite", "sqlite3"])
                            .pick_file()
                        {
                            self.load_catalog_path = path.to_string_lossy().to_string();
                        }
                    }
                });

                if !self.recent_catalog_paths.is_empty() {
                    ui.label("Recent paths");
                    egui::ComboBox::from_id_source("load_catalog_recent_paths")
                        .selected_text("Choose recent path")
                        .show_ui(ui, |ui| {
                            for path in &self.recent_catalog_paths {
                                if ui.selectable_label(false, path.as_str()).clicked() {
                                    self.load_catalog_path = path.clone();
                                }
                            }
                        });
                }

                ui.horizontal(|ui| {
                    if ui.button("Load").clicked() {
                        self.import_catalog();
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_load_catalog_modal = false;
                    }
                });
            });

        self.show_load_catalog_modal = open;
    }

    fn render_new_catalog_confirm_modal(&mut self, ctx: &egui::Context) {
        if !self.show_new_catalog_confirm_modal {
            return;
        }

        let mut open = self.show_new_catalog_confirm_modal;
        egui::Window::new("New Catalog")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Create a new empty catalog?");
                ui.label("This removes all systems, links, notes, and technologies in the current catalog.");

                ui.horizontal(|ui| {
                    if ui.button("Create New Catalog").clicked() {
                        self.new_catalog();
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_new_catalog_confirm_modal = false;
                    }
                });
            });

        self.show_new_catalog_confirm_modal = open;
    }

    fn select_system(&mut self, system_id: i64) {
        self.selected_system_id = Some(system_id);
        if let Err(error) = self.load_selected_data(system_id) {
            self.status_message = format!("Failed to load selection: {error}");
        }
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.active_sidebar_tab, SidebarTab::Systems, "Systems");
            ui.selectable_value(
                &mut self.active_sidebar_tab,
                SidebarTab::TechCatalog,
                "Tech Catalog",
            );
        });
        ui.separator();

        match self.active_sidebar_tab {
            SidebarTab::Systems => {
                ui.heading("Systems List");
                ui.horizontal(|ui| {
                    // if ui.button("Refresh").clicked() {
                    //     if let Err(error) = self.refresh_systems() {
                    //         self.status_message = format!("Refresh failed: {error}");
                    //     }
                    // }
                    // ui.label(RichText::new("Hierarchy").weak());

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
                                    let button = egui::Button::new(icon).small();
                                    if ui.add_sized([18.0, 18.0], button).clicked() {
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
            }
            SidebarTab::TechCatalog => {
                ui.heading("Tech Catalog");
                ui.label("Technologies");
                if ui
                    .checkbox(
                        &mut self.fast_add_selected_catalog_tech_on_map,
                        "Fast add selected tech on map click",
                    )
                    .changed()
                {
                    self.settings_dirty = true;
                }
                let mut clicked_tech_id: Option<i64> = None;
                let editor_reserved_height = 220.0;
                let list_height = (ui.available_height() - editor_reserved_height).max(300.0);
                egui::ScrollArea::vertical()
                    .max_height(list_height)
                    .show(ui, |ui| {
                        for tech in &self.tech_catalog {
                            let was_selected =
                                self.selected_catalog_tech_id_for_edit == Some(tech.id);
                            let response = ui.add_sized(
                                [ui.available_width(), 22.0],
                                egui::SelectableLabel::new(was_selected, tech.name.as_str()),
                            );
                            if response.clicked() {
                                clicked_tech_id = Some(tech.id);
                            }
                        }
                    });

                if let Some(tech_id) = clicked_tech_id {
                    if self.selected_catalog_tech_id_for_edit == Some(tech_id) {
                        self.selected_catalog_tech_id_for_edit = None;
                        self.refresh_selected_tech_highlight();
                        self.edited_tech_name.clear();
                        self.edited_tech_description.clear();
                        self.edited_tech_documentation_link.clear();
                        self.edited_tech_color = None;
                        self.edited_tech_display_priority = 0;
                    } else {
                        self.selected_catalog_tech_id_for_edit = Some(tech_id);
                        self.refresh_selected_tech_highlight();
                        if let Some(tech) = self.tech_catalog.iter().find(|tech| tech.id == tech_id)
                        {
                            self.edited_tech_name = tech.name.clone();
                            self.edited_tech_description =
                                tech.description.clone().unwrap_or_default();
                            self.edited_tech_documentation_link =
                                tech.documentation_link.clone().unwrap_or_default();
                            self.edited_tech_color =
                                tech.color.as_deref().and_then(Self::color_from_setting_value);
                            self.edited_tech_display_priority = tech.display_priority;
                        }
                    }
                }

                ui.separator();

                ui.label("Name");
                ui.text_edit_singleline(&mut self.edited_tech_name);

                ui.label("Description (optional)");
                ui.add(
                    egui::TextEdit::multiline(&mut self.edited_tech_description).desired_rows(3),
                );

                ui.label("Documentation link (optional)");
                ui.text_edit_singleline(&mut self.edited_tech_documentation_link);

                ui.horizontal(|ui| {
                    ui.label("Color (optional)");
                    let mut use_color = self.edited_tech_color.is_some();
                    if ui.checkbox(&mut use_color, "Enable").changed() {
                        self.edited_tech_color = if use_color {
                            Some(Color32::from_rgb(120, 180, 255))
                        } else {
                            None
                        };
                    }
                    if let Some(mut color) = self.edited_tech_color {
                        if ui.color_edit_button_srgba(&mut color).changed() {
                            self.edited_tech_color = Some(color);
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Display priority");
                    ui.add(egui::DragValue::new(&mut self.edited_tech_display_priority));
                });

                ui.horizontal(|ui| {
                    if ui.button("Update").clicked() {
                        self.update_selected_catalog_tech();
                    }
                    if ui.button("Delete").clicked() {
                        self.delete_selected_catalog_tech();
                    }
                });
            }
        }
    }

    fn render_details(&mut self, ui: &mut egui::Ui) {
        ui.heading("System Details");

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
        ui.label("Name");
        ui.text_edit_singleline(&mut self.edited_system_name);

        ui.label("Description");
        ui.add(
            egui::TextEdit::multiline(&mut self.edited_system_description).desired_rows(3),
        );

        ui.separator();
        ui.label("Naming path");
        ui.checkbox(
            &mut self.selected_system_naming_root,
            "Treat this system as naming root",
        );
        ui.horizontal(|ui| {
            ui.label("Delimiter");
            ui.text_edit_singleline(&mut self.selected_system_naming_delimiter);
        });
        ui.label(format!("Current path: {}", self.naming_path_for_system(system.id)));

        ui.horizontal(|ui| {
            if ui.button("Save details").clicked() {
                self.update_selected_system_details();
            }
        });

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

        let previous_parent_id = self.selected_system_parent_id;
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

        if self.selected_system_parent_id != previous_parent_id {
            self.update_selected_system_parent();
        }

        ui.horizontal(|ui| {
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

        let previous_tech_id = self.selected_tech_id_for_assignment;
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

        if self.selected_tech_id_for_assignment != previous_tech_id
            && self.selected_tech_id_for_assignment.is_some()
        {
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
                            if let Some(link) = tech
                                .documentation_link
                                .as_deref()
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                            {
                                ui.hyperlink_to(tech.name.as_str(), link);
                            } else {
                                ui.label(tech.name.to_string());
                            }

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
        let selected_note_label = self
            .selected_note_id_for_edit
            .and_then(|id| {
                self.selected_notes
                    .iter()
                    .find(|note| note.id == id)
                    .map(|note| {
                        let snippet = note.body.trim();
                        let title = if snippet.is_empty() {
                            "(empty note)".to_owned()
                        } else {
                            snippet.chars().take(28).collect::<String>()
                        };
                        format!("#{} {} [{}]", note.id, title, note.updated_at)
                    })
            })
            .unwrap_or_else(|| "New note".to_owned());

        egui::ComboBox::from_label("Select note")
            .selected_text(selected_note_label)
            .show_ui(ui, |ui| {
                let notes_snapshot = self.selected_notes.clone();
                for note in notes_snapshot {
                    let snippet = note.body.trim();
                    let title = if snippet.is_empty() {
                        "(empty note)".to_owned()
                    } else {
                        snippet.chars().take(28).collect::<String>()
                    };

                    let label = format!("#{} {}", note.id, title);
                    let was_selected = self.selected_note_id_for_edit == Some(note.id);
                    if ui.selectable_label(was_selected, label).clicked() {
                        self.select_note_for_edit(note.id);
                    }
                }
            });

        ui.add(egui::TextEdit::multiline(&mut self.note_text).desired_rows(8));
        ui.horizontal(|ui| {
            if ui.button("New note").clicked() {
                self.create_note_for_selected_system();
            }
            if ui.button("Save note").clicked() {
                self.save_note();
            }
            if ui.button("Delete note").clicked() {
                self.delete_selected_note();
            }
        });
    }

    fn apply_map_zoom_anchored_to_view_center(&mut self, map_rect: Rect, target_zoom: f32) {
        let old_zoom = self.map_zoom;
        let new_zoom = target_zoom.clamp(0.25, 1.5);
        if (new_zoom - old_zoom).abs() <= f32::EPSILON {
            return;
        }

        let center = map_rect.center();
        let local_at_center = Pos2::new(
            (center.x - map_rect.left() - self.map_pan.x) / old_zoom,
            (center.y - map_rect.top() - self.map_pan.y) / old_zoom,
        );

        self.map_zoom = new_zoom;
        self.map_pan = Vec2::new(
            center.x - map_rect.left() - (local_at_center.x * new_zoom),
            center.y - map_rect.top() - (local_at_center.y * new_zoom),
        );
        self.settings_dirty = true;
    }

    fn render_map_canvas(&mut self, ui: &mut egui::Ui) {
        ui.heading("Mind Map");
        ui.label("Hold Space and drag to pan. Scroll to zoom. Shift+drag from a node to create an interaction. Drag on empty map space to box-select systems.");

        let mut requested_zoom: Option<f32> = None;

        ui.horizontal(|ui| {
            ui.label("Zoom");

            if ui.small_button("-").clicked() {
                requested_zoom = Some((self.map_zoom - 0.1).max(0.25));
            }

            if ui.small_button("+").clicked() {
                requested_zoom = Some((self.map_zoom + 0.1).min(1.5));
            }

            if ui.small_button("Reset zoom").clicked() {
                requested_zoom = Some(1.0);
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

            if ui.small_button("Clear selection set").clicked() {
                self.selected_map_system_ids.clear();
            }

            if ui
                .checkbox(&mut self.snap_to_grid, "Snap to grid")
                .changed()
            {
                self.settings_dirty = true;
            }

            ui.label(format!("{:.0}%", self.map_zoom * 100.0));
        });

        let mut desired_size = ui.available_size();
        desired_size.y = desired_size.y.max(420.0);

        let (map_rect, map_response) =
            ui.allocate_exact_size(desired_size, Sense::click_and_drag());
        let painter = ui.painter_at(map_rect);

        if map_response.clicked() {
            map_response.request_focus();
        }

        let space_down = ui.input(|input| input.key_down(egui::Key::Space));
        let pan_mode_active = space_down && map_response.dragged();
        if pan_mode_active {
            let pointer_delta = ui.input(|input| input.pointer.delta());
            self.map_pan += pointer_delta;
            self.settings_dirty = true;
        }

        let wheel_delta_y = ui.input(|input| input.smooth_scroll_delta.y);
        let pointer_over_map = ui
            .input(|input| input.pointer.hover_pos())
            .map(|pos| map_rect.contains(pos))
            .unwrap_or(false);
        let zoom_active = pointer_over_map || map_response.has_focus();
        if zoom_active && wheel_delta_y.abs() > f32::EPSILON {
            let zoom_step = (wheel_delta_y / 400.0).clamp(-0.15, 0.15);
            requested_zoom = Some((self.map_zoom + zoom_step).clamp(0.25, 1.5));
        }

        if let Some(target_zoom) = requested_zoom {
            self.apply_map_zoom_anchored_to_view_center(map_rect, target_zoom);
        }

        painter.rect_filled(map_rect, 6.0, Color32::from_gray(24));
        painter.rect_stroke(map_rect, 6.0, Stroke::new(1.0, Color32::from_gray(60)));

        self.ensure_map_positions();

        let zoom = self.map_zoom;
        let pan = self.map_pan;

        self.map_last_view_center_local = Some(Pos2::new(
            ((map_rect.center().x - map_rect.left() - pan.x) / zoom).clamp(0.0, MAP_WORLD_SIZE.x),
            ((map_rect.center().y - map_rect.top() - pan.y) / zoom).clamp(0.0, MAP_WORLD_SIZE.y),
        ));

        let to_screen = |local: Pos2| -> Pos2 {
            Pos2::new(
                map_rect.left() + pan.x + (local.x * zoom),
                map_rect.top() + pan.y + (local.y * zoom),
            )
        };

        let visible_local_min = Pos2::new(
            ((map_rect.left() - map_rect.left() - pan.x) / zoom).max(0.0),
            ((map_rect.top() - map_rect.top() - pan.y) / zoom).max(0.0),
        );
        let visible_local_max = Pos2::new(
            ((map_rect.right() - map_rect.left() - pan.x) / zoom).min(MAP_WORLD_SIZE.x),
            ((map_rect.bottom() - map_rect.top() - pan.y) / zoom).min(MAP_WORLD_SIZE.y),
        );

        let usable_world_top_left = to_screen(Pos2::new(0.0, 0.0));
        let usable_world_bottom_right = to_screen(Pos2::new(MAP_WORLD_SIZE.x, MAP_WORLD_SIZE.y));
        let usable_rect = Rect::from_min_max(usable_world_top_left, usable_world_bottom_right)
            .intersect(map_rect);

        if usable_rect.width() > 0.0 && usable_rect.height() > 0.0 {
            let grid_stroke = Stroke::new(1.0, Color32::from_gray(32));

            let mut x = (visible_local_min.x / MAP_GRID_SPACING).floor() * MAP_GRID_SPACING;
            while x <= visible_local_max.x {
                let screen_x = to_screen(Pos2::new(x, 0.0)).x;
                painter.line_segment(
                    [
                        Pos2::new(screen_x, usable_rect.top()),
                        Pos2::new(screen_x, usable_rect.bottom()),
                    ],
                    grid_stroke,
                );
                x += MAP_GRID_SPACING;
            }

            let mut y = (visible_local_min.y / MAP_GRID_SPACING).floor() * MAP_GRID_SPACING;
            while y <= visible_local_max.y {
                let screen_y = to_screen(Pos2::new(0.0, y)).y;
                painter.line_segment(
                    [
                        Pos2::new(usable_rect.left(), screen_y),
                        Pos2::new(usable_rect.right(), screen_y),
                    ],
                    grid_stroke,
                );
                y += MAP_GRID_SPACING;
            }
        }

        let visible_ids = self.visible_system_ids();
        let visible_systems = self
            .systems
            .iter()
            .filter(|system| visible_ids.contains(&system.id))
            .cloned()
            .collect::<Vec<_>>();

        let mut node_rects: HashMap<i64, Rect> = HashMap::new();
        for system in &visible_systems {
            // This is where rendering the system cards happens.
            if let Some(local_position) = self.map_positions.get(&system.id) {
                let node_size_screen = self.map_node_size_for(system.name.as_str()) * zoom;
                let rect = Rect::from_min_size(to_screen(*local_position), node_size_screen);
                node_rects.insert(system.id, rect);
            }
        }

        let selected_id = self.selected_system_id;
        let tech_filter_active = self.selected_catalog_tech_id_for_edit.is_some();

        if !space_down {
            if map_response.drag_started() {
                let pointer_pos = ui.input(|input| input.pointer.interact_pos());
                self.map_drag_started_on_node = pointer_pos
                    .map(|pointer| node_rects.values().any(|rect| rect.contains(pointer)))
                    .unwrap_or(false);

                if !self.map_drag_started_on_node {
                    self.map_selection_start_screen = pointer_pos;
                    self.map_selection_end_screen = pointer_pos;
                }
            }

            if map_response.dragged() && !self.map_drag_started_on_node {
                self.map_selection_end_screen = ui.input(|input| input.pointer.interact_pos());
            }

            if !self.map_drag_started_on_node {
                if let (Some(start), Some(end)) = (
                    self.map_selection_start_screen,
                    self.map_selection_end_screen,
                ) {
                    let selection_rect = Rect::from_two_pos(start, end);
                    painter.rect_stroke(selection_rect, 2.0, Stroke::new(1.5, Color32::WHITE));

                    let released = ui.input(|input| input.pointer.any_released());
                    if released {
                        self.selected_map_system_ids.clear();
                        for (system_id, rect) in &node_rects {
                            if selection_rect.intersects(*rect) {
                                self.selected_map_system_ids.insert(*system_id);
                            }
                        }

                        self.map_selection_start_screen = None;
                        self.map_selection_end_screen = None;
                    }
                }
            }

            if ui.input(|input| input.pointer.any_released()) {
                self.map_drag_started_on_node = false;
            }
        }

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
                let dimmed_for_tech = selected_id.is_some()
                    && tech_filter_active
                    && (!self
                        .systems_using_selected_catalog_tech
                        .contains(&parent_id)
                        || !self
                            .systems_using_selected_catalog_tech
                            .contains(&system.id));
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
                    dimmed || dimmed_for_tech,
                    selected_id.is_some() && boosted,
                );
            }
        }

        if self.show_interaction_lines {
            for (source_system_id, target_system_id) in self.deduped_visible_interaction_edges() {
                let Some(source_rect) = node_rects.get(&source_system_id) else {
                    continue;
                };
                let Some(target_rect) = node_rects.get(&target_system_id) else {
                    continue;
                };

                let dimmed = selected_id
                    .map(|id| id != source_system_id && id != target_system_id)
                    .unwrap_or(false);
                let dimmed_for_tech = selected_id.is_some()
                    && tech_filter_active
                    && (!self
                        .systems_using_selected_catalog_tech
                        .contains(&source_system_id)
                        || !self
                            .systems_using_selected_catalog_tech
                            .contains(&target_system_id));
                let boosted = selected_id
                    .map(|id| id == source_system_id || id == target_system_id)
                    .unwrap_or(false);

                let interaction_style =
                    self.interaction_line_style_for(source_system_id, target_system_id);

                let (from, to) = self.card_to_card_endpoints(*source_rect, *target_rect);

                self.draw_directed_connection(
                    &painter,
                    from,
                    to,
                    interaction_style,
                    dimmed || dimmed_for_tech,
                    selected_id.is_some() && boosted,
                );
            }
        }

        let mut snap_preview_positions: HashMap<i64, Pos2> = HashMap::new();

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
                } else if self.fast_add_selected_catalog_tech_on_map {
                    self.select_system(system.id);
                    self.selected_map_system_ids.clear();
                    self.selected_map_system_ids.insert(system.id);
                    self.fast_add_selected_catalog_tech_to_system(system.id);
                } else {
                    self.select_system(system.id);
                    self.selected_map_system_ids.clear();
                    self.selected_map_system_ids.insert(system.id);
                }
            }

            if response.drag_started() {
                let shift_held = ui.input(|input| input.modifiers.shift);
                if shift_held {
                    self.map_link_drag_from = Some(system.id);
                } else {
                    self.push_map_undo_snapshot();
                }
            }

            if response.dragged() {
                let shift_held = ui.input(|input| input.modifiers.shift);

                if self.map_link_drag_from == Some(system.id) || shift_held {
                    self.map_link_drag_from = Some(system.id);
                } else {
                    let pointer_delta = ui.input(|input| input.pointer.delta());
                    let local_delta = pointer_delta / self.map_zoom;

                    let move_ids = if self.selected_map_system_ids.contains(&system.id) {
                        self.selected_map_system_ids.clone()
                    } else {
                        let mut set = HashSet::new();
                        set.insert(system.id);
                        set
                    };

                    for move_id in &move_ids {
                        if let Some(existing_position) = self.map_positions.get(move_id).copied() {
                            let next_position = Pos2::new(
                                existing_position.x + local_delta.x,
                                existing_position.y + local_delta.y,
                            );
                            let move_node_size = self
                                .systems
                                .iter()
                                .find(|candidate| candidate.id == *move_id)
                                .map(|candidate| self.map_node_size_for(candidate.name.as_str()))
                                .unwrap_or(node_size);
                            let clamped =
                                self.clamp_node_position(map_rect, next_position, move_node_size);

                            if self.snap_to_grid {
                                let snapped = self.snap_to_open_grid_position(
                                    *move_id,
                                    clamped,
                                    move_node_size,
                                    &move_ids,
                                );
                                snap_preview_positions.insert(*move_id, snapped);
                            }

                            self.map_positions.insert(*move_id, clamped);
                        }
                    }
                }
            }

            if response.drag_stopped() && self.map_link_drag_from != Some(system.id) {
                let persist_ids = if self.selected_map_system_ids.contains(&system.id) {
                    self.selected_map_system_ids.clone()
                } else {
                    let mut set = HashSet::new();
                    set.insert(system.id);
                    set
                };

                if self.snap_to_grid {
                    for persist_id in &persist_ids {
                        if let Some(current_position) = self.map_positions.get(persist_id).copied()
                        {
                            let persist_node_size = self
                                .systems
                                .iter()
                                .find(|candidate| candidate.id == *persist_id)
                                .map(|candidate| self.map_node_size_for(candidate.name.as_str()))
                                .unwrap_or(MAP_NODE_SIZE);
                            let snapped = self.snap_to_open_grid_position(
                                *persist_id,
                                current_position,
                                persist_node_size,
                                &persist_ids,
                            );
                            self.map_positions.insert(*persist_id, snapped);
                        }
                    }
                }

                for persist_id in persist_ids {
                    if let Some(position) = self.map_positions.get(&persist_id).copied() {
                        self.persist_map_position(persist_id, position);
                    }
                }
            }

            let is_selected = self.selected_system_id == Some(system.id);
            let in_selected_set = self.selected_map_system_ids.contains(&system.id);
            let uses_selected_tech = self
                .systems_using_selected_catalog_tech
                .contains(&system.id);

            let fill = if tech_filter_active && !uses_selected_tech {
                Color32::from_gray(30)
            } else if is_selected || in_selected_set {
                Color32::from_gray(74)
            } else {
                Color32::from_gray(46)
            };

            let border_color = if tech_filter_active && uses_selected_tech {
                Color32::WHITE
            } else if in_selected_set {
                Color32::from_gray(210)
            } else {
                Color32::from_gray(120)
            };

            let border_width = if tech_filter_active && uses_selected_tech {
                2.0
            } else if in_selected_set {
                1.5
            } else {
                1.0
            };

            let tech_border_colors = if self.show_tech_border_colors {
                self.tech_border_colors_for_system(system.id)
            } else {
                Vec::new()
            };

            painter.rect_filled(node_rect, 6.0, fill);
            if tech_border_colors.is_empty() {
                painter.rect_stroke(node_rect, 6.0, Stroke::new(border_width, border_color));
            } else {
                self.draw_gradient_card_border(&painter, node_rect, border_width, &tech_border_colors);
            }
            let text_color = Color32::from_gray(230);
            let text_scale_multiplier = self.map_text_scale_multiplier();
            let font_size = ((15.0 * self.map_zoom).clamp(8.0, 22.0) * text_scale_multiplier)
                .clamp(6.0, 22.0);
            let font_id = FontId::proportional(font_size);
            let text_wrap_width =
                (node_rect.width() - (MAP_CARD_HORIZONTAL_PADDING * self.map_zoom)).max(24.0);
            let wrapped_text = painter.layout(
                system.name.to_owned(),
                font_id,
                text_color,
                text_wrap_width,
            );
            let text_pos = Pos2::new(
                node_rect.center().x - (wrapped_text.size().x * 0.5),
                node_rect.center().y - (wrapped_text.size().y * 0.5),
            );
            painter
                .with_clip_rect(node_rect.shrink(1.0))
                .galley(text_pos, wrapped_text, text_color);

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

        if self.snap_to_grid {
            for (system_id, preview_position) in &snap_preview_positions {
                let preview_node_size = self
                    .systems
                    .iter()
                    .find(|candidate| candidate.id == *system_id)
                    .map(|candidate| self.map_node_size_for(candidate.name.as_str()))
                    .unwrap_or(MAP_NODE_SIZE);

                let preview_rect =
                    Rect::from_min_size(to_screen(*preview_position), preview_node_size * zoom);

                painter.rect_filled(
                    preview_rect,
                    6.0,
                    Color32::from_rgba_unmultiplied(220, 220, 220, 16),
                );
                painter.rect_stroke(
                    preview_rect,
                    6.0,
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(235, 235, 235, 110)),
                );
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
        let open_add_system =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::N));
        let open_add_tech =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::ALT, egui::Key::N));
        let undo_map_change =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Z));

        if open_add_system {
            self.open_add_system_modal_with_prefill(self.selected_system_id);
        }

        if open_add_tech {
            self.show_add_tech_modal = true;
            self.focus_add_tech_name_on_open = true;
        }

        if undo_map_change {
            self.undo_map_positions();
        }

        if let Err(error) = self.validate_before_render() {
            self.status_message = format!("State warning: {error}");
        }

        egui::TopBottomPanel::top("header_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Add System").clicked() {
                    self.open_add_system_modal_with_prefill(self.selected_system_id);
                }
                if ui.button("Add Technology").clicked() {
                    self.show_add_tech_modal = true;
                }
                if ui.button("Save Catalog").clicked() {
                    self.show_save_catalog_modal = true;
                }
                if ui.button("Load Catalog").clicked() {
                    self.show_load_catalog_modal = true;
                }
                if ui.button("New Catalog").clicked() {
                    self.show_new_catalog_confirm_modal = true;
                }
                if ui.button("Connection Style").clicked() {
                    self.show_line_style_modal = true;
                }

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
        self.render_save_catalog_modal(ctx);
        self.render_load_catalog_modal(ctx);
        self.render_new_catalog_confirm_modal(ctx);
        self.render_line_style_modal(ctx);
        self.save_ui_settings_if_dirty();
    }
}
