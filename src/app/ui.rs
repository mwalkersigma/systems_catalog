use std::collections::{HashMap, HashSet};
use std::path::Path;

use arboard::Clipboard;
use eframe::egui::{
    self, Align, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Shape, Stroke, Vec2,
};
use egui_material_icons::icons::{ICON_ADD, ICON_REMOVE};
use rfd::FileDialog;

use crate::app::{
    AppModal, ChildSpawnMode, FlowInspectorPickTarget, InteractionKind, LineLayerDepth,
    LineLayerOrder, LinePattern, LineStyle, LineTerminator, SidebarTab, SystemDetailsTab,
    SystemsCatalogApp,
    ZoneDragKind, MAP_MAX_ZOOM, MAP_MIN_ZOOM, MAP_NODE_SIZE,
};
use crate::models::SystemRecord;

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
const MAP_ZONE_OVERVIEW_HIDE_CARDS_ZOOM: f32 = 0.05;
const INTERACTION_NOTE_POPUP_OPEN_DELAY_SECS: f64 = 0.2;
const INTERACTION_NOTE_POPUP_CLOSE_DELAY_SECS: f64 = 0.2;
const DETAILS_LABEL_CHAR_LIMIT: usize = 72;

impl SystemsCatalogApp {
    pub(super) fn copy_selected_systems_to_clipboard(&mut self) {
        self.copy_selected_map_systems();

        let Some(payload) = self.copied_systems_payload() else {
            return;
        };

        match Clipboard::new().and_then(|mut clipboard| clipboard.set_text(payload)) {
            Ok(_) => {
                self.status_message.push_str(" (system clipboard updated)");
            }
            Err(error) => {
                self.status_message =
                    format!("{} (clipboard write failed: {error})", self.status_message);
            }
        }
    }

