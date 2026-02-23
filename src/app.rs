use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Result};
use eframe::egui::{self, Align, Layout, RichText};

use crate::db::Repository;
use crate::models::{SystemLink, SystemRecord, TechItem};

/// Primary UI application state.
///
/// TypeScript analogy: this struct is similar to a React component's local state + service
/// dependencies, except Rust stores it in one explicit data structure.
pub struct SystemsCatalogApp {
    repo: Repository,
    systems: Vec<SystemRecord>,
    tech_catalog: Vec<TechItem>,
    selected_system_id: Option<i64>,
    selected_links: Vec<SystemLink>,
    selected_system_tech: Vec<TechItem>,
    selected_cumulative_child_tech: Vec<String>,
    note_text: String,

    new_system_name: String,
    new_system_description: String,
    new_system_parent_id: Option<i64>,

    new_link_target_id: Option<i64>,
    new_link_label: String,

    new_tech_name: String,
    selected_tech_id_for_assignment: Option<i64>,

    status_message: String,
}

impl SystemsCatalogApp {
    pub fn new(repo: Repository) -> Result<Self> {
        let mut app = Self {
            repo,
            systems: Vec::new(),
            tech_catalog: Vec::new(),
            selected_system_id: None,
            selected_links: Vec::new(),
            selected_system_tech: Vec::new(),
            selected_cumulative_child_tech: Vec::new(),
            note_text: String::new(),
            new_system_name: String::new(),
            new_system_description: String::new(),
            new_system_parent_id: None,
            new_link_target_id: None,
            new_link_label: String::new(),
            new_tech_name: String::new(),
            selected_tech_id_for_assignment: None,
            status_message: "Ready".to_owned(),
        };

        app.refresh_systems()?;
        Ok(app)
    }

