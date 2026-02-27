mod actions;
mod ui;

use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Result};
use eframe::egui::{Color32, Pos2, Rect, Vec2};

use crate::db::Repository;
use crate::models::{SystemLink, SystemNote, SystemRecord, TechItem, ZoneRecord};

pub(crate) const MAP_NODE_SIZE: Vec2 = Vec2::new(170.0, 64.0);
pub(crate) const MAP_WORLD_SIZE: Vec2 = Vec2::new(12000.0, 12000.0);
pub(crate) const MAP_WORLD_MIN_SIZE: Vec2 = Vec2::new(4000.0, 4000.0);
pub(crate) const MAP_WORLD_MAX_SIZE: Vec2 = Vec2::new(50000.0, 50000.0);
pub(crate) const MAP_GRID_SPACING: f32 = 48.0;
pub(crate) const MAP_MIN_ZOOM: f32 = 0.05;
pub(crate) const MAP_MAX_ZOOM: f32 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineTerminator {
    None,
    Arrow,
    FilledArrow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinePattern {
    Solid,
    Dashed,
    Mitered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineLayerDepth {
    BehindCards,
    AboveCards,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineLayerOrder {
    ParentThenInteraction,
    InteractionThenParent,
}

#[derive(Debug, Clone, Copy)]
pub struct LineStyle {
    pub width: f32,
    pub color: Color32,
    pub terminator: LineTerminator,
    pub pattern: LinePattern,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarTab {
    Systems,
    TechCatalog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildSpawnMode {
    RightOfPrevious,
    BelowPrevious,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionKind {
    Standard,
    Pull,
    Push,
    Bidirectional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneDragKind {
    Move,
    ResizeBottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppModal {
    AddSystem,
    BulkAddSystems,
    AddTech,
    Hotkeys,
    LineStyle,
    SaveCatalog,
    LoadCatalog,
    NewCatalogConfirm,
}

#[derive(Debug, Clone)]
pub struct VisibleInteraction {
    pub source_system_id: i64,
    pub target_system_id: i64,
    pub note: String,
    pub kind: InteractionKind,
}

#[derive(Debug, Clone)]
pub struct InteractionPopupState {
    pub source_system_name: String,
    pub target_system_name: String,
    pub note: String,
    pub anchor_screen: Pos2,
}

#[derive(Debug, Clone)]
pub struct CopiedSystemEntry {
    pub name: String,
    pub description: String,
    pub parent_index: Option<usize>,
    pub relative_x: f32,
    pub relative_y: f32,
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
    selected_notes: Vec<SystemNote>,
    selected_note_id_for_edit: Option<i64>,
    selected_system_line_color_override: Option<Color32>,
    note_text: String,

    new_system_name: String,
    new_system_description: String,
    new_system_parent_id: Option<i64>,
    copied_system_entries: Vec<CopiedSystemEntry>,
    bulk_new_system_names: String,
    bulk_new_system_parent_id: Option<i64>,
    new_child_spawn_mode: ChildSpawnMode,
    new_system_tech_id_for_assignment: Option<i64>,
    new_system_assigned_tech_ids: HashSet<i64>,
    selected_system_parent_id: Option<i64>,
    edited_system_name: String,
    edited_system_description: String,
    selected_system_naming_root: bool,
    selected_system_naming_delimiter: String,

    new_link_target_id: Option<i64>,
    new_link_label: String,
    selected_link_id_for_edit: Option<i64>,
    edited_link_label: String,
    edited_link_note: String,
    edited_link_kind: InteractionKind,

    new_tech_name: String,
    new_tech_description: String,
    new_tech_documentation_link: String,
    selected_tech_id_for_assignment: Option<i64>,
    selected_catalog_tech_id_for_edit: Option<i64>,
    systems_using_selected_catalog_tech: HashSet<i64>,
    edited_tech_name: String,
    edited_tech_description: String,
    edited_tech_documentation_link: String,
    edited_tech_color: Option<Color32>,
    edited_tech_display_priority: i64,
    new_tech_color: Option<Color32>,
    new_tech_display_priority: i64,
    system_tech_ids_by_system: HashMap<i64, Vec<i64>>,
    fast_add_selected_catalog_tech_on_map: bool,

    map_positions: HashMap<i64, Pos2>,
    zones: Vec<ZoneRecord>,
    selected_zone_id: Option<i64>,
    selected_zone_name: String,
    selected_zone_color: Color32,
    selected_zone_render_priority: i64,
    zone_draw_mode: bool,
    zone_draw_start_screen: Option<Pos2>,
    zone_draw_end_screen: Option<Pos2>,
    zone_drag_kind: Option<ZoneDragKind>,
    zone_drag_start_local: Option<Pos2>,
    zone_drag_initial_x: f32,
    zone_drag_initial_y: f32,
    zone_drag_initial_width: f32,
    zone_drag_initial_height: f32,
    map_link_drag_from: Option<i64>,
    map_interaction_drag_from: Option<i64>,
    map_interaction_drag_kind: InteractionKind,
    map_link_click_source: Option<i64>,
    selected_map_system_ids: HashSet<i64>,
    map_selection_start_screen: Option<Pos2>,
    map_selection_end_screen: Option<Pos2>,
    map_drag_started_on_node: bool,
    map_undo_stack: Vec<HashMap<i64, Pos2>>,
    snap_to_grid: bool,
    map_world_size: Vec2,
    map_zoom: f32,
    map_pan: Vec2,
    map_last_view_center_local: Option<Pos2>,
    interaction_popup_pending: Option<InteractionPopupState>,
    interaction_popup_pending_open_at_secs: Option<f64>,
    interaction_popup_active: Option<InteractionPopupState>,
    interaction_popup_close_at_secs: Option<f64>,
    flow_inspector_from_system_id: Option<i64>,
    flow_inspector_to_system_id: Option<i64>,
    collapsed_system_ids: HashSet<i64>,

    show_add_system_modal: bool,
    show_bulk_add_systems_modal: bool,
    focus_bulk_add_system_names_on_open: bool,
    focus_add_system_name_on_open: bool,
    focus_add_tech_name_on_open: bool,
    show_add_tech_modal: bool,
    show_hotkeys_modal: bool,
    show_line_style_modal: bool,
    show_save_catalog_modal: bool,
    show_load_catalog_modal: bool,
    show_new_catalog_confirm_modal: bool,
    modal_open_stack: Vec<AppModal>,
    save_catalog_path: String,
    load_catalog_path: String,
    recent_catalog_paths: Vec<String>,
    active_sidebar_tab: SidebarTab,

    parent_line_style: LineStyle,
    interaction_line_style: LineStyle,
    interaction_standard_line_style: LineStyle,
    interaction_pull_line_style: LineStyle,
    interaction_push_line_style: LineStyle,
    interaction_bidirectional_line_style: LineStyle,
    show_parent_lines: bool,
    show_interaction_lines: bool,
    line_layer_depth: LineLayerDepth,
    line_layer_order: LineLayerOrder,
    dimmed_line_opacity_percent: f32,
    selected_line_brightness_percent: f32,
    show_tech_border_colors: bool,
    tech_border_max_colors: usize,
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
            selected_notes: Vec::new(),
            selected_note_id_for_edit: None,
            selected_system_line_color_override: None,
            note_text: String::new(),
            new_system_name: String::new(),
            new_system_description: String::new(),
            new_system_parent_id: None,
            copied_system_entries: Vec::new(),
            bulk_new_system_names: String::new(),
            bulk_new_system_parent_id: None,
            new_child_spawn_mode: ChildSpawnMode::RightOfPrevious,
            new_system_tech_id_for_assignment: None,
            new_system_assigned_tech_ids: HashSet::new(),
            selected_system_parent_id: None,
            edited_system_name: String::new(),
            edited_system_description: String::new(),
            selected_system_naming_root: false,
            selected_system_naming_delimiter: "/".to_owned(),
            new_link_target_id: None,
            new_link_label: String::new(),
            selected_link_id_for_edit: None,
            edited_link_label: String::new(),
            edited_link_note: String::new(),
            edited_link_kind: InteractionKind::Standard,
            new_tech_name: String::new(),
            new_tech_description: String::new(),
            new_tech_documentation_link: String::new(),
            selected_tech_id_for_assignment: None,
            selected_catalog_tech_id_for_edit: None,
            systems_using_selected_catalog_tech: HashSet::new(),
            edited_tech_name: String::new(),
            edited_tech_description: String::new(),
            edited_tech_documentation_link: String::new(),
            edited_tech_color: None,
            edited_tech_display_priority: 0,
            new_tech_color: None,
            new_tech_display_priority: 0,
            system_tech_ids_by_system: HashMap::new(),
            fast_add_selected_catalog_tech_on_map: false,
            map_positions: HashMap::new(),
            zones: Vec::new(),
            selected_zone_id: None,
            selected_zone_name: String::new(),
            selected_zone_color: Color32::from_rgba_unmultiplied(96, 140, 255, 48),
            selected_zone_render_priority: 1,
            zone_draw_mode: false,
            zone_draw_start_screen: None,
            zone_draw_end_screen: None,
            zone_drag_kind: None,
            zone_drag_start_local: None,
            zone_drag_initial_x: 0.0,
            zone_drag_initial_y: 0.0,
            zone_drag_initial_width: 0.0,
            zone_drag_initial_height: 0.0,
            map_link_drag_from: None,
            map_interaction_drag_from: None,
            map_interaction_drag_kind: InteractionKind::Standard,
            map_link_click_source: None,
            selected_map_system_ids: HashSet::new(),
            map_selection_start_screen: None,
            map_selection_end_screen: None,
            map_drag_started_on_node: false,
            map_undo_stack: Vec::new(),
            snap_to_grid: false,
            map_world_size: MAP_WORLD_SIZE,
            map_zoom: 1.0,
            map_pan: Vec2::ZERO,
            map_last_view_center_local: None,
            interaction_popup_pending: None,
            interaction_popup_pending_open_at_secs: None,
            interaction_popup_active: None,
            interaction_popup_close_at_secs: None,
            flow_inspector_from_system_id: None,
            flow_inspector_to_system_id: None,
            collapsed_system_ids: HashSet::new(),
            show_add_system_modal: false,
            show_bulk_add_systems_modal: false,
            focus_bulk_add_system_names_on_open: false,
            focus_add_system_name_on_open: false,
            focus_add_tech_name_on_open: false,
            show_add_tech_modal: false,
            show_hotkeys_modal: false,
            show_line_style_modal: false,
            show_save_catalog_modal: false,
            show_load_catalog_modal: false,
            show_new_catalog_confirm_modal: false,
            modal_open_stack: Vec::new(),
            save_catalog_path: "systems_catalog_export.db".to_owned(),
            load_catalog_path: "systems_catalog_export.db".to_owned(),
            recent_catalog_paths: Vec::new(),
            active_sidebar_tab: SidebarTab::Systems,
            parent_line_style: LineStyle {
                width: 1.0,
                color: Color32::from_gray(90),
                terminator: LineTerminator::Arrow,
                pattern: LinePattern::Solid,
            },
            interaction_line_style: LineStyle {
                width: 1.5,
                color: Color32::from_gray(140),
                terminator: LineTerminator::FilledArrow,
                pattern: LinePattern::Solid,
            },
            interaction_standard_line_style: LineStyle {
                width: 1.5,
                color: Color32::from_gray(140),
                terminator: LineTerminator::Arrow,
                pattern: LinePattern::Solid,
            },
            interaction_pull_line_style: LineStyle {
                width: 1.5,
                color: Color32::from_gray(140),
                terminator: LineTerminator::Arrow,
                pattern: LinePattern::Solid,
            },
            interaction_push_line_style: LineStyle {
                width: 1.5,
                color: Color32::from_gray(140),
                terminator: LineTerminator::FilledArrow,
                pattern: LinePattern::Solid,
            },
            interaction_bidirectional_line_style: LineStyle {
                width: 1.5,
                color: Color32::from_gray(140),
                terminator: LineTerminator::Arrow,
                pattern: LinePattern::Solid,
            },
            show_parent_lines: true,
            show_interaction_lines: true,
            line_layer_depth: LineLayerDepth::BehindCards,
            line_layer_order: LineLayerOrder::ParentThenInteraction,
            dimmed_line_opacity_percent: 18.0,
            selected_line_brightness_percent: 135.0,
            show_tech_border_colors: false,
            tech_border_max_colors: 2,
            settings_dirty: false,
            status_message: "Ready".to_owned(),
        };

        app.remove_legacy_window_settings()?;
        app.refresh_systems()?;
        app.load_ui_settings()?;
        Ok(app)
    }

    fn is_modal_open(&self, modal: AppModal) -> bool {
        match modal {
            AppModal::AddSystem => self.show_add_system_modal,
            AppModal::BulkAddSystems => self.show_bulk_add_systems_modal,
            AppModal::AddTech => self.show_add_tech_modal,
            AppModal::Hotkeys => self.show_hotkeys_modal,
            AppModal::LineStyle => self.show_line_style_modal,
            AppModal::SaveCatalog => self.show_save_catalog_modal,
            AppModal::LoadCatalog => self.show_load_catalog_modal,
            AppModal::NewCatalogConfirm => self.show_new_catalog_confirm_modal,
        }
    }

    fn set_modal_open(&mut self, modal: AppModal, is_open: bool) {
        match modal {
            AppModal::AddSystem => self.show_add_system_modal = is_open,
            AppModal::BulkAddSystems => self.show_bulk_add_systems_modal = is_open,
            AppModal::AddTech => self.show_add_tech_modal = is_open,
            AppModal::Hotkeys => self.show_hotkeys_modal = is_open,
            AppModal::LineStyle => self.show_line_style_modal = is_open,
            AppModal::SaveCatalog => self.show_save_catalog_modal = is_open,
            AppModal::LoadCatalog => self.show_load_catalog_modal = is_open,
            AppModal::NewCatalogConfirm => self.show_new_catalog_confirm_modal = is_open,
        }
    }

    fn open_modal(&mut self, modal: AppModal) {
        self.set_modal_open(modal, true);
        self.modal_open_stack.retain(|active| *active != modal);
        self.modal_open_stack.push(modal);
    }

    fn close_most_recent_open_modal(&mut self) -> bool {
        while let Some(modal) = self.modal_open_stack.pop() {
            if self.is_modal_open(modal) {
                self.set_modal_open(modal, false);
                return true;
            }
        }

        false
    }

    fn prune_closed_modals_from_stack(&mut self) {
        let mut still_open = Vec::new();
        for modal in &self.modal_open_stack {
            if self.is_modal_open(*modal) {
                still_open.push(*modal);
            }
        }
        self.modal_open_stack = still_open;
    }

    fn remove_legacy_window_settings(&mut self) -> Result<()> {
        self.repo.delete_settings(&[
            "window_width",
            "window_height",
            "window_x",
            "window_y",
        ])
    }

    fn refresh_systems(&mut self) -> Result<()> {
        self.systems = self.repo.list_systems()?;
        self.all_links = self.repo.list_links()?;
        self.tech_catalog = self.repo.list_tech_catalog()?;
        self.zones = self.repo.list_zones()?;
        let assignments = self.repo.list_system_tech_assignments()?;
        self.system_tech_ids_by_system.clear();
        for (system_id, tech_id) in assignments {
            self.system_tech_ids_by_system
                .entry(system_id)
                .or_default()
                .push(tech_id);
        }
        self.new_system_assigned_tech_ids
            .retain(|tech_id| self.tech_catalog.iter().any(|tech| tech.id == *tech_id));
        if let Some(tech_id) = self.new_system_tech_id_for_assignment {
            let exists = self.tech_catalog.iter().any(|tech| tech.id == tech_id);
            if !exists {
                self.new_system_tech_id_for_assignment = None;
            }
        }
        self.refresh_selected_tech_highlight();

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

        if let Some(selected_zone_id) = self.selected_zone_id {
            if self.zones.iter().any(|zone| zone.id == selected_zone_id) {
                self.select_zone(selected_zone_id);
            } else {
                self.selected_zone_id = None;
                self.selected_zone_name.clear();
                self.selected_zone_render_priority = 1;
                self.zone_drag_kind = None;
                self.zone_drag_start_local = None;
            }
        }

        Ok(())
    }

    fn select_zone(&mut self, zone_id: i64) {
        self.selected_zone_id = Some(zone_id);

        if let Some(zone) = self.zones.iter().find(|zone| zone.id == zone_id) {
            self.selected_zone_name = zone.name.clone();
            self.selected_zone_color = zone
                .color
                .as_deref()
                .and_then(Self::color_from_setting_value)
                .unwrap_or(Color32::from_rgba_unmultiplied(96, 140, 255, 48));
            self.selected_zone_render_priority = zone.render_priority;
        }
    }

    fn load_selected_data(&mut self, system_id: i64) -> Result<()> {
        self.selected_links = self.repo.list_links_for_system(system_id)?;
        self.selected_system_tech = self.repo.list_tech_for_system(system_id)?;
        self.selected_notes = self.repo.list_notes_for_system(system_id)?;

        if let Some(selected_link_id) = self.selected_link_id_for_edit {
            let still_exists = self
                .selected_links
                .iter()
                .any(|link| link.id == selected_link_id);
            if !still_exists {
                self.selected_link_id_for_edit = None;
                self.edited_link_label.clear();
                self.edited_link_note.clear();
                self.edited_link_kind = InteractionKind::Standard;
            }
        }

        if self.selected_link_id_for_edit.is_none() {
            if let Some(first_link) = self.selected_links.first() {
                self.selected_link_id_for_edit = Some(first_link.id);
                self.edited_link_label = first_link.label.clone();
                self.edited_link_note = first_link.note.clone();
                self.edited_link_kind =
                    Self::interaction_kind_from_setting_value(first_link.kind.as_str());
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

        if let Some(selected_catalog_tech_id) = self.selected_catalog_tech_id_for_edit {
            if let Some(selected_tech) = self
                .tech_catalog
                .iter()
                .find(|tech| tech.id == selected_catalog_tech_id)
            {
                self.edited_tech_name = selected_tech.name.clone();
                self.edited_tech_description =
                    selected_tech.description.clone().unwrap_or_default();
                self.edited_tech_documentation_link =
                    selected_tech.documentation_link.clone().unwrap_or_default();
                self.edited_tech_color = selected_tech
                    .color
                    .as_deref()
                    .and_then(Self::color_from_setting_value);
                self.edited_tech_display_priority = selected_tech.display_priority;
            }
        }

        self.selected_system_parent_id = self
            .systems
            .iter()
            .find(|system| system.id == system_id)
            .and_then(|system| system.parent_id);

        if let Some(system) = self.systems.iter().find(|system| system.id == system_id) {
            self.edited_system_name = system.name.clone();
            self.edited_system_description = system.description.clone();
            self.selected_system_naming_root = system.naming_root;
            self.selected_system_naming_delimiter = if system.naming_delimiter.trim().is_empty() {
                "/".to_owned()
            } else {
                system.naming_delimiter.clone()
            };
        }

        if let Some(selected_note_id) = self.selected_note_id_for_edit {
            if !self
                .selected_notes
                .iter()
                .any(|note| note.id == selected_note_id)
            {
                self.selected_note_id_for_edit = None;
            }
        }

        if self.selected_note_id_for_edit.is_none() {
            self.selected_note_id_for_edit = self.selected_notes.first().map(|note| note.id);
        }

        self.note_text = self
            .selected_note_id_for_edit
            .and_then(|note_id| {
                self.selected_notes
                    .iter()
                    .find(|note| note.id == note_id)
                    .map(|note| note.body.clone())
            })
            .unwrap_or_default();

        self.selected_cumulative_child_tech = self.cumulative_child_tech_names(system_id);
        if self.flow_inspector_from_system_id.is_none() {
            self.flow_inspector_from_system_id = Some(system_id);
        }
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
        self.refresh_selected_tech_highlight();
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

    fn naming_path_for_system(&self, system_id: i64) -> String {
        let by_id = self
            .systems
            .iter()
            .map(|system| (system.id, system))
            .collect::<HashMap<_, _>>();

        let mut segments = Vec::new();
        let mut delimiter = "/".to_owned();
        let mut visited = HashSet::new();
        let mut current = Some(system_id);

        while let Some(current_id) = current {
            if !visited.insert(current_id) {
                break;
            }

            let Some(system) = by_id.get(&current_id) else {
                break;
            };

            let segment = system.name.trim();
            if segment.is_empty() {
                segments.push(format!("system-{current_id}"));
            } else {
                segments.push(segment.to_owned());
            }

            if system.naming_root {
                let candidate_delimiter = system.naming_delimiter.trim();
                if !candidate_delimiter.is_empty() {
                    delimiter = candidate_delimiter.to_owned();
                }
                break;
            }

            current = system.parent_id;
        }

        segments.reverse();
        segments.join(delimiter.as_str())
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

    fn representative_visible_system_id(
        &self,
        system_id: i64,
        visible_ids: &HashSet<i64>,
        parent_by_id: &HashMap<i64, Option<i64>>,
    ) -> Option<i64> {
        if visible_ids.contains(&system_id) {
            return Some(system_id);
        }

        let mut current = parent_by_id.get(&system_id).copied().flatten();
        let mut visited = HashSet::new();

        while let Some(parent_id) = current {
            if !visited.insert(parent_id) {
                break;
            }

            if visible_ids.contains(&parent_id) && self.collapsed_system_ids.contains(&parent_id) {
                return Some(parent_id);
            }

            current = parent_by_id.get(&parent_id).copied().flatten();
        }

        None
    }

    fn visible_representative_system_map(&self) -> HashMap<i64, i64> {
        let visible_ids = self.visible_system_ids();
        let parent_by_id = self
            .systems
            .iter()
            .map(|system| (system.id, system.parent_id))
            .collect::<HashMap<_, _>>();

        let mut representative_by_system = HashMap::new();

        for system in &self.systems {
            if let Some(representative_id) =
                self.representative_visible_system_id(system.id, &visible_ids, &parent_by_id)
            {
                representative_by_system.insert(system.id, representative_id);
            }
        }

        representative_by_system
    }

    fn deduped_visible_interactions(&self) -> Vec<VisibleInteraction> {
        let representative_by_system = self.visible_representative_system_map();
        let mut by_edge: HashMap<(i64, i64), (String, InteractionKind)> = HashMap::new();

        for link in &self.all_links {
            let Some(source_id) = representative_by_system.get(&link.source_system_id).copied()
            else {
                continue;
            };

            let Some(target_id) = representative_by_system.get(&link.target_system_id).copied()
            else {
                continue;
            };

            if source_id == target_id {
                continue;
            }

            by_edge
                .entry((source_id, target_id))
                .and_modify(|(note, _kind)| {
                    if note.trim().is_empty() && !link.note.trim().is_empty() {
                        *note = link.note.clone();
                    }
                })
                .or_insert_with(|| {
                    (
                        link.note.clone(),
                        Self::interaction_kind_from_setting_value(link.kind.as_str()),
                    )
                });
        }

        let mut interactions = by_edge
            .into_iter()
            .map(|((source_system_id, target_system_id), (note, kind))| VisibleInteraction {
                source_system_id,
                target_system_id,
                note,
                kind,
            })
            .collect::<Vec<_>>();

        interactions.sort_by_key(|interaction| {
            (interaction.source_system_id, interaction.target_system_id)
        });
        interactions
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
        self.selected_map_system_ids.clear();
        self.selected_links.clear();
        self.selected_system_tech.clear();
        self.selected_cumulative_child_tech.clear();
        self.selected_notes.clear();
        self.selected_note_id_for_edit = None;
        self.selected_system_line_color_override = None;
        self.note_text.clear();
        self.selected_system_parent_id = None;
        self.edited_system_name.clear();
        self.edited_system_description.clear();
        self.selected_system_naming_root = false;
        self.selected_system_naming_delimiter = "/".to_owned();
        self.selected_link_id_for_edit = None;
        self.edited_link_label.clear();
        self.edited_link_note.clear();
        self.edited_link_kind = InteractionKind::Standard;
        self.map_link_drag_from = None;
        self.map_interaction_drag_from = None;
        self.map_interaction_drag_kind = InteractionKind::Standard;
        self.map_link_click_source = None;
        self.interaction_popup_pending = None;
        self.interaction_popup_pending_open_at_secs = None;
        self.interaction_popup_active = None;
        self.interaction_popup_close_at_secs = None;
    }

    fn terminator_to_setting_value(terminator: LineTerminator) -> &'static str {
        match terminator {
            LineTerminator::None => "none",
            LineTerminator::Arrow => "arrow",
            LineTerminator::FilledArrow => "filled_arrow",
        }
    }

    fn pattern_to_setting_value(pattern: LinePattern) -> &'static str {
        match pattern {
            LinePattern::Solid => "solid",
            LinePattern::Dashed => "dashed",
            LinePattern::Mitered => "mitered",
        }
    }

    fn line_layer_depth_to_setting_value(depth: LineLayerDepth) -> &'static str {
        match depth {
            LineLayerDepth::BehindCards => "behind_cards",
            LineLayerDepth::AboveCards => "above_cards",
        }
    }

    fn line_layer_order_to_setting_value(order: LineLayerOrder) -> &'static str {
        match order {
            LineLayerOrder::ParentThenInteraction => "parent_then_interaction",
            LineLayerOrder::InteractionThenParent => "interaction_then_parent",
        }
    }

    fn child_spawn_mode_to_setting_value(mode: ChildSpawnMode) -> &'static str {
        match mode {
            ChildSpawnMode::RightOfPrevious => "right_of_previous",
            ChildSpawnMode::BelowPrevious => "below_previous",
        }
    }

    fn child_spawn_mode_from_setting_value(value: &str) -> Option<ChildSpawnMode> {
        match value {
            "right_of_previous" => Some(ChildSpawnMode::RightOfPrevious),
            "below_previous" => Some(ChildSpawnMode::BelowPrevious),
            _ => None,
        }
    }

    fn interaction_kind_to_setting_value(kind: InteractionKind) -> &'static str {
        match kind {
            InteractionKind::Standard => "standard",
            InteractionKind::Pull => "pull",
            InteractionKind::Push => "push",
            InteractionKind::Bidirectional => "bidirectional",
        }
    }

    fn interaction_kind_from_setting_value(value: &str) -> InteractionKind {
        match value {
            "pull" => InteractionKind::Pull,
            "push" => InteractionKind::Push,
            "bidirectional" => InteractionKind::Bidirectional,
            _ => InteractionKind::Standard,
        }
    }

    fn interaction_kind_label(kind: InteractionKind) -> &'static str {
        match kind {
            InteractionKind::Standard => "Standard",
            InteractionKind::Pull => "Pull",
            InteractionKind::Push => "Push",
            InteractionKind::Bidirectional => "Bidirectional",
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

    fn pattern_from_setting_value(value: &str) -> Option<LinePattern> {
        match value {
            "solid" => Some(LinePattern::Solid),
            "dashed" => Some(LinePattern::Dashed),
            "mitered" => Some(LinePattern::Mitered),
            _ => None,
        }
    }

    fn line_layer_depth_from_setting_value(value: &str) -> Option<LineLayerDepth> {
        match value {
            "behind_cards" => Some(LineLayerDepth::BehindCards),
            "above_cards" => Some(LineLayerDepth::AboveCards),
            _ => None,
        }
    }

    fn line_layer_order_from_setting_value(value: &str) -> Option<LineLayerOrder> {
        match value {
            "parent_then_interaction" => Some(LineLayerOrder::ParentThenInteraction),
            "interaction_then_parent" => Some(LineLayerOrder::InteractionThenParent),
            _ => None,
        }
    }

    fn color_to_setting_value(color: Color32) -> String {
        let [r, g, b, a] = color.to_srgba_unmultiplied();
        format!("{r},{g},{b},{a}")
    }

    fn color_from_setting_value(value: &str) -> Option<Color32> {
        let trimmed = value.trim();

        if let Some(hex) = trimmed.strip_prefix('#') {
            if hex.len() == 6 {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                return Some(Color32::from_rgba_unmultiplied(r, g, b, 255));
            }

            if hex.len() == 8 {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                return Some(Color32::from_rgba_unmultiplied(r, g, b, a));
            }
        }

        let parts = trimmed
            .split(',')
            .map(|part| part.trim().parse::<u8>().ok())
            .collect::<Vec<_>>();

        if parts.len() == 4 {
            return Some(Color32::from_rgba_unmultiplied(
                parts[0]?, parts[1]?, parts[2]?, parts[3]?,
            ));
        }

        if parts.len() == 3 {
            return Some(Color32::from_rgba_unmultiplied(
                parts[0]?, parts[1]?, parts[2]?, 255,
            ));
        }

        None
    }

    fn load_ui_settings(&mut self) -> Result<()> {
        if let Some(value) = self.repo.get_setting("map_zoom")? {
            if let Ok(parsed) = value.parse::<f32>() {
                self.map_zoom = parsed.clamp(MAP_MIN_ZOOM, MAP_MAX_ZOOM);
            }
        }

        if let Some(value) = self.repo.get_setting("map_world_width")? {
            if let Ok(parsed) = value.parse::<f32>() {
                self.map_world_size.x = parsed.clamp(MAP_WORLD_MIN_SIZE.x, MAP_WORLD_MAX_SIZE.x);
            }
        }

        if let Some(value) = self.repo.get_setting("map_world_height")? {
            if let Ok(parsed) = value.parse::<f32>() {
                self.map_world_size.y = parsed.clamp(MAP_WORLD_MIN_SIZE.y, MAP_WORLD_MAX_SIZE.y);
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

        if let Some(value) = self.repo.get_setting("parent_line_pattern")? {
            if let Some(parsed) = Self::pattern_from_setting_value(&value) {
                self.parent_line_style.pattern = parsed;
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

        if let Some(value) = self.repo.get_setting("interaction_line_pattern")? {
            if let Some(parsed) = Self::pattern_from_setting_value(&value) {
                self.interaction_line_style.pattern = parsed;
            }
        }

        self.interaction_standard_line_style = self.interaction_line_style;
        self.interaction_pull_line_style = self.interaction_line_style;
        self.interaction_push_line_style = self.interaction_line_style;
        self.interaction_bidirectional_line_style = self.interaction_line_style;

        if let Some(value) = self.repo.get_setting("interaction_standard_line_color")? {
            if let Some(parsed) = Self::color_from_setting_value(&value) {
                self.interaction_standard_line_style.color = parsed;
            }
        }
        if let Some(value) = self.repo.get_setting("interaction_standard_line_terminator")? {
            if let Some(parsed) = Self::terminator_from_setting_value(&value) {
                self.interaction_standard_line_style.terminator = parsed;
            }
        }
        if let Some(value) = self.repo.get_setting("interaction_standard_line_pattern")? {
            if let Some(parsed) = Self::pattern_from_setting_value(&value) {
                self.interaction_standard_line_style.pattern = parsed;
            }
        }

        if let Some(value) = self.repo.get_setting("interaction_pull_line_color")? {
            if let Some(parsed) = Self::color_from_setting_value(&value) {
                self.interaction_pull_line_style.color = parsed;
            }
        }
        if let Some(value) = self.repo.get_setting("interaction_pull_line_terminator")? {
            if let Some(parsed) = Self::terminator_from_setting_value(&value) {
                self.interaction_pull_line_style.terminator = parsed;
            }
        }
        if let Some(value) = self.repo.get_setting("interaction_pull_line_pattern")? {
            if let Some(parsed) = Self::pattern_from_setting_value(&value) {
                self.interaction_pull_line_style.pattern = parsed;
            }
        }

        if let Some(value) = self.repo.get_setting("interaction_push_line_color")? {
            if let Some(parsed) = Self::color_from_setting_value(&value) {
                self.interaction_push_line_style.color = parsed;
            }
        }
        if let Some(value) = self.repo.get_setting("interaction_push_line_terminator")? {
            if let Some(parsed) = Self::terminator_from_setting_value(&value) {
                self.interaction_push_line_style.terminator = parsed;
            }
        }
        if let Some(value) = self.repo.get_setting("interaction_push_line_pattern")? {
            if let Some(parsed) = Self::pattern_from_setting_value(&value) {
                self.interaction_push_line_style.pattern = parsed;
            }
        }

        if let Some(value) = self.repo.get_setting("interaction_bidirectional_line_color")? {
            if let Some(parsed) = Self::color_from_setting_value(&value) {
                self.interaction_bidirectional_line_style.color = parsed;
            }
        }
        if let Some(value) = self
            .repo
            .get_setting("interaction_bidirectional_line_terminator")?
        {
            if let Some(parsed) = Self::terminator_from_setting_value(&value) {
                self.interaction_bidirectional_line_style.terminator = parsed;
            }
        }
        if let Some(value) = self.repo.get_setting("interaction_bidirectional_line_pattern")? {
            if let Some(parsed) = Self::pattern_from_setting_value(&value) {
                self.interaction_bidirectional_line_style.pattern = parsed;
            }
        }

        if let Some(value) = self.repo.get_setting("show_parent_lines")? {
            self.show_parent_lines = value == "true";
        }

        if let Some(value) = self.repo.get_setting("show_interaction_lines")? {
            self.show_interaction_lines = value == "true";
        }

        if let Some(value) = self.repo.get_setting("line_layer_depth")? {
            if let Some(parsed) = Self::line_layer_depth_from_setting_value(&value) {
                self.line_layer_depth = parsed;
            }
        }

        if let Some(value) = self.repo.get_setting("line_layer_order")? {
            if let Some(parsed) = Self::line_layer_order_from_setting_value(&value) {
                self.line_layer_order = parsed;
            }
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

        if let Some(value) = self.repo.get_setting("snap_to_grid")? {
            self.snap_to_grid = value == "true";
        }

        if let Some(value) = self.repo.get_setting("show_tech_border_colors")? {
            self.show_tech_border_colors = value == "true";
        }

        if let Some(value) = self.repo.get_setting("tech_border_max_colors")? {
            if let Ok(parsed) = value.parse::<usize>() {
                self.tech_border_max_colors = parsed.clamp(1, 5);
            }
        }

        if let Some(value) = self.repo.get_setting("recent_catalog_paths")? {
            self.recent_catalog_paths = value
                .split('\n')
                .map(|path| path.trim())
                .filter(|path| !path.is_empty())
                .map(|path| path.to_owned())
                .collect();
        }

        if let Some(value) = self.repo.get_setting("fast_add_selected_catalog_tech_on_map")? {
            self.fast_add_selected_catalog_tech_on_map = value == "true";
        }

        if let Some(value) = self.repo.get_setting("new_child_spawn_mode")? {
            if let Some(parsed) = Self::child_spawn_mode_from_setting_value(&value) {
                self.new_child_spawn_mode = parsed;
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
                .set_setting("map_world_width", &self.map_world_size.x.to_string())?;
            self.repo
                .set_setting("map_world_height", &self.map_world_size.y.to_string())?;
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
                "parent_line_pattern",
                Self::pattern_to_setting_value(self.parent_line_style.pattern),
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
                "interaction_line_pattern",
                Self::pattern_to_setting_value(self.interaction_line_style.pattern),
            )?;

            self.repo.set_setting(
                "interaction_standard_line_color",
                &Self::color_to_setting_value(self.interaction_standard_line_style.color),
            )?;
            self.repo.set_setting(
                "interaction_standard_line_terminator",
                Self::terminator_to_setting_value(self.interaction_standard_line_style.terminator),
            )?;
            self.repo.set_setting(
                "interaction_standard_line_pattern",
                Self::pattern_to_setting_value(self.interaction_standard_line_style.pattern),
            )?;

            self.repo.set_setting(
                "interaction_pull_line_color",
                &Self::color_to_setting_value(self.interaction_pull_line_style.color),
            )?;
            self.repo.set_setting(
                "interaction_pull_line_terminator",
                Self::terminator_to_setting_value(self.interaction_pull_line_style.terminator),
            )?;
            self.repo.set_setting(
                "interaction_pull_line_pattern",
                Self::pattern_to_setting_value(self.interaction_pull_line_style.pattern),
            )?;

            self.repo.set_setting(
                "interaction_push_line_color",
                &Self::color_to_setting_value(self.interaction_push_line_style.color),
            )?;
            self.repo.set_setting(
                "interaction_push_line_terminator",
                Self::terminator_to_setting_value(self.interaction_push_line_style.terminator),
            )?;
            self.repo.set_setting(
                "interaction_push_line_pattern",
                Self::pattern_to_setting_value(self.interaction_push_line_style.pattern),
            )?;

            self.repo.set_setting(
                "interaction_bidirectional_line_color",
                &Self::color_to_setting_value(self.interaction_bidirectional_line_style.color),
            )?;
            self.repo.set_setting(
                "interaction_bidirectional_line_terminator",
                Self::terminator_to_setting_value(
                    self.interaction_bidirectional_line_style.terminator,
                ),
            )?;
            self.repo.set_setting(
                "interaction_bidirectional_line_pattern",
                Self::pattern_to_setting_value(self.interaction_bidirectional_line_style.pattern),
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
                "line_layer_depth",
                Self::line_layer_depth_to_setting_value(self.line_layer_depth),
            )?;

            self.repo.set_setting(
                "line_layer_order",
                Self::line_layer_order_to_setting_value(self.line_layer_order),
            )?;

            self.repo.set_setting(
                "dimmed_line_opacity_percent",
                &self.dimmed_line_opacity_percent.to_string(),
            )?;

            self.repo.set_setting(
                "selected_line_brightness_percent",
                &self.selected_line_brightness_percent.to_string(),
            )?;

            self.repo.set_setting(
                "snap_to_grid",
                if self.snap_to_grid { "true" } else { "false" },
            )?;

            self.repo.set_setting(
                "show_tech_border_colors",
                if self.show_tech_border_colors {
                    "true"
                } else {
                    "false"
                },
            )?;

            self.repo.set_setting(
                "tech_border_max_colors",
                &self.tech_border_max_colors.to_string(),
            )?;

            if !self.recent_catalog_paths.is_empty() {
                self.repo.set_setting(
                    "recent_catalog_paths",
                    &self.recent_catalog_paths.join("\n"),
                )?;
            }

            self.repo.set_setting(
                "fast_add_selected_catalog_tech_on_map",
                if self.fast_add_selected_catalog_tech_on_map {
                    "true"
                } else {
                    "false"
                },
            )?;

            self.repo.set_setting(
                "new_child_spawn_mode",
                Self::child_spawn_mode_to_setting_value(self.new_child_spawn_mode),
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

    fn open_add_system_modal_with_prefill(&mut self, parent_id: Option<i64>) {
        self.new_system_parent_id = parent_id;
        self.new_system_tech_id_for_assignment = None;
        self.new_system_assigned_tech_ids.clear();

        if let Some(parent_system_id) = parent_id {
            if let Some(parent_tech_ids) = self.system_tech_ids_by_system.get(&parent_system_id) {
                for tech_id in parent_tech_ids {
                    if self.tech_catalog.iter().any(|tech| tech.id == *tech_id) {
                        self.new_system_assigned_tech_ids.insert(*tech_id);
                    }
                }
            }
        }

        self.open_modal(AppModal::AddSystem);
        self.focus_add_system_name_on_open = true;
    }

    fn open_bulk_add_systems_modal_with_prefill(&mut self, parent_id: Option<i64>) {
        self.bulk_new_system_parent_id = parent_id;
        self.open_modal(AppModal::BulkAddSystems);
        self.focus_bulk_add_system_names_on_open = true;
    }

    fn ensure_valid_bulk_parent_selection(&mut self) {
        if let Some(parent_id) = self.bulk_new_system_parent_id {
            let exists = self.systems.iter().any(|system| system.id == parent_id);
            if !exists {
                self.bulk_new_system_parent_id = None;
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

    fn ensure_valid_flow_inspector_selection(&mut self) {
        if let Some(system_id) = self.flow_inspector_from_system_id {
            let exists = self.systems.iter().any(|system| system.id == system_id);
            if !exists {
                self.flow_inspector_from_system_id = None;
            }
        }

        if let Some(system_id) = self.flow_inspector_to_system_id {
            let exists = self.systems.iter().any(|system| system.id == system_id);
            if !exists {
                self.flow_inspector_to_system_id = None;
            }
        }
    }

    fn flow_inspector_edges(&self) -> Vec<(i64, i64, InteractionKind)> {
        let mut edges = Vec::new();

        for link in &self.all_links {
            let kind = Self::interaction_kind_from_setting_value(link.kind.as_str());
            match kind {
                InteractionKind::Standard | InteractionKind::Push => {
                    edges.push((link.source_system_id, link.target_system_id, kind));
                }
                InteractionKind::Pull => {
                    edges.push((link.target_system_id, link.source_system_id, kind));
                }
                InteractionKind::Bidirectional => {
                    edges.push((
                        link.source_system_id,
                        link.target_system_id,
                        InteractionKind::Bidirectional,
                    ));
                    edges.push((
                        link.target_system_id,
                        link.source_system_id,
                        InteractionKind::Bidirectional,
                    ));
                }
            }
        }

        edges
    }

    fn focused_flow_shortest_path(
        &self,
        source_system_id: i64,
        target_system_id: i64,
    ) -> Option<Vec<(i64, InteractionKind, i64)>> {
        if source_system_id == target_system_id {
            return Some(Vec::new());
        }

        let edges = self.flow_inspector_edges();
        let mut adjacency: HashMap<i64, Vec<(i64, InteractionKind)>> = HashMap::new();
        for (from, to, kind) in edges {
            adjacency.entry(from).or_default().push((to, kind));
        }

        let mut queue = std::collections::VecDeque::new();
        let mut visited = HashSet::new();
        let mut previous: HashMap<i64, (i64, InteractionKind)> = HashMap::new();

        queue.push_back(source_system_id);
        visited.insert(source_system_id);

        while let Some(current) = queue.pop_front() {
            if current == target_system_id {
                break;
            }

            if let Some(neighbors) = adjacency.get(&current) {
                for (next, kind) in neighbors {
                    if visited.insert(*next) {
                        previous.insert(*next, (current, *kind));
                        queue.push_back(*next);
                    }
                }
            }
        }

        if !visited.contains(&target_system_id) {
            return None;
        }

        let mut result = Vec::new();
        let mut current = target_system_id;
        while current != source_system_id {
            let (prev, kind) = previous.get(&current).copied()?;
            result.push((prev, kind, current));
            current = prev;
        }

        result.reverse();
        Some(result)
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
                self.edited_link_note.clear();
                self.edited_link_kind = InteractionKind::Standard;
            }
        }
    }

    fn ensure_valid_selected_note(&mut self) {
        if let Some(note_id) = self.selected_note_id_for_edit {
            let exists = self.selected_notes.iter().any(|note| note.id == note_id);
            if !exists {
                self.selected_note_id_for_edit = None;
                self.note_text.clear();
            }
        }
    }

    fn ensure_valid_selected_catalog_tech(&mut self) {
        if let Some(tech_id) = self.selected_catalog_tech_id_for_edit {
            let exists = self.tech_catalog.iter().any(|tech| tech.id == tech_id);
            if !exists {
                self.selected_catalog_tech_id_for_edit = None;
                self.systems_using_selected_catalog_tech.clear();
                self.edited_tech_name.clear();
                self.edited_tech_description.clear();
                self.edited_tech_documentation_link.clear();
            }
        }
    }

    fn refresh_selected_tech_highlight(&mut self) {
        self.systems_using_selected_catalog_tech.clear();

        if let Some(tech_id) = self.selected_catalog_tech_id_for_edit {
            if let Ok(system_ids) = self.repo.list_system_ids_for_tech(tech_id) {
                self.systems_using_selected_catalog_tech = system_ids.into_iter().collect();
            }
        }
    }

    fn text_to_option(value: &str) -> Option<&str> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    fn push_recent_catalog_path(&mut self, path: &str) {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return;
        }

        self.recent_catalog_paths
            .retain(|existing| existing != trimmed);
        self.recent_catalog_paths.insert(0, trimmed.to_owned());
        self.recent_catalog_paths.truncate(8);
        self.settings_dirty = true;
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

    fn system_and_ancestor_ids(&self, system_id: i64) -> HashSet<i64> {
        let mut result = HashSet::new();
        let mut current = Some(system_id);

        while let Some(id) = current {
            if !result.insert(id) {
                break;
            }

            current = self
                .systems
                .iter()
                .find(|system| system.id == id)
                .and_then(|system| system.parent_id);
        }

        result
    }

    fn system_and_descendant_ids(&self, system_id: i64) -> HashSet<i64> {
        let mut children_by_parent: HashMap<Option<i64>, Vec<i64>> = HashMap::new();
        for system in &self.systems {
            children_by_parent
                .entry(system.parent_id)
                .or_default()
                .push(system.id);
        }

        let mut descendant_ids = Vec::new();
        self.collect_descendant_ids(system_id, &children_by_parent, &mut descendant_ids);

        let mut selected = descendant_ids.into_iter().collect::<HashSet<_>>();
        selected.insert(system_id);
        selected
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

    fn find_next_free_child_spawn_position(&self, parent_id: Option<i64>) -> Option<Pos2> {
        let parent_position = parent_id.and_then(|id| self.map_positions.get(&id).copied())?;

        let step_x = MAP_NODE_SIZE.x + 24.0;
        let step_y = MAP_NODE_SIZE.y + 20.0;

        let previous_child_position = parent_id.and_then(|id| {
            self.systems
                .iter()
                .filter(|system| system.parent_id == Some(id))
                .filter_map(|system| self.map_positions.get(&system.id).copied())
                .max_by(|left, right| {
                    if (left.y - right.y).abs() <= f32::EPSILON {
                        left.x
                            .partial_cmp(&right.x)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    } else {
                        left.y
                            .partial_cmp(&right.y)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    }
                })
        });

        let base = previous_child_position.unwrap_or(parent_position);
        let preferred = match self.new_child_spawn_mode {
            ChildSpawnMode::RightOfPrevious => Pos2::new(base.x + step_x, base.y),
            ChildSpawnMode::BelowPrevious => Pos2::new(base.x, base.y + step_y),
        };

        for primary_offset in 0..=220 {
            for secondary_offset in 0..=220 {
                let (dx, dy) = match self.new_child_spawn_mode {
                    ChildSpawnMode::RightOfPrevious => (
                        primary_offset as f32 * step_x,
                        secondary_offset as f32 * step_y,
                    ),
                    ChildSpawnMode::BelowPrevious => (
                        secondary_offset as f32 * step_x,
                        primary_offset as f32 * step_y,
                    ),
                };

                let candidate = Pos2::new(preferred.x + dx, preferred.y + dy);
                let snapped = self.snap_spawn_position_to_grid(candidate, MAP_NODE_SIZE);

                if !self.spawn_position_overlaps(snapped) {
                    return Some(snapped);
                }
            }
        }

        Some(self.snap_spawn_position_to_grid(preferred, MAP_NODE_SIZE))
    }

    fn find_next_free_root_spawn_position(&self) -> Pos2 {
        let fallback = Pos2::new(24.0, 24.0);
        let center = self.map_last_view_center_local.unwrap_or(fallback);
        let anchor = Pos2::new(
            center.x - (MAP_NODE_SIZE.x * 0.5),
            center.y - (MAP_NODE_SIZE.y * 0.5),
        );

        let step_x = MAP_NODE_SIZE.x + 24.0;
        let step_y = MAP_NODE_SIZE.y + 20.0;

        for radius in 0..=120_i32 {
            for row in -radius..=radius {
                for col in -radius..=radius {
                    if radius > 0
                        && row.abs() != radius
                        && col.abs() != radius
                    {
                        continue;
                    }

                    let candidate = Pos2::new(
                        anchor.x + (col as f32 * step_x),
                        anchor.y + (row as f32 * step_y),
                    );
                    let snapped = self.snap_spawn_position_to_grid(candidate, MAP_NODE_SIZE);

                    if !self.spawn_position_overlaps(snapped) {
                        return snapped;
                    }
                }
            }
        }

        self.snap_spawn_position_to_grid(anchor, MAP_NODE_SIZE)
    }

    fn snap_spawn_position_to_grid(&self, position: Pos2, node_size: Vec2) -> Pos2 {
        let snapped = Pos2::new(
            (position.x / MAP_GRID_SPACING).round() * MAP_GRID_SPACING,
            (position.y / MAP_GRID_SPACING).round() * MAP_GRID_SPACING,
        );
        self.clamp_node_position(Rect::NOTHING, snapped, node_size)
    }

    fn spawn_position_overlaps(&self, position: Pos2) -> bool {
        self.map_positions.values().any(|existing| {
            (existing.x - position.x).abs() < (MAP_NODE_SIZE.x * 0.75)
                && (existing.y - position.y).abs() < (MAP_NODE_SIZE.y * 0.75)
        })
    }

    fn clamp_node_position(&self, map_rect: Rect, position: Pos2, node_size: Vec2) -> Pos2 {
        let _ = map_rect;
        let max_x = self.map_world_size.x - node_size.x - 8.0;
        let max_y = self.map_world_size.y - node_size.y - 8.0;

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
        self.push_map_undo_snapshot();
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

    fn push_map_undo_snapshot(&mut self) {
        if self
            .map_undo_stack
            .last()
            .map(|snapshot| snapshot == &self.map_positions)
            .unwrap_or(false)
        {
            return;
        }

        self.map_undo_stack.push(self.map_positions.clone());
        if self.map_undo_stack.len() > 100 {
            self.map_undo_stack.remove(0);
        }
    }

    fn undo_map_positions(&mut self) {
        let Some(previous_positions) = self.map_undo_stack.pop() else {
            self.status_message = "Nothing to undo".to_owned();
            return;
        };

        self.map_positions = previous_positions;
        for (system_id, position) in self.map_positions.clone() {
            self.persist_map_position(system_id, position);
        }

        self.status_message = "Undid last map change".to_owned();
    }

    fn validate_before_render(&mut self) -> Result<()> {
        self.ensure_valid_parent_selection();
        self.ensure_valid_bulk_parent_selection();
        self.ensure_valid_selected_parent_selection();
        self.ensure_valid_flow_inspector_selection();
        self.ensure_valid_link_target_selection();
        self.ensure_valid_tech_selection();
        self.ensure_valid_selected_link();
        self.ensure_valid_selected_note();
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