    pub(super) fn load_copied_systems_from_clipboard(&mut self) {
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

        let galley = painter.layout(text, FontId::proportional(14.0), text_color, max_text_width);

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
            ICON_ADD
        } else {
            ICON_REMOVE
        }
    }

    fn map_card_icon_for_system_type(system_type: &str) -> &'static str {
        crate::app::entities::map_icon_for_system_type(system_type)
    }

    fn map_node_size_for(&self, system: &SystemRecord, label: &str) -> Vec2 {
        let max_line_char_count = label
            .lines()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0) as f32;
        let minimum_required_width =
            (max_line_char_count * MAP_CARD_CHAR_WIDTH_ESTIMATE) + MAP_CARD_HORIZONTAL_PADDING;
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
        let mut height = (estimated_text_height + MAP_CARD_VERTICAL_PADDING)
            .clamp(MAP_CARD_MIN_HEIGHT, MAP_CARD_MAX_HEIGHT);

        if self.snap_to_grid && system.system_type == "database" {
            height = ((height / MAP_GRID_SPACING).round() * MAP_GRID_SPACING)
                .clamp(MAP_CARD_MIN_HEIGHT, MAP_CARD_MAX_HEIGHT);
        }

        Vec2::new(width, height)
    }

    fn map_card_label_for_system(&self, system: &SystemRecord) -> String {
        self.system_entity_for(system).render_map_label(self, system)
    }

    fn refresh_map_card_caches_if_needed(&mut self) {
        let live_ids: HashSet<i64> = self.systems.iter().map(|system| system.id).collect();
        self.map_card_label_cache
            .retain(|system_id, _| live_ids.contains(system_id));
        self.map_node_size_cache
            .retain(|system_id, _| live_ids.contains(system_id));

        for system in &self.systems {
            if self.map_card_label_cache.contains_key(&system.id)
                && self.map_node_size_cache.contains_key(&system.id)
            {
                continue;
            }

            let label = self.map_card_label_for_system(system);
            let size = self.map_node_size_for(system, label.as_str());
            self.map_card_label_cache.insert(system.id, label);
            self.map_node_size_cache.insert(system.id, size);
        }
    }

    fn map_card_label_cached_for_system(&self, system: &SystemRecord) -> String {
        self.map_card_label_cache
            .get(&system.id)
            .cloned()
            .unwrap_or_else(|| self.map_card_label_for_system(system))
    }

    fn map_node_size_cached_for_system(&self, system: &SystemRecord) -> Vec2 {
        self.map_node_size_cache
            .get(&system.id)
            .copied()
            .unwrap_or_else(|| {
                self.map_node_size_for(system, self.map_card_label_for_system(system).as_str())
            })
    }

    fn map_node_size_cached_by_id(&self, system_id: i64) -> Vec2 {
        self.map_node_size_cache
            .get(&system_id)
            .copied()
            .or_else(|| {
                self.systems
                    .iter()
                    .find(|system| system.id == system_id)
                    .map(|system| self.map_node_size_cached_for_system(system))
            })
            .unwrap_or(MAP_NODE_SIZE)
    }

    fn endpoint_reference_names_for_system(&self, system_id: i64) -> Vec<String> {
        let supports_references = self
            .systems
            .iter()
            .find(|system| system.id == system_id)
            .map(|system| {
                self.entity_supports_row_references_for_type(system.system_type.as_str())
            })
            .unwrap_or(false);

        if !supports_references {
            return Vec::new();
        }

        self.database_columns_by_system
            .get(&system_id)
            .map(|columns| {
                let mut names = columns
                    .iter()
                    .map(|column| column.column_name.trim())
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_owned())
                    .collect::<Vec<_>>();
                names.sort();
                names.dedup();
                names
            })
            .unwrap_or_default()
    }

    fn reference_term_for_system(&self, system_id: i64) -> &'static str {
        let entity_key = self
            .systems
            .iter()
            .find(|system| system.id == system_id)
            .map(|system| self.system_entity_for(system).entity_key())
            .unwrap_or("service");

        if entity_key == "step_processor" {
            "step"
        } else {
            "endpoint"
        }
    }

    fn mapping_section_title_for_link(&self, link: &crate::models::SystemLink) -> String {
        let source_term = self.reference_term_for_system(link.source_system_id);
        let target_term = self.reference_term_for_system(link.target_system_id);

        if source_term == "step" || target_term == "step" {
            "Step-level mapping".to_owned()
        } else {
            "Endpoint-level mapping".to_owned()
        }
    }

    fn row_reference_at_pointer_for_system(
        &self,
        system: &SystemRecord,
        node_rect: Rect,
        pointer_pos: Pos2,
    ) -> Option<String> {
        if !self.entity_supports_row_references_for_type(system.system_type.as_str()) {
            return None;
        }

        let references = self
            .database_columns_by_system
            .get(&system.id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|row| row.column_name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .collect::<Vec<_>>();

        if references.is_empty() {
            return None;
        }

        let icon_only_zoom = self.map_zoom <= 0.20;
        let compact_database_zoom = self.map_zoom <= 0.30;
        let hide_service_content_zoom = self.map_zoom <= 0.10;
        let is_api_type = self.system_entity_for(system).entity_key() == "api";
        let is_service_like_type = !is_api_type;

        // Row references only exist visually in the expanded card layout.
        if icon_only_zoom || compact_database_zoom || (hide_service_content_zoom && is_service_like_type) {
            return None;
        }

        let top_padding = (8.0 * self.map_zoom).clamp(4.0, 10.0);
        let row_height = (22.0 * self.map_zoom).clamp(14.0, 30.0);
        let separator_y = node_rect.top() + top_padding + row_height;
        let mut row_y = separator_y + 4.0;

        for reference_name in references {
            if row_y + row_height > node_rect.bottom() - 2.0 {
                break;
            }

            let row_rect = Rect::from_min_max(
                Pos2::new(node_rect.left() + 4.0, row_y),
                Pos2::new(node_rect.right() - 4.0, row_y + row_height),
            );
            if row_rect.contains(pointer_pos) {
                return Some(reference_name);
            }

            row_y += row_height;
        }

        None
    }

    fn row_center_for_reference_in_rect(
        &self,
        system: &SystemRecord,
        node_rect: Rect,
        reference_name: &str,
    ) -> Option<Pos2> {
        if !self.entity_supports_row_references_for_type(system.system_type.as_str()) {
            return None;
        }

        let references = self
            .database_columns_by_system
            .get(&system.id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|row| row.column_name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .collect::<Vec<_>>();

        if references.is_empty() {
            return None;
        }

        let icon_only_zoom = self.map_zoom <= 0.20;
        let compact_database_zoom = self.map_zoom <= 0.30;
        let hide_service_content_zoom = self.map_zoom <= 0.10;
        let is_api_type = self.system_entity_for(system).entity_key() == "api";
        let is_service_like_type = !is_api_type;
        if icon_only_zoom || compact_database_zoom || (hide_service_content_zoom && is_service_like_type) {
            return None;
        }

        let top_padding = (8.0 * self.map_zoom).clamp(4.0, 10.0);
        let row_height = (22.0 * self.map_zoom).clamp(14.0, 30.0);
        let separator_y = node_rect.top() + top_padding + row_height;
        let mut row_y = separator_y + 4.0;

        for candidate in references {
            if row_y + row_height > node_rect.bottom() - 2.0 {
                break;
            }

            if candidate == reference_name {
                let row_center_y = row_y + (row_height * 0.5);
                return Some(Pos2::new(node_rect.center().x, row_center_y));
            }

            row_y += row_height;
        }

        None
    }

    fn interaction_endpoint_owner_and_reference(
        &self,
        endpoint_system_id: i64,
        fallback_reference: Option<&str>,
    ) -> Option<(i64, Option<String>)> {
        let normalized_reference = fallback_reference
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);

        let endpoint = self
            .systems
            .iter()
            .find(|candidate| candidate.id == endpoint_system_id)?;

        if Self::is_internal_step_system(endpoint) {
            let owner_id = endpoint.parent_id?;
            let owner_reference = normalized_reference
                .or_else(|| {
                    let name = endpoint.description.trim();
                    if name.is_empty() {
                        None
                    } else {
                        Some(name.to_owned())
                    }
                });
            Some((owner_id, owner_reference))
        } else {
            Some((endpoint_system_id, normalized_reference))
        }
    }

    fn interaction_endpoint_anchor_point(
        &self,
        endpoint_system_id: i64,
        endpoint_reference: Option<&str>,
        peer_point: Pos2,
        pattern: LinePattern,
        node_rects: &HashMap<i64, Rect>,
    ) -> Option<(i64, Pos2)> {
        let (owner_system_id, owner_reference) =
            self.interaction_endpoint_owner_and_reference(endpoint_system_id, endpoint_reference)?;
        let owner_rect = *node_rects.get(&owner_system_id)?;

        let anchor = if let Some(reference) = owner_reference.as_deref() {
            if let Some(owner_system) = self
                .systems
                .iter()
                .find(|candidate| candidate.id == owner_system_id)
            {
                if let Some(row_center) =
                    self.row_center_for_reference_in_rect(owner_system, owner_rect, reference)
                {
                    let row_anchor_rect = Rect::from_center_size(
                        row_center,
                        Vec2::new((owner_rect.width() - 10.0).max(10.0), 6.0),
                    );
                    Self::rect_anchor_point(row_anchor_rect, peer_point - row_center, pattern)
                } else {
                    self.rect_to_point_endpoint(owner_rect, peer_point, pattern)
                }
            } else {
                self.rect_to_point_endpoint(owner_rect, peer_point, pattern)
            }
        } else {
            self.rect_to_point_endpoint(owner_rect, peer_point, pattern)
        };

        Some((owner_system_id, anchor))
    }

    fn max_chars_per_line_for_width(&self, width: f32) -> usize {
        let usable_width = (width - MAP_CARD_HORIZONTAL_PADDING).max(MAP_CARD_CHAR_WIDTH_ESTIMATE);
        (usable_width / MAP_CARD_CHAR_WIDTH_ESTIMATE)
            .floor()
            .max(1.0) as usize
    }

    fn wrap_label_for_width(&self, label: &str, width: f32) -> String {
        fn wrap_single_line(line: &str, max_chars_per_line: usize) -> String {
            if line.chars().count() <= max_chars_per_line {
                return line.to_owned();
            }

            let mut lines: Vec<String> = Vec::new();
            let mut current_line = String::new();

            for word in line.split_whitespace() {
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
                line.to_owned()
            } else {
                lines.join("\n")
            }
        }

        let max_chars_per_line = self.max_chars_per_line_for_width(width);
        label
            .lines()
            .map(|line| wrap_single_line(line, max_chars_per_line))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn estimate_wrapped_line_count(&self, label: &str, width: f32) -> usize {
        self.wrap_label_for_width(label, width)
            .lines()
            .count()
            .max(1)
    }

    fn map_text_scale_multiplier(&self) -> f32 {
        if self.map_zoom >= MAP_TEXT_SCALE_THRESHOLD_ZOOM {
            return 1.0;
        }

        let ratio = (self.map_zoom / MAP_TEXT_SCALE_THRESHOLD_ZOOM).clamp(0.0, 1.0);
        MAP_TEXT_MIN_LOW_ZOOM_MULTIPLIER + ((1.0 - MAP_TEXT_MIN_LOW_ZOOM_MULTIPLIER) * ratio)
    }

    fn grid_spot_is_open(
        &self,
        system_id: i64,
        candidate_position: Pos2,
        node_size: Vec2,
        moving_ids: &HashSet<i64>,
    ) -> bool {
        let candidate_rect = Rect::from_min_size(candidate_position, node_size);

        for other in &self.systems {
            let other_id = other.id;
            if other_id == system_id || moving_ids.contains(&other_id) {
                continue;
            }

            let Some(other_position) = self.effective_map_position(other_id) else {
                continue;
            };

            let other_size = self.map_node_size_cached_for_system(other);
            let other_rect = Rect::from_min_size(other_position, other_size);

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
                            if to_center.x >= from_center.x {
                                1.0
                            } else {
                                -1.0
                            }
                        } else {
                            direction.x
                        };

                        let vertical_component = if direction.y.abs() <= f32::EPSILON {
                            if to_center.y >= from_center.y {
                                1.0
                            } else {
                                -1.0
                            }
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

    fn rect_to_point_endpoint(
        &self,
        from_rect: Rect,
        to_point: Pos2,
        pattern: LinePattern,
    ) -> Pos2 {
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

        // Check if we have a selected interaction link from the details panel
        let selected_interaction_match = self.selected_link_id_for_edit.and_then(|link_id| {
            self.selected_links
                .iter()
                .find(|link| link.id == link_id)
                .map(|link| (link.source_system_id, link.target_system_id))
        });
        let selected_internal_step_id = selected_id.and_then(|id| {
            self.systems
                .iter()
                .find(|system| system.id == id)
                .and_then(|system| {
                    if Self::is_internal_step_system(system) {
                        Some(id)
                    } else {
                        None
                    }
                })
        });

        for interaction in self.deduped_visible_interactions() {
            let interaction_style = self.interaction_line_style_for_kind(
                interaction.source_system_id,
                interaction.target_system_id,
                interaction.kind,
            );

            let Some((rendered_source_system_id, _)) = self
                .interaction_endpoint_owner_and_reference(
                    interaction.source_system_id,
                    interaction.source_column_name.as_deref(),
                )
            else {
                continue;
            };
            let Some((rendered_target_system_id, _)) = self
                .interaction_endpoint_owner_and_reference(
                    interaction.target_system_id,
                    interaction.target_column_name.as_deref(),
                )
            else {
                continue;
            };

            let Some(source_rect) = node_rects.get(&rendered_source_system_id) else {
                continue;
            };
            let Some(target_rect) = node_rects.get(&rendered_target_system_id) else {
                continue;
            };

            let source_peer = target_rect.center();
            let target_peer = source_rect.center();
            let Some((_, source_anchor)) = self.interaction_endpoint_anchor_point(
                interaction.source_system_id,
                interaction.source_column_name.as_deref(),
                source_peer,
                interaction_style.pattern,
                node_rects,
            ) else {
                continue;
            };
            let Some((_, target_anchor)) = self.interaction_endpoint_anchor_point(
                interaction.target_system_id,
                interaction.target_column_name.as_deref(),
                target_peer,
                interaction_style.pattern,
                node_rects,
            ) else {
                continue;
            };

            let in_primary_selection = if let Some(internal_id) = selected_internal_step_id {
                interaction.raw_source_system_id == internal_id
                    || interaction.raw_target_system_id == internal_id
            } else {
                selected_id
                    .map(|id| id == rendered_source_system_id || id == rendered_target_system_id)
                    .unwrap_or(false)
            };
            let in_selection_set = self.selected_map_system_ids.contains(&rendered_source_system_id)
                || self.selected_map_system_ids.contains(&rendered_target_system_id);
            let has_any_selection =
                selected_id.is_some() || !self.selected_map_system_ids.is_empty();
            let is_selected_interaction = selected_interaction_match
                .map(|(src, tgt)| {
                    src == interaction.raw_source_system_id
                        && tgt == interaction.raw_target_system_id
                })
                .unwrap_or(false);

            let dimmed = has_any_selection && !(in_primary_selection || in_selection_set || is_selected_interaction);
            let dimmed_for_tech = selected_id.is_some()
                && tech_filter_active
                && (!self
                    .systems_using_selected_catalog_tech
                    .contains(&rendered_source_system_id)
                    || !self
                        .systems_using_selected_catalog_tech
                        .contains(&rendered_target_system_id));
            let boosted = in_primary_selection || in_selection_set || is_selected_interaction;
            let in_focused_flow_path = match interaction.kind {
                InteractionKind::Standard | InteractionKind::Push => {
                    focused_flow_edges
                        .contains(&(interaction.source_system_id, interaction.target_system_id))
                }
                InteractionKind::Pull => {
                    focused_flow_edges
                        .contains(&(interaction.target_system_id, interaction.source_system_id))
                }
                InteractionKind::Bidirectional => {
                    focused_flow_edges
                        .contains(&(interaction.source_system_id, interaction.target_system_id))
                        || focused_flow_edges.contains(&(
                            interaction.target_system_id,
                            interaction.source_system_id,
                        ))
                }
            };
            let dimmed_for_focused_flow = focused_flow_highlight_active && !in_focused_flow_path;
            let should_dim_interaction =
                (dimmed || dimmed_for_tech || dimmed_for_focused_flow) && !in_focused_flow_path;
            let (from, to) = match interaction.kind {
                InteractionKind::Pull => (target_anchor, source_anchor),
                InteractionKind::Push | InteractionKind::Standard | InteractionKind::Bidirectional => {
                    (source_anchor, target_anchor)
                }
            };

            if interaction.kind == InteractionKind::Bidirectional {
                self.draw_bidirectional_connection(
                    painter,
                    from,
                    to,
                    interaction_style,
                    should_dim_interaction,
                    (selected_id.is_some() && boosted) || in_focused_flow_path,
                );
            } else {
                self.draw_directed_connection(
                    painter,
                    from,
                    to,
                    interaction_style,
                    should_dim_interaction,
                    (selected_id.is_some() && boosted) || in_focused_flow_path,
                );
            }

            if !interaction.note.trim().is_empty() {
                if let Some(pointer) = pointer_hover.filter(|pos| map_rect.contains(*pos)) {
                    let hover_distance = Self::point_to_segment_distance(pointer, from, to);
                    let hover_threshold = (10.0 * self.map_zoom).clamp(8.0, 18.0);
                    if hover_distance <= hover_threshold {
                        let popup_state = crate::app::InteractionPopupState {
                            source_system_name: self.system_name_by_id(interaction.source_system_id),
                            target_system_name: self.system_name_by_id(interaction.target_system_id),
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

                ui.horizontal(|ui| {
                    ui.label("System type");
                    let selected_type_label =
                        self.system_type_display_label(self.new_system_type.as_str());
                    let system_types = self.supported_system_types();
                    egui::ComboBox::from_label("Type")
                        .selected_text(selected_type_label.as_str())
                        .show_ui(ui, |ui| {
                            for option in system_types {
                                ui.selectable_value(
                                    &mut self.new_system_type,
                                    option.key,
                                    option.label,
                                );
                            }
                        });
                });

                if self
                    .system_entity_for_type(self.new_system_type.as_str())
                    .selectable_inputs()
                    .can_select_route_methods
                {
                    ui.label("Route methods handled");
                    ui.horizontal_wrapped(|ui| {
                        for method in Self::supported_http_methods() {
                            let mut enabled = self.new_system_route_methods.contains(*method);
                            if ui.checkbox(&mut enabled, *method).changed() {
                                if enabled {
                                    self.new_system_route_methods.insert((*method).to_owned());
                                } else {
                                    self.new_system_route_methods.remove(*method);
                                }
                            }
                        }
                    });
                }

                let selected_parent_label = self
                    .new_system_parent_id
                    .map(|id| self.system_dropdown_label(id))
                    .unwrap_or_else(|| "No parent (root system)".to_owned());

                let zone_parent_candidates = self.zone_filtered_system_candidates(None);

                egui::ComboBox::from_label("Parent")
                    .selected_text(selected_parent_label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.new_system_parent_id,
                            None,
                            "No parent (root system)",
                        );
                        for (system_id, _) in &zone_parent_candidates {
                            let option_label = self.system_dropdown_label(*system_id);
                            ui.selectable_value(
                                &mut self.new_system_parent_id,
                                Some(*system_id),
                                option_label,
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
                    .map(|id| self.system_dropdown_label(id))
                    .unwrap_or_else(|| "No parent (root systems)".to_owned());

                let zone_parent_candidates = self.zone_filtered_system_candidates(None);

                egui::ComboBox::from_label("Parent")
                    .selected_text(selected_parent_label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.bulk_new_system_parent_id,
                            None,
                            "No parent (root systems)",
                        );

                        for (system_id, _) in &zone_parent_candidates {
                            let option_label = self.system_dropdown_label(*system_id);
                            ui.selectable_value(
                                &mut self.bulk_new_system_parent_id,
                                Some(*system_id),
                                option_label,
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
                ui.label("Ctrl+S  -> Save Project");
                ui.label("Alt+N  -> Add Technology");
                ui.label("Hold Z + drag  -> Draw zone");
                ui.label("Alt+C  -> Copy highlighted cards");
                ui.label("Alt+V  -> Paste copied cards");
                ui.label("Delete  -> Delete selected system(s)");
                ui.label("Ctrl+Z  -> Undo map move");
                ui.label("Esc  -> Close most recently opened modal");
                ui.separator();
                ui.label("Ctrl+Click  -> Select descendants (+ apply selected tech to subtree)");
                ui.label("Alt+Click  -> Select system + ancestors");
                ui.separator();
                ui.label("Shift + drag (child -> parent)  -> Assign parent");
                ui.label("Ctrl+R then click source + target  -> Standard interaction");
                ui.label("Ctrl+B then click source + target  -> Pull interaction");
                ui.label("Ctrl+F then click source + target  -> Push interaction");
                ui.label("Ctrl+D then click source + target  -> Bidirectional interaction");
                if ui.button("Close").clicked() {
                    self.show_hotkeys_modal = false;
                }
            });

        self.show_hotkeys_modal = open;
    }

    fn render_help_modal(&mut self, ctx: &egui::Context, title: &str, content: &str, modal: crate::app::AppModal) {
        if !self.is_modal_open(modal) {
            return;
        }

        let mut open = self.is_modal_open(modal);
        egui::Window::new(title)
            .collapsible(false)
            .resizable(true)
            .default_width(700.0)
            .default_height(500.0)
            .open(&mut open)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.monospace(content);
                });
                ui.separator();
                if ui.button("Close").clicked() {
                    self.set_modal_open(modal, false);
                }
            });

        self.set_modal_open(modal, open);
    }

    fn render_help_getting_started_modal(&mut self, ctx: &egui::Context) {
        self.render_help_modal(
            ctx,
            "Getting Started",
            crate::app::help_text::HelpText::getting_started(),
            crate::app::AppModal::HelpGettingStarted,
        );
    }

    fn render_help_creating_interactions_modal(&mut self, ctx: &egui::Context) {
        self.render_help_modal(
            ctx,
            "Creating & Managing Interactions",
            crate::app::help_text::HelpText::creating_interactions(),
            crate::app::AppModal::HelpCreatingInteractions,
        );
    }

    fn render_help_managing_technology_modal(&mut self, ctx: &egui::Context) {
        self.render_help_modal(
            ctx,
            "Managing Your Tech Catalog",
            crate::app::help_text::HelpText::managing_technology(),
            crate::app::AppModal::HelpManagingTechnology,
        );
    }

    fn render_help_understanding_map_modal(&mut self, ctx: &egui::Context) {
        self.render_help_modal(
            ctx,
            "Understanding the Visual Map",
            crate::app::help_text::HelpText::understanding_the_map(),
            crate::app::AppModal::HelpUnderstandingMap,
        );
    }

    fn render_help_zones_modal(&mut self, ctx: &egui::Context) {
        self.render_help_modal(
            ctx,
            "Zones & Organization",
            crate::app::help_text::HelpText::zones_and_organization(),
            crate::app::AppModal::HelpZones,
        );
    }

    fn render_help_keyboard_shortcuts_modal(&mut self, ctx: &egui::Context) {
        self.render_help_modal(
            ctx,
            "Keyboard Shortcuts",
            crate::app::help_text::HelpText::keyboard_shortcuts(),
            crate::app::AppModal::HelpKeyboardShortcuts,
        );
    }

    fn render_help_troubleshooting_modal(&mut self, ctx: &egui::Context) {
        self.render_help_modal(
            ctx,
            "Troubleshooting & FAQ",
            crate::app::help_text::HelpText::troubleshooting(),
            crate::app::AppModal::HelpTroubleshooting,
        );
    }


    fn render_interaction_style_modal(&mut self, ctx: &egui::Context) {
        if !self.show_interaction_style_modal {
            return;
        }

        let kind = self.interaction_style_modal_kind;
        let title = format!("{} Interaction Style", Self::interaction_kind_label(kind));

        let mut open = self.show_interaction_style_modal;
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                let mut changed = false;

                ui.horizontal(|ui| {
                    ui.label("Color");
                    changed |= match kind {
                        InteractionKind::Standard => ui
                            .color_edit_button_srgba(
                                &mut self.interaction_standard_line_style.color,
                            )
                            .changed(),
                        InteractionKind::Pull => ui
                            .color_edit_button_srgba(&mut self.interaction_pull_line_style.color)
                            .changed(),
                        InteractionKind::Push => ui
                            .color_edit_button_srgba(&mut self.interaction_push_line_style.color)
                            .changed(),
                        InteractionKind::Bidirectional => ui
                            .color_edit_button_srgba(
                                &mut self.interaction_bidirectional_line_style.color,
                            )
                            .changed(),
                    };
                });

                let terminator_changed = match kind {
                    InteractionKind::Standard => {
                        let old = self.interaction_standard_line_style.terminator;
                        Self::render_terminator_combo(
                            ui,
                            "modal_int_standard_term",
                            "Arrow",
                            &mut self.interaction_standard_line_style.terminator,
                        );
                        old != self.interaction_standard_line_style.terminator
                    }
                    InteractionKind::Pull => {
                        let old = self.interaction_pull_line_style.terminator;
                        Self::render_terminator_combo(
                            ui,
                            "modal_int_pull_term",
                            "Arrow",
                            &mut self.interaction_pull_line_style.terminator,
                        );
                        old != self.interaction_pull_line_style.terminator
                    }
                    InteractionKind::Push => {
                        let old = self.interaction_push_line_style.terminator;
                        Self::render_terminator_combo(
                            ui,
                            "modal_int_push_term",
                            "Arrow",
                            &mut self.interaction_push_line_style.terminator,
                        );
                        old != self.interaction_push_line_style.terminator
                    }
                    InteractionKind::Bidirectional => {
                        let old = self.interaction_bidirectional_line_style.terminator;
                        Self::render_terminator_combo(
                            ui,
                            "modal_int_bidirectional_term",
                            "Arrow",
                            &mut self.interaction_bidirectional_line_style.terminator,
                        );
                        old != self.interaction_bidirectional_line_style.terminator
                    }
                };
                changed |= terminator_changed;

                let pattern_changed = match kind {
                    InteractionKind::Standard => {
                        let old = self.interaction_standard_line_style.pattern;
                        Self::render_pattern_combo(
                            ui,
                            "modal_int_standard_pattern",
                            "Pattern",
                            &mut self.interaction_standard_line_style.pattern,
                        );
                        old != self.interaction_standard_line_style.pattern
                    }
                    InteractionKind::Pull => {
                        let old = self.interaction_pull_line_style.pattern;
                        Self::render_pattern_combo(
                            ui,
                            "modal_int_pull_pattern",
                            "Pattern",
                            &mut self.interaction_pull_line_style.pattern,
                        );
                        old != self.interaction_pull_line_style.pattern
                    }
                    InteractionKind::Push => {
                        let old = self.interaction_push_line_style.pattern;
                        Self::render_pattern_combo(
                            ui,
                            "modal_int_push_pattern",
                            "Pattern",
                            &mut self.interaction_push_line_style.pattern,
                        );
                        old != self.interaction_push_line_style.pattern
                    }
                    InteractionKind::Bidirectional => {
                        let old = self.interaction_bidirectional_line_style.pattern;
                        Self::render_pattern_combo(
                            ui,
                            "modal_int_bidirectional_pattern",
                            "Pattern",
                            &mut self.interaction_bidirectional_line_style.pattern,
                        );
                        old != self.interaction_bidirectional_line_style.pattern
                    }
                };
                changed |= pattern_changed;

                if changed {
                    self.settings_dirty = true;
                }
            });

        self.show_interaction_style_modal = open;
    }

    fn render_save_catalog_modal(&mut self, ctx: &egui::Context) {
        if !self.show_save_catalog_modal {
            return;
        }

        let mut open = self.show_save_catalog_modal;
        let mut close_requested = false;
        egui::Window::new("Save Project")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Project folder path");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.save_catalog_path);
                    if ui.button("Browse...").clicked() {
                        let mut dialog = FileDialog::new();

                        if let Some(parent) = Path::new(&self.save_catalog_path).parent() {
                            if !parent.as_os_str().is_empty() {
                                dialog = dialog.set_directory(parent);
                            }
                        }

                        if let Some(path) = dialog.pick_folder() {
                            self.save_catalog_path = path.to_string_lossy().to_string();
                        }
                    }
                });

                ui.separator();
                ui.label(format!(
                    "Session system changes: {} new, {} dirty",
                    self.new_system_ids.len(),
                    self.dirty_system_ids.len()
                ));
                ui.label(format!(
                    "Project has unsaved changes: {}",
                    if self.has_unsaved_project_changes() {
                        "yes"
                    } else {
                        "no"
                    }
                ));

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
                        if !self.show_save_catalog_modal {
                            close_requested = true;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_save_catalog_modal = false;
                        close_requested = true;
                    }
                });
            });

        if close_requested {
            open = false;
        }

        self.show_save_catalog_modal = open;
    }

    fn render_load_catalog_modal(&mut self, ctx: &egui::Context) {
        if !self.show_load_catalog_modal {
            return;
        }

        let mut open = self.show_load_catalog_modal;
        let mut close_requested = false;
        egui::Window::new("Load Project")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Project folder path");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.load_catalog_path);
                    if ui.button("Browse...").clicked() {
                        let mut dialog = FileDialog::new();

                        if let Some(parent) = Path::new(&self.load_catalog_path).parent() {
                            if !parent.as_os_str().is_empty() {
                                dialog = dialog.set_directory(parent);
                            }
                        }

                        if let Some(path) = dialog.pick_folder() {
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
                        if !self.show_load_catalog_modal {
                            close_requested = true;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_load_catalog_modal = false;
                        close_requested = true;
                    }
                });
            });

        if close_requested {
            open = false;
        }

        self.show_load_catalog_modal = open;
    }

    fn render_new_catalog_confirm_modal(&mut self, ctx: &egui::Context) {
        if !self.show_new_catalog_confirm_modal {
            return;
        }

        let mut open = self.show_new_catalog_confirm_modal;
        let mut close_requested = false;
        egui::Window::new("New Project")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Create a named project.");
                ui.label("This resets the current in-app model and saves it as a new project file.");

                ui.separator();
                ui.label("Project name");
                ui.text_edit_singleline(&mut self.new_catalog_name);

                ui.label("Project directory");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.new_catalog_directory);
                    if ui.button("Browse...").clicked() {
                        let mut dialog = FileDialog::new();
                        if !self.new_catalog_directory.trim().is_empty() {
                            dialog = dialog.set_directory(self.new_catalog_directory.trim());
                        }

                        if let Some(path) = dialog.pick_folder() {
                            self.new_catalog_directory = path.to_string_lossy().to_string();
                        }
                    }
                });

                if self.new_catalog_directory.trim().is_empty() {
                    ui.label("Directory defaults to current workspace folder.");
                }

                ui.separator();
                ui.label("Migration source DB (optional, one-time)");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.new_catalog_migration_db_path);
                    if ui.button("Browse DB...").clicked() {
                        let mut dialog = FileDialog::new();
                        if let Some(parent) = Path::new(&self.new_catalog_migration_db_path).parent() {
                            if !parent.as_os_str().is_empty() {
                                dialog = dialog.set_directory(parent);
                            }
                        }

                        if let Some(path) = dialog
                            .add_filter("Legacy SQLite DB", &["db", "sqlite", "sqlite3"])
                            .pick_file()
                        {
                            self.new_catalog_migration_db_path =
                                path.to_string_lossy().to_string();
                        }
                    }
                });
                ui.label("Leave empty to create a blank project.");

                ui.horizontal(|ui| {
                    if ui.button("Create Project").clicked() {
                        self.create_named_catalog();
                        if !self.show_new_catalog_confirm_modal {
                            close_requested = true;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_new_catalog_confirm_modal = false;
                        close_requested = true;
                    }
                });
            });

        if close_requested {
            open = false;
        }

        self.show_new_catalog_confirm_modal = open;
    }

    fn render_step_processor_conversion_confirm_modal(&mut self, ctx: &egui::Context) {
        if !self.show_step_processor_conversion_confirm_modal {
            return;
        }

        let mut open = self.show_step_processor_conversion_confirm_modal;
        let mut confirm_requested = false;
        let mut cancel_requested = false;

        egui::Window::new("Convert Child Systems To Steps")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.label(
                    "This conversion found existing child systems. You can keep them as systems, or convert them into internal step endpoints.",
                );
                ui.small(
                    "Converting them hides child cards and turns them into step-level endpoints behind the Step Processor.",
                );
                ui.separator();

                ui.checkbox(
                    &mut self.pending_step_processor_conversion_keep_steps_as_systems,
                    "Keep steps as systems",
                );

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Convert").clicked() {
                        confirm_requested = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel_requested = true;
                    }
                });
            });

        if confirm_requested {
            self.show_step_processor_conversion_confirm_modal = false;
            if self.pending_step_processor_conversion_single_details {
                self.update_selected_system_details();
            } else if let Some(target_type) =
                self.pending_step_processor_conversion_target_type.clone()
            {
                self.bulk_convert_selected_system_types(target_type.as_str());
            } else {
                self.clear_pending_step_processor_conversion_prompt();
                self.status_message =
                    "No pending step processor conversion request found".to_owned();
            }
            return;
        }

        if cancel_requested || !open {
            self.show_step_processor_conversion_confirm_modal = false;
            self.clear_pending_step_processor_conversion_prompt();
            if cancel_requested {
                self.status_message = "Step processor conversion canceled".to_owned();
            }
            return;
        }

        self.show_step_processor_conversion_confirm_modal = open;
    }

    fn render_ddl_table_mapping_modal(&mut self, ctx: &egui::Context) {
        if !self.show_ddl_table_mapping_modal {
            return;
        }

        let mut open = self.show_ddl_table_mapping_modal;
        let mut close_requested = false;

        let mut database_candidates = self
            .systems
            .iter()
            .filter(|system| system.system_type.eq_ignore_ascii_case("database"))
            .map(|system| (system.id, system.name.clone()))
            .collect::<Vec<_>>();
        database_candidates.sort_by(|left, right| left.1.to_lowercase().cmp(&right.1.to_lowercase()));

        egui::Window::new("DDL Table Mapping")
            .collapsible(false)
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Map imported DDL tables to existing database systems.");
                ui.label("Choose an existing system to update, or keep 'Create new system'.");
                ui.separator();

                while self.pending_ddl_target_system_ids.len() < self.pending_ddl_drafts.len() {
                    self.pending_ddl_target_system_ids.push(None);
                }

                for (index, draft) in self.pending_ddl_drafts.iter().enumerate() {
                    let selected_target = self
                        .pending_ddl_target_system_ids
                        .get(index)
                        .copied()
                        .flatten();

                    ui.horizontal(|ui| {
                        ui.label(format!("{}", draft.name));

                        egui::ComboBox::from_id_source(format!("ddl_mapping_target_{index}"))
                            .selected_text(
                                selected_target
                                    .and_then(|target_id| {
                                        database_candidates
                                            .iter()
                                            .find(|(candidate_id, _)| *candidate_id == target_id)
                                            .map(|(_, candidate_name)| candidate_name.clone())
                                    })
                                    .unwrap_or_else(|| "Create new system".to_owned()),
                            )
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(selected_target.is_none(), "Create new system")
                                    .clicked()
                                {
                                    self.pending_ddl_target_system_ids[index] = None;
                                }

                                for (candidate_id, candidate_name) in &database_candidates {
                                    if ui
                                        .selectable_label(
                                            selected_target == Some(*candidate_id),
                                            candidate_name,
                                        )
                                        .clicked()
                                    {
                                        self.pending_ddl_target_system_ids[index] = Some(*candidate_id);
                                    }
                                }
                            });
                    });
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Apply Mapping").clicked() {
                        self.apply_pending_ddl_table_mapping();
                        close_requested = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.cancel_pending_ddl_table_mapping();
                        close_requested = true;
                    }
                });
            });

        if close_requested {
            open = false;
        }

        self.show_ddl_table_mapping_modal = open;
    }

    fn select_system(&mut self, system_id: i64) {
        self.selected_zone_id = None;
        self.selected_zone_name.clear();
        self.selected_zone_render_priority = 1;
        self.selected_zone_parent_zone_id = None;
        self.selected_zone_minimized = false;
        self.selected_zone_representative_system_id = None;
        self.selected_system_id = Some(system_id);
        if let Err(error) = self.load_selected_data(system_id) {
            self.status_message = format!("Failed to load selection: {error}");
        }
    }

    pub(super) fn system_dropdown_label(&self, system_id: i64) -> String {
        self.naming_path_for_system(system_id)
    }

    pub(super) fn clamp_text_to_width(text: &str, available_width: f32) -> String {
        let safe_width = available_width.max(80.0);
        let max_chars = ((safe_width / 7.0).floor() as usize).clamp(12, 120);
        Self::clamp_text_to_width_with_limit(text, max_chars)
    }

    fn clamp_text_to_width_with_limit(text: &str, max_chars: usize) -> String {
        let char_count = text.chars().count();
        if char_count <= max_chars {
            return text.to_owned();
        }

        let visible = max_chars.saturating_sub(1);
        let truncated = text.chars().take(visible).collect::<String>();
        format!("{truncated}…")
    }

    pub(super) fn toggle_left_sidebar_tab(&mut self, tab: SidebarTab) {
        if self.show_left_sidebar && self.active_sidebar_tab == tab {
            self.show_left_sidebar = false;
        } else {
            self.active_sidebar_tab = tab;
            self.show_left_sidebar = true;
        }
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        match self.active_sidebar_tab {
            SidebarTab::Systems => {
                ui.heading("Systems List");
                ui.add(
                    egui::TextEdit::singleline(&mut self.systems_sidebar_search)
                        .hint_text("Search systems"),
                );
                ui.horizontal(|ui| {
                    if ui.small_button("Show all").clicked() {
                        self.clear_subset_visibility();
                    }
                });

                ui.separator();

                let query = self.systems_sidebar_search.trim().to_lowercase();
                let rows = self.visible_hierarchy_rows();
                let filtered_rows = if query.is_empty() {
                    rows
                } else {
                    rows
                        .into_iter()
                        .filter(|(_, _, name, _, _)| name.to_lowercase().contains(query.as_str()))
                        .collect::<Vec<_>>()
                };

                if filtered_rows.is_empty() {
                    if query.is_empty() {
                        ui.label("No systems yet.");
                    } else {
                        ui.label("No systems match the current search.");
                    }
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (depth, system_id, name, has_children, is_collapsed) in filtered_rows {
                            let indent = "  ".repeat(depth);
                            let row_text = format!("{indent}• {name}");
                            let selected = self.selected_system_id == Some(system_id);

                            ui.horizontal(|ui| {
                                if has_children {
                                    let icon = Self::disclosure_icon(is_collapsed);
                                    let button = egui::Button::new(icon).small();
                                    if ui.add_sized([18.0, 18.0], button).clicked() {
                                        let zone_ids = self
                                            .visible_minimized_zone_ids_for_disclosure_system(
                                                system_id,
                                            );
                                        if zone_ids.is_empty() {
                                            self.on_disclosure_click(system_id);
                                        } else {
                                            for zone_id in zone_ids {
                                                self.toggle_zone_minimized(zone_id);
                                            }

                                            if self.collapsed_system_ids.contains(&system_id) {
                                                self.on_disclosure_click(system_id);
                                            }
                                        }
                                    }
                                }

                                let row_response = ui.add_sized(
                                    [ui.available_width(), 20.0],
                                    egui::SelectableLabel::new(selected, row_text),
                                );
                                if row_response.clicked() {
                                    self.select_system(system_id);
                                    self.pending_map_focus_system_id = Some(system_id);
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
                            self.edited_tech_color = tech
                                .color
                                .as_deref()
                                .and_then(Self::color_from_setting_value);
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

    fn render_zone_details(&mut self, ui: &mut egui::Ui) {
        let _zone_entity = self.zone_render_entity().entity_key();
        self.zone_render_entity().render_details_panel(self, ui);
    }

    pub(super) fn render_connection_style_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Connection Style", |ui| {
            ui.set_min_width(260.0);
            ui.with_layout(Layout::top_down_justified(Align::Min), |ui| {
                ui.menu_button("Parent Lines", |ui| {
                    let mut changed = false;

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

                    let old_terminator = self.parent_line_style.terminator;
                    Self::render_terminator_combo(
                        ui,
                        "menu_parent_terminator",
                        "Terminator",
                        &mut self.parent_line_style.terminator,
                    );
                    if old_terminator != self.parent_line_style.terminator {
                        changed = true;
                    }

                    let old_pattern = self.parent_line_style.pattern;
                    Self::render_pattern_combo(
                        ui,
                        "menu_parent_pattern",
                        "Pattern",
                        &mut self.parent_line_style.pattern,
                    );
                    if old_pattern != self.parent_line_style.pattern {
                        changed = true;
                    }

                    if changed {
                        self.settings_dirty = true;
                    }
                });

                ui.menu_button("Interaction Lines", |ui| {
                    let mut changed = false;

                    changed |= ui
                        .checkbox(&mut self.show_interaction_lines, "Show interaction lines")
                        .changed();

                    changed |= ui
                        .add(
                            egui::Slider::new(&mut self.interaction_line_style.width, 0.5..=6.0)
                                .text("Width"),
                        )
                        .changed();

                    self.interaction_standard_line_style.width = self.interaction_line_style.width;
                    self.interaction_pull_line_style.width = self.interaction_line_style.width;
                    self.interaction_push_line_style.width = self.interaction_line_style.width;
                    self.interaction_bidirectional_line_style.width =
                        self.interaction_line_style.width;

                    ui.separator();
                    ui.label("Type styles");
                    if ui.button("Standard...").clicked() {
                        self.interaction_style_modal_kind = InteractionKind::Standard;
                        self.open_modal(AppModal::InteractionStyle);
                        ui.close_menu();
                    }
                    if ui.button("Pull...").clicked() {
                        self.interaction_style_modal_kind = InteractionKind::Pull;
                        self.open_modal(AppModal::InteractionStyle);
                        ui.close_menu();
                    }
                    if ui.button("Push...").clicked() {
                        self.interaction_style_modal_kind = InteractionKind::Push;
                        self.open_modal(AppModal::InteractionStyle);
                        ui.close_menu();
                    }
                    if ui.button("Bidirectional...").clicked() {
                        self.interaction_style_modal_kind = InteractionKind::Bidirectional;
                        self.open_modal(AppModal::InteractionStyle);
                        ui.close_menu();
                    }

                    if changed {
                        self.settings_dirty = true;
                    }
                });
            });

            ui.separator();

            {
                let mut changed = false;

                ui.horizontal(|ui| {
                    ui.label("Line layer");
                    let previous = self.line_layer_depth;
                    egui::ComboBox::from_id_source("menu_line_layer_depth")
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
                    ui.label("Draw order");
                    let previous = self.line_layer_order;
                    egui::ComboBox::from_id_source("menu_line_layer_order")
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
                        .text("Selected brightness %"),
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
            }
        });
    }

    pub(super) fn render_stats_menu(&mut self, ui: &mut egui::Ui) {
        let mut standard_count = 0usize;
        let mut pull_count = 0usize;
        let mut push_count = 0usize;
        let mut bidirectional_count = 0usize;

        for link in &self.all_links {
            match Self::interaction_kind_from_setting_value(link.kind.as_str()) {
                InteractionKind::Standard => standard_count += 1,
                InteractionKind::Pull => pull_count += 1,
                InteractionKind::Push => push_count += 1,
                InteractionKind::Bidirectional => bidirectional_count += 1,
            }
        }

        ui.menu_button("Stats", |ui| {
            ui.set_min_width(230.0);
            ui.label(format!("Systems: {}", self.systems.len()));
            ui.label(format!("Tech: {}", self.tech_catalog.len()));
            ui.separator();
            ui.label(format!("Interactions (Total): {}", self.all_links.len()));
            ui.label(format!("• Standard: {}", standard_count));
            ui.label(format!("• Pull: {}", pull_count));
            ui.label(format!("• Push: {}", push_count));
            ui.label(format!("• Bidirectional: {}", bidirectional_count));
        });
    }

    fn render_details(&mut self, ui: &mut egui::Ui) {
        ui.set_max_width(ui.available_width());

        ui.heading("System Details");

        let Some(system) = self.selected_system().cloned() else {
            ui.label("No System Selected");
            return;
        };

        let mut incoming_connections = 0usize;
        let mut outgoing_connections = 0usize;
        let mut standard_connections = 0usize;
        let mut pull_connections = 0usize;
        let mut push_connections = 0usize;
        let mut bidirectional_connections = 0usize;

        for link in &self.all_links {
            let touches_system =
                link.source_system_id == system.id || link.target_system_id == system.id;
            if !touches_system {
                continue;
            }

            let kind = Self::interaction_kind_from_setting_value(link.kind.as_str());
            match kind {
                InteractionKind::Standard => standard_connections += 1,
                InteractionKind::Pull => pull_connections += 1,
                InteractionKind::Push => push_connections += 1,
                InteractionKind::Bidirectional => bidirectional_connections += 1,
            }

            match kind {
                InteractionKind::Standard | InteractionKind::Push => {
                    if link.source_system_id == system.id {
                        outgoing_connections += 1;
                    }
                    if link.target_system_id == system.id {
                        incoming_connections += 1;
                    }
                }
                InteractionKind::Pull => {
                    if link.target_system_id == system.id {
                        outgoing_connections += 1;
                    }
                    if link.source_system_id == system.id {
                        incoming_connections += 1;
                    }
                }
                InteractionKind::Bidirectional => {
                    if touches_system {
                        incoming_connections += 1;
                        outgoing_connections += 1;
                    }
                }
            }
        }

        let total_connections = incoming_connections + outgoing_connections;

        let subtree_ids = self.system_and_descendant_ids(system.id);
        let descendant_ids = subtree_ids
            .iter()
            .copied()
            .filter(|id| *id != system.id)
            .collect::<std::collections::HashSet<_>>();
        let subtree_subsystem_count = descendant_ids.len();
        let mut subtree_total_connections = 0usize;
        let mut subtree_standard_connections = 0usize;
        let mut subtree_pull_connections = 0usize;
        let mut subtree_push_connections = 0usize;
        let mut subtree_bidirectional_connections = 0usize;

        for link in &self.all_links {
            let touches_descendant = descendant_ids.contains(&link.source_system_id)
                || descendant_ids.contains(&link.target_system_id);
            if !touches_descendant {
                continue;
            }

            subtree_total_connections += 1;
            match Self::interaction_kind_from_setting_value(link.kind.as_str()) {
                InteractionKind::Standard => subtree_standard_connections += 1,
                InteractionKind::Pull => subtree_pull_connections += 1,
                InteractionKind::Push => subtree_push_connections += 1,
                InteractionKind::Bidirectional => subtree_bidirectional_connections += 1,
            }
        }

        let badge_bg = Color32::from_rgba_unmultiplied(65, 85, 120, 80);
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!("Connections {}", total_connections))
                    .small()
                    .strong()
                    .background_color(badge_bg),
            );
            ui.label(
                RichText::new(format!("In {}", incoming_connections))
                    .small()
                    .strong()
                    .background_color(badge_bg),
            );
            ui.label(
                RichText::new(format!("Out {}", outgoing_connections))
                    .small()
                    .strong()
                    .background_color(badge_bg),
            );
            ui.label(
                RichText::new(format!(
                    "Subtree {} subsystems / {} links",
                    subtree_subsystem_count, subtree_total_connections
                ))
                .small()
                .strong()
                .background_color(badge_bg),
            );
        });

        egui::CollapsingHeader::new("Stats")
            .default_open(false)
            .show(ui, |ui| {
                ui.label("Direct system connections");
                ui.label(format!("Total: {}", total_connections));
                ui.label(format!("Incoming: {}", incoming_connections));
                ui.label(format!("Outgoing: {}", outgoing_connections));
                ui.label(format!("• Standard: {}", standard_connections));
                ui.label(format!("• Pull: {}", pull_connections));
                ui.label(format!("• Push: {}", push_connections));
                ui.label(format!("• Bidirectional: {}", bidirectional_connections));

                ui.separator();
                ui.label("Subtree connections (selected system + all descendants)");
                ui.label(format!(
                    "Subsystems in subtree: {}",
                    subtree_subsystem_count
                ));
                ui.label(format!(
                    "Total links touching subsystems: {}",
                    subtree_total_connections
                ));
                ui.label(format!("• Standard: {}", subtree_standard_connections));
                ui.label(format!("• Pull: {}", subtree_pull_connections));
                ui.label(format!("• Push: {}", subtree_push_connections));
                ui.label(format!(
                    "• Bidirectional: {}",
                    subtree_bidirectional_connections
                ));
            });

        ui.separator();
        ui.heading("Basics");
        ui.label("Name");
        ui.text_edit_singleline(&mut self.edited_system_name);

        ui.label("Description");
        ui.add(egui::TextEdit::multiline(&mut self.edited_system_description).desired_rows(3));

        ui.separator();
        ui.label("System classification");
        let selected_type_label = self.system_type_display_label(self.selected_system_type.as_str());
        let system_types = self.supported_system_types();
        egui::ComboBox::from_label("Type")
            .selected_text(selected_type_label.as_str())
            .show_ui(ui, |ui| {
                for option in system_types {
                    ui.selectable_value(
                        &mut self.selected_system_type,
                        option.key,
                        option.label,
                    );
                }
            });

        let selected_count = if self.selected_map_system_ids.is_empty() {
            usize::from(self.selected_system_id.is_some())
        } else {
            self.selected_map_system_ids.len()
        };

        if selected_count > 1 {
            ui.horizontal(|ui| {
                if ui
                    .button(format!(
                        "Apply '{}' type to {} selected systems",
                        selected_type_label, selected_count
                    ))
                    .clicked()
                {
                    let target_type = self.selected_system_type.clone();
                    self.bulk_convert_selected_system_types(target_type.as_str());
                }
            });
        }

        let system_entity = self.system_entity_for(&system);
        let selectable_inputs = system_entity.selectable_inputs();



        ui.separator();
        ui.heading("System-Specific Settings");
        ui.small("Custom fields from this system type.");
        ui.group(|ui| {
            system_entity.render_details_panel(self, ui, &system);
        });
        ui.horizontal(|ui| {
            if ui.button("Save system details").clicked() {
                self.update_selected_system_details();
            }
        });
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(
                &mut self.active_system_details_tab,
                SystemDetailsTab::Structure,
                "Structure",
            );
            ui.selectable_value(
                &mut self.active_system_details_tab,
                SystemDetailsTab::Interactions,
                "Interactions",
            );
            ui.selectable_value(
                &mut self.active_system_details_tab,
                SystemDetailsTab::Notes,
                "Notes",
            );
        });

        let show_structure_tab = self.active_system_details_tab == SystemDetailsTab::Structure;
        let show_interactions_tab = self.active_system_details_tab == SystemDetailsTab::Interactions;
        let show_notes_tab = self.active_system_details_tab == SystemDetailsTab::Notes;

        let tab_scroll_height = ui.available_height().max(140.0);
        egui::ScrollArea::vertical()
            .id_source("details_tab_content_scroll")
            .max_height(tab_scroll_height)
            .show(ui, |ui| {

        if show_structure_tab {
            let mut deleted_selected_system = false;
            ui.separator();
            ui.label("Structure & Layout");
                ui.label("Naming");
                ui.checkbox(
                    &mut self.selected_system_naming_root,
                    "Treat as naming root",
                );
                ui.horizontal(|ui| {
                    ui.label("Delimiter");
                    ui.text_edit_singleline(&mut self.selected_system_naming_delimiter);
                });
                ui.small(format!(
                    "Current path: {}",
                    self.naming_path_for_system(system.id)
                ));

                ui.separator();
                if selectable_inputs.can_select_parent {
                    ui.label("Parent assignment");

                    let selected_parent_label = self
                        .selected_system_parent_id
                        .map(|id| self.system_dropdown_label(id))
                        .unwrap_or_else(|| "No parent (root system)".to_owned());
                    let selected_parent_label = Self::clamp_text_to_width_with_limit(
                        &selected_parent_label,
                        ((ui.available_width().max(80.0) / 7.0).floor() as usize)
                            .clamp(12, DETAILS_LABEL_CHAR_LIMIT),
                    );

                    let valid_parent_candidates = self
                        .zone_filtered_system_candidates(Some(system.id))
                        .into_iter()
                        .filter(|(candidate_id, _)| {
                            !self.would_create_parent_cycle(system.id, *candidate_id)
                        })
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

                            for (candidate_id, _) in &valid_parent_candidates {
                                let option_label = self.system_dropdown_label(*candidate_id);
                                ui.selectable_value(
                                    &mut self.selected_system_parent_id,
                                    Some(*candidate_id),
                                    option_label,
                                );
                            }
                        });

                    if self.selected_system_parent_id != previous_parent_id {
                        self.update_selected_system_parent();
                    }

                    ui.separator();
                }

                ui.label("Subsystem layout");
                ui.horizontal(|ui| {
                    if ui.button("File tree").clicked() {
                        self.layout_selected_subsystem_file_tree();
                    }
                    if ui.button("Regular tree").clicked() {
                        self.layout_selected_subsystem_regular_tree();
                    }
                });

                ui.separator();
                if ui.button("Delete system").clicked() {
                    self.delete_selected_system();
                    deleted_selected_system = true;
                }

            if deleted_selected_system {
                return;
            }

            ui.separator();
            ui.label("Line color override");
                ui.horizontal(|ui| {
                    let mut use_override = self.selected_system_line_color_override.is_some();
                    let mut changed = false;
                    if ui.checkbox(&mut use_override, "Enable override").changed() {
                        if use_override {
                            self.selected_system_line_color_override =
                                Some(self.interaction_line_style.color);
                        } else {
                            self.selected_system_line_color_override = None;
                        }
                        changed = true;
                    }

                    if let Some(mut color) = self.selected_system_line_color_override {
                        if ui.color_edit_button_srgba(&mut color).changed() {
                            self.selected_system_line_color_override = Some(color);
                            changed = true;
                        }
                    }

                    if changed {
                        self.update_selected_system_line_color_override();
                    }
                });

                ui.horizontal(|ui| {
                    if ui.button("Clear override").clicked() {
                        self.selected_system_line_color_override = None;
                        self.update_selected_system_line_color_override();
                    }
                });
        }

        if show_interactions_tab {
            ui.separator();
            ui.heading("Interactions");
            ui.small("Create new links, transfer links, or edit existing links.");

        if self.map_interaction_drag_from == Some(system.id) {
            if let Some(reference_name) = self.map_interaction_drag_from_reference.as_deref() {
                ui.small(format!(
                    "Map source selected: {}:{} (click target card or row)",
                    self.system_name_by_id(system.id),
                    reference_name
                ));
            } else {
                ui.small(format!(
                    "Map source selected: {} (click target card or row)",
                    self.system_name_by_id(system.id)
                ));
            }
        }

        ui.label("Create interaction");
                let selected_target_label = self
                    .new_link_target_id
                    .map(|id| self.system_dropdown_label(id))
                    .unwrap_or_else(|| "Select target system".to_owned());
                let selected_target_label = Self::clamp_text_to_width_with_limit(
                    &selected_target_label,
                    ((ui.available_width().max(80.0) / 7.0).floor() as usize)
                        .clamp(12, DETAILS_LABEL_CHAR_LIMIT),
                );

                egui::ComboBox::from_label("Target")
                    .selected_text(selected_target_label)
                    .show_ui(ui, |ui| {
                        for (candidate_id, _) in self.zone_filtered_system_candidates(Some(system.id)) {
                            let option_label = self.system_dropdown_label(candidate_id);
                            ui.selectable_value(
                                &mut self.new_link_target_id,
                                Some(candidate_id),
                                option_label,
                            );
                        }
                    });

                ui.text_edit_singleline(&mut self.new_link_label);
                if ui.button("Add interaction").clicked() {
                    self.create_link();
                }
        ui.separator();

        ui.label("Transfer interactions");
                let transfer_target_label = self
                    .selected_interaction_transfer_target_id
                    .map(|id| self.system_dropdown_label(id))
                    .unwrap_or_else(|| "Select destination system".to_owned());
                let transfer_target_label = Self::clamp_text_to_width_with_limit(
                    &transfer_target_label,
                    ((ui.available_width().max(80.0) / 7.0).floor() as usize)
                        .clamp(12, DETAILS_LABEL_CHAR_LIMIT),
                );

                egui::ComboBox::from_label("Transfer to")
                    .selected_text(transfer_target_label)
                    .show_ui(ui, |ui| {
                        for (candidate_id, _) in self.zone_filtered_system_candidates(Some(system.id)) {
                            let option_label = self.system_dropdown_label(candidate_id);
                            ui.selectable_value(
                                &mut self.selected_interaction_transfer_target_id,
                                Some(candidate_id),
                                option_label,
                            );
                        }
                    });

                let transfer_pick_active = self.interaction_transfer_pick_source_id == Some(system.id);
                ui.group(|ui| {
                    if ui
                        .button(if transfer_pick_active {
                            "Cancel map target pick"
                        } else {
                            "Select target on map"
                        })
                        .clicked()
                    {
                        if transfer_pick_active {
                            self.interaction_transfer_pick_source_id = None;
                            self.status_message = "Transfer target pick canceled".to_owned();
                        } else {
                            self.interaction_transfer_pick_source_id = Some(system.id);
                            self.selected_interaction_transfer_target_id = None;
                            self.status_message =
                                "Click a destination system on the map for transfer".to_owned();
                        }
                    }

                    if ui.button("Transfer interactions").clicked() {
                        self.transfer_selected_system_interactions();
                    }

                    if transfer_pick_active {
                        ui.label("Picking transfer target: click a destination card on the map.");
                    }
                });

        ui.separator();

            ui.label(format!("Manage existing ({})", self.selected_links.len()));
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
            let selected_link_label = Self::clamp_text_to_width_with_limit(
                &selected_link_label,
                ((ui.available_width().max(80.0) / 7.0).floor() as usize)
                    .clamp(12, DETAILS_LABEL_CHAR_LIMIT),
            );

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
                        let label = Self::clamp_text_to_width_with_limit(
                            &label,
                            ((ui.available_width().max(80.0) / 7.0).floor() as usize)
                                .clamp(12, DETAILS_LABEL_CHAR_LIMIT),
                        );

                        let was_selected = self.selected_link_id_for_edit == Some(link.id);
                        if ui.selectable_label(was_selected, label).clicked() {
                            self.selected_link_id_for_edit = Some(link.id);
                            self.edited_link_label = link.label.clone();
                            self.edited_link_note = link.note.clone();
                            self.edited_link_kind =
                                Self::interaction_kind_from_setting_value(link.kind.as_str());
                            self.edited_link_source_column_name =
                                link.source_column_name.clone();
                            self.edited_link_target_column_name =
                                link.target_column_name.clone();
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

            if let Some(link_id) = self.selected_link_id_for_edit {
                if let Some(link) = self.selected_links.iter().find(|link| link.id == link_id) {
                    let source_columns =
                        self.endpoint_reference_names_for_system(link.source_system_id);
                    let target_columns =
                        self.endpoint_reference_names_for_system(link.target_system_id);

                    if !source_columns.is_empty() || !target_columns.is_empty() {
                        ui.separator();
                        ui.label(self.mapping_section_title_for_link(link));

                        let source_term = self.reference_term_for_system(link.source_system_id);
                        let target_term = self.reference_term_for_system(link.target_system_id);
                        let source_label_text = format!("Source {}", source_term);
                        let target_label_text = format!("Target {}", target_term);

                        let source_label = self
                            .edited_link_source_column_name
                            .clone()
                            .unwrap_or_else(|| "(none)".to_owned());
                        egui::ComboBox::from_label(source_label_text)
                            .selected_text(source_label)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.edited_link_source_column_name,
                                    None,
                                    "(none)",
                                );
                                for column_name in &source_columns {
                                    ui.selectable_value(
                                        &mut self.edited_link_source_column_name,
                                        Some(column_name.clone()),
                                        column_name,
                                    );
                                }
                            });

                        let target_label = self
                            .edited_link_target_column_name
                            .clone()
                            .unwrap_or_else(|| "(none)".to_owned());
                        egui::ComboBox::from_label(target_label_text)
                            .selected_text(target_label)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.edited_link_target_column_name,
                                    None,
                                    "(none)",
                                );
                                for column_name in &target_columns {
                                    ui.selectable_value(
                                        &mut self.edited_link_target_column_name,
                                        Some(column_name.clone()),
                                        column_name,
                                    );
                                }
                            });
                    }
                }
            }

            ui.horizontal(|ui| {
                if ui.button("Update interaction").clicked() {
                    self.update_selected_link();
                }
                if ui.button("Delete interaction").clicked() {
                    self.delete_selected_link();
                }
            });
            }
        }

        if show_structure_tab {
            ui.separator();
            ui.label("Technology");
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
                ui.label("Cumulative child tech stack");
                if self.selected_cumulative_child_tech.is_empty() {
                    ui.label("No child-system technologies found.");
                } else {
                    ui.vertical(|ui| {
                        for tech_name in &self.selected_cumulative_child_tech {
                            ui.label(format!("• {tech_name}"));
                        }
                    });
                }
        }

        if show_notes_tab {
            ui.separator();
            ui.heading("Notes");
            let editing_note_active = self.selected_note_id_for_edit.is_some();

            ui.label("New note");
            if editing_note_active {
                ui.small("Finish or cancel the current edit before adding a new note.");
            }
            ui.add_enabled_ui(!editing_note_active, |ui| {
                ui.add(egui::TextEdit::multiline(&mut self.note_text).desired_rows(4));
                if ui.button("Add note").clicked() {
                    self.create_note_for_selected_system();
                }
            });

            ui.separator();

            if self.selected_notes.is_empty() {
                ui.label("No notes recorded.");
            } else {
                let notes_snapshot = self.selected_notes.clone();
                for note in notes_snapshot {
                    let is_editing = self.selected_note_id_for_edit == Some(note.id);
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(format!("#{} [{}]", note.id, note.updated_at));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if is_editing {
                                    if ui
                                        .small_button(egui_material_icons::icons::ICON_CLOSE)
                                        .clicked()
                                    {
                                        self.selected_note_id_for_edit = None;
                                        self.note_text.clear();
                                        self.status_message = "Note edit canceled".to_owned();
                                    }
                                    if ui
                                        .small_button(egui_material_icons::icons::ICON_CHECK)
                                        .clicked()
                                    {
                                        self.save_note();
                                        if !self.status_message.starts_with("Failed") {
                                            self.selected_note_id_for_edit = None;
                                            self.note_text.clear();
                                        }
                                    }
                                } else {
                                    if ui
                                        .small_button(egui_material_icons::icons::ICON_DELETE)
                                        .clicked()
                                    {
                                        self.pending_note_delete_id = Some(note.id);
                                    }
                                    if ui
                                        .small_button(egui_material_icons::icons::ICON_EDIT)
                                        .clicked()
                                    {
                                        self.select_note_for_edit(note.id);
                                    }
                                }
                            });
                        });

                        if is_editing {
                            ui.add(egui::TextEdit::multiline(&mut self.note_text).desired_rows(6));
                        } else {
                            let body = note.body.trim();
                            if body.is_empty() {
                                ui.small("(empty note)");
                            } else {
                                ui.label(body);
                            }
                        }
                    });
                }
            }

            if let Some(note_id) = self.pending_note_delete_id {
                let mut open = true;
                let mut confirm_delete = false;
                let mut cancel_delete = false;

                egui::Window::new("Delete note?")
                    .collapsible(false)
                    .resizable(false)
                    .open(&mut open)
                    .show(ui.ctx(), |ui| {
                        ui.label("Are you sure you want to delete this note?");
                        ui.horizontal(|ui| {
                            if ui.button("Delete").clicked() {
                                confirm_delete = true;
                            }
                            if ui.button("Cancel").clicked() {
                                cancel_delete = true;
                            }
                        });
                    });

                if confirm_delete {
                    self.selected_note_id_for_edit = Some(note_id);
                    self.delete_selected_note();
                    self.selected_note_id_for_edit = None;
                    self.note_text.clear();
                    self.pending_note_delete_id = None;
                } else if cancel_delete || !open {
                    self.pending_note_delete_id = None;
                }
            }
        }
            });
    }

    fn process_flow_inspector_pick_from_selection(&mut self) {
        if !self.show_flow_inspector_modal {
            self.flow_inspector_pick_target = None;
            self.flow_inspector_last_seen_selected_system_id = self.selected_system_id;
            return;
        }

        let selected = self.selected_system_id;
        if selected == self.flow_inspector_last_seen_selected_system_id {
            return;
        }

        self.flow_inspector_last_seen_selected_system_id = selected;

        let Some(system_id) = selected else {
            return;
        };

        let Some(pick_target) = self.flow_inspector_pick_target else {
            return;
        };

        match pick_target {
            FlowInspectorPickTarget::Start => {
                self.flow_inspector_from_system_id = Some(system_id);
                self.status_message = format!(
                    "Flow Inspector start set to {}",
                    self.system_name_by_id(system_id)
                );
            }
            FlowInspectorPickTarget::Stop => {
                self.flow_inspector_to_system_id = Some(system_id);
                self.status_message = format!(
                    "Flow Inspector stop set to {}",
                    self.system_name_by_id(system_id)
                );
            }
        }

        self.flow_inspector_pick_target = None;
    }

    fn render_flow_inspector_modal(&mut self, ctx: &egui::Context) {
        if !self.show_flow_inspector_modal {
            return;
        }

        let mut open = self.show_flow_inspector_modal;
        egui::Window::new("Flow Inspector")
            .collapsible(false)
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Click Pick Start or Pick Stop, then click a system in the map or sidebar.");

                ui.separator();

                let start_name = self
                    .flow_inspector_from_system_id
                    .map(|id| self.system_name_by_id(id))
                    .unwrap_or_else(|| "(none)".to_owned());
                let stop_name = self
                    .flow_inspector_to_system_id
                    .map(|id| self.system_name_by_id(id))
                    .unwrap_or_else(|| "(none)".to_owned());

                ui.horizontal(|ui| {
                    ui.label("Start:");
                    ui.monospace(start_name);

                    let picking_start =
                        self.flow_inspector_pick_target == Some(FlowInspectorPickTarget::Start);
                    let pick_label = if picking_start {
                        "Picking start..."
                    } else {
                        "Pick Start"
                    };
                    if ui.button(pick_label).clicked() {
                        self.flow_inspector_pick_target = Some(FlowInspectorPickTarget::Start);
                        self.flow_inspector_last_seen_selected_system_id = self.selected_system_id;
                    }

                    if ui.small_button("Use selected").clicked() {
                        if let Some(selected_id) = self.selected_system_id {
                            self.flow_inspector_from_system_id = Some(selected_id);
                            self.flow_inspector_pick_target = None;
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Stop:");
                    ui.monospace(stop_name);

                    let picking_stop =
                        self.flow_inspector_pick_target == Some(FlowInspectorPickTarget::Stop);
                    let pick_label = if picking_stop {
                        "Picking stop..."
                    } else {
                        "Pick Stop"
                    };
                    if ui.button(pick_label).clicked() {
                        self.flow_inspector_pick_target = Some(FlowInspectorPickTarget::Stop);
                        self.flow_inspector_last_seen_selected_system_id = self.selected_system_id;
                    }

                    if ui.small_button("Use selected").clicked() {
                        if let Some(selected_id) = self.selected_system_id {
                            self.flow_inspector_to_system_id = Some(selected_id);
                            self.flow_inspector_pick_target = None;
                        }
                    }
                });

                ui.horizontal(|ui| {
                    if ui.button("Swap").clicked() {
                        std::mem::swap(
                            &mut self.flow_inspector_from_system_id,
                            &mut self.flow_inspector_to_system_id,
                        );
                    }
                    if ui.button("Clear").clicked() {
                        self.flow_inspector_from_system_id = None;
                        self.flow_inspector_to_system_id = None;
                        self.flow_inspector_pick_target = None;
                    }
                });

                ui.separator();
                ui.label("Flow result");

                if let (Some(from_id), Some(to_id)) =
                    (self.flow_inspector_from_system_id, self.flow_inspector_to_system_id)
                {
                    let (start_incoming, start_outgoing) =
                        self.flow_directional_counts_for_system(from_id);
                    let (stop_incoming, stop_outgoing) =
                        self.flow_directional_counts_for_system(to_id);
                    ui.label(format!(
                        "Start In/Out: {start_incoming}/{start_outgoing}    Stop In/Out: {stop_incoming}/{stop_outgoing}"
                    ));

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
                    ui.label("Pick start and stop systems to inspect data flow.");
                }
            });

        self.show_flow_inspector_modal = open;
    }

    fn apply_map_zoom_anchored_to_screen_point(
        &mut self,
        map_rect: Rect,
        target_zoom: f32,
        anchor_screen: Pos2,
    ) {
        let old_zoom = self.map_zoom;
        let new_zoom = target_zoom.clamp(MAP_MIN_ZOOM, MAP_MAX_ZOOM);
        if (new_zoom - old_zoom).abs() <= f32::EPSILON {
            return;
        }

        let local_at_anchor = Pos2::new(
            (anchor_screen.x - map_rect.left() - self.map_pan.x) / old_zoom,
            (anchor_screen.y - map_rect.top() - self.map_pan.y) / old_zoom,
        );

        self.map_zoom = new_zoom;
        self.map_pan = Vec2::new(
            anchor_screen.x - map_rect.left() - (local_at_anchor.x * new_zoom),
            anchor_screen.y - map_rect.top() - (local_at_anchor.y * new_zoom),
        );
        self.settings_dirty = true;
    }

    fn focus_map_on_system(&mut self, map_rect: Rect, system_id: i64, target_zoom: f32) {
        let Some(local_position) = self.effective_map_position(system_id) else {
            return;
        };

        let node_size = self.map_node_size_cached_by_id(system_id);
        let local_center = Pos2::new(
            local_position.x + (node_size.x * 0.5),
            local_position.y + (node_size.y * 0.5),
        );
        let zoom = target_zoom.clamp(MAP_MIN_ZOOM, MAP_MAX_ZOOM);
        self.map_zoom = zoom;
        self.map_pan = Vec2::new(
            map_rect.center().x - map_rect.left() - (local_center.x * zoom),
            map_rect.center().y - map_rect.top() - (local_center.y * zoom),
        );
        self.settings_dirty = true;
    }

    fn render_map_canvas(&mut self, ui: &mut egui::Ui) {
        let mut requested_zoom: Option<f32> = None;
        let mut requested_zoom_anchor: Option<Pos2> = None;

        ui.horizontal(|ui| {
            ui.heading("Mind Map");

            let info_response = ui.small_button(egui_material_icons::icons::ICON_DETAILS);
            if info_response.hovered() {
                egui::show_tooltip(ui.ctx(), ui.id().with("map_help_tip"), |ui| {
                    ui.label("Space + drag -> Pan");
                    ui.label("Z + drag -> Draw zone");
                    ui.label("Ctrl + Scroll -> Zoom");
                    ui.label("Scroll -> Pan vertical/horizontal");
                    ui.label("Shift + drag (child -> parent) -> Assign parent");
                    ui.label("Ctrl+R/B/F/D then click source + target -> Interaction");
                    ui.label("Alt+C / Alt+V -> Copy / Paste cards");
                    ui.label("Drag empty space -> Box-select systems");
                });
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

        let smooth_scroll_delta = ui.input(|input| input.smooth_scroll_delta);
        let raw_scroll_delta = ui.input(|input| input.raw_scroll_delta);
        let pointer_over_map = ui
            .input(|input| input.pointer.hover_pos())
            .map(|pos| map_rect.contains(pos))
            .unwrap_or(false);

        let z_down_for_draw = ui
            .input(|input| input.key_down(egui::Key::Z) && !input.modifiers.ctrl)
            && (pointer_over_map || map_response.has_focus());

        let modal_open = self.show_add_system_modal
            || self.show_bulk_add_systems_modal
            || self.show_add_tech_modal
            || self.show_hotkeys_modal
            || self.show_interaction_style_modal
            || self.show_flow_inspector_modal
            || self.show_save_catalog_modal
            || self.show_load_catalog_modal
            || self.show_new_catalog_confirm_modal
            || self.show_step_processor_conversion_confirm_modal
            || self.show_ddl_table_mapping_modal
            || self.show_help_getting_started_modal
            || self.show_help_creating_interactions_modal
            || self.show_help_managing_technology_modal
            || self.show_help_understanding_map_modal
            || self.show_help_zones_modal
            || self.show_help_keyboard_shortcuts_modal
            || self.show_help_troubleshooting_modal;

        if z_down_for_draw != self.zone_draw_mode {
            self.zone_draw_mode = z_down_for_draw;
            self.zone_draw_start_screen = None;
            self.zone_draw_end_screen = None;
            self.zone_drag_kind = None;
            self.zone_drag_start_local = None;
            self.zone_drag_captured_system_positions.clear();
            self.zone_drag_descendant_initial_positions.clear();
            self.zone_drag_moves_captured_systems = true;
        }

        let zoom_active = !modal_open && pointer_over_map;
        let ctrl_down = ui.input(|input| input.modifiers.ctrl);
        if zoom_active {
            if ctrl_down {
                let zoom_anchor = if self.map_zoom_anchor_to_pointer {
                    ui.input(|input| input.pointer.hover_pos())
                        .filter(|pointer| map_rect.contains(*pointer))
                        .unwrap_or(map_rect.center())
                } else {
                    map_rect.center()
                };
                let zoom_factor = ui.input(|input| input.zoom_delta());
                if (zoom_factor - 1.0).abs() > f32::EPSILON {
                    requested_zoom =
                        Some((self.map_zoom * zoom_factor).clamp(MAP_MIN_ZOOM, MAP_MAX_ZOOM));
                    requested_zoom_anchor = Some(zoom_anchor);
                } else {
                    let wheel_y = if raw_scroll_delta.y.abs() > f32::EPSILON {
                        raw_scroll_delta.y
                    } else {
                        smooth_scroll_delta.y
                    };

                    if wheel_y.abs() > f32::EPSILON {
                        let zoom_step = (wheel_y / 520.0).clamp(-0.10, 0.10);
                        requested_zoom =
                            Some((self.map_zoom + zoom_step).clamp(MAP_MIN_ZOOM, MAP_MAX_ZOOM));
                        requested_zoom_anchor = Some(zoom_anchor);
                    }
                }
            } else if smooth_scroll_delta.x.abs() > f32::EPSILON
                || smooth_scroll_delta.y.abs() > f32::EPSILON
            {
                self.map_pan += smooth_scroll_delta;
                self.settings_dirty = true;
            } else if raw_scroll_delta.x.abs() > f32::EPSILON
                || raw_scroll_delta.y.abs() > f32::EPSILON
            {
                self.map_pan += raw_scroll_delta;
                self.settings_dirty = true;
            }
        }

        if let Some(target_zoom) = requested_zoom {
            self.apply_map_zoom_anchored_to_screen_point(
                map_rect,
                target_zoom,
                requested_zoom_anchor.unwrap_or_else(|| map_rect.center()),
            );
        }

        painter.rect_filled(map_rect, 6.0, Color32::from_gray(24));
        painter.rect_stroke(map_rect, 6.0, Stroke::new(1.0, Color32::from_gray(60)));

        self.ensure_map_positions();
        self.refresh_map_card_caches_if_needed();

        if let Some(system_id) = self.pending_map_focus_system_id.take() {
            self.focus_map_on_system(map_rect, system_id, 0.6);
        }

        let zoom = self.map_zoom;
        let pan = self.map_pan;
        let zone_overview_mode = zoom <= MAP_ZONE_OVERVIEW_HIDE_CARDS_ZOOM;

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
            logical_rect: Rect,
            name: String,
            fill_color: Color32,
            render_priority: i64,
            parent_zone_id: Option<i64>,
            ancestry_depth: usize,
            containment_depth: usize,
            minimized: bool,
        }

        let mut minimized_hidden_system_ids = HashSet::new();
        let mut minimized_hidden_zone_ids = HashSet::new();

        for zone in &self.zones {
            let resolved_representative = self.zone_resolved_representative_system_id(zone.id);
            let effective_minimized = zone.minimized && resolved_representative.is_some();
            if effective_minimized {
                minimized_hidden_zone_ids.extend(self.zone_nested_child_ids(zone.id));
            }
        }

        let mut zone_render_items = self
            .zones
            .iter()
            .filter_map(|zone| {
                if minimized_hidden_zone_ids.contains(&zone.id) {
                    return None;
                }

                let resolved_representative = self.zone_resolved_representative_system_id(zone.id);
                let effective_minimized = zone.minimized && resolved_representative.is_some();

                if effective_minimized {
                    if let Some(zone_ids) = self.zone_system_ids(zone.id) {
                        minimized_hidden_system_ids.extend(zone_ids);
                    }

                    if !minimized_hidden_zone_ids.contains(&zone.id) {
                        if let Some(representative_id) = resolved_representative {
                            minimized_hidden_system_ids.remove(&representative_id);
                        }
                    }
                }

                let logical_rect = if effective_minimized && !zone_overview_mode {
                    let representative_id = resolved_representative?;
                    let representative_position = self.effective_map_position(representative_id)?;
                    let tile_size = self.map_node_size_cached_by_id(representative_id) * zoom;
                    Rect::from_min_size(to_screen(representative_position), tile_size)
                } else {
                    let top_left = to_screen(Pos2::new(zone.x, zone.y));
                    let bottom_right =
                        to_screen(Pos2::new(zone.x + zone.width, zone.y + zone.height));
                    Rect::from_two_pos(top_left, bottom_right)
                };

                let zone_rect = logical_rect.intersect(map_rect);

                if zone_rect.width() <= 0.0 || zone_rect.height() <= 0.0 {
                    return None;
                }

                let fill_color = self.zone_render_entity().fill_color_for_map(self, zone);

                Some(ZoneRenderItem {
                    id: zone.id,
                    rect: zone_rect,
                    logical_rect,
                    name: self.zone_render_entity().render_name_for_map(
                        self,
                        zone,
                        zone_overview_mode,
                        resolved_representative,
                    ),
                    fill_color,
                    render_priority: zone.render_priority,
                    parent_zone_id: zone.parent_zone_id,
                    ancestry_depth: 0,
                    containment_depth: 0,
                    minimized: effective_minimized,
                })
            })
            .collect::<Vec<_>>();

        let parent_zone_by_id = self
            .zones
            .iter()
            .map(|zone| (zone.id, zone.parent_zone_id))
            .collect::<HashMap<_, _>>();

        for item in &mut zone_render_items {
            let mut depth = 0usize;
            let mut visited = HashSet::new();
            let mut current = item.parent_zone_id;

            while let Some(parent_id) = current {
                if !visited.insert(parent_id) {
                    break;
                }

                depth += 1;
                current = parent_zone_by_id.get(&parent_id).copied().flatten();
            }

            item.ancestry_depth = depth;
        }

        let containment_rects = zone_render_items
            .iter()
            .map(|item| (item.id, item.logical_rect))
            .collect::<HashMap<_, _>>();

        for item in &mut zone_render_items {
            let mut depth = 0usize;
            let Some(item_rect) = containment_rects.get(&item.id).copied() else {
                continue;
            };

            for (other_id, other_rect) in &containment_rects {
                if *other_id == item.id {
                    continue;
                }

                let fully_contains = other_rect.contains(item_rect.min)
                    && other_rect.contains(item_rect.max);

                if fully_contains {
                    depth += 1;
                }
            }

            item.containment_depth = depth;
        }

        zone_render_items.sort_by(|left, right| {
            left
                .ancestry_depth
                .max(left.containment_depth)
                .cmp(&right.ancestry_depth.max(right.containment_depth))
                .then_with(|| left.render_priority.cmp(&right.render_priority))
                .then_with(|| left.id.cmp(&right.id))
        });

        let mut zone_disclosure_hitboxes: Vec<(i64, Rect)> = Vec::new();

        let zone_id_at_pointer = |pointer_pos: Pos2| -> Option<i64> {
            zone_render_items
                .iter()
                .filter(|zone| zone.rect.contains(pointer_pos))
                .min_by(|left, right| {
                    let left_area = left.rect.width() * left.rect.height();
                    let right_area = right.rect.width() * right.rect.height();

                    left_area
                        .total_cmp(&right_area)
                        .then_with(|| right.render_priority.cmp(&left.render_priority))
                        .then_with(|| right.id.cmp(&left.id))
                })
                .map(|zone| zone.id)
        };

        let mut draw_zone_group = |draw_above_grid: bool, painter: &egui::Painter| {
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
                if zone_overview_mode {
                    painter.text(
                        zone.rect.center(),
                        egui::Align2::CENTER_CENTER,
                        zone.name.as_str(),
                        FontId::proportional((14.0 * self.map_zoom).clamp(10.0, 16.0)),
                        Color32::from_gray(220),
                    );
                } else {
                    painter.text(
                        zone.rect.left_top() + Vec2::new(6.0, 4.0),
                        egui::Align2::LEFT_TOP,
                        zone.name.as_str(),
                        FontId::proportional((12.0 * self.map_zoom).clamp(10.0, 14.0)),
                        Color32::from_gray(210),
                    );
                }

                // Skip the zone disclosure button for minimized zones — the
                // representative card's own disclosure handles un-minimize so
                // that we avoid a double-fire where both handlers react to the
                // same click.
                if !zone.minimized && !zone_overview_mode {
                    let disclosure_radius = (8.5 * self.map_zoom).clamp(7.0, 13.0);
                    let disclosure_center = Pos2::new(
                        zone.rect.left() + (disclosure_radius + 2.0),
                        zone.rect.top() + (disclosure_radius + 2.0),
                    );
                    let disclosure_rect = Rect::from_center_size(
                        disclosure_center,
                        Vec2::splat(disclosure_radius * 2.0),
                    )
                    .intersect(map_rect);
                    zone_disclosure_hitboxes.push((zone.id, disclosure_rect));

                    painter.circle_filled(
                        disclosure_center,
                        disclosure_radius,
                        Color32::from_gray(210),
                    );
                    painter.text(
                        disclosure_center,
                        egui::Align2::CENTER_CENTER,
                        Self::disclosure_icon(false),
                        FontId::proportional((13.0 * self.map_zoom).clamp(10.0, 18.0)),
                        Color32::from_gray(20),
                    );
                }
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

        let disclosure_clicked_zone_id = ui.input(|input| {
            if !input.pointer.primary_clicked() {
                return None;
            }

            let pointer_pos = input.pointer.interact_pos()?;
            zone_disclosure_hitboxes
                .iter()
                .rev()
                .find(|(_, rect)| rect.contains(pointer_pos))
                .map(|(zone_id, _)| *zone_id)
        });

        if let Some(zone_id) = disclosure_clicked_zone_id {
            self.toggle_zone_minimized(zone_id);
            self.select_zone(zone_id);
        }

        let disclosure_click_consumed = disclosure_clicked_zone_id.is_some();

        let visible_systems = if zone_overview_mode {
            Vec::new()
        } else {
            let visible_ids = self.visible_system_ids();
            self.systems
                .iter()
                .filter(|system| {
                    visible_ids.contains(&system.id)
                        && !minimized_hidden_system_ids.contains(&system.id)
                })
                .cloned()
                .collect::<Vec<_>>()
        };

        let mut node_rects: HashMap<i64, Rect> = HashMap::new();
        let mut card_row_hitboxes: Vec<(i64, String, Rect)> = Vec::new();
        for system in &visible_systems {
            // This is where rendering the system cards happens.
            if let Some(local_position) = self.effective_map_position(system.id) {
                let node_size_screen = self.map_node_size_cached_for_system(system) * zoom;
                let rect = Rect::from_min_size(to_screen(local_position), node_size_screen);
                node_rects.insert(system.id, rect);
            }
        }

        if !space_down && !self.zone_draw_mode {
            if let Some(selected_zone_id) = self.selected_zone_id {
                let selected_zone_item = zone_render_items
                    .iter()
                    .find(|zone| zone.id == selected_zone_id);
                let selected_zone_rect = selected_zone_item.map(|zone| zone.rect);
                let selected_zone_minimized = selected_zone_item
                    .map(|zone| zone.minimized)
                    .unwrap_or(false);

                if let Some(zone_rect) = selected_zone_rect {
                    painter.rect_stroke(zone_rect, 4.0, Stroke::new(1.5, Color32::from_gray(235)));

                    let handle_rect =
                        Rect::from_center_size(zone_rect.right_bottom(), Vec2::splat(12.0))
                            .intersect(map_rect);

                    if !selected_zone_minimized {
                        painter.rect_filled(handle_rect, 2.0, Color32::from_gray(225));
                        painter.rect_stroke(
                            handle_rect,
                            2.0,
                            Stroke::new(1.0, Color32::from_gray(25)),
                        );
                    }

                    let move_response = ui.interact(
                        zone_rect,
                        ui.id().with(("zone_move", selected_zone_id)),
                        Sense::click_and_drag(),
                    );
                    let resize_response = if selected_zone_minimized {
                        None
                    } else {
                        Some(ui.interact(
                            handle_rect,
                            ui.id().with(("zone_resize", selected_zone_id)),
                            Sense::click_and_drag(),
                        ))
                    };

                    if move_response.clicked() {
                        if let Some(pointer_pos) = ui.input(|input| input.pointer.interact_pos()) {
                            let clicked_zone_id = zone_id_at_pointer(pointer_pos);

                            if let Some(clicked_zone_id) = clicked_zone_id {
                                if clicked_zone_id != selected_zone_id {
                                    self.select_zone(clicked_zone_id);
                                    self.status_message = "Zone selected".to_owned();
                                }
                            }
                        }
                    }

                    if resize_response
                        .as_ref()
                        .map(|response| response.drag_started())
                        .unwrap_or(false)
                    {
                        if let Some(pointer_pos) = ui.input(|input| input.pointer.interact_pos()) {
                            let start_local = to_local(pointer_pos);
                            self.zone_drag_kind = Some(ZoneDragKind::ResizeBottomRight);
                            self.zone_drag_start_local = Some(start_local);
                            self.zone_drag_captured_system_positions.clear();
                            self.zone_drag_descendant_initial_positions.clear();
                            self.zone_drag_moves_captured_systems = false;
                            self.push_map_undo_snapshot();

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
                            self.zone_drag_captured_system_positions.clear();
                            self.zone_drag_descendant_initial_positions.clear();
                            let move_zone_only = ui.input(|input| input.modifiers.alt);
                            self.zone_drag_moves_captured_systems = !move_zone_only;
                            self.push_map_undo_snapshot();

                            if let Some(existing) =
                                self.zones.iter().find(|zone| zone.id == selected_zone_id)
                            {
                                let zone_x = existing.x;
                                let zone_y = existing.y;
                                let zone_width = existing.width;
                                let zone_height = existing.height;

                                self.zone_drag_initial_x = zone_x;
                                self.zone_drag_initial_y = zone_y;
                                self.zone_drag_initial_width = zone_width;
                                self.zone_drag_initial_height = zone_height;

                                let descendant_ids = self.zone_nested_child_ids(selected_zone_id);
                                for descendant_zone in self
                                    .zones
                                    .iter()
                                    .filter(|zone| descendant_ids.contains(&zone.id))
                                {
                                    self.zone_drag_descendant_initial_positions.insert(
                                        descendant_zone.id,
                                        Pos2::new(descendant_zone.x, descendant_zone.y),
                                    );
                                }

                                if self.zone_drag_moves_captured_systems {
                                    let zone_local_rect = Rect::from_min_size(
                                        Pos2::new(zone_x, zone_y),
                                        Vec2::new(zone_width, zone_height),
                                    );

                                    let mut bindings_to_apply: Vec<(i64, Pos2)> = Vec::new();

                                    for system in &self.systems {
                                        let Some(system_position) =
                                            self.effective_map_position(system.id)
                                        else {
                                            continue;
                                        };

                                        let node_size = self.map_node_size_cached_for_system(system);
                                        let node_center = Pos2::new(
                                            system_position.x + (node_size.x * 0.5),
                                            system_position.y + (node_size.y * 0.5),
                                        );

                                        if zone_local_rect.contains(node_center) {
                                            let offset = Pos2::new(
                                                system_position.x - zone_x,
                                                system_position.y - zone_y,
                                            );
                                            bindings_to_apply.push((system.id, offset));
                                        }
                                    }

                                    for (system_id, offset) in bindings_to_apply {
                                        self.assign_system_to_zone_offset(
                                            system_id,
                                            selected_zone_id,
                                            offset,
                                        );
                                        self.zone_drag_captured_system_positions
                                            .insert(system_id, offset);
                                    }
                                } else {
                                    for (system_id, (zone_id, offset)) in
                                        &self.zone_offsets_by_system
                                    {
                                        if *zone_id == selected_zone_id {
                                            self.zone_drag_captured_system_positions
                                                .insert(*system_id, *offset);
                                        }
                                    }
                                }
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

                        if self.snap_to_grid {
                            next_x = ((next_x / MAP_GRID_SPACING).round() * MAP_GRID_SPACING)
                                .clamp(0.0, max_x);
                            next_y = ((next_y / MAP_GRID_SPACING).round() * MAP_GRID_SPACING)
                                .clamp(0.0, max_y);

                            next_width = ((next_width / MAP_GRID_SPACING).round()
                                * MAP_GRID_SPACING)
                                .max(MAP_GRID_SPACING)
                                .min((self.map_world_size.x - next_x).max(MAP_GRID_SPACING));
                            next_height = ((next_height / MAP_GRID_SPACING).round()
                                * MAP_GRID_SPACING)
                                .max(MAP_GRID_SPACING)
                                .min((self.map_world_size.y - next_y).max(MAP_GRID_SPACING));
                        }

                        if matches!(drag_kind, ZoneDragKind::Move)
                            && !self.zone_drag_moves_captured_systems
                        {
                            let zone_delta = Pos2::new(
                                next_x - self.zone_drag_initial_x,
                                next_y - self.zone_drag_initial_y,
                            );

                            let snapshot = self.zone_drag_captured_system_positions.clone();
                            for (system_id, initial_offset) in snapshot {
                                let adjusted = Pos2::new(
                                    initial_offset.x - zone_delta.x,
                                    initial_offset.y - zone_delta.y,
                                );
                                self.assign_system_to_zone_offset(
                                    system_id,
                                    selected_zone_id,
                                    adjusted,
                                );
                            }
                        }

                        if matches!(drag_kind, ZoneDragKind::Move)
                            && self.zone_drag_moves_captured_systems
                        {
                            let zone_delta = Pos2::new(
                                next_x - self.zone_drag_initial_x,
                                next_y - self.zone_drag_initial_y,
                            );

                            let snapshot = self.zone_drag_descendant_initial_positions.clone();
                            for (descendant_id, initial_pos) in snapshot {
                                if let Some(descendant_zone) =
                                    self.zones.iter_mut().find(|zone| zone.id == descendant_id)
                                {
                                    let max_descendant_x =
                                        (self.map_world_size.x - descendant_zone.width).max(0.0);
                                    let max_descendant_y =
                                        (self.map_world_size.y - descendant_zone.height).max(0.0);
                                    descendant_zone.x =
                                        (initial_pos.x + zone_delta.x).clamp(0.0, max_descendant_x);
                                    descendant_zone.y =
                                        (initial_pos.y + zone_delta.y).clamp(0.0, max_descendant_y);
                                }
                            }
                        }

                        if let Some(zone) = self
                            .zones
                            .iter_mut()
                            .find(|zone| zone.id == selected_zone_id)
                        {
                            zone.x = next_x;
                            zone.y = next_y;
                            zone.width = next_width;
                            zone.height = next_height;
                        }
                    }

                    if self.zone_drag_kind.is_some()
                        && ui.input(|input| input.pointer.any_released())
                    {
                        if matches!(self.zone_drag_kind, Some(ZoneDragKind::Move)) {
                            let snapshot = self.zone_drag_captured_system_positions.clone();
                            for (system_id, _initial_offset) in snapshot {
                                let Some((_, offset)) =
                                    self.zone_offsets_by_system.get(&system_id).copied()
                                else {
                                    continue;
                                };
                                self.persist_system_zone_offset(
                                    system_id,
                                    selected_zone_id,
                                    offset,
                                );
                            }

                            if self.zone_drag_moves_captured_systems {
                                let descendant_ids = self
                                    .zone_drag_descendant_initial_positions
                                    .keys()
                                    .copied()
                                    .collect::<HashSet<_>>();

                                let descendants_to_persist = self
                                    .zones
                                    .iter()
                                    .filter(|zone| descendant_ids.contains(&zone.id))
                                    .cloned()
                                    .collect::<Vec<_>>();

                                for descendant_zone in descendants_to_persist {
                                    if let Err(error) = self.repo.update_zone(
                                        descendant_zone.id,
                                        descendant_zone.name.as_str(),
                                        descendant_zone.x,
                                        descendant_zone.y,
                                        descendant_zone.width,
                                        descendant_zone.height,
                                        descendant_zone.color.as_deref(),
                                        descendant_zone.render_priority,
                                        descendant_zone.parent_zone_id,
                                        descendant_zone.minimized,
                                        descendant_zone.representative_system_id,
                                    ) {
                                        self.status_message =
                                            format!("Failed to update nested zone geometry: {error}");
                                        break;
                                    } else {
                                        self.mark_project_as_dirty();
                                    }
                                }
                            }
                        }

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
                        self.zone_drag_captured_system_positions.clear();
                        self.zone_drag_descendant_initial_positions.clear();
                        self.zone_drag_moves_captured_systems = true;
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

            if let (Some(start), Some(end)) =
                (self.zone_draw_start_screen, self.zone_draw_end_screen)
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
                if let (Some(start), Some(end)) = (
                    self.zone_draw_start_screen.take(),
                    self.zone_draw_end_screen.take(),
                ) {
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
            let Some(current_local_position) = self.effective_map_position(system.id) else {
                continue;
            };

            let node_size = self.map_node_size_cached_for_system(&system);
            let node_size_screen = node_size * zoom;

            let node_rect =
                Rect::from_min_size(to_screen(current_local_position), node_size_screen);
            let node_interact_rect = node_rect.intersect(map_rect);
            if node_interact_rect.width() <= 0.0 || node_interact_rect.height() <= 0.0 {
                continue;
            }
            let interaction_sense =
                if space_down || self.zone_draw_mode || self.zone_drag_kind.is_some() {
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
                if disclosure_click_consumed {
                    continue;
                }

                if let Some(pick_source_id) = self.interaction_transfer_pick_source_id {
                    if pick_source_id == system.id {
                        self.status_message = "Choose a different destination system".to_owned();
                    } else {
                        self.selected_interaction_transfer_target_id = Some(system.id);
                        self.interaction_transfer_pick_source_id = None;
                        self.status_message = format!(
                            "Transfer destination set to '{}'",
                            self.system_name_by_id(system.id)
                        );
                    }
                    continue;
                }

                let ctrl_held = ui.input(|input| input.modifiers.ctrl);
                let alt_held = ui.input(|input| input.modifiers.alt);
                let clicked_reference = ui
                    .input(|input| input.pointer.interact_pos())
                    .and_then(|pointer_pos| {
                        card_row_hitboxes
                            .iter()
                            .rev()
                            .find(|(system_id, _, rect)| {
                                *system_id == system.id && rect.contains(pointer_pos)
                            })
                            .map(|(_, reference_name, _)| reference_name.clone())
                            .or_else(|| {
                                self.row_reference_at_pointer_for_system(
                                    &system,
                                    node_rect,
                                    pointer_pos,
                                )
                            })
                    });
                let interaction_chord_kind = ui.input(|input| {
                    if input.modifiers.ctrl && input.key_down(egui::Key::R) {
                        Some(InteractionKind::Standard)
                    } else if input.modifiers.ctrl && input.key_down(egui::Key::B) {
                        Some(InteractionKind::Pull)
                    } else if input.modifiers.ctrl && input.key_down(egui::Key::F) {
                        Some(InteractionKind::Push)
                    } else if input.modifiers.ctrl && input.key_down(egui::Key::D) {
                        Some(InteractionKind::Bidirectional)
                    } else {
                        None
                    }
                });

                if let Some(source_id) = self.map_interaction_drag_from {
                    if source_id == system.id {
                        self.status_message = "Select a different target system".to_owned();
                    } else {
                        let interaction_kind = self.map_interaction_drag_kind;
                        let source_reference = self.map_interaction_drag_from_reference.clone();
                        self.map_interaction_drag_from = None;
                        self.map_interaction_drag_from_reference = None;
                        self.create_link_between_kind_with_references(
                            source_id,
                            system.id,
                            "",
                            interaction_kind,
                            source_reference.as_deref(),
                            clicked_reference.as_deref(),
                        );
                    }
                } else if let Some(kind) = interaction_chord_kind {
                    self.map_interaction_drag_from = Some(system.id);
                    self.map_interaction_drag_from_reference = clicked_reference.clone();
                    self.map_interaction_drag_kind = kind;
                    self.select_system(system.id);
                    self.selected_map_system_ids.clear();
                    self.selected_map_system_ids.insert(system.id);
                    self.status_message = if let Some(reference_name) = clicked_reference {
                        format!(
                            "Interaction source '{}:{}' selected ({}) — click target",
                            self.system_name_by_id(system.id),
                            reference_name,
                            Self::interaction_kind_label(kind)
                        )
                    } else {
                        format!(
                            "Interaction source '{}' selected ({}) — click target",
                            self.system_name_by_id(system.id),
                            Self::interaction_kind_label(kind)
                        )
                    };
                } else if let Some(reference_name) = clicked_reference {
                    self.select_step_reference_endpoint_from_map(system.id, reference_name.as_str());
                } else if let Some(source_id) = self.map_link_click_source {
                    if source_id != system.id {
                        self.create_link_between(source_id, system.id, "");
                        self.map_link_click_source = None;
                    }
                } else if ctrl_held {
                    self.select_system(system.id);
                    self.selected_map_system_ids = self.system_and_descendant_ids(system.id);
                    if self.selected_catalog_tech_id_for_edit.is_some() {
                        self.fast_add_selected_catalog_tech_to_subtree(system.id);
                    }
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

                if shift_held {
                    self.map_link_drag_from = Some(system.id);
                } else if ctrl_held {
                    self.push_map_undo_snapshot();
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
                        if let Some(existing_position) = self.effective_map_position(*move_id) {
                            let next_position = Pos2::new(
                                existing_position.x + local_delta.x,
                                existing_position.y + local_delta.y,
                            );
                            let move_node_size = self
                                .systems
                                .iter()
                                .find(|candidate| candidate.id == *move_id)
                                .map(|candidate| self.map_node_size_cached_for_system(candidate))
                                .unwrap_or(node_size);
                            let clamped =
                                self.clamp_node_position(map_rect, next_position, move_node_size);

                            let bound_zone = self.zone_offsets_by_system.get(move_id).and_then(
                                |(zone_id, _)| {
                                    self.zones
                                        .iter()
                                        .find(|candidate| candidate.id == *zone_id)
                                        .map(|zone| (*zone_id, zone.x, zone.y))
                                },
                            );

                            if self.snap_to_grid {
                                let snapped = self.snap_to_open_grid_position(
                                    *move_id,
                                    clamped,
                                    move_node_size,
                                    &move_ids,
                                );
                                snap_preview_positions.insert(*move_id, snapped);
                            }

                            if let Some((zone_id, zone_x, zone_y)) = bound_zone {
                                let offset = Pos2::new(clamped.x - zone_x, clamped.y - zone_y);
                                self.assign_system_to_zone_offset(*move_id, zone_id, offset);
                            } else {
                                self.map_positions.insert(*move_id, clamped);
                            }
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
                        let Some(current_position) = self.effective_map_position(*persist_id)
                        else {
                            continue;
                        };

                        let persist_node_size = self
                            .systems
                            .iter()
                            .find(|candidate| candidate.id == *persist_id)
                            .map(|candidate| self.map_node_size_cached_for_system(candidate))
                            .unwrap_or(MAP_NODE_SIZE);
                        let snapped = self.snap_to_open_grid_position(
                            *persist_id,
                            current_position,
                            persist_node_size,
                            &persist_ids,
                        );

                        if let Some((zone_id, (bound_zone_id, _))) = self
                            .zone_offsets_by_system
                            .get(persist_id)
                            .map(|entry| (*persist_id, *entry))
                        {
                            if let Some(zone) = self
                                .zones
                                .iter()
                                .find(|candidate| candidate.id == bound_zone_id)
                            {
                                let offset = Pos2::new(snapped.x - zone.x, snapped.y - zone.y);
                                self.assign_system_to_zone_offset(zone_id, bound_zone_id, offset);
                            }
                        } else {
                            self.map_positions.insert(*persist_id, snapped);
                        }
                    }
                }

                for persist_id in persist_ids {
                    if let Some((zone_id, offset)) =
                        self.zone_offsets_by_system.get(&persist_id).copied()
                    {
                        self.persist_system_zone_offset(persist_id, zone_id, offset);
                    } else if let Some(position) = self.map_positions.get(&persist_id).copied() {
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
                self.draw_gradient_card_border(
                    &painter,
                    node_rect,
                    border_width,
                    &tech_border_colors,
                );
            }
            let text_color = Color32::from_gray(230);
            let text_scale_multiplier = self.map_text_scale_multiplier();
            let font_size =
                ((15.0 * self.map_zoom).clamp(8.0, 22.0) * text_scale_multiplier).clamp(6.0, 22.0);

            let entity_key = self.system_entity_for(&system).entity_key();
            let supports_row_references = self
                .entity_supports_row_references_for_type(system.system_type.as_str());
            let database_columns = self.database_columns_by_system.get(&system.id);

            let endpoint_rows = if supports_row_references {
                database_columns
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .enumerate()
                    .filter_map(|(index, row)| {
                        let reference_name = row.column_name.trim().to_owned();
                        if reference_name.is_empty() {
                            return None;
                        }

                        if entity_key == "step_processor" {
                            Some((
                                reference_name.clone(),
                                format!("{} ) {}", index + 1, reference_name),
                                None,
                                None,
                            ))
                        } else {
                            let right = row.column_type.trim();
                            let secondary = row
                                .constraints
                                .as_deref()
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .map(ToOwned::to_owned);

                            Some((
                                reference_name.clone(),
                                reference_name,
                                Some(right.to_owned()),
                                secondary,
                            ))
                        }
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };

            let has_endpoint_rows = !endpoint_rows.is_empty();
            let icon_only_zoom = self.map_zoom <= 0.20;
            let compact_database_zoom = self.map_zoom <= 0.30;
            let hide_service_content_zoom = self.map_zoom <= 0.10;
            let is_api_type = entity_key == "api";
            let is_service_like_type = !is_api_type;

            if hide_service_content_zoom && is_service_like_type {
                let font_id = FontId::proportional(font_size);
                let text_wrap_width =
                    (node_rect.width() - (MAP_CARD_HORIZONTAL_PADDING * self.map_zoom)).max(24.0);
                let wrapped_text = painter.layout(
                    system.name.clone(),
                    font_id,
                    text_color,
                    text_wrap_width,
                );
                let text_pos = Pos2::new(
                    node_rect.center().x - (wrapped_text.size().x * 0.5),
                    node_rect.center().y - (wrapped_text.size().y * 0.5),
                );
                painter.with_clip_rect(node_rect.shrink(1.0)).galley(
                    text_pos,
                    wrapped_text,
                    text_color,
                );
            } else if (is_service_like_type && !has_endpoint_rows) && compact_database_zoom {
                let font_id = FontId::proportional(font_size);
                let text_wrap_width =
                    (node_rect.width() - (MAP_CARD_HORIZONTAL_PADDING * self.map_zoom)).max(24.0);
                let wrapped_text = painter.layout(
                    format!(
                        "{} {}",
                        Self::map_card_icon_for_system_type(system.system_type.as_str()),
                        system.name
                    ),
                    font_id,
                    text_color,
                    text_wrap_width,
                );
                let text_pos = Pos2::new(
                    node_rect.center().x - (wrapped_text.size().x * 0.5),
                    node_rect.center().y - (wrapped_text.size().y * 0.5),
                );
                painter.with_clip_rect(node_rect.shrink(1.0)).galley(
                    text_pos,
                    wrapped_text,
                    text_color,
                );
            } else if supports_row_references && has_endpoint_rows && compact_database_zoom {
                let font_id = FontId::proportional(font_size);
                let text_wrap_width =
                    (node_rect.width() - (MAP_CARD_HORIZONTAL_PADDING * self.map_zoom)).max(24.0);
                let wrapped_text = painter.layout(
                    format!(
                        "{} {}",
                        Self::map_card_icon_for_system_type(system.system_type.as_str()),
                        system.name
                    ),
                    font_id,
                    text_color,
                    text_wrap_width,
                );
                let text_pos = Pos2::new(
                    node_rect.center().x - (wrapped_text.size().x * 0.5),
                    node_rect.center().y - (wrapped_text.size().y * 0.5),
                );
                painter.with_clip_rect(node_rect.shrink(1.0)).galley(
                    text_pos,
                    wrapped_text,
                    text_color,
                );
            } else if icon_only_zoom {
                let icon = Self::map_card_icon_for_system_type(system.system_type.as_str());
                let icon_font_size = if is_service_like_type {
                    (16.0 * self.map_zoom).clamp(6.0, 11.0)
                } else {
                    (22.0 * self.map_zoom).clamp(8.0, 16.0)
                };
                painter.text(
                    node_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    icon,
                    FontId::proportional(icon_font_size),
                    text_color,
                );
            } else if supports_row_references && has_endpoint_rows {
                let header_font = FontId::proportional(font_size);
                let row_font = FontId::monospace((font_size * 0.86).clamp(6.0, 18.0));
                let constraints_font = FontId::proportional((font_size * 0.68).clamp(6.0, 14.0));

                let title = format!(
                    "{} {}",
                    Self::map_card_icon_for_system_type(system.system_type.as_str()),
                    system.name
                );
                let top_padding = (8.0 * self.map_zoom).clamp(4.0, 10.0);
                let left_padding = (8.0 * self.map_zoom).clamp(4.0, 10.0);
                let right_padding = left_padding;
                let row_height = (22.0 * self.map_zoom).clamp(14.0, 30.0);
                let row_vertical_padding = (3.0 * self.map_zoom).clamp(2.0, 5.0);
                let separator_y = node_rect.top() + top_padding + row_height;

                painter.text(
                    Pos2::new(node_rect.center().x, node_rect.top() + top_padding),
                    egui::Align2::CENTER_TOP,
                    title,
                    header_font,
                    text_color,
                );

                painter.line_segment(
                    [
                        Pos2::new(node_rect.left() + 4.0, separator_y),
                        Pos2::new(node_rect.right() - 4.0, separator_y),
                    ],
                    Stroke::new(1.0, Color32::from_gray(96)),
                );

                let mut row_y = separator_y + 4.0;
                for (reference_name, left_text, right_text, secondary_text) in endpoint_rows {
                    if row_y + row_height > node_rect.bottom() - 2.0 {
                        break;
                    }

                    let row_center_y = row_y + (row_height * 0.5);
                    let row_rect = Rect::from_min_max(
                        Pos2::new(node_rect.left() + 4.0, row_y),
                        Pos2::new(node_rect.right() - 4.0, row_y + row_height),
                    );
                    let row_is_selected_interaction_source = self.map_interaction_drag_from
                        == Some(system.id)
                        && self
                            .map_interaction_drag_from_reference
                            .as_deref()
                            == Some(reference_name.as_str());
                    let row_is_hovered = ui
                        .input(|input| input.pointer.hover_pos())
                        .map(|pointer_pos| row_rect.contains(pointer_pos))
                        .unwrap_or(false);

                    if row_is_hovered && !row_is_selected_interaction_source {
                        painter.rect_filled(
                            row_rect,
                            3.0,
                            Color32::from_rgba_unmultiplied(130, 130, 130, 32),
                        );
                    }

                    if row_is_selected_interaction_source {
                        painter.rect_filled(
                            row_rect,
                            3.0,
                            Color32::from_rgba_unmultiplied(114, 194, 255, 56),
                        );
                        painter.rect_stroke(
                            row_rect,
                            3.0,
                            Stroke::new(1.0, Color32::from_rgb(130, 210, 255)),
                        );
                    }

                    painter.text(
                        Pos2::new(node_rect.left() + left_padding, row_center_y),
                        egui::Align2::LEFT_CENTER,
                        left_text,
                        row_font.clone(),
                        text_color,
                    );

                    if let Some(value) = right_text {
                        painter.text(
                            Pos2::new(node_rect.right() - right_padding, row_center_y),
                            egui::Align2::RIGHT_CENTER,
                            value,
                            row_font.clone(),
                            text_color,
                        );
                    }

                    if !compact_database_zoom {
                        if let Some(secondary) = secondary_text {
                            painter.text(
                                Pos2::new(node_rect.center().x, row_y + row_height - row_vertical_padding),
                                egui::Align2::CENTER_BOTTOM,
                                secondary,
                                constraints_font.clone(),
                                Color32::from_gray(176),
                            );
                        }
                    }

                    card_row_hitboxes.push((
                        system.id,
                        reference_name,
                        row_rect.intersect(node_rect),
                    ));

                    let line_y = row_y + row_height - 2.0;
                    painter.line_segment(
                        [
                            Pos2::new(node_rect.left() + 4.0, line_y),
                            Pos2::new(node_rect.right() - 4.0, line_y),
                        ],
                        Stroke::new(1.0, Color32::from_gray(72)),
                    );

                    row_y += row_height;
                }
            } else {
                let font_id = FontId::proportional(font_size);
                let text_wrap_width =
                    (node_rect.width() - (MAP_CARD_HORIZONTAL_PADDING * self.map_zoom)).max(24.0);
                let wrapped_text = painter.layout(
                    self.map_card_label_cached_for_system(&system),
                    font_id,
                    text_color,
                    text_wrap_width,
                );
                let text_pos = Pos2::new(
                    node_rect.center().x - (wrapped_text.size().x * 0.5),
                    node_rect.center().y - (wrapped_text.size().y * 0.5),
                );
                painter.with_clip_rect(node_rect.shrink(1.0)).galley(
                    text_pos,
                    wrapped_text,
                    text_color,
                );
            }

            let has_children = self.system_has_children(system.id);
            if has_children && self.map_zoom > 0.30 {
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
                    let zone_ids = self.visible_minimized_zone_ids_for_disclosure_system(system.id);
                    if zone_ids.is_empty() {
                        self.on_disclosure_click(system.id);
                    } else {
                        for zone_id in zone_ids {
                            self.toggle_zone_minimized(zone_id);
                        }

                        if self.collapsed_system_ids.contains(&system.id) {
                            self.on_disclosure_click(system.id);
                        }
                    }
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
                    .map(|candidate| self.map_node_size_cached_for_system(candidate))
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

        if map_response.clicked() && !space_down {
            if disclosure_click_consumed {
                return;
            }

            let clicked_zone_id =
                ui.input(|input| input.pointer.interact_pos())
                    .and_then(|pointer_pos| zone_id_at_pointer(pointer_pos));

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
                self.selected_zone_parent_zone_id = None;
                self.selected_zone_minimized = false;
                self.selected_zone_representative_system_id = None;
                self.zone_drag_kind = None;
                self.zone_drag_start_local = None;
                self.zone_drag_captured_system_positions.clear();
                self.zone_drag_descendant_initial_positions.clear();
                self.zone_drag_moves_captured_systems = true;
                self.status_message = "Selection cleared".to_owned();
            }
        }
    }

    fn render_llm_detailed_import_modal(&mut self, ctx: &egui::Context) {
        if !self.show_llm_detailed_import_modal {
            return;
        }

        let mut open = self.show_llm_detailed_import_modal;
        let mut close_requested = false;

        egui::Window::new("LLM Detailed Import")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Root system name for this import");
                ui.text_edit_singleline(&mut self.pending_llm_detailed_root_name);
                ui.small(format!(
                    "Importing {} systems and {} interactions",
                    self.pending_llm_detailed_system_drafts.len(),
                    self.pending_llm_detailed_interaction_drafts.len()
                ));

                ui.horizontal(|ui| {
                    if ui.button("Import").clicked() {
                        self.apply_pending_llm_detailed_import();
                        if !self.show_llm_detailed_import_modal {
                            close_requested = true;
                        }
                    }

                    if ui.button("Cancel").clicked() {
                        self.cancel_pending_llm_detailed_import();
                        close_requested = true;
                    }
                });
            });

        if close_requested {
            open = false;
        }

        self.show_llm_detailed_import_modal = open;
    }
}

impl eframe::App for SystemsCatalogApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.refresh_visible_system_ids_cache();
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
        let save_project =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::S));
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

        if save_project {
            if self.save_catalog_path.trim().is_empty() && !self.current_catalog_path.trim().is_empty() {
                self.save_catalog_path = self.current_catalog_path.clone();
            }

            if self.save_catalog_path.trim().is_empty() {
                self.open_modal(AppModal::SaveCatalog);
            } else {
                self.export_catalog();
            }
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

        self.render_top_toolbar(ctx);

        if self.show_left_sidebar {
            egui::SidePanel::left("systems_panel")
                .resizable(true)
                .min_width(120.0)
                .default_width(180.0)
                .max_width(560.0)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        self.render_sidebar(ui);
                    });
                });
        }

        if self.pending_catalog_switch_path.is_some() {
            if !self.pending_catalog_switch_armed {
                self.pending_catalog_switch_armed = true;
                if let Some(path) = &self.pending_catalog_switch_path {
                    self.status_message = format!("Loading project {}...", path);
                }
                ctx.request_repaint();
            } else if let Some(path) = self.pending_catalog_switch_path.take() {
                self.pending_catalog_switch_armed = false;
                self.switch_to_recent_catalog(path.as_str());
            }
        }

        if self.selected_system_id.is_some() || self.selected_zone_id.is_some() {
            egui::SidePanel::right("details_panel")
                .resizable(true)
                .min_width(120.0)
                .default_width(120.0)
                .max_width(560.0)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        self.render_zone_details(ui);
                        self.render_details(ui);
                    });
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_map_canvas(ui);
        });

        self.process_flow_inspector_pick_from_selection();

        self.render_add_system_modal(ctx);
        self.render_bulk_add_systems_modal(ctx);
        self.render_add_tech_modal(ctx);
        self.render_save_catalog_modal(ctx);
        self.render_load_catalog_modal(ctx);
        self.render_new_catalog_confirm_modal(ctx);
        self.render_step_processor_conversion_confirm_modal(ctx);
        self.render_llm_detailed_import_modal(ctx);
        self.render_ddl_table_mapping_modal(ctx);
        self.render_hotkeys_modal(ctx);
        self.render_interaction_style_modal(ctx);
        self.render_flow_inspector_modal(ctx);
        self.render_help_getting_started_modal(ctx);
        self.render_help_creating_interactions_modal(ctx);
        self.render_help_managing_technology_modal(ctx);
        self.render_help_understanding_map_modal(ctx);
        self.render_help_zones_modal(ctx);
        self.render_help_keyboard_shortcuts_modal(ctx);
        self.render_help_troubleshooting_modal(ctx);

        egui::Window::new("🔍 Inspection")
            .open(&mut self.show_debug_inspection_window)
            .vscroll(true)
            .show(ctx, |ui| {
                ctx.inspection_ui(ui);
            });

        egui::Window::new("📝 Memory")
            .open(&mut self.show_debug_memory_window)
            .resizable(false)
            .show(ctx, |ui| {
                ctx.memory_ui(ui);
            });

        let now_secs = ctx.input(|input| input.time);
        self.maybe_autosave_project(now_secs);

        self.save_ui_settings_if_dirty();
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.save_ui_settings_if_dirty();
        eframe::set_value(storage, eframe::APP_KEY, &self.to_eframe_persisted_state());
    }

}