    fn refresh_systems(&mut self) -> Result<()> {
        self.systems = self.repo.list_systems()?;
        self.tech_catalog = self.repo.list_tech_catalog()?;

        if let Some(selected) = self.selected_system_id {
            let still_exists = self.systems.iter().any(|system| system.id == selected);
            if !still_exists {
                self.selected_system_id = None;
                self.selected_links.clear();
                self.selected_system_tech.clear();
                self.selected_cumulative_child_tech.clear();
                self.note_text.clear();
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
        self.note_text = self
            .repo
            .get_note(system_id)?
            .map(|note| {
                // Touch both fields to make it clear notes carry identity + audit timestamp.
                let _id = note.system_id;
                let _updated = note.updated_at;
                note.body
            })
            .unwrap_or_default();

        self.selected_cumulative_child_tech = self.cumulative_child_tech_names(system_id);
        Ok(())
    }

    fn create_tech_item(&mut self) {
        let name = self.new_tech_name.trim();
        if name.is_empty() {
            self.status_message = "Technology name is required".to_owned();
            return;
        }

        let result = self
            .repo
            .create_tech_item(name)
            .and_then(|_| self.refresh_systems());

        match result {
            Ok(_) => {
                self.new_tech_name.clear();
                self.status_message = "Technology saved to catalog".to_owned();
            }
            Err(error) => {
                self.status_message = format!("Failed to save technology: {error}");
            }
        }
    }

    fn add_selected_tech_to_system(&mut self) {
        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let Some(tech_id) = self.selected_tech_id_for_assignment else {
            self.status_message = "Select a technology to assign".to_owned();
            return;
        };

        let result = self
            .repo
            .add_tech_to_system(system_id, tech_id)
            .and_then(|_| self.load_selected_data(system_id));

        match result {
            Ok(_) => {
                self.selected_tech_id_for_assignment = None;
                self.status_message = "Technology assigned to system".to_owned();
            }
            Err(error) => {
                self.status_message = format!("Failed to assign technology: {error}");
            }
        }
    }

    fn selected_system(&self) -> Option<&SystemRecord> {
        self.selected_system_id
            .and_then(|id| self.systems.iter().find(|system| system.id == id))
    }

    fn create_system(&mut self) {
        let name = self.new_system_name.trim();
        if name.is_empty() {
            self.status_message = "System name is required".to_owned();
            return;
        }

        let description = self.new_system_description.trim();

        let result = self
            .repo
            .create_system(name, description, self.new_system_parent_id)
            .and_then(|_| self.refresh_systems());

        match result {
            Ok(_) => {
                self.new_system_name.clear();
                self.new_system_description.clear();
                self.new_system_parent_id = None;
                self.status_message = "System created".to_owned();
            }
            Err(error) => {
                self.status_message = format!("Failed to create system: {error}");
            }
        }
    }

    fn save_note(&mut self) {
        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        match self.repo.upsert_note(system_id, self.note_text.trim()) {
            Ok(_) => self.status_message = "Notes saved".to_owned(),
            Err(error) => self.status_message = format!("Failed to save notes: {error}"),
        }
    }

    fn create_link(&mut self) {
        let Some(source_id) = self.selected_system_id else {
            self.status_message = "Select a system to create an interaction".to_owned();
            return;
        };

        let Some(target_id) = self.new_link_target_id else {
            self.status_message = "Select a target system".to_owned();
            return;
        };

        if source_id == target_id {
            self.status_message = "A system cannot link to itself".to_owned();
            return;
        }

        let label = self.new_link_label.trim();

        let result = self
            .repo
            .create_link(source_id, target_id, label)
            .and_then(|_| self.load_selected_data(source_id));

        match result {
            Ok(_) => {
                self.new_link_label.clear();
                self.new_link_target_id = None;
                self.status_message = "Interaction saved".to_owned();
            }
            Err(error) => {
                self.status_message = format!("Failed to create interaction: {error}");
            }
        }
    }

    fn system_name_by_id(&self, id: i64) -> String {
        self.systems
            .iter()
            .find(|system| system.id == id)
            .map(|system| system.name.clone())
            .unwrap_or_else(|| format!("Unknown ({id})"))
    }

    fn hierarchy_rows(&self) -> Vec<(usize, i64, String)> {
        let mut by_parent: HashMap<Option<i64>, Vec<&SystemRecord>> = HashMap::new();
        for system in &self.systems {
            by_parent.entry(system.parent_id).or_default().push(system);
        }

        for children in by_parent.values_mut() {
            children.sort_by_key(|system| system.name.to_lowercase());
        }

        let mut rows = Vec::new();
        self.walk_hierarchy(None, 0, &by_parent, &mut rows);
        rows
    }

    fn walk_hierarchy(
        &self,
        parent_id: Option<i64>,
        depth: usize,
        by_parent: &HashMap<Option<i64>, Vec<&SystemRecord>>,
        rows: &mut Vec<(usize, i64, String)>,
    ) {
        if let Some(children) = by_parent.get(&parent_id) {
            for child in children {
                rows.push((depth, child.id, child.name.clone()));
                self.walk_hierarchy(Some(child.id), depth + 1, by_parent, rows);
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

    fn tech_name_by_id(&self, id: i64) -> String {
        self.tech_catalog
            .iter()
            .find(|tech| tech.id == id)
            .map(|tech| tech.name.clone())
            .unwrap_or_else(|| format!("Unknown tech ({id})"))
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

    fn validate_before_render(&mut self) -> Result<()> {
        self.ensure_valid_parent_selection();
        self.ensure_valid_link_target_selection();
        self.ensure_valid_tech_selection();

        if self.systems.is_empty() && self.selected_system_id.is_some() {
            return Err(anyhow!(
                "invalid state: selected system exists while systems list is empty"
            ));
        }

        Ok(())
    }
}

impl eframe::App for SystemsCatalogApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Err(error) = self.validate_before_render() {
            self.status_message = format!("State warning: {error}");
        }

        egui::TopBottomPanel::top("header_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Systems Catalog");
                ui.label("Track systems, dependencies, and notes");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(&self.status_message).italics());
                });
            });
        });

        egui::SidePanel::left("systems_panel")
            .default_width(320.0)
            .show(ctx, |ui| {
                ui.heading("Systems");

                if ui.button("Refresh").clicked() {
                    if let Err(error) = self.refresh_systems() {
                        self.status_message = format!("Refresh failed: {error}");
                    }
                }

                ui.separator();
                ui.label("Add system");
                ui.text_edit_singleline(&mut self.new_system_name);
                ui.add(egui::TextEdit::multiline(&mut self.new_system_description).desired_rows(3));

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

                if ui.button("Create system").clicked() {
                    self.create_system();
                }

                ui.separator();
                ui.label("Tech catalog");
                ui.text_edit_singleline(&mut self.new_tech_name);
                if ui.button("Save technology").clicked() {
                    self.create_tech_item();
                }

                ui.separator();
                ui.label("Hierarchy");

                let rows = self.hierarchy_rows();
                if rows.is_empty() {
                    ui.label("No systems yet. Create your first system above.");
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (depth, system_id, name) in rows {
                            let indent = "  ".repeat(depth);
                            let row_text = format!("{indent}• {name}");
                            let selected = self.selected_system_id == Some(system_id);

                            if ui.selectable_label(selected, row_text).clicked() {
                                self.selected_system_id = Some(system_id);
                                if let Err(error) = self.load_selected_data(system_id) {
                                    self.status_message =
                                        format!("Failed to load selection: {error}");
                                }
                            }
                        }
                    });
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Details");

            let Some(system) = self.selected_system().cloned() else {
                ui.label("Select a system from the left panel.");
                return;
            };

            ui.label(RichText::new(system.name.clone()).strong());
            if let Some(parent_id) = system.parent_id {
                ui.label(format!("Parent: {}", self.system_name_by_id(parent_id)));
            } else {
                ui.label("Parent: none (root)");
            }

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
                egui::ScrollArea::vertical()
                    .max_height(150.0)
                    .show(ui, |ui| {
                        for link in &self.selected_links {
                            let source = self.system_name_by_id(link.source_system_id);
                            let target = self.system_name_by_id(link.target_system_id);
                            let label = if link.label.trim().is_empty() {
                                "(no label)".to_owned()
                            } else {
                                link.label.clone()
                            };

                            ui.label(format!("#{:03} {source} → {target} : {label}", link.id));
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
                for tech in &self.selected_system_tech {
                    ui.label(format!("• {}", tech.name));
                }
            }

            ui.separator();
            ui.label("Cumulative child tech stack (deduped)");
            if self.selected_cumulative_child_tech.is_empty() {
                ui.label("No child-system technologies found.");
            } else {
                for tech_name in &self.selected_cumulative_child_tech {
                    ui.label(format!("• {tech_name}"));
                }
            }

            ui.separator();
            ui.label("Notes");
            ui.add(egui::TextEdit::multiline(&mut self.note_text).desired_rows(12));
            if ui.button("Save notes").clicked() {
                self.save_note();
            }
        });
    }
}
