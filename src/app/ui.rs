use std::collections::{HashMap, HashSet};
use std::path::Path;

use arboard::Clipboard;
use eframe::egui::{
    self, Align, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Shape, Stroke, Vec2,
};
use rfd::FileDialog;

use crate::models::SystemRecord;
use crate::app::{
    AppModal, ChildSpawnMode, InteractionKind, LineLayerDepth, LineLayerOrder, LinePattern,
    LineStyle, LineTerminator, SidebarTab, SystemsCatalogApp, ZoneDragKind, MAP_MAX_ZOOM,
    MAP_MIN_ZOOM, MAP_NODE_SIZE, MAP_WORLD_MAX_SIZE, MAP_WORLD_MIN_SIZE,
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
const INTERACTION_NOTE_POPUP_OPEN_DELAY_SECS: f64 = 0.2;
const INTERACTION_NOTE_POPUP_CLOSE_DELAY_SECS: f64 = 0.2;

impl SystemsCatalogApp {
    fn copy_selected_systems_to_clipboard(&mut self) {
        self.copy_selected_map_systems();

        let Some(payload) = self.copied_systems_payload() else {
            return;
        };

        match Clipboard::new().and_then(|mut clipboard| clipboard.set_text(payload)) {
            Ok(_) => {
                self.status_message.push_str(" (system clipboard updated)");
            }
            Err(error) => {
                self.status_message = format!("{} (clipboard write failed: {error})", self.status_message);
            }
        }
    }

    fn load_copied_systems_from_clipboard(&mut self) {
        let Ok(mut clipboard) = Clipboard::new() else {
            return;
        };

        let Ok(text) = clipboard.get_text() else {
            return;
        };

        if self.load_copied_systems_from_payload(text.as_str()) {
            self.status_message = format!(
                "Loaded {} card(s) from system clipboard",
                self.copied_system_entries.len()
            );
        }
    }

    fn point_to_segment_distance(point: Pos2, segment_start: Pos2, segment_end: Pos2) -> f32 {
        let segment = segment_end - segment_start;
        let segment_length_sq = segment.length_sq();
        if segment_length_sq <= f32::EPSILON {
            return point.distance(segment_start);
        }

        let to_point = point - segment_start;
        let projection = (to_point.dot(segment) / segment_length_sq).clamp(0.0, 1.0);
        let projected_point = segment_start + (segment * projection);
        point.distance(projected_point)
    }

    fn update_interaction_note_popup_state(
        &mut self,
        hovered: Option<crate::app::InteractionPopupState>,
        now_secs: f64,
    ) {
        if let Some(mut hovered_state) = hovered {
            self.interaction_popup_close_at_secs = None;

            if let Some(active) = self.interaction_popup_active.as_mut() {
                if active.source_system_name == hovered_state.source_system_name
                    && active.target_system_name == hovered_state.target_system_name
                    && active.note == hovered_state.note
                {
                    active.anchor_screen = hovered_state.anchor_screen;
                    self.interaction_popup_pending = None;
                    self.interaction_popup_pending_open_at_secs = None;
                    return;
                }
            }

            if let Some(pending) = self.interaction_popup_pending.as_mut() {
                let same_pending = pending.source_system_name == hovered_state.source_system_name
                    && pending.target_system_name == hovered_state.target_system_name
                    && pending.note == hovered_state.note;

                if same_pending {
                    pending.anchor_screen = hovered_state.anchor_screen;
                } else {
                    self.interaction_popup_pending = Some(hovered_state.clone());
                    self.interaction_popup_pending_open_at_secs =
                        Some(now_secs + INTERACTION_NOTE_POPUP_OPEN_DELAY_SECS);
                }
            } else {
                self.interaction_popup_pending = Some(hovered_state.clone());
                self.interaction_popup_pending_open_at_secs =
                    Some(now_secs + INTERACTION_NOTE_POPUP_OPEN_DELAY_SECS);
            }

            if let Some(open_at) = self.interaction_popup_pending_open_at_secs {
                if now_secs >= open_at {
                    if let Some(pending) = self.interaction_popup_pending.take() {
                        hovered_state.anchor_screen = pending.anchor_screen;
                        self.interaction_popup_active = Some(pending);
                        self.interaction_popup_pending_open_at_secs = None;
                    }
                }
            }

            return;
        }

        self.interaction_popup_pending = None;
        self.interaction_popup_pending_open_at_secs = None;

        if self.interaction_popup_active.is_some() {
            let close_at = self
                .interaction_popup_close_at_secs
                .get_or_insert(now_secs + INTERACTION_NOTE_POPUP_CLOSE_DELAY_SECS);

            if now_secs >= *close_at {
                self.interaction_popup_active = None;
                self.interaction_popup_close_at_secs = None;
            }
        }
    }

    fn render_interaction_note_popup(&self, painter: &egui::Painter, map_rect: Rect) {
        let Some(popup) = &self.interaction_popup_active else {
            return;
        };

        if popup.note.trim().is_empty() {
            return;
        }

        let text = format!(
            "{} -> {}\n{}",
            popup.source_system_name, popup.target_system_name, popup.note
        );

        let text_color = Color32::from_gray(235);
        let background = Color32::from_rgba_unmultiplied(28, 28, 30, 245);
        let border = Color32::from_gray(130);
        let padding = Vec2::new(10.0, 8.0);
        let max_text_width = 320.0;

        let galley = painter.layout(
            text,
            FontId::proportional(14.0),
            text_color,
            max_text_width,
        );

        let popup_size = galley.size() + (padding * 2.0);
        let mut popup_pos = popup.anchor_screen + Vec2::new(14.0, 14.0);

        let max_x = (map_rect.right() - popup_size.x - 6.0).max(map_rect.left() + 6.0);
        let max_y = (map_rect.bottom() - popup_size.y - 6.0).max(map_rect.top() + 6.0);
        popup_pos.x = popup_pos.x.clamp(map_rect.left() + 6.0, max_x);
        popup_pos.y = popup_pos.y.clamp(map_rect.top() + 6.0, max_y);

        let popup_rect = Rect::from_min_size(popup_pos, popup_size);
        painter.rect_filled(popup_rect, 6.0, background);
        painter.rect_stroke(popup_rect, 6.0, Stroke::new(1.0, border));
        painter.galley(popup_pos + padding, galley, text_color);
    }

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

        let max_columns = ((self.map_world_size.x / MAP_GRID_SPACING).ceil() as i32).max(1);
        let max_rows = ((self.map_world_size.y / MAP_GRID_SPACING).ceil() as i32).max(1);
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

    fn interaction_line_style_for_kind(
        &self,
        source_system_id: i64,
        target_system_id: i64,
        kind: InteractionKind,
    ) -> LineStyle {
        let mut style = match kind {
            InteractionKind::Standard => self.interaction_standard_line_style,
            InteractionKind::Pull => self.interaction_pull_line_style,
            InteractionKind::Push => self.interaction_push_line_style,
            InteractionKind::Bidirectional => self.interaction_bidirectional_line_style,
        };

        style.width = self.interaction_line_style.width;

        if let Some(override_color) = self.system_line_override_color(source_system_id) {
            style.color = override_color;
        } else if let Some(override_color) = self.system_line_override_color(target_system_id) {
            style.color = override_color;
        }

        style
    }

    fn rect_side_midpoint(rect: Rect, direction_from_center: Vec2) -> Pos2 {
        let center = rect.center();
        if direction_from_center.length_sq() <= f32::EPSILON {
            return center;
        }

        if direction_from_center.x.abs() >= direction_from_center.y.abs() {
            if direction_from_center.x >= 0.0 {
                Pos2::new(rect.right(), center.y)
            } else {
                Pos2::new(rect.left(), center.y)
            }
        } else if direction_from_center.y >= 0.0 {
            Pos2::new(center.x, rect.bottom())
        } else {
            Pos2::new(center.x, rect.top())
        }
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

    fn rect_anchor_point(rect: Rect, direction_from_center: Vec2, pattern: LinePattern) -> Pos2 {
        match pattern {
            LinePattern::Mitered => Self::rect_side_midpoint(rect, direction_from_center),
            LinePattern::Solid | LinePattern::Dashed => {
                Self::rect_edge_point(rect, direction_from_center)
            }
        }
    }

    fn card_to_card_endpoints(
        &self,
        from_rect: Rect,
        to_rect: Rect,
        pattern: LinePattern,
    ) -> (Pos2, Pos2) {
        let from_center = from_rect.center();
        let to_center = to_rect.center();
        let direction = to_center - from_center;

        if direction.length_sq() <= f32::EPSILON {
            return (from_center, to_center);
        }

        let (outgoing_direction, incoming_direction) = match pattern {
            LinePattern::Mitered => {
                let near_straight_threshold = 6.0;
                if direction.x.abs() <= near_straight_threshold
                    || direction.y.abs() <= near_straight_threshold
                {
                    (direction, direction)
                } else {
                let mut start = Self::rect_side_midpoint(from_rect, direction);
                let mut end = Self::rect_side_midpoint(to_rect, -direction);

                for _ in 0..2 {
                    let delta = end - start;
                    let horizontal_first = delta.x.abs() >= delta.y.abs();

                    let horizontal_component = if direction.x.abs() <= f32::EPSILON {
                        if to_center.x >= from_center.x { 1.0 } else { -1.0 }
                    } else {
                        direction.x
                    };

                    let vertical_component = if direction.y.abs() <= f32::EPSILON {
                        if to_center.y >= from_center.y { 1.0 } else { -1.0 }
                    } else {
                        direction.y
                    };

                    let outgoing = if horizontal_first {
                        Vec2::new(horizontal_component, 0.0)
                    } else {
                        Vec2::new(0.0, vertical_component)
                    };
                    let incoming = if horizontal_first {
                        Vec2::new(0.0, vertical_component)
                    } else {
                        Vec2::new(horizontal_component, 0.0)
                    };

                    start = Self::rect_side_midpoint(from_rect, outgoing);
                    end = Self::rect_side_midpoint(to_rect, -incoming);
                }

                (start - from_center, to_center - end)
                }
            }
            LinePattern::Solid | LinePattern::Dashed => (direction, direction),
        };

        let start = Self::rect_anchor_point(from_rect, outgoing_direction, pattern);
        let end = Self::rect_anchor_point(to_rect, -incoming_direction, pattern);
        (start, end)
    }

    fn rect_to_point_endpoint(&self, from_rect: Rect, to_point: Pos2, pattern: LinePattern) -> Pos2 {
        let direction = to_point - from_rect.center();
        Self::rect_anchor_point(from_rect, direction, pattern)
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
        let mut points = match style.pattern {
            LinePattern::Mitered => {
                let dx = to.x - from.x;
                let dy = to.y - from.y;
                let elbow = if dx.abs() >= dy.abs() {
                    Pos2::new(to.x, from.y)
                } else {
                    Pos2::new(from.x, to.y)
                };

                if from.distance(elbow) < 2.0 || elbow.distance(to) < 2.0 {
                    vec![from, to]
                } else {
                    vec![from, elbow, to]
                }
            }
            LinePattern::Solid | LinePattern::Dashed => vec![from, to],
        };

        if points.len() < 2 {
            return;
        }

        let last_index = points.len() - 1;
        let last_start = points[last_index - 1];
        let last_end = points[last_index];
        let tail = last_end - last_start;
        if tail.length_sq() <= f32::EPSILON {
            return;
        }

        let unit = tail.normalized();
        let normal = Vec2::new(-unit.y, unit.x);
        let arrow_size = (9.0 * self.map_zoom).clamp(7.0, 18.0);

        match style.terminator {
            LineTerminator::None => {
                self.draw_line_path(painter, &points, stroke, style.pattern);
            }
            LineTerminator::Arrow => {
                let line_end = to - (unit * (arrow_size + 2.0));
                points[last_index] = line_end;
                self.draw_line_path(painter, &points, stroke, style.pattern);

                let arrow_left = to - (unit * arrow_size) + (normal * (arrow_size * 0.45));
                let arrow_right = to - (unit * arrow_size) - (normal * (arrow_size * 0.45));

                painter.line_segment([to, arrow_left], stroke);
                painter.line_segment([to, arrow_right], stroke);
                painter.line_segment([arrow_left, arrow_right], stroke);
            }
            LineTerminator::FilledArrow => {
                let tip = to;
                let base = to - (unit * arrow_size);
                let arrow_left = base + (normal * (arrow_size * 0.5));
                let arrow_right = base - (normal * (arrow_size * 0.5));
                let line_end = base - (unit * 1.0);

                points[last_index] = line_end;
                self.draw_line_path(painter, &points, stroke, style.pattern);
                painter.add(Shape::convex_polygon(
                    vec![tip, arrow_left, arrow_right],
                    color,
                    Stroke::NONE,
                ));
            }
        }
    }

    fn draw_bidirectional_connection(
        &self,
        painter: &egui::Painter,
        from: Pos2,
        to: Pos2,
        style: LineStyle,
        dimmed: bool,
        boosted: bool,
    ) {
        if style.terminator == LineTerminator::None {
            self.draw_directed_connection(painter, from, to, style, dimmed, boosted);
            return;
        }

        self.draw_directed_connection(painter, from, to, style, dimmed, boosted);
        self.draw_directed_connection(painter, to, from, style, dimmed, boosted);
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

    fn pattern_label(pattern: LinePattern) -> &'static str {
        match pattern {
            LinePattern::Solid => "Solid",
            LinePattern::Dashed => "Dashed",
            LinePattern::Mitered => "Mitered",
        }
    }

    fn render_pattern_combo(ui: &mut egui::Ui, id: &str, label: &str, pattern: &mut LinePattern) {
        egui::ComboBox::from_id_source(id)
            .selected_text(Self::pattern_label(*pattern))
            .show_ui(ui, |ui| {
                ui.selectable_value(pattern, LinePattern::Solid, "Solid");
                ui.selectable_value(pattern, LinePattern::Dashed, "Dashed");
                ui.selectable_value(pattern, LinePattern::Mitered, "Mitered");
            });
        ui.label(label);
    }

    fn line_layer_depth_label(depth: LineLayerDepth) -> &'static str {
        match depth {
            LineLayerDepth::BehindCards => "Behind cards",
            LineLayerDepth::AboveCards => "Above cards",
        }
    }

    fn line_layer_order_label(order: LineLayerOrder) -> &'static str {
        match order {
            LineLayerOrder::ParentThenInteraction => "Parent below interaction",
            LineLayerOrder::InteractionThenParent => "Parent above interaction",
        }
    }

    fn draw_parent_lines_layer(
        &self,
        painter: &egui::Painter,
        visible_systems: &[SystemRecord],
        node_rects: &HashMap<i64, Rect>,
        selected_id: Option<i64>,
        tech_filter_active: bool,
    ) {
        if !self.show_parent_lines {
            return;
        }

        for system in visible_systems {
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
            let (from, to) =
                self.card_to_card_endpoints(*parent_rect, *child_rect, parent_style.pattern);

            self.draw_directed_connection(
                painter,
                from,
                to,
                parent_style,
                dimmed || dimmed_for_tech,
                selected_id.is_some() && boosted,
            );
        }
    }

    fn draw_interaction_lines_layer(
        &self,
        painter: &egui::Painter,
        map_rect: Rect,
        pointer_hover: Option<Pos2>,
        node_rects: &HashMap<i64, Rect>,
        selected_id: Option<i64>,
        tech_filter_active: bool,
        focused_flow_edges: &HashSet<(i64, i64)>,
        focused_flow_highlight_active: bool,
    ) -> Option<(crate::app::InteractionPopupState, f32)> {
        if !self.show_interaction_lines {
            return None;
        }

        let mut closest_hovered_interaction: Option<(crate::app::InteractionPopupState, f32)> =
            None;

        for interaction in self.deduped_visible_interactions() {
            let source_system_id = interaction.source_system_id;
            let target_system_id = interaction.target_system_id;

            let Some(source_rect) = node_rects.get(&source_system_id) else {
                continue;
            };
            let Some(target_rect) = node_rects.get(&target_system_id) else {
                continue;
            };

            let in_primary_selection = selected_id
                .map(|id| id == source_system_id || id == target_system_id)
                .unwrap_or(false);
            let in_selection_set = self.selected_map_system_ids.contains(&source_system_id)
                || self.selected_map_system_ids.contains(&target_system_id);
            let has_any_selection = selected_id.is_some() || !self.selected_map_system_ids.is_empty();

            let dimmed = has_any_selection && !(in_primary_selection || in_selection_set);
            let dimmed_for_tech = selected_id.is_some()
                && tech_filter_active
                && (!self
                    .systems_using_selected_catalog_tech
                    .contains(&source_system_id)
                    || !self
                        .systems_using_selected_catalog_tech
                        .contains(&target_system_id));
            let boosted = in_primary_selection || in_selection_set;
            let in_focused_flow_path = match interaction.kind {
                InteractionKind::Standard | InteractionKind::Push => {
                    focused_flow_edges.contains(&(source_system_id, target_system_id))
                }
                InteractionKind::Pull => {
                    focused_flow_edges.contains(&(target_system_id, source_system_id))
                }
                InteractionKind::Bidirectional => {
                    focused_flow_edges.contains(&(source_system_id, target_system_id))
                        || focused_flow_edges.contains(&(target_system_id, source_system_id))
                }
            };
            let dimmed_for_focused_flow = focused_flow_highlight_active && !in_focused_flow_path;

            let interaction_style =
                self.interaction_line_style_for_kind(source_system_id, target_system_id, interaction.kind);

            let (from, to) = match interaction.kind {
                InteractionKind::Pull => self.card_to_card_endpoints(
                    *target_rect,
                    *source_rect,
                    interaction_style.pattern,
                ),
                InteractionKind::Push | InteractionKind::Standard => self.card_to_card_endpoints(
                    *source_rect,
                    *target_rect,
                    interaction_style.pattern,
                ),
                InteractionKind::Bidirectional => self.card_to_card_endpoints(
                    *source_rect,
                    *target_rect,
                    interaction_style.pattern,
                ),
            };

            if interaction.kind == InteractionKind::Bidirectional {
                self.draw_bidirectional_connection(
                    painter,
                    from,
                    to,
                    interaction_style,
                    dimmed || dimmed_for_tech || dimmed_for_focused_flow,
                    (selected_id.is_some() && boosted) || in_focused_flow_path,
                );
            } else {
                self.draw_directed_connection(
                    painter,
                    from,
                    to,
                    interaction_style,
                    dimmed || dimmed_for_tech || dimmed_for_focused_flow,
                    (selected_id.is_some() && boosted) || in_focused_flow_path,
                );
            }

            if !interaction.note.trim().is_empty() {
                if let Some(pointer) = pointer_hover.filter(|pos| map_rect.contains(*pos)) {
                    let hover_distance = Self::point_to_segment_distance(pointer, from, to);
                    let hover_threshold = (10.0 * self.map_zoom).clamp(8.0, 18.0);
                    if hover_distance <= hover_threshold {
                        let popup_state = crate::app::InteractionPopupState {
                            source_system_name: self.system_name_by_id(source_system_id),
                            target_system_name: self.system_name_by_id(target_system_id),
                            note: interaction.note.clone(),
                            anchor_screen: pointer,
                        };

                        match &closest_hovered_interaction {
                            Some((_, best_distance)) if *best_distance <= hover_distance => {}
                            _ => {
                                closest_hovered_interaction = Some((popup_state, hover_distance));
                            }
                        }
                    }
                }
            }
        }

        closest_hovered_interaction
    }

    fn draw_dashed_segment(&self, painter: &egui::Painter, from: Pos2, to: Pos2, stroke: Stroke) {
        let direction = to - from;
        let distance = direction.length();
        if distance < 1.0 {
            return;
        }

        let unit = direction / distance;
        let dash_len = (10.0 * self.map_zoom).clamp(6.0, 20.0);
        let gap_len = (6.0 * self.map_zoom).clamp(4.0, 12.0);
        let step = dash_len + gap_len;

        let mut offset = 0.0;
        while offset < distance {
            let start = from + (unit * offset);
            let end_offset = (offset + dash_len).min(distance);
            let end = from + (unit * end_offset);
            painter.line_segment([start, end], stroke);
            offset += step;
        }
    }

    fn draw_line_path(
        &self,
        painter: &egui::Painter,
        points: &[Pos2],
        stroke: Stroke,
        pattern: LinePattern,
    ) {
        if points.len() < 2 {
            return;
        }

        for segment in points.windows(2) {
            let from = segment[0];
            let to = segment[1];
            if pattern == LinePattern::Dashed {
                self.draw_dashed_segment(painter, from, to, stroke);
            } else {
                painter.line_segment([from, to], stroke);
            }
        }
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
                    .map(|color| Color32::from_rgb(color.r(), color.g(), color.b()))
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

                ui.horizontal(|ui| {
                    ui.label("Child placement");
                    egui::ComboBox::from_id_source("new_child_spawn_mode")
                        .selected_text(match self.new_child_spawn_mode {
                            ChildSpawnMode::RightOfPrevious => "Right of previous",
                            ChildSpawnMode::BelowPrevious => "Below previous",
                        })
                        .show_ui(ui, |ui| {
                            let right_changed = ui
                                .selectable_value(
                                    &mut self.new_child_spawn_mode,
                                    ChildSpawnMode::RightOfPrevious,
                                    "Right of previous",
                                )
                                .changed();
                            let below_changed = ui
                                .selectable_value(
                                    &mut self.new_child_spawn_mode,
                                    ChildSpawnMode::BelowPrevious,
                                    "Below previous",
                                )
                                .changed();

                            if right_changed || below_changed {
                                self.settings_dirty = true;
                            }
                        });
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

    fn render_bulk_add_systems_modal(&mut self, ctx: &egui::Context) {
        if !self.show_bulk_add_systems_modal {
            return;
        }

        let mut open = self.show_bulk_add_systems_modal;
        let mut close_requested = false;

        egui::Window::new("Bulk Add Systems")
            .collapsible(false)
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Enter comma-separated system names");
                ui.label("Example: users, orders, invoices");
                let input_response = ui.add(
                    egui::TextEdit::multiline(&mut self.bulk_new_system_names)
                        .desired_rows(6)
                        .hint_text("users, orders, invoices"),
                );
                if self.focus_bulk_add_system_names_on_open {
                    input_response.request_focus();
                    self.focus_bulk_add_system_names_on_open = false;
                }

                let selected_parent_label = self
                    .bulk_new_system_parent_id
                    .map(|id| self.system_name_by_id(id))
                    .unwrap_or_else(|| "No parent (root systems)".to_owned());

                egui::ComboBox::from_label("Parent")
                    .selected_text(selected_parent_label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.bulk_new_system_parent_id,
                            None,
                            "No parent (root systems)",
                        );

                        for system in &self.systems {
                            ui.selectable_value(
                                &mut self.bulk_new_system_parent_id,
                                Some(system.id),
                                system.name.as_str(),
                            );
                        }
                    });

                ui.horizontal(|ui| {
                    if ui.button("Create all").clicked() {
                        self.create_systems_bulk_from_list();
                        close_requested = !self.show_bulk_add_systems_modal;
                    }

                    if ui.button("Cancel").clicked() {
                        close_requested = true;
                    }
                });
            });

        if close_requested {
            open = false;
        }

        self.show_bulk_add_systems_modal = self.show_bulk_add_systems_modal && open;
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

                let old_parent_pattern = self.parent_line_style.pattern;
                Self::render_pattern_combo(
                    ui,
                    "parent_pattern",
                    "Line pattern",
                    &mut self.parent_line_style.pattern,
                );
                if old_parent_pattern != self.parent_line_style.pattern {
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
                    ui.label("Line layer");
                    let previous = self.line_layer_depth;
                    egui::ComboBox::from_id_source("line_layer_depth")
                        .selected_text(Self::line_layer_depth_label(self.line_layer_depth))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.line_layer_depth,
                                LineLayerDepth::BehindCards,
                                "Behind cards",
                            );
                            ui.selectable_value(
                                &mut self.line_layer_depth,
                                LineLayerDepth::AboveCards,
                                "Above cards",
                            );
                        });
                    if previous != self.line_layer_depth {
                        changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Parent vs interaction");
                    let previous = self.line_layer_order;
                    egui::ComboBox::from_id_source("line_layer_order")
                        .selected_text(Self::line_layer_order_label(self.line_layer_order))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.line_layer_order,
                                LineLayerOrder::ParentThenInteraction,
                                "Parent below interaction",
                            );
                            ui.selectable_value(
                                &mut self.line_layer_order,
                                LineLayerOrder::InteractionThenParent,
                                "Parent above interaction",
                            );
                        });
                    if previous != self.line_layer_order {
                        changed = true;
                    }
                });

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
                self.interaction_standard_line_style.width = self.interaction_line_style.width;
                self.interaction_pull_line_style.width = self.interaction_line_style.width;
                self.interaction_push_line_style.width = self.interaction_line_style.width;
                self.interaction_bidirectional_line_style.width = self.interaction_line_style.width;

                ui.label("Per interaction type");

                ui.group(|ui| {
                    ui.label("Standard");
                    ui.horizontal(|ui| {
                        ui.label("Color");
                        changed |= ui
                            .color_edit_button_srgba(&mut self.interaction_standard_line_style.color)
                            .changed();
                    });
                    let old_terminator = self.interaction_standard_line_style.terminator;
                    Self::render_terminator_combo(
                        ui,
                        "interaction_standard_terminator",
                        "Arrow type",
                        &mut self.interaction_standard_line_style.terminator,
                    );
                    if old_terminator != self.interaction_standard_line_style.terminator {
                        changed = true;
                    }
                    let old_pattern = self.interaction_standard_line_style.pattern;
                    Self::render_pattern_combo(
                        ui,
                        "interaction_standard_pattern",
                        "Line type",
                        &mut self.interaction_standard_line_style.pattern,
                    );
                    if old_pattern != self.interaction_standard_line_style.pattern {
                        changed = true;
                    }
                });

                ui.group(|ui| {
                    ui.label("Pull");
                    ui.horizontal(|ui| {
                        ui.label("Color");
                        changed |= ui
                            .color_edit_button_srgba(&mut self.interaction_pull_line_style.color)
                            .changed();
                    });
                    let old_terminator = self.interaction_pull_line_style.terminator;
                    Self::render_terminator_combo(
                        ui,
                        "interaction_pull_terminator",
                        "Arrow type",
                        &mut self.interaction_pull_line_style.terminator,
                    );
                    if old_terminator != self.interaction_pull_line_style.terminator {
                        changed = true;
                    }
                    let old_pattern = self.interaction_pull_line_style.pattern;
                    Self::render_pattern_combo(
                        ui,
                        "interaction_pull_pattern",
                        "Line type",
                        &mut self.interaction_pull_line_style.pattern,
                    );
                    if old_pattern != self.interaction_pull_line_style.pattern {
                        changed = true;
                    }
                });

                ui.group(|ui| {
                    ui.label("Push");
                    ui.horizontal(|ui| {
                        ui.label("Color");
                        changed |= ui
                            .color_edit_button_srgba(&mut self.interaction_push_line_style.color)
                            .changed();
                    });
                    let old_terminator = self.interaction_push_line_style.terminator;
                    Self::render_terminator_combo(
                        ui,
                        "interaction_push_terminator",
                        "Arrow type",
                        &mut self.interaction_push_line_style.terminator,
                    );
                    if old_terminator != self.interaction_push_line_style.terminator {
                        changed = true;
                    }
                    let old_pattern = self.interaction_push_line_style.pattern;
                    Self::render_pattern_combo(
                        ui,
                        "interaction_push_pattern",
                        "Line type",
                        &mut self.interaction_push_line_style.pattern,
                    );
                    if old_pattern != self.interaction_push_line_style.pattern {
                        changed = true;
                    }
                });

                ui.group(|ui| {
                    ui.label("Bidirectional");
                    ui.horizontal(|ui| {
                        ui.label("Color");
                        changed |= ui
                            .color_edit_button_srgba(
                                &mut self.interaction_bidirectional_line_style.color,
                            )
                            .changed();
                    });
                    let old_terminator = self.interaction_bidirectional_line_style.terminator;
                    Self::render_terminator_combo(
                        ui,
                        "interaction_bidirectional_terminator",
                        "Arrow type",
                        &mut self.interaction_bidirectional_line_style.terminator,
                    );
                    if old_terminator != self.interaction_bidirectional_line_style.terminator {
                        changed = true;
                    }
                    let old_pattern = self.interaction_bidirectional_line_style.pattern;
                    Self::render_pattern_combo(
                        ui,
                        "interaction_bidirectional_pattern",
                        "Line type",
                        &mut self.interaction_bidirectional_line_style.pattern,
                    );
                    if old_pattern != self.interaction_bidirectional_line_style.pattern {
                        changed = true;
                    }
                });

                ui.separator();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.tech_border_max_colors, 1..=5)
                            .text("Top tech colors"),
                    )
                    .changed();

                if changed {
                    self.settings_dirty = true;
                }

            });

        self.show_line_style_modal = open;
    }

    fn render_hotkeys_modal(&mut self, ctx: &egui::Context) {
        if !self.show_hotkeys_modal {
            return;
        }

        let mut open = self.show_hotkeys_modal;
        egui::Window::new("Hotkeys")
            .collapsible(false)
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Ctrl+N  -> Add System");
                ui.label("Ctrl+Shift+N  -> Bulk Add Systems");
                ui.label("Alt+N  -> Add Technology");
                ui.label("Alt+C  -> Copy highlighted cards");
                ui.label("Alt+V  -> Paste copied cards");
                ui.label("Delete  -> Delete selected system");
                ui.label("Ctrl+Z  -> Undo map move");
                ui.label("Esc  -> Close most recently opened modal");
                ui.separator();
                ui.label("Ctrl+Click  -> Select system + descendants");
                ui.label("Alt+Click  -> Select system + ancestors");
                ui.separator();
                ui.label("Shift + drag (child -> parent)  -> Assign parent");
                ui.label("Ctrl+R + drag (A -> B)  -> Standard interaction");
                ui.label("Ctrl+B + drag (A -> B)  -> Pull interaction");
                ui.label("Ctrl+F + drag (A -> B)  -> Push interaction");
                ui.label("Ctrl+D + drag (A <-> B)  -> Bidirectional interaction");
                if ui.button("Close").clicked() {
                    self.show_hotkeys_modal = false;
                }
            });

        self.show_hotkeys_modal = open;
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
                            self.edited_link_note = link.note.clone();
                            self.edited_link_kind =
                                Self::interaction_kind_from_setting_value(link.kind.as_str());
                        }
                    }
                });

            ui.label("Interaction label");
            ui.text_edit_singleline(&mut self.edited_link_label);
            egui::ComboBox::from_label("Interaction type")
                .selected_text(Self::interaction_kind_label(self.edited_link_kind))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.edited_link_kind,
                        InteractionKind::Standard,
                        Self::interaction_kind_label(InteractionKind::Standard),
                    );
                    ui.selectable_value(
                        &mut self.edited_link_kind,
                        InteractionKind::Pull,
                        Self::interaction_kind_label(InteractionKind::Pull),
                    );
                    ui.selectable_value(
                        &mut self.edited_link_kind,
                        InteractionKind::Push,
                        Self::interaction_kind_label(InteractionKind::Push),
                    );
                    ui.selectable_value(
                        &mut self.edited_link_kind,
                        InteractionKind::Bidirectional,
                        Self::interaction_kind_label(InteractionKind::Bidirectional),
                    );
                });
            ui.label("Interaction note");
            ui.add(egui::TextEdit::multiline(&mut self.edited_link_note).desired_rows(3));
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

        ui.separator();
        ui.label("Focused flow inspector");

        let from_label = self
            .flow_inspector_from_system_id
            .map(|id| self.system_name_by_id(id))
            .unwrap_or_else(|| "Select source system".to_owned());
        let to_label = self
            .flow_inspector_to_system_id
            .map(|id| self.system_name_by_id(id))
            .unwrap_or_else(|| "Select target system".to_owned());

        egui::ComboBox::from_label("From")
            .selected_text(from_label)
            .show_ui(ui, |ui| {
                for candidate in &self.systems {
                    ui.selectable_value(
                        &mut self.flow_inspector_from_system_id,
                        Some(candidate.id),
                        candidate.name.as_str(),
                    );
                }
            });

        egui::ComboBox::from_label("To")
            .selected_text(to_label)
            .show_ui(ui, |ui| {
                for candidate in &self.systems {
                    ui.selectable_value(
                        &mut self.flow_inspector_to_system_id,
                        Some(candidate.id),
                        candidate.name.as_str(),
                    );
                }
            });

        if let (Some(from_id), Some(to_id)) =
            (self.flow_inspector_from_system_id, self.flow_inspector_to_system_id)
        {
            if from_id == to_id {
                ui.label("Select two different systems.");
            } else if let Some(path) = self.focused_flow_shortest_path(from_id, to_id) {
                if path.is_empty() {
                    ui.label("Source and target are the same system.");
                } else {
                    ui.label("Shortest data-flow path");
                    for (from, kind, to) in path {
                        ui.label(format!(
                            "{} -[{}]-> {}",
                            self.system_name_by_id(from),
                            Self::interaction_kind_label(kind),
                            self.system_name_by_id(to)
                        ));
                    }
                }
            } else {
                ui.label("No directed data-flow path found with current interactions.");
            }
        } else {
            ui.label("Pick source and target to inspect data flow.");
        }
    }

    fn apply_map_zoom_anchored_to_view_center(&mut self, map_rect: Rect, target_zoom: f32) {
        let old_zoom = self.map_zoom;
        let new_zoom = target_zoom.clamp(MAP_MIN_ZOOM, MAP_MAX_ZOOM);
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
        ui.label("Hold Space and drag to pan. Scroll to zoom. Shift+drag child -> parent to assign parent. Ctrl+R/B/F + drag creates Standard/Pull/Push interaction. Alt+C/Alt+V copies and pastes highlighted cards. Drag on empty map space to box-select systems.");

        let mut requested_zoom: Option<f32> = None;

        ui.horizontal(|ui| {
            ui.label("Zoom");

            if ui.small_button("-").clicked() {
                requested_zoom = Some((self.map_zoom - 0.1).max(MAP_MIN_ZOOM));
            }

            if ui.small_button("+").clicked() {
                requested_zoom = Some((self.map_zoom + 0.1).min(MAP_MAX_ZOOM));
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

        ui.horizontal(|ui| {
            ui.label("Canvas");

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
                self.map_world_size.x = width.clamp(MAP_WORLD_MIN_SIZE.x, MAP_WORLD_MAX_SIZE.x);
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
                self.map_world_size.y = height.clamp(MAP_WORLD_MIN_SIZE.y, MAP_WORLD_MAX_SIZE.y);
                self.settings_dirty = true;
            }
        });

        ui.horizontal(|ui| {
            let draw_zone_label = if self.zone_draw_mode {
                "Drawing zones: ON"
            } else {
                "Draw Zone"
            };

            if ui.selectable_label(self.zone_draw_mode, draw_zone_label).clicked() {
                self.zone_draw_mode = !self.zone_draw_mode;
                self.zone_draw_start_screen = None;
                self.zone_draw_end_screen = None;
                self.zone_drag_kind = None;
                self.zone_drag_start_local = None;
            }

            if let Some(_zone_id) = self.selected_zone_id {
                ui.label("Zone");
                if ui
                    .text_edit_singleline(&mut self.selected_zone_name)
                    .changed()
                {
                    self.update_selected_zone_properties();
                }

                if ui
                    .color_edit_button_srgba(&mut self.selected_zone_color)
                    .changed()
                {
                    self.update_selected_zone_properties();
                }

                let mut render_priority = self.selected_zone_render_priority;
                if ui
                    .add(
                        egui::DragValue::new(&mut render_priority)
                            .speed(1.0)
                            .prefix("Priority "),
                    )
                    .changed()
                {
                    self.selected_zone_render_priority = render_priority;
                    self.update_selected_zone_properties();
                }

                if ui.small_button("Delete Zone").clicked() {
                    self.delete_selected_zone();
                }
            }
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
            let zoom_step = (wheel_delta_y / 520.0).clamp(-0.10, 0.10);
            requested_zoom = Some((self.map_zoom + zoom_step).clamp(MAP_MIN_ZOOM, MAP_MAX_ZOOM));
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
            ((map_rect.center().x - map_rect.left() - pan.x) / zoom)
                .clamp(0.0, self.map_world_size.x),
            ((map_rect.center().y - map_rect.top() - pan.y) / zoom)
                .clamp(0.0, self.map_world_size.y),
        ));

        let to_screen = |local: Pos2| -> Pos2 {
            Pos2::new(
                map_rect.left() + pan.x + (local.x * zoom),
                map_rect.top() + pan.y + (local.y * zoom),
            )
        };

        let map_world_width = self.map_world_size.x;
        let map_world_height = self.map_world_size.y;

        let to_local = |screen: Pos2| -> Pos2 {
            Pos2::new(
                ((screen.x - map_rect.left() - pan.x) / zoom).clamp(0.0, map_world_width),
                ((screen.y - map_rect.top() - pan.y) / zoom).clamp(0.0, map_world_height),
            )
        };

        let visible_local_min = Pos2::new(
            ((map_rect.left() - map_rect.left() - pan.x) / zoom).max(0.0),
            ((map_rect.top() - map_rect.top() - pan.y) / zoom).max(0.0),
        );
        let visible_local_max = Pos2::new(
            ((map_rect.right() - map_rect.left() - pan.x) / zoom).min(self.map_world_size.x),
            ((map_rect.bottom() - map_rect.top() - pan.y) / zoom).min(self.map_world_size.y),
        );

        let usable_world_top_left = to_screen(Pos2::new(0.0, 0.0));
        let usable_world_bottom_right =
            to_screen(Pos2::new(self.map_world_size.x, self.map_world_size.y));
        let usable_rect = Rect::from_min_max(usable_world_top_left, usable_world_bottom_right)
            .intersect(map_rect);

        struct ZoneRenderItem {
            id: i64,
            rect: Rect,
            name: String,
            fill_color: Color32,
            render_priority: i64,
        }

        let mut zone_render_items = self
            .zones
            .iter()
            .filter_map(|zone| {
                let top_left = to_screen(Pos2::new(zone.x, zone.y));
                let bottom_right = to_screen(Pos2::new(zone.x + zone.width, zone.y + zone.height));
                let zone_rect = Rect::from_two_pos(top_left, bottom_right).intersect(map_rect);

                if zone_rect.width() <= 0.0 || zone_rect.height() <= 0.0 {
                    return None;
                }

                let fill_color = zone
                    .color
                    .as_deref()
                    .and_then(Self::color_from_setting_value)
                    .unwrap_or(Color32::from_rgba_unmultiplied(96, 140, 255, 40));

                Some(ZoneRenderItem {
                    id: zone.id,
                    rect: zone_rect,
                    name: zone.name.clone(),
                    fill_color,
                    render_priority: zone.render_priority,
                })
            })
            .collect::<Vec<_>>();

        zone_render_items.sort_by(|left, right| {
            left.render_priority
                .cmp(&right.render_priority)
                .then_with(|| left.id.cmp(&right.id))
        });

        let draw_zone_group = |draw_above_grid: bool, painter: &egui::Painter| {
            for zone in &zone_render_items {
                let should_draw_above_grid = zone.render_priority > 0;
                if should_draw_above_grid != draw_above_grid {
                    continue;
                }

                let draw_fill = Color32::from_rgba_unmultiplied(
                    zone.fill_color.r(),
                    zone.fill_color.g(),
                    zone.fill_color.b(),
                    zone.fill_color.a().max(20),
                );

                painter.rect_filled(zone.rect, 4.0, draw_fill);
                painter.rect_stroke(
                    zone.rect,
                    4.0,
                    Stroke::new(
                        1.0,
                        Color32::from_rgb(
                            zone.fill_color.r(),
                            zone.fill_color.g(),
                            zone.fill_color.b(),
                        ),
                    ),
                );
                painter.text(
                    zone.rect.left_top() + Vec2::new(6.0, 4.0),
                    egui::Align2::LEFT_TOP,
                    zone.name.as_str(),
                    FontId::proportional((12.0 * self.map_zoom).clamp(10.0, 14.0)),
                    Color32::from_gray(210),
                );
            }
        };

        draw_zone_group(false, &painter);

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

        draw_zone_group(true, &painter);

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

        if !space_down && !self.zone_draw_mode {
            if let Some(selected_zone_id) = self.selected_zone_id {
                let selected_zone_rect = zone_render_items
                    .iter()
                    .find(|zone| zone.id == selected_zone_id)
                    .map(|zone| zone.rect);

                if let Some(zone_rect) = selected_zone_rect {
                    painter.rect_stroke(zone_rect, 4.0, Stroke::new(1.5, Color32::from_gray(235)));

                    let handle_rect = Rect::from_center_size(
                        zone_rect.right_bottom(),
                        Vec2::splat(12.0),
                    )
                    .intersect(map_rect);

                    painter.rect_filled(handle_rect, 2.0, Color32::from_gray(225));
                    painter.rect_stroke(handle_rect, 2.0, Stroke::new(1.0, Color32::from_gray(25)));

                    let move_response = ui.interact(
                        zone_rect,
                        ui.id().with(("zone_move", selected_zone_id)),
                        Sense::click_and_drag(),
                    );
                    let resize_response = ui.interact(
                        handle_rect,
                        ui.id().with(("zone_resize", selected_zone_id)),
                        Sense::click_and_drag(),
                    );

                    if resize_response.drag_started() {
                        if let Some(pointer_pos) = ui.input(|input| input.pointer.interact_pos()) {
                            let start_local = to_local(pointer_pos);
                            self.zone_drag_kind = Some(ZoneDragKind::ResizeBottomRight);
                            self.zone_drag_start_local = Some(start_local);

                            if let Some(existing) =
                                self.zones.iter().find(|zone| zone.id == selected_zone_id)
                            {
                                self.zone_drag_initial_x = existing.x;
                                self.zone_drag_initial_y = existing.y;
                                self.zone_drag_initial_width = existing.width;
                                self.zone_drag_initial_height = existing.height;
                            }
                        }
                    } else if move_response.drag_started() {
                        if let Some(pointer_pos) = ui.input(|input| input.pointer.interact_pos()) {
                            let start_local = to_local(pointer_pos);
                            self.zone_drag_kind = Some(ZoneDragKind::Move);
                            self.zone_drag_start_local = Some(start_local);

                            if let Some(existing) =
                                self.zones.iter().find(|zone| zone.id == selected_zone_id)
                            {
                                self.zone_drag_initial_x = existing.x;
                                self.zone_drag_initial_y = existing.y;
                                self.zone_drag_initial_width = existing.width;
                                self.zone_drag_initial_height = existing.height;
                            }
                        }
                    }

                    if let (Some(drag_kind), Some(start_local), Some(pointer_pos)) = (
                        self.zone_drag_kind,
                        self.zone_drag_start_local,
                        ui.input(|input| input.pointer.interact_pos()),
                    ) {
                        let current_local = to_local(pointer_pos);
                        let delta = current_local - start_local;

                        let (mut next_x, mut next_y, mut next_width, mut next_height) = (
                            self.zone_drag_initial_x,
                            self.zone_drag_initial_y,
                            self.zone_drag_initial_width,
                            self.zone_drag_initial_height,
                        );

                        match drag_kind {
                            ZoneDragKind::Move => {
                                next_x += delta.x;
                                next_y += delta.y;
                            }
                            ZoneDragKind::ResizeBottomRight => {
                                next_width = (next_width + delta.x).max(24.0);
                                next_height = (next_height + delta.y).max(24.0);
                            }
                        }

                        let max_x = (self.map_world_size.x - next_width).max(0.0);
                        let max_y = (self.map_world_size.y - next_height).max(0.0);
                        next_x = next_x.clamp(0.0, max_x);
                        next_y = next_y.clamp(0.0, max_y);
                        next_width = next_width.min((self.map_world_size.x - next_x).max(24.0));
                        next_height = next_height.min((self.map_world_size.y - next_y).max(24.0));

                        if let Some(zone) = self.zones.iter_mut().find(|zone| zone.id == selected_zone_id) {
                            zone.x = next_x;
                            zone.y = next_y;
                            zone.width = next_width;
                            zone.height = next_height;
                        }
                    }

                    if self.zone_drag_kind.is_some() && ui.input(|input| input.pointer.any_released()) {
                        if let Some(updated) = self
                            .zones
                            .iter()
                            .find(|zone| zone.id == selected_zone_id)
                            .cloned()
                        {
                            self.update_selected_zone_geometry(
                                updated.x,
                                updated.y,
                                updated.width,
                                updated.height,
                            );
                        }

                        self.zone_drag_kind = None;
                        self.zone_drag_start_local = None;
                    }
                }
            }
        }

        let selected_id = self.selected_system_id;
        let tech_filter_active = self.selected_catalog_tech_id_for_edit.is_some();

        if !space_down && !self.zone_draw_mode && self.zone_drag_kind.is_none() {
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

        if self.zone_draw_mode && !space_down {
            if map_response.drag_started() {
                let pointer_pos = ui.input(|input| input.pointer.interact_pos());
                self.zone_draw_start_screen = pointer_pos;
                self.zone_draw_end_screen = pointer_pos;
            }

            if map_response.dragged() {
                self.zone_draw_end_screen = ui.input(|input| input.pointer.interact_pos());
            }

            if let (Some(start), Some(end)) = (self.zone_draw_start_screen, self.zone_draw_end_screen)
            {
                let preview_rect = Rect::from_two_pos(start, end).intersect(map_rect);
                painter.rect_filled(
                    preview_rect,
                    4.0,
                    Color32::from_rgba_unmultiplied(
                        self.selected_zone_color.r(),
                        self.selected_zone_color.g(),
                        self.selected_zone_color.b(),
                        self.selected_zone_color.a().max(18),
                    ),
                );
                painter.rect_stroke(
                    preview_rect,
                    4.0,
                    Stroke::new(
                        1.5,
                        Color32::from_rgb(
                            self.selected_zone_color.r(),
                            self.selected_zone_color.g(),
                            self.selected_zone_color.b(),
                        ),
                    ),
                );
            }

            let released = ui.input(|input| input.pointer.any_released());
            if released {
                if let (Some(start), Some(end)) =
                    (self.zone_draw_start_screen.take(), self.zone_draw_end_screen.take())
                {
                    let local_start = to_local(start);
                    let local_end = to_local(end);
                    let min_x = local_start.x.min(local_end.x);
                    let min_y = local_start.y.min(local_end.y);
                    let width = (local_end.x - local_start.x).abs();
                    let height = (local_end.y - local_start.y).abs();

                    if width >= 16.0 && height >= 16.0 {
                        self.create_zone_from_rect(min_x, min_y, width, height);
                    }
                }
            }
        }

        let pointer_hover = ui.input(|input| input.pointer.hover_pos());
        let focused_flow_edges = if let (Some(from_id), Some(to_id)) = (
            self.flow_inspector_from_system_id,
            self.flow_inspector_to_system_id,
        ) {
            if from_id == to_id {
                HashSet::new()
            } else {
                self.focused_flow_shortest_path(from_id, to_id)
                    .map(|path| {
                        path.into_iter()
                            .map(|(from, _, to)| (from, to))
                            .collect::<HashSet<(i64, i64)>>()
                    })
                    .unwrap_or_default()
            }
        } else {
            HashSet::new()
        };
        let focused_flow_highlight_active = !focused_flow_edges.is_empty();
        let mut closest_hovered_interaction: Option<(crate::app::InteractionPopupState, f32)> =
            None;

        if self.line_layer_depth == LineLayerDepth::BehindCards {
            match self.line_layer_order {
                LineLayerOrder::ParentThenInteraction => {
                    self.draw_parent_lines_layer(
                        &painter,
                        &visible_systems,
                        &node_rects,
                        selected_id,
                        tech_filter_active,
                    );
                    closest_hovered_interaction = self.draw_interaction_lines_layer(
                        &painter,
                        map_rect,
                        pointer_hover,
                        &node_rects,
                        selected_id,
                        tech_filter_active,
                        &focused_flow_edges,
                        focused_flow_highlight_active,
                    );
                }
                LineLayerOrder::InteractionThenParent => {
                    closest_hovered_interaction = self.draw_interaction_lines_layer(
                        &painter,
                        map_rect,
                        pointer_hover,
                        &node_rects,
                        selected_id,
                        tech_filter_active,
                        &focused_flow_edges,
                        focused_flow_highlight_active,
                    );
                    self.draw_parent_lines_layer(
                        &painter,
                        &visible_systems,
                        &node_rects,
                        selected_id,
                        tech_filter_active,
                    );
                }
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
            let node_interact_rect = node_rect.intersect(map_rect);
            if node_interact_rect.width() <= 0.0 || node_interact_rect.height() <= 0.0 {
                continue;
            }
            let interaction_sense = if space_down || self.zone_draw_mode || self.zone_drag_kind.is_some() {
                Sense::hover()
            } else {
                Sense::click_and_drag()
            };

            let response = ui.interact(
                node_interact_rect,
                ui.id().with(("map_node", system.id)),
                interaction_sense,
            );

            if response.clicked() {
                let ctrl_held = ui.input(|input| input.modifiers.ctrl);
                let alt_held = ui.input(|input| input.modifiers.alt);
                if let Some(source_id) = self.map_link_click_source {
                    if source_id != system.id {
                        self.create_link_between(source_id, system.id, "");
                        self.map_link_click_source = None;
                    }
                } else if ctrl_held {
                    self.select_system(system.id);
                    self.selected_map_system_ids = self.system_and_descendant_ids(system.id);
                } else if alt_held {
                    self.select_system(system.id);
                    self.selected_map_system_ids = self.system_and_ancestor_ids(system.id);
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
                let ctrl_held = ui.input(|input| input.modifiers.ctrl);
                let interaction_drag_kind = ui.input(|input| {
                    if input.key_down(egui::Key::R) {
                        Some(InteractionKind::Standard)
                    } else if input.key_down(egui::Key::B) {
                        Some(InteractionKind::Pull)
                    } else if input.key_down(egui::Key::F) {
                        Some(InteractionKind::Push)
                    } else if input.key_down(egui::Key::D) {
                        Some(InteractionKind::Bidirectional)
                    } else {
                        None
                    }
                });

                if shift_held {
                    self.map_link_drag_from = Some(system.id);
                } else if ctrl_held {
                    if let Some(kind) = interaction_drag_kind {
                        self.map_interaction_drag_from = Some(system.id);
                        self.map_interaction_drag_kind = kind;
                    } else {
                        self.push_map_undo_snapshot();
                    }
                } else {
                    self.push_map_undo_snapshot();
                }
            }

            if response.dragged() {
                let shift_held = ui.input(|input| input.modifiers.shift);

                if self.map_link_drag_from == Some(system.id) || shift_held {
                    self.map_link_drag_from = Some(system.id);
                } else if self.map_interaction_drag_from == Some(system.id) {
                    // preview rendered outside this block; no map movement while creating interactions
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
                if self.map_interaction_drag_from == Some(system.id) {
                    continue;
                }

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

        if self.line_layer_depth == LineLayerDepth::AboveCards {
            match self.line_layer_order {
                LineLayerOrder::ParentThenInteraction => {
                    self.draw_parent_lines_layer(
                        &painter,
                        &visible_systems,
                        &node_rects,
                        selected_id,
                        tech_filter_active,
                    );
                    closest_hovered_interaction = self.draw_interaction_lines_layer(
                        &painter,
                        map_rect,
                        pointer_hover,
                        &node_rects,
                        selected_id,
                        tech_filter_active,
                        &focused_flow_edges,
                        focused_flow_highlight_active,
                    );
                }
                LineLayerOrder::InteractionThenParent => {
                    closest_hovered_interaction = self.draw_interaction_lines_layer(
                        &painter,
                        map_rect,
                        pointer_hover,
                        &node_rects,
                        selected_id,
                        tech_filter_active,
                        &focused_flow_edges,
                        focused_flow_highlight_active,
                    );
                    self.draw_parent_lines_layer(
                        &painter,
                        &visible_systems,
                        &node_rects,
                        selected_id,
                        tech_filter_active,
                    );
                }
            }
        }

        let now_secs = ui.input(|input| input.time);
        let clicked_on_interaction = ui.input(|input| input.pointer.primary_clicked());
        if clicked_on_interaction {
            if let Some((popup_state, _)) = closest_hovered_interaction.clone() {
                self.interaction_popup_active = Some(popup_state);
                self.interaction_popup_pending = None;
                self.interaction_popup_pending_open_at_secs = None;
                self.interaction_popup_close_at_secs = None;
            }
        }

        let hovered_popup_state = closest_hovered_interaction.map(|(state, _)| state);
        self.update_interaction_note_popup_state(hovered_popup_state, now_secs);
        self.render_interaction_note_popup(&painter, map_rect);

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
                    let from = self.rect_to_point_endpoint(
                        *source_rect,
                        pointer_pos,
                        self.parent_line_style.pattern,
                    );
                    self.draw_directed_connection(
                        &painter,
                        from,
                        pointer_pos,
                        self.parent_line_style,
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
                    self.assign_parent_between(source_id, target_id);
                }
            }
        }

        if let Some(source_id) = self.map_interaction_drag_from {
            if let Some(source_rect) = node_rects.get(&source_id) {
                if let Some(pointer_pos) = ui.input(|input| input.pointer.interact_pos()) {
                    let preview_style = self.interaction_line_style_for_kind(
                        source_id,
                        source_id,
                        self.map_interaction_drag_kind,
                    );
                    let (from, to) = match self.map_interaction_drag_kind {
                        InteractionKind::Pull => {
                            let endpoint = self.rect_to_point_endpoint(
                                *source_rect,
                                pointer_pos,
                                preview_style.pattern,
                            );
                            (pointer_pos, endpoint)
                        }
                        InteractionKind::Push | InteractionKind::Standard => {
                            let endpoint = self.rect_to_point_endpoint(
                                *source_rect,
                                pointer_pos,
                                preview_style.pattern,
                            );
                            (endpoint, pointer_pos)
                        }
                        InteractionKind::Bidirectional => {
                            let endpoint = self.rect_to_point_endpoint(
                                *source_rect,
                                pointer_pos,
                                preview_style.pattern,
                            );
                            (endpoint, pointer_pos)
                        }
                    };

                    self.draw_directed_connection(
                        &painter,
                        from,
                        to,
                        preview_style,
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

                let interaction_kind = self.map_interaction_drag_kind;
                self.map_interaction_drag_from = None;

                if let Some(target_id) = target {
                    self.create_link_between_kind(source_id, target_id, "", interaction_kind);
                }
            }
        }

        if map_response.clicked() && !space_down {
            let clicked_zone_id = ui
                .input(|input| input.pointer.interact_pos())
                .and_then(|pointer_pos| {
                    zone_render_items
                        .iter()
                        .rev()
                        .find(|zone| zone.rect.contains(pointer_pos))
                        .map(|zone| zone.id)
                });

            if let Some(zone_id) = clicked_zone_id {
                self.select_zone(zone_id);
                self.status_message = "Zone selected".to_owned();
                return;
            }

            let clicked_on_node = ui
                .input(|input| input.pointer.interact_pos())
                .map(|pointer_pos| node_rects.values().any(|rect| rect.contains(pointer_pos)))
                .unwrap_or(false);

            if !clicked_on_node {
                self.clear_selection();
                self.selected_zone_id = None;
                self.selected_zone_name.clear();
                self.selected_zone_render_priority = 1;
                self.zone_drag_kind = None;
                self.zone_drag_start_local = None;
                self.status_message = "Selection cleared".to_owned();
            }
        }
    }
}

impl eframe::App for SystemsCatalogApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.prune_closed_modals_from_stack();

        let close_recent_modal =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let open_bulk_add_system = ctx.input_mut(|input| {
            input.consume_key(
                egui::Modifiers {
                    ctrl: true,
                    shift: true,
                    ..Default::default()
                },
                egui::Key::N,
            )
        });
        let open_add_system =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::N));
        let open_add_tech =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::ALT, egui::Key::N));
        let copy_selected_cards =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::ALT, egui::Key::C));
        let paste_copied_cards =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::ALT, egui::Key::V));
        let undo_map_change =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Z));
        let delete_selected =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Delete));
        let open_hotkeys =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F1));

        if close_recent_modal && self.close_most_recent_open_modal() {
            self.status_message = "Closed dialog".to_owned();
        }

        if open_bulk_add_system {
            self.open_bulk_add_systems_modal_with_prefill(self.selected_system_id);
        }

        if open_add_system {
            self.open_add_system_modal_with_prefill(self.selected_system_id);
        }

        if open_add_tech {
            self.open_modal(AppModal::AddTech);
            self.focus_add_tech_name_on_open = true;
        }

        if copy_selected_cards {
            self.copy_selected_systems_to_clipboard();
        }

        if paste_copied_cards {
            self.load_copied_systems_from_clipboard();
            self.paste_copied_systems();
        }

        if undo_map_change {
            self.undo_map_positions();
        }

        if delete_selected {
            self.delete_selected_system();
        }

        if open_hotkeys {
            self.open_modal(AppModal::Hotkeys);
        }

        if let Err(error) = self.validate_before_render() {
            self.status_message = format!("State warning: {error}");
        }

        egui::TopBottomPanel::top("header_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Add System").clicked() {
                    self.open_add_system_modal_with_prefill(self.selected_system_id);
                }
                if ui.button("Bulk Add").clicked() {
                    self.open_bulk_add_systems_modal_with_prefill(self.selected_system_id);
                }
                if ui.button("Copy Selected").clicked() {
                    self.copy_selected_systems_to_clipboard();
                }
                if ui.button("Paste Cards").clicked() {
                    self.load_copied_systems_from_clipboard();
                    self.paste_copied_systems();
                }
                if ui.button("Add Technology").clicked() {
                    self.open_modal(AppModal::AddTech);
                    self.focus_add_tech_name_on_open = true;
                }
                if ui.button("Save Catalog").clicked() {
                    self.open_modal(AppModal::SaveCatalog);
                }
                if ui.button("Load Catalog").clicked() {
                    self.open_modal(AppModal::LoadCatalog);
                }
                if ui.button("New Catalog").clicked() {
                    self.open_modal(AppModal::NewCatalogConfirm);
                }
                if ui.button("Connection Style").clicked() {
                    self.open_modal(AppModal::LineStyle);
                }
                if ui.button("Hotkeys").clicked() {
                    self.open_modal(AppModal::Hotkeys);
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
        self.render_bulk_add_systems_modal(ctx);
        self.render_add_tech_modal(ctx);
        self.render_save_catalog_modal(ctx);
        self.render_load_catalog_modal(ctx);
        self.render_new_catalog_confirm_modal(ctx);
        self.render_line_style_modal(ctx);
        self.render_hotkeys_modal(ctx);
        self.save_ui_settings_if_dirty();
    }
}
