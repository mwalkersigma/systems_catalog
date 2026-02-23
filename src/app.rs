mod actions;
mod ui;

use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Result};
use eframe::egui::{Color32, Pos2, Rect, Vec2};

use crate::db::Repository;
use crate::models::{SystemLink, SystemRecord, TechItem};

const MAP_NODE_SIZE: Vec2 = Vec2::new(170.0, 64.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineTerminator {
    None,
    Arrow,
    FilledArrow,
}

#[derive(Debug, Clone, Copy)]
pub struct LineStyle {
    pub width: f32,
    pub color: Color32,
    pub terminator: LineTerminator,
}

/// Primary UI application state.
///
/// TypeScript analogy: this struct is similar to a React component's local state + service
/// dependencies, except Rust stores it in one explicit data structure.
pub struct SystemsCatalogApp {
    repo: Repository,
    systems: Vec<SystemRecord>,
    all_links: Vec<SystemLink>,
    tech_catalog: Vec<TechItem>,
    selected_system_id: Option<i64>,
    selected_links: Vec<SystemLink>,
    selected_system_tech: Vec<TechItem>,
    selected_cumulative_child_tech: Vec<String>,
    selected_system_line_color_override: Option<Color32>,
    note_text: String,

    new_system_name: String,
    new_system_description: String,
    new_system_parent_id: Option<i64>,
    selected_system_parent_id: Option<i64>,

    new_link_target_id: Option<i64>,
    new_link_label: String,
    selected_link_id_for_edit: Option<i64>,
    edited_link_label: String,

    new_tech_name: String,
    selected_tech_id_for_assignment: Option<i64>,
    selected_catalog_tech_id_for_edit: Option<i64>,
    edited_tech_name: String,

    map_positions: HashMap<i64, Pos2>,
    map_link_drag_from: Option<i64>,
    map_link_click_source: Option<i64>,
    map_zoom: f32,
    map_pan: Vec2,
    collapsed_system_ids: HashSet<i64>,

    show_add_system_modal: bool,
    show_add_tech_modal: bool,
    show_line_style_modal: bool,

    parent_line_style: LineStyle,
    interaction_line_style: LineStyle,
    show_parent_lines: bool,
    show_interaction_lines: bool,
    dimmed_line_opacity_percent: f32,
    selected_line_brightness_percent: f32,
    settings_dirty: bool,

    status_message: String,
}

impl SystemsCatalogApp {
    pub fn new(repo: Repository) -> Result<Self> {
        let mut app = Self {
            repo,
            systems: Vec::new(),
            all_links: Vec::new(),
            tech_catalog: Vec::new(),
            selected_system_id: None,
            selected_links: Vec::new(),
            selected_system_tech: Vec::new(),
            selected_cumulative_child_tech: Vec::new(),
            selected_system_line_color_override: None,
            note_text: String::new(),
            new_system_name: String::new(),
            new_system_description: String::new(),
            new_system_parent_id: None,
            selected_system_parent_id: None,
            new_link_target_id: None,
            new_link_label: String::new(),
            selected_link_id_for_edit: None,
            edited_link_label: String::new(),
            new_tech_name: String::new(),
            selected_tech_id_for_assignment: None,
            selected_catalog_tech_id_for_edit: None,
            edited_tech_name: String::new(),
            map_positions: HashMap::new(),
            map_link_drag_from: None,
            map_link_click_source: None,
            map_zoom: 1.0,
            map_pan: Vec2::ZERO,
            collapsed_system_ids: HashSet::new(),
            show_add_system_modal: false,
            show_add_tech_modal: false,
            show_line_style_modal: false,
            parent_line_style: LineStyle {
                width: 1.0,
                color: Color32::from_gray(90),
                terminator: LineTerminator::Arrow,
            },
            interaction_line_style: LineStyle {
                width: 1.5,
                color: Color32::from_gray(140),
                terminator: LineTerminator::FilledArrow,
            },
            show_parent_lines: true,
            show_interaction_lines: true,
            dimmed_line_opacity_percent: 18.0,
            selected_line_brightness_percent: 135.0,
            settings_dirty: false,
            status_message: "Ready".to_owned(),
        };

        app.refresh_systems()?;
        app.load_ui_settings()?;
        Ok(app)
    }

    fn refresh_systems(&mut self) -> Result<()> {
        self.systems = self.repo.list_systems()?;
        self.all_links = self.repo.list_links()?;
        self.tech_catalog = self.repo.list_tech_catalog()?;

        self.map_positions
            .retain(|system_id, _| self.systems.iter().any(|system| system.id == *system_id));

        self.collapsed_system_ids
            .retain(|system_id| self.systems.iter().any(|system| system.id == *system_id));

        for system in &self.systems {
            if let (Some(map_x), Some(map_y)) = (system.map_x, system.map_y) {
                self.map_positions
                    .insert(system.id, Pos2::new(map_x, map_y));
            }
        }

        if let Some(selected) = self.selected_system_id {
            let still_exists = self.systems.iter().any(|system| system.id == selected);
            if !still_exists {
                self.clear_selection();
            }
        }

        if let Some(selected) = self.selected_system_id {
            let visible = self.visible_system_ids();
            if !visible.contains(&selected) {
                self.clear_selection();
            }
        }

        if let Some(selected) = self.selected_system_id {
            self.load_selected_data(selected)?;
        }

        Ok(())
    }

    fn load_selected_data(&mut self, system_id: i64) -> Result<()> {
        self.selected_links = self.repo.list_links_for_system(system_id)?;
        self.selected_system_tech = self.repo.list_tech_for_system(system_id)?;

        if let Some(selected_link_id) = self.selected_link_id_for_edit {
            let still_exists = self
                .selected_links
                .iter()
                .any(|link| link.id == selected_link_id);
            if !still_exists {
                self.selected_link_id_for_edit = None;
                self.edited_link_label.clear();
            }
        }

        if self.selected_link_id_for_edit.is_none() {
            if let Some(first_link) = self.selected_links.first() {
                self.selected_link_id_for_edit = Some(first_link.id);
                self.edited_link_label = first_link.label.clone();
            }
        }

        if let Some(selected_catalog_tech_id) = self.selected_catalog_tech_id_for_edit {
            let still_exists = self
                .tech_catalog
                .iter()
                .any(|tech| tech.id == selected_catalog_tech_id);
            if !still_exists {
                self.selected_catalog_tech_id_for_edit = None;
                self.edited_tech_name.clear();
            }
        }

        if self.selected_catalog_tech_id_for_edit.is_none() {
            if let Some(first_tech) = self.tech_catalog.first() {
                self.selected_catalog_tech_id_for_edit = Some(first_tech.id);
                self.edited_tech_name = first_tech.name.clone();
            }
        }

        self.selected_system_parent_id = self
            .systems
            .iter()
            .find(|system| system.id == system_id)
            .and_then(|system| system.parent_id);
        self.note_text = self
            .repo
            .get_note(system_id)?
            .map(|note| {
                let _id = note.system_id;
                let _updated = note.updated_at;
                note.body
            })
            .unwrap_or_default();

        self.selected_cumulative_child_tech = self.cumulative_child_tech_names(system_id);
        self.selected_system_line_color_override = self
            .systems
            .iter()
            .find(|system| system.id == system_id)
            .and_then(|system| {
                system
                    .line_color_override
                    .as_deref()
                    .and_then(Self::color_from_setting_value)
            });
        Ok(())
    }

    fn selected_system(&self) -> Option<&SystemRecord> {
        self.selected_system_id
            .and_then(|id| self.systems.iter().find(|system| system.id == id))
    }

    fn system_name_by_id(&self, id: i64) -> String {
        self.systems
            .iter()
            .find(|system| system.id == id)
            .map(|system| system.name.clone())
            .unwrap_or_else(|| format!("Unknown ({id})"))
    }

    fn tech_name_by_id(&self, id: i64) -> String {
        self.tech_catalog
            .iter()
            .find(|tech| tech.id == id)
            .map(|tech| tech.name.clone())
            .unwrap_or_else(|| format!("Unknown tech ({id})"))
    }

    fn visible_hierarchy_rows(&self) -> Vec<(usize, i64, String, bool, bool)> {
        let mut children_by_parent: HashMap<Option<i64>, Vec<&SystemRecord>> = HashMap::new();
        for system in &self.systems {
            children_by_parent
                .entry(system.parent_id)
                .or_default()
                .push(system);
        }

        for children in children_by_parent.values_mut() {
            children.sort_by_key(|system| system.name.to_lowercase());
        }

        let mut rows = Vec::new();
        self.walk_visible_hierarchy(None, 0, &children_by_parent, &mut rows);

        rows
    }

    fn walk_visible_hierarchy(
        &self,
        parent_id: Option<i64>,
        depth: usize,
        by_parent: &HashMap<Option<i64>, Vec<&SystemRecord>>,
        rows: &mut Vec<(usize, i64, String, bool, bool)>,
    ) {
        if let Some(children) = by_parent.get(&parent_id) {
            for child in children {
                let has_children = by_parent
                    .get(&Some(child.id))
                    .map(|nested| !nested.is_empty())
                    .unwrap_or(false);
                let is_collapsed = self.collapsed_system_ids.contains(&child.id);
                rows.push((
                    depth,
                    child.id,
                    child.name.clone(),
                    has_children,
                    is_collapsed,
                ));

                if !is_collapsed {
                    self.walk_visible_hierarchy(Some(child.id), depth + 1, by_parent, rows);
                }
            }
        }
    }

    fn visible_system_ids(&self) -> HashSet<i64> {
        self.visible_hierarchy_rows()
            .into_iter()
            .map(|(_, system_id, _, _, _)| system_id)
            .collect()
    }

    fn on_disclosure_click(&mut self, system_id: i64) {
        if self.collapsed_system_ids.contains(&system_id) {
            self.collapsed_system_ids.remove(&system_id);
        } else {
            self.collapsed_system_ids.insert(system_id);
        }

        let visible = self.visible_system_ids();
        if let Some(selected_system_id) = self.selected_system_id {
            if !visible.contains(&selected_system_id) {
                self.clear_selection();
            }
        }
    }

    fn clear_subset_visibility(&mut self) {
        self.collapsed_system_ids.clear();
    }

    fn clear_selection(&mut self) {
        self.selected_system_id = None;
        self.selected_links.clear();
        self.selected_system_tech.clear();
        self.selected_cumulative_child_tech.clear();
        self.selected_system_line_color_override = None;
        self.note_text.clear();
        self.selected_system_parent_id = None;
        self.selected_link_id_for_edit = None;
        self.edited_link_label.clear();
        self.map_link_click_source = None;
    }

    fn terminator_to_setting_value(terminator: LineTerminator) -> &'static str {
        match terminator {
            LineTerminator::None => "none",
            LineTerminator::Arrow => "arrow",
            LineTerminator::FilledArrow => "filled_arrow",
        }
    }

    fn terminator_from_setting_value(value: &str) -> Option<LineTerminator> {
        match value {
            "none" => Some(LineTerminator::None),
            "arrow" => Some(LineTerminator::Arrow),
            "filled_arrow" => Some(LineTerminator::FilledArrow),
            _ => None,
        }
    }

    fn color_to_setting_value(color: Color32) -> String {
        format!("{},{},{},{}", color.r(), color.g(), color.b(), color.a())
    }

    fn color_from_setting_value(value: &str) -> Option<Color32> {
        let parts = value
            .split(',')
            .map(|part| part.trim().parse::<u8>().ok())
            .collect::<Vec<_>>();

        if parts.len() != 4 {
            return None;
        }

        Some(Color32::from_rgba_unmultiplied(
            parts[0]?, parts[1]?, parts[2]?, parts[3]?,
        ))
    }

    fn load_ui_settings(&mut self) -> Result<()> {
        if let Some(value) = self.repo.get_setting("map_zoom")? {
            if let Ok(parsed) = value.parse::<f32>() {
                self.map_zoom = parsed.clamp(0.5, 2.5);
            }
        }

        if let Some(value) = self.repo.get_setting("map_pan_x")? {
            if let Ok(parsed) = value.parse::<f32>() {
                self.map_pan.x = parsed;
            }
        }

        if let Some(value) = self.repo.get_setting("map_pan_y")? {
            if let Ok(parsed) = value.parse::<f32>() {
                self.map_pan.y = parsed;
            }
        }

        if let Some(value) = self.repo.get_setting("parent_line_width")? {
            if let Ok(parsed) = value.parse::<f32>() {
                self.parent_line_style.width = parsed.clamp(0.5, 6.0);
            }
        }

        if let Some(value) = self.repo.get_setting("parent_line_color")? {
            if let Some(parsed) = Self::color_from_setting_value(&value) {
                self.parent_line_style.color = parsed;
            }
        }

        if let Some(value) = self.repo.get_setting("parent_line_terminator")? {
            if let Some(parsed) = Self::terminator_from_setting_value(&value) {
                self.parent_line_style.terminator = parsed;
            }
        }

        if let Some(value) = self.repo.get_setting("interaction_line_width")? {
            if let Ok(parsed) = value.parse::<f32>() {
                self.interaction_line_style.width = parsed.clamp(0.5, 6.0);
            }
        }

        if let Some(value) = self.repo.get_setting("interaction_line_color")? {
            if let Some(parsed) = Self::color_from_setting_value(&value) {
                self.interaction_line_style.color = parsed;
            }
        }

        if let Some(value) = self.repo.get_setting("interaction_line_terminator")? {
            if let Some(parsed) = Self::terminator_from_setting_value(&value) {
                self.interaction_line_style.terminator = parsed;
            }
        }

        if let Some(value) = self.repo.get_setting("show_parent_lines")? {
            self.show_parent_lines = value == "true";
        }

        if let Some(value) = self.repo.get_setting("show_interaction_lines")? {
            self.show_interaction_lines = value == "true";
        }

        if let Some(value) = self.repo.get_setting("dimmed_line_opacity_percent")? {
            if let Ok(parsed) = value.parse::<f32>() {
                self.dimmed_line_opacity_percent = parsed.clamp(0.0, 100.0);
            }
        }

        if let Some(value) = self.repo.get_setting("selected_line_brightness_percent")? {
            if let Ok(parsed) = value.parse::<f32>() {
                self.selected_line_brightness_percent = parsed.clamp(100.0, 220.0);
            }
        }

        Ok(())
    }

    fn save_ui_settings_if_dirty(&mut self) {
        if !self.settings_dirty {
            return;
        }

        let result = (|| -> Result<()> {
            self.repo
                .set_setting("map_zoom", &self.map_zoom.to_string())?;
            self.repo
                .set_setting("map_pan_x", &self.map_pan.x.to_string())?;
            self.repo
                .set_setting("map_pan_y", &self.map_pan.y.to_string())?;

            self.repo.set_setting(
                "parent_line_width",
                &self.parent_line_style.width.to_string(),
            )?;
            self.repo.set_setting(
                "parent_line_color",
                &Self::color_to_setting_value(self.parent_line_style.color),
            )?;
            self.repo.set_setting(
                "parent_line_terminator",
                Self::terminator_to_setting_value(self.parent_line_style.terminator),
            )?;

            self.repo.set_setting(
                "interaction_line_width",
                &self.interaction_line_style.width.to_string(),
            )?;
            self.repo.set_setting(
                "interaction_line_color",
                &Self::color_to_setting_value(self.interaction_line_style.color),
            )?;
            self.repo.set_setting(
                "interaction_line_terminator",
                Self::terminator_to_setting_value(self.interaction_line_style.terminator),
            )?;

            self.repo.set_setting(
                "show_parent_lines",
                if self.show_parent_lines {
                    "true"
                } else {
                    "false"
                },
            )?;
            self.repo.set_setting(
                "show_interaction_lines",
                if self.show_interaction_lines {
                    "true"
                } else {
                    "false"
                },
            )?;

            self.repo.set_setting(
                "dimmed_line_opacity_percent",
                &self.dimmed_line_opacity_percent.to_string(),
            )?;

            self.repo.set_setting(
                "selected_line_brightness_percent",
                &self.selected_line_brightness_percent.to_string(),
            )?;

            Ok(())
        })();

        match result {
            Ok(_) => {
                self.settings_dirty = false;
            }
            Err(error) => {
                self.status_message = format!("Failed to persist UI settings: {error}");
            }
        }
    }

    fn ensure_valid_parent_selection(&mut self) {
        if let Some(parent_id) = self.new_system_parent_id {
            let exists = self.systems.iter().any(|system| system.id == parent_id);
            if !exists {
                self.new_system_parent_id = None;
            }
        }
    }

    fn ensure_valid_selected_parent_selection(&mut self) {
        if let Some(parent_id) = self.selected_system_parent_id {
            let exists = self.systems.iter().any(|system| system.id == parent_id);
            if !exists {
                self.selected_system_parent_id = None;
            }
        }
    }

    fn ensure_valid_link_target_selection(&mut self) {
        if let Some(target_id) = self.new_link_target_id {
            let exists = self.systems.iter().any(|system| system.id == target_id);
            if !exists {
                self.new_link_target_id = None;
            }
        }
    }

    fn ensure_valid_tech_selection(&mut self) {
        if let Some(tech_id) = self.selected_tech_id_for_assignment {
            let exists = self.tech_catalog.iter().any(|tech| tech.id == tech_id);
            if !exists {
                self.selected_tech_id_for_assignment = None;
            }
        }
    }

    fn ensure_valid_selected_link(&mut self) {
        if let Some(link_id) = self.selected_link_id_for_edit {
            let exists = self.selected_links.iter().any(|link| link.id == link_id);
            if !exists {
                self.selected_link_id_for_edit = None;
                self.edited_link_label.clear();
            }
        }
    }

    fn ensure_valid_selected_catalog_tech(&mut self) {
        if let Some(tech_id) = self.selected_catalog_tech_id_for_edit {
            let exists = self.tech_catalog.iter().any(|tech| tech.id == tech_id);
            if !exists {
                self.selected_catalog_tech_id_for_edit = None;
                self.edited_tech_name.clear();
            }
        }
    }

    fn cumulative_child_tech_names(&self, parent_system_id: i64) -> Vec<String> {
        let mut children_by_parent: HashMap<Option<i64>, Vec<i64>> = HashMap::new();
        for system in &self.systems {
            children_by_parent
                .entry(system.parent_id)
                .or_default()
                .push(system.id);
        }

        let mut descendant_ids = Vec::new();
        self.collect_descendant_ids(parent_system_id, &children_by_parent, &mut descendant_ids);

        let mut names = HashSet::new();
        for descendant_id in descendant_ids {
            if let Ok(technologies) = self.repo.list_tech_for_system(descendant_id) {
                for technology in technologies {
                    names.insert(technology.name);
                }
            }
        }

        let mut sorted = names.into_iter().collect::<Vec<_>>();
        sorted.sort_by_key(|name| name.to_lowercase());
        sorted
    }

    fn collect_descendant_ids(
        &self,
        parent_system_id: i64,
        children_by_parent: &HashMap<Option<i64>, Vec<i64>>,
        descendant_ids: &mut Vec<i64>,
    ) {
        if let Some(children) = children_by_parent.get(&Some(parent_system_id)) {
            for child_id in children {
                descendant_ids.push(*child_id);
                self.collect_descendant_ids(*child_id, children_by_parent, descendant_ids);
            }
        }
    }

    fn would_create_parent_cycle(&self, system_id: i64, candidate_parent_id: i64) -> bool {
        if system_id == candidate_parent_id {
            return true;
        }

        let mut current_parent = self
            .systems
            .iter()
            .find(|system| system.id == candidate_parent_id)
            .and_then(|system| system.parent_id);

        while let Some(parent_id) = current_parent {
            if parent_id == system_id {
                return true;
            }

            current_parent = self
                .systems
                .iter()
                .find(|system| system.id == parent_id)
                .and_then(|system| system.parent_id);
        }

        false
    }

    fn ensure_map_positions(&mut self) {
        let mut index = 0usize;
        let columns = 4usize;

        for system in &self.systems {
            if self.map_positions.contains_key(&system.id) {
                continue;
            }

            let col = index % columns;
            let row = index / columns;
            let x = 24.0 + (col as f32 * (MAP_NODE_SIZE.x + 24.0));
            let y = 24.0 + (row as f32 * (MAP_NODE_SIZE.y + 20.0));

            self.map_positions.insert(system.id, Pos2::new(x, y));
            index += 1;
        }
    }

    fn clamp_node_position(&self, map_rect: Rect, position: Pos2, node_size: Vec2) -> Pos2 {
        let max_x = (map_rect.width() / self.map_zoom) - node_size.x - 8.0;
        let max_y = (map_rect.height() / self.map_zoom) - node_size.y - 8.0;

        Pos2::new(
            position.x.clamp(8.0, max_x.max(8.0)),
            position.y.clamp(8.0, max_y.max(8.0)),
        )
    }

    fn persist_map_position(&mut self, system_id: i64, position: Pos2) {
        if let Err(error) =
            self.repo
                .update_system_position(system_id, position.x as f64, position.y as f64)
        {
            self.status_message = format!("Failed to persist map position: {error}");
        }
    }

    fn reset_map_layout(&mut self) {
        let result = self.repo.clear_system_positions().and_then(|_| {
            self.map_positions.clear();
            self.ensure_map_positions();

            for (system_id, position) in self.map_positions.clone() {
                self.repo.update_system_position(
                    system_id,
                    position.x as f64,
                    position.y as f64,
                )?;
            }

            Ok(())
        });

        match result {
            Ok(_) => self.status_message = "Map layout reset".to_owned(),
            Err(error) => self.status_message = format!("Failed to reset map layout: {error}"),
        }
    }

    fn validate_before_render(&mut self) -> Result<()> {
        self.ensure_valid_parent_selection();
        self.ensure_valid_selected_parent_selection();
        self.ensure_valid_link_target_selection();
        self.ensure_valid_tech_selection();
        self.ensure_valid_selected_link();
        self.ensure_valid_selected_catalog_tech();

        let visible_ids = self.visible_system_ids();
        if let Some(selected_system_id) = self.selected_system_id {
            if !visible_ids.contains(&selected_system_id) {
                self.clear_selection();
            }
        }

        if self.systems.is_empty() && self.selected_system_id.is_some() {
            return Err(anyhow!(
                "invalid state: selected system exists while systems list is empty"
            ));
        }

        Ok(())
    }
}
