use std::collections::{HashMap, HashSet};

use crate::app::{CopiedSystemEntry, InteractionKind, SystemsCatalogApp};

impl SystemsCatalogApp {
    pub(super) fn create_zone_from_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) {
        let clamped_width = width.max(24.0);
        let clamped_height = height.max(24.0);
        let parent_zone_id =
            self.zone_parent_for_rect(x, y, clamped_width, clamped_height, None);

        let zone_name = format!("Zone {}", self.zones.len() + 1);
        let color_value = Some(Self::color_to_setting_value(self.selected_zone_color));

        let result = self
            .repo
            .create_zone(
                zone_name.as_str(),
                x,
                y,
                clamped_width,
                clamped_height,
                color_value.as_deref(),
                self.selected_zone_render_priority,
                parent_zone_id,
                false,
                None,
            )
            .and_then(|new_zone_id| {
                self.refresh_systems()?;
                self.select_zone(new_zone_id);
                Ok(())
            });

        match result {
            Ok(_) => {
                self.status_message = "Zone created".to_owned();
            }
            Err(error) => {
                self.status_message = format!("Failed to create zone: {error}");
            }
        }
    }

    pub(super) fn update_selected_zone_properties(&mut self) {
        let Some(zone_id) = self.selected_zone_id else {
            return;
        };

        let Some(existing) = self.zones.iter().find(|zone| zone.id == zone_id).cloned() else {
            return;
        };

        let name = self.selected_zone_name.trim();
        let name = if name.is_empty() { "Zone" } else { name };

        let color_value = Some(Self::color_to_setting_value(self.selected_zone_color));
        let result = self
            .repo
            .update_zone(
                zone_id,
                name,
                existing.x,
                existing.y,
                existing.width,
                existing.height,
                color_value.as_deref(),
                self.selected_zone_render_priority,
                self.selected_zone_parent_zone_id,
                self.selected_zone_minimized,
                self.selected_zone_representative_system_id,
            )
            .and_then(|_| self.refresh_systems());

        if result.is_err() {
            self.status_message = "Failed to update zone".to_owned();
        }
    }

    pub(super) fn update_selected_zone_geometry(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) {
        let Some(zone_id) = self.selected_zone_id else {
            return;
        };

        let Some(existing) = self.zones.iter().find(|zone| zone.id == zone_id).cloned() else {
            return;
        };

        let clamped_width = width.max(24.0);
        let clamped_height = height.max(24.0);
        let clamped_x = x.max(0.0);
        let clamped_y = y.max(0.0);

        let max_x = (self.map_world_size.x - clamped_width).max(0.0);
        let max_y = (self.map_world_size.y - clamped_height).max(0.0);

        let name = self.selected_zone_name.trim();
        let name = if name.is_empty() { "Zone" } else { name };
        let color_value = Some(Self::color_to_setting_value(self.selected_zone_color));

        let result = self
            .repo
            .update_zone(
                zone_id,
                name,
                clamped_x.min(max_x),
                clamped_y.min(max_y),
                clamped_width,
                clamped_height,
                color_value.as_deref(),
                self.selected_zone_render_priority,
                existing.parent_zone_id,
                self.selected_zone_minimized,
                self.selected_zone_representative_system_id,
            )
            .and_then(|_| self.refresh_systems())
            .and_then(|_| {
                self.select_zone(zone_id);
                Ok(())
            });

        if result.is_err() {
            self.status_message = "Failed to update zone geometry".to_owned();
        }
    }

    pub(super) fn toggle_zone_minimized(&mut self, zone_id: i64) {
        let Some(existing) = self.zones.iter().find(|zone| zone.id == zone_id).cloned() else {
            return;
        };

        let representative = self.zone_resolved_representative_system_id(zone_id);
        let next_minimized = !existing.minimized;
        let next_representative = representative.or(existing.representative_system_id);

        if !existing.minimized && next_representative.is_none() {
            self.status_message =
                "Cannot minimize zone: choose a representative ancestor system".to_owned();
            return;
        }

        let nested_child_ids = self.zone_nested_child_ids(zone_id);
        let nested_children = self
            .zones
            .iter()
            .filter(|zone| nested_child_ids.contains(&zone.id))
            .cloned()
            .collect::<Vec<_>>();
        let mut affected_representative_ids = HashSet::new();
        if let Some(representative_id) = self.zone_resolved_representative_system_id(zone_id) {
            affected_representative_ids.insert(representative_id);
        }
        for child in &nested_children {
            if let Some(representative_id) = self.zone_resolved_representative_system_id(child.id) {
                affected_representative_ids.insert(representative_id);
            }
        }

        let result = self
            .repo
            .update_zone(
                zone_id,
                existing.name.as_str(),
                existing.x,
                existing.y,
                existing.width,
                existing.height,
                existing.color.as_deref(),
                existing.render_priority,
                existing.parent_zone_id,
                next_minimized,
                next_representative,
            )
            .and_then(|_| {
                for child in &nested_children {
                    self.repo.update_zone(
                        child.id,
                        child.name.as_str(),
                        child.x,
                        child.y,
                        child.width,
                        child.height,
                        child.color.as_deref(),
                        child.render_priority,
                        child.parent_zone_id,
                        next_minimized,
                        child.representative_system_id,
                    )?;
                }
                Ok(())
            })
            .and_then(|_| self.refresh_systems())
            .and_then(|_| {
                if !next_minimized {
                    for representative_id in &affected_representative_ids {
                        self.auto_collapsed_zone_representative_ids
                            .remove(representative_id);

                        if self.collapsed_system_ids.contains(representative_id) {
                            self.on_disclosure_click(*representative_id);
                        }
                    }
                }
                self.select_zone(zone_id);
                Ok(())
            });

        match result {
            Ok(_) => {
                self.status_message = if next_minimized {
                    "Zone minimized".to_owned()
                } else {
                    "Zone expanded".to_owned()
                };
            }
            Err(error) => {
                self.status_message = format!("Failed to toggle zone state: {error}");
            }
        }
    }

    pub(super) fn delete_selected_zone(&mut self) {
        let Some(zone_id) = self.selected_zone_id else {
            return;
        };

        let zone = self.zones.iter().find(|zone| zone.id == zone_id).cloned();

        if let Some(zone) = zone {
            let bound_system_ids = self
                .zone_offsets_by_system
                .iter()
                .filter_map(|(system_id, (bound_zone_id, offset))| {
                    if *bound_zone_id != zone_id {
                        return None;
                    }

                    let absolute = eframe::egui::Pos2::new(zone.x + offset.x, zone.y + offset.y);
                    Some((*system_id, absolute))
                })
                .collect::<Vec<_>>();

            for (system_id, absolute) in bound_system_ids {
                self.map_positions.insert(system_id, absolute);
                self.persist_map_position(system_id, absolute);
            }
        }

        let result = self.repo.delete_zone(zone_id).and_then(|_| self.refresh_systems());

        match result {
            Ok(_) => {
                self.selected_zone_id = None;
                self.selected_zone_name.clear();
                self.selected_zone_render_priority = 1;
                self.selected_zone_parent_zone_id = None;
                self.selected_zone_minimized = false;
                self.selected_zone_representative_system_id = None;
                self.status_message = "Zone deleted".to_owned();
            }
            Err(error) => {
                self.status_message = format!("Failed to delete zone: {error}");
            }
        }
    }

    fn escape_clipboard_field(value: &str) -> String {
        value
            .replace('\\', "\\\\")
            .replace('\t', "\\t")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    }

    fn unescape_clipboard_field(value: &str) -> String {
        let mut result = String::new();
        let mut chars = value.chars();

        while let Some(ch) = chars.next() {
            if ch != '\\' {
                result.push(ch);
                continue;
            }

            match chars.next() {
                Some('t') => result.push('\t'),
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('\\') => result.push('\\'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        }

        result
    }

    pub(super) fn copied_systems_payload(&self) -> Option<String> {
        if self.copied_system_entries.is_empty() {
            return None;
        }

        let mut payload = String::from("systems-catalog-cards:v2\n");
        for entry in &self.copied_system_entries {
            let escaped_name = Self::escape_clipboard_field(entry.name.as_str());
            let escaped_description = Self::escape_clipboard_field(entry.description.as_str());
            let parent_index = entry
                .parent_index
                .map(|index| index.to_string())
                .unwrap_or_else(|| "-1".to_owned());
            payload.push_str(
                format!(
                    "{}\t{}\t{}\t{}\t{}\n",
                    escaped_name,
                    escaped_description,
                    parent_index,
                    entry.relative_x,
                    entry.relative_y
                )
                .as_str(),
            );
        }

        Some(payload)
    }

    pub(super) fn load_copied_systems_from_payload(&mut self, payload: &str) -> bool {
        let trimmed = payload.trim_end();
        if let Some(rest) = trimmed.strip_prefix("systems-catalog-cards:v2") {
            let entries = rest
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .filter_map(|line| {
                    let columns = line.split('\t').collect::<Vec<_>>();
                    if columns.len() < 5 {
                        return None;
                    }

                    let name = Self::unescape_clipboard_field(columns[0]);
                    if name.trim().is_empty() {
                        return None;
                    }

                    let description = Self::unescape_clipboard_field(columns[1]);
                    let parent_index = columns[2]
                        .parse::<isize>()
                        .ok()
                        .and_then(|index| if index >= 0 { Some(index as usize) } else { None });
                    let relative_x = columns[3].parse::<f32>().ok().unwrap_or(0.0);
                    let relative_y = columns[4].parse::<f32>().ok().unwrap_or(0.0);

                    Some(CopiedSystemEntry {
                        name,
                        description,
                        parent_index,
                        relative_x,
                        relative_y,
                    })
                })
                .collect::<Vec<_>>();

            if entries.is_empty() {
                return false;
            }

            self.copied_system_entries = entries;
            return true;
        }

        let Some(rest) = trimmed.strip_prefix("systems-catalog-cards:v1") else {
            return false;
        };

        let copied = rest
            .lines()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| name.to_owned())
            .collect::<Vec<_>>();

        if copied.is_empty() {
            return false;
        }

        self.copied_system_entries = copied
            .into_iter()
            .enumerate()
            .map(|(index, name)| CopiedSystemEntry {
                name,
                description: String::new(),
                parent_index: None,
                relative_x: index as f32 * 36.0,
                relative_y: 0.0,
            })
            .collect();
        true
    }

    fn create_system_with_generated_unique_name(
        &self,
        base_name: &str,
        description: &str,
        parent_id: Option<i64>,
    ) -> anyhow::Result<i64> {
        let base = base_name.trim();
        if base.is_empty() {
            return Err(anyhow::anyhow!("System name is required"));
        }

        if let Ok(id) = self.repo.create_system(base, description, parent_id) {
            return Ok(id);
        }

        for attempt in 1..=250 {
            let candidate = if attempt == 1 {
                format!("{base} Copy")
            } else {
                format!("{base} Copy {attempt}")
            };

            if let Ok(id) = self.repo.create_system(candidate.as_str(), description, parent_id) {
                return Ok(id);
            }
        }

        Err(anyhow::anyhow!(
            "Unable to create unique system name for '{}'",
            base
        ))
    }

    pub(super) fn copy_selected_map_systems(&mut self) {
        let mut source_ids = self.selected_map_system_ids.clone();
        if source_ids.is_empty() {
            if let Some(system_id) = self.selected_system_id {
                source_ids.insert(system_id);
            }
        }

        if source_ids.is_empty() {
            self.status_message =
                "Copy failed: select one or more cards (or a system) first".to_owned();
            return;
        }

        let mut copied_systems = self
            .systems
            .iter()
            .filter(|system| source_ids.contains(&system.id))
            .cloned()
            .collect::<Vec<_>>();

        copied_systems.sort_by_key(|system| system.id);

        if copied_systems.is_empty() {
            self.status_message = "Copy failed: no matching systems found".to_owned();
            return;
        }

        let id_to_index = copied_systems
            .iter()
            .enumerate()
            .map(|(index, system)| (system.id, index))
            .collect::<HashMap<_, _>>();

        let copied_positions = copied_systems
            .iter()
            .map(|system| {
                self.map_positions
                    .get(&system.id)
                    .copied()
                    .or_else(|| match (system.map_x, system.map_y) {
                        (Some(x), Some(y)) => Some(eframe::egui::Pos2::new(x, y)),
                        _ => None,
                    })
                    .unwrap_or(eframe::egui::Pos2::new(0.0, 0.0))
            })
            .collect::<Vec<_>>();

        let min_x = copied_positions
            .iter()
            .map(|position| position.x)
            .fold(f32::INFINITY, f32::min);
        let min_y = copied_positions
            .iter()
            .map(|position| position.y)
            .fold(f32::INFINITY, f32::min);

        self.copied_system_entries = copied_systems
            .iter()
            .enumerate()
            .map(|(index, system)| {
                let position = copied_positions[index];
                CopiedSystemEntry {
                    name: system.name.clone(),
                    description: system.description.clone(),
                    parent_index: system
                        .parent_id
                        .and_then(|parent_id| id_to_index.get(&parent_id).copied()),
                    relative_x: position.x - min_x,
                    relative_y: position.y - min_y,
                }
            })
            .collect();

        self.status_message = format!(
            "Copied {} card(s) to clipboard",
            self.copied_system_entries.len()
        );
    }

    pub(super) fn paste_copied_systems(&mut self) {
        if self.copied_system_entries.is_empty() {
            self.status_message =
                "Paste failed: clipboard is empty (use Ctrl+C first)".to_owned();
            return;
        }

        let parent_id = self.selected_system_id;
        let parent_tech_ids = parent_id
            .and_then(|id| self.system_tech_ids_by_system.get(&id).cloned())
            .unwrap_or_default();

        let entries_to_create = self.copied_system_entries.clone();
        let result = (|| -> anyhow::Result<usize> {
            let mut created_ids_by_entry = HashMap::<usize, i64>::new();
            let mut created_order = Vec::<usize>::new();
            let mut remaining = (0..entries_to_create.len()).collect::<HashSet<_>>();

            while !remaining.is_empty() {
                let mut progress = false;
                let pending = remaining.iter().copied().collect::<Vec<_>>();

                for entry_index in pending {
                    let entry = &entries_to_create[entry_index];

                    let effective_parent_id = match entry.parent_index {
                        Some(parent_entry_index) => {
                            let Some(created_parent_id) =
                                created_ids_by_entry.get(&parent_entry_index).copied()
                            else {
                                continue;
                            };

                            Some(created_parent_id)
                        }
                        None => parent_id,
                    };

                    let new_id = self.create_system_with_generated_unique_name(
                        entry.name.as_str(),
                        entry.description.as_str(),
                        effective_parent_id,
                    )?;

                    if effective_parent_id == parent_id {
                        for tech_id in &parent_tech_ids {
                            let _ = self.repo.add_tech_to_system(new_id, *tech_id);
                        }
                    }

                    created_ids_by_entry.insert(entry_index, new_id);
                    created_order.push(entry_index);
                    remaining.remove(&entry_index);
                    progress = true;
                }

                if !progress {
                    return Err(anyhow::anyhow!(
                        "Unable to resolve clipboard hierarchy for paste"
                    ));
                }
            }

            self.refresh_systems()?;

            let anchor_position = if parent_id.is_some() {
                self.find_next_free_child_spawn_position(parent_id)
                    .unwrap_or_else(|| self.find_next_free_root_spawn_position())
            } else {
                self.find_next_free_root_spawn_position()
            };

            let mut pasted_set = HashSet::new();
            for entry_index in created_order {
                let Some(created_id) = created_ids_by_entry.get(&entry_index).copied() else {
                    continue;
                };

                let entry = &entries_to_create[entry_index];
                let desired_position = eframe::egui::Pos2::new(
                    anchor_position.x + entry.relative_x,
                    anchor_position.y + entry.relative_y,
                );

                let node_size = self
                    .systems
                    .iter()
                    .find(|system| system.id == created_id)
                    .map(|_system| crate::app::MAP_NODE_SIZE)
                    .unwrap_or(crate::app::MAP_NODE_SIZE);

                let clamped = self.clamp_node_position(
                    eframe::egui::Rect::NOTHING,
                    desired_position,
                    node_size,
                );

                self.map_positions.insert(created_id, clamped);
                self.persist_map_position(created_id, clamped);
                pasted_set.insert(created_id);
            }

            if let Some(first_created_id) = created_ids_by_entry.values().next().copied() {
                self.selected_system_id = Some(first_created_id);
                let _ = self.load_selected_data(first_created_id);
            }

            self.selected_map_system_ids = pasted_set;

            Ok(created_ids_by_entry.len())
        })();

        match result {
            Ok(count) => {
                if let Some(parent_id) = parent_id {
                    self.status_message = format!(
                        "Pasted {} card(s) as children of '{}'",
                        count,
                        self.system_name_by_id(parent_id)
                    );
                } else {
                    self.status_message = format!("Pasted {} card(s) at root", count);
                }
            }
            Err(error) => {
                self.status_message = format!("Failed to paste cards: {error}");
            }
        }
    }

    pub(super) fn update_selected_system_details(&mut self) {
        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let edited_name = self.edited_system_name.trim();
        if edited_name.is_empty() {
            self.status_message = "System name is required".to_owned();
            return;
        }

        let naming_delimiter = self.selected_system_naming_delimiter.trim();
        let naming_delimiter = if naming_delimiter.is_empty() {
            "/"
        } else {
            naming_delimiter
        };

        let result = self
            .repo
            .update_system_details(
                system_id,
                edited_name,
                self.edited_system_description.trim(),
                self.selected_system_naming_root,
                naming_delimiter,
            )
            .and_then(|_| self.refresh_systems())
            .and_then(|_| self.load_selected_data(system_id));

        match result {
            Ok(_) => self.status_message = "System details updated".to_owned(),
            Err(error) => {
                self.status_message = format!("Failed to update system details: {error}")
            }
        }
    }

    pub(super) fn select_note_for_edit(&mut self, note_id: i64) {
        self.selected_note_id_for_edit = Some(note_id);
        self.note_text = self
            .selected_notes
            .iter()
            .find(|note| note.id == note_id)
            .map(|note| note.body.clone())
            .unwrap_or_default();
    }

    pub(super) fn create_note_for_selected_system(&mut self) {
        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        match self.repo.create_note(system_id, "") {
            Ok(_) => {
                if let Err(error) = self.load_selected_data(system_id) {
                    self.status_message = format!("Failed to load notes: {error}");
                    return;
                }

                if let Some(first_note) = self.selected_notes.first() {
                    self.select_note_for_edit(first_note.id);
                }

                self.status_message = "Note created".to_owned();
            }
            Err(error) => self.status_message = format!("Failed to create note: {error}"),
        }
    }

    pub(super) fn save_note(&mut self) {
        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let result = if let Some(note_id) = self.selected_note_id_for_edit {
            self.repo.update_note(note_id, self.note_text.trim())
        } else {
            self.repo.create_note(system_id, self.note_text.trim())
        }
        .and_then(|_| self.load_selected_data(system_id));

        match result {
            Ok(_) => self.status_message = "Note saved".to_owned(),
            Err(error) => self.status_message = format!("Failed to save note: {error}"),
        }
    }

    pub(super) fn delete_selected_note(&mut self) {
        let Some(note_id) = self.selected_note_id_for_edit else {
            self.status_message = "Select a note first".to_owned();
            return;
        };

        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let result = self
            .repo
            .delete_note(note_id)
            .and_then(|_| self.load_selected_data(system_id));

        match result {
            Ok(_) => self.status_message = "Note deleted".to_owned(),
            Err(error) => self.status_message = format!("Failed to delete note: {error}"),
        }
    }

    pub(super) fn export_catalog(&mut self) {
        let path = self.save_catalog_path.trim().to_owned();
        if path.is_empty() {
            self.status_message = "Save path is required".to_owned();
            return;
        }

        match self.repo.export_catalog_to_path(path.as_str()) {
            Ok(_) => {
                self.push_recent_catalog_path(path.as_str());
                self.load_catalog_path = path.clone();
                self.show_save_catalog_modal = false;
                self.status_message = format!("Catalog saved to {}", path);
            }
            Err(error) => self.status_message = format!("Failed to save catalog: {error}"),
        }
    }

    pub(super) fn import_catalog(&mut self) {
        let path = self.load_catalog_path.trim().to_owned();
        if path.is_empty() {
            self.status_message = "Load path is required".to_owned();
            return;
        }

        let result = self
            .repo
            .import_catalog_from_path(path.as_str())
            .and_then(|_| self.refresh_systems())
            .and_then(|_| self.load_ui_settings());

        match result {
            Ok(_) => {
                self.push_recent_catalog_path(path.as_str());
                self.save_catalog_path = path.clone();
                self.show_load_catalog_modal = false;
                self.clear_selection();
                self.status_message = format!("Catalog loaded from {}", path);
            }
            Err(error) => self.status_message = format!("Failed to load catalog: {error}"),
        }
    }

    pub(super) fn new_catalog(&mut self) {
        let result = self
            .repo
            .clear_catalog_data()
            .and_then(|_| self.refresh_systems());

        match result {
            Ok(_) => {
                self.clear_selection();
                self.map_positions.clear();
                self.show_add_system_modal = false;
                self.show_add_tech_modal = false;
                self.show_save_catalog_modal = false;
                self.show_load_catalog_modal = false;
                self.show_new_catalog_confirm_modal = false;
                self.status_message = "Created new empty catalog".to_owned();
            }
            Err(error) => self.status_message = format!("Failed to create new catalog: {error}"),
        }
    }

    pub(super) fn update_selected_system_line_color_override(&mut self) {
        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let override_value = self
            .selected_system_line_color_override
            .map(Self::color_to_setting_value);

        let result = self
            .repo
            .update_system_line_color_override(system_id, override_value.as_deref())
            .and_then(|_| self.refresh_systems())
            .and_then(|_| self.load_selected_data(system_id));

        match result {
            Ok(_) => self.status_message = "System line color override updated".to_owned(),
            Err(error) => {
                self.status_message = format!("Failed to update line color override: {error}")
            }
        }
    }

    pub(super) fn update_selected_link(&mut self) {
        let Some(link_id) = self.selected_link_id_for_edit else {
            self.status_message = "Select an interaction first".to_owned();
            return;
        };

        let label = self.edited_link_label.trim();
        let note = self.edited_link_note.trim();
        let kind = Self::interaction_kind_to_setting_value(self.edited_link_kind);
        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let result = self
            .repo
            .update_link_details(link_id, label, note, kind)
            .and_then(|_| self.refresh_systems())
            .and_then(|_| self.load_selected_data(system_id));

        match result {
            Ok(_) => self.status_message = "Interaction updated".to_owned(),
            Err(error) => self.status_message = format!("Failed to update interaction: {error}"),
        }
    }

    pub(super) fn delete_selected_link(&mut self) {
        let Some(link_id) = self.selected_link_id_for_edit else {
            self.status_message = "Select an interaction first".to_owned();
            return;
        };

        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let result = self
            .repo
            .delete_link(link_id)
            .and_then(|_| self.refresh_systems())
            .and_then(|_| self.load_selected_data(system_id));

        match result {
            Ok(_) => self.status_message = "Interaction removed".to_owned(),
            Err(error) => self.status_message = format!("Failed to remove interaction: {error}"),
        }
    }

    pub(super) fn update_selected_catalog_tech(&mut self) {
        let Some(tech_id) = self.selected_catalog_tech_id_for_edit else {
            self.status_message = "Select a technology first".to_owned();
            return;
        };

        let name = self.edited_tech_name.trim();
        if name.is_empty() {
            self.status_message = "Technology name is required".to_owned();
            return;
        }

        let description = Self::text_to_option(&self.edited_tech_description);
        let documentation_link = Self::text_to_option(&self.edited_tech_documentation_link);
        let color = self.edited_tech_color.map(Self::color_to_setting_value);
        let display_priority = self.edited_tech_display_priority;

        let result = self
            .repo
            .update_tech_item(
                tech_id,
                name,
                description,
                documentation_link,
                color.as_deref(),
                display_priority,
            )
            .and_then(|_| {
                self.refresh_systems().and_then(|_| {
                    if let Some(system_id) = self.selected_system_id {
                        self.load_selected_data(system_id)?;
                    }
                    Ok(())
                })
            });

        match result {
            Ok(_) => self.status_message = "Technology updated".to_owned(),
            Err(error) => self.status_message = format!("Failed to update technology: {error}"),
        }
    }

    pub(super) fn delete_selected_catalog_tech(&mut self) {
        let Some(tech_id) = self.selected_catalog_tech_id_for_edit else {
            self.status_message = "Select a technology first".to_owned();
            return;
        };

        let result = self.repo.delete_tech_item(tech_id).and_then(|_| {
            self.refresh_systems().and_then(|_| {
                if let Some(system_id) = self.selected_system_id {
                    self.load_selected_data(system_id)?;
                }
                Ok(())
            })
        });

        match result {
            Ok(_) => self.status_message = "Technology removed from catalog".to_owned(),
            Err(error) => self.status_message = format!("Failed to remove technology: {error}"),
        }
    }

    pub(super) fn remove_tech_from_selected_system(&mut self, tech_id: i64) {
        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let result = self
            .repo
            .remove_tech_from_system(system_id, tech_id)
            .and_then(|_| self.refresh_systems())
            .and_then(|_| self.load_selected_data(system_id));

        match result {
            Ok(_) => self.status_message = "Technology removed from system".to_owned(),
            Err(error) => {
                self.status_message = format!("Failed to remove technology from system: {error}")
            }
        }
    }

    pub(super) fn update_selected_system_parent(&mut self) {
        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        if let Some(parent_id) = self.selected_system_parent_id {
            if self.would_create_parent_cycle(system_id, parent_id) {
                self.status_message = "Invalid parent: this would create a cycle".to_owned();
                return;
            }
        }

        let result = self
            .repo
            .update_system_parent(system_id, self.selected_system_parent_id)
            .and_then(|_| self.refresh_systems())
            .and_then(|_| self.load_selected_data(system_id));

        match result {
            Ok(_) => self.status_message = "Parent updated".to_owned(),
            Err(error) => self.status_message = format!("Failed to update parent: {error}"),
        }
    }

    pub(super) fn delete_selected_system(&mut self) {
        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let system_name = self.system_name_by_id(system_id);
        let result = self
            .repo
            .delete_system(system_id)
            .and_then(|_| self.refresh_systems());

        match result {
            Ok(_) => {
                self.status_message = format!("Deleted system: {system_name}");
                self.map_link_click_source = None;
                self.map_link_drag_from = None;
                self.map_interaction_drag_from = None;
            }
            Err(error) => self.status_message = format!("Failed to delete system: {error}"),
        }
    }

    pub(super) fn create_tech_item(&mut self) {
        let name = self.new_tech_name.trim();
        if name.is_empty() {
            self.status_message = "Technology name is required".to_owned();
            return;
        }

        let description = Self::text_to_option(&self.new_tech_description);
        let documentation_link = Self::text_to_option(&self.new_tech_documentation_link);
        let color = self.new_tech_color.map(Self::color_to_setting_value);
        let display_priority = self.new_tech_display_priority;

        let result = self
            .repo
            .create_tech_item(
                name,
                description,
                documentation_link,
                color.as_deref(),
                display_priority,
            )
            .and_then(|_| self.refresh_systems());

        match result {
            Ok(_) => {
                self.new_tech_name.clear();
                self.new_tech_description.clear();
                self.new_tech_documentation_link.clear();
                self.new_tech_color = None;
                self.new_tech_display_priority = 0;
                self.status_message = "Technology saved to catalog".to_owned();
            }
            Err(error) => {
                self.status_message = format!("Failed to save technology: {error}");
            }
        }
    }

    pub(super) fn add_selected_tech_to_system(&mut self) {
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
            .and_then(|_| self.refresh_systems())
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

    pub(super) fn create_system(&mut self) {
        let name = self.new_system_name.trim();
        if name.is_empty() {
            self.status_message = "System name is required".to_owned();
            return;
        }

        let parent_id = self.new_system_parent_id;
        let description = self.new_system_description.trim();
        let assigned_tech_ids = self
            .new_system_assigned_tech_ids
            .iter()
            .copied()
            .collect::<Vec<_>>();

        let result = self
            .repo
            .create_system(name, description, parent_id)
            .and_then(|new_id| {
                for tech_id in &assigned_tech_ids {
                    self.repo.add_tech_to_system(new_id, *tech_id)?;
                }

                self.refresh_systems()?;

                let spawn_position = if parent_id.is_some() {
                    self.find_next_free_child_spawn_position(parent_id)
                } else {
                    Some(self.find_next_free_root_spawn_position())
                };

                if let Some(spawn_position) = spawn_position {
                    self.map_positions.insert(new_id, spawn_position);
                    self.persist_map_position(new_id, spawn_position);
                }

                Ok(())
            });

        match result {
            Ok(_) => {
                self.new_system_name.clear();
                self.new_system_description.clear();
                self.new_system_parent_id = None;
                self.new_system_tech_id_for_assignment = None;
                self.new_system_assigned_tech_ids.clear();
                self.show_add_system_modal = false;
                self.show_add_tech_modal = false;
                self.status_message = "System created".to_owned();
            }
            Err(error) => {
                self.status_message = format!("Failed to create system: {error}");
            }
        }
    }

    pub(super) fn create_systems_bulk_from_list(&mut self) {
        let names = self
            .bulk_new_system_names
            .split(|character| character == ',' || character == '\n' || character == '\r')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| name.to_owned())
            .collect::<Vec<_>>();

        if names.is_empty() {
            self.status_message = "Enter at least one system name".to_owned();
            return;
        }

        let parent_id = self.bulk_new_system_parent_id;
        let parent_tech_ids = parent_id
            .and_then(|id| self.system_tech_ids_by_system.get(&id).cloned())
            .unwrap_or_default();

        let result = (|| -> anyhow::Result<()> {
            let mut created_ids = Vec::new();
            for name in &names {
                let new_id = self.repo.create_system(name, "", parent_id)?;

                for tech_id in &parent_tech_ids {
                    let _ = self.repo.add_tech_to_system(new_id, *tech_id);
                }

                created_ids.push(new_id);
            }

            self.refresh_systems()?;

            for created_id in created_ids {
                let spawn_position = if parent_id.is_some() {
                    self.find_next_free_child_spawn_position(parent_id)
                } else {
                    Some(self.find_next_free_root_spawn_position())
                };

                if let Some(position) = spawn_position {
                    self.map_positions.insert(created_id, position);
                    self.persist_map_position(created_id, position);
                }
            }

            Ok(())
        })();

        match result {
            Ok(_) => {
                self.bulk_new_system_names.clear();
                self.show_bulk_add_systems_modal = false;
                self.status_message = format!("Created {} systems", names.len());
            }
            Err(error) => {
                self.status_message = format!("Failed to bulk-create systems: {error}");
            }
        }
    }

    pub(super) fn fast_add_selected_catalog_tech_to_system(&mut self, system_id: i64) {
        let Some(tech_id) = self.selected_catalog_tech_id_for_edit else {
            self.status_message = "Select a technology in Tech Catalog first".to_owned();
            return;
        };

        let already_assigned = self
            .system_tech_ids_by_system
            .get(&system_id)
            .map(|tech_ids| tech_ids.contains(&tech_id))
            .unwrap_or(false);

        if already_assigned {
            return;
        }

        let tech_name = self.tech_name_by_id(tech_id);
        let system_name = self.system_name_by_id(system_id);

        let result = self
            .repo
            .add_tech_to_system(system_id, tech_id)
            .and_then(|_| self.refresh_systems())
            .and_then(|_| {
                if self.selected_system_id == Some(system_id) {
                    self.load_selected_data(system_id)?;
                }
                Ok(())
            });

        match result {
            Ok(_) => {
                self.status_message =
                    format!("Added technology '{tech_name}' to '{system_name}'");
            }
            Err(error) => {
                self.status_message = format!("Failed to fast-assign technology: {error}");
            }
        }
    }

    pub(super) fn fast_add_selected_catalog_tech_to_subtree(&mut self, parent_system_id: i64) {
        let Some(tech_id) = self.selected_catalog_tech_id_for_edit else {
            return;
        };

        let subtree_ids = self.system_and_descendant_ids(parent_system_id);
        if subtree_ids.is_empty() {
            return;
        }

        let mut added_count = 0usize;
        let result = (|| -> anyhow::Result<()> {
            for system_id in &subtree_ids {
                let already_assigned = self
                    .system_tech_ids_by_system
                    .get(system_id)
                    .map(|tech_ids| tech_ids.contains(&tech_id))
                    .unwrap_or(false);

                if already_assigned {
                    continue;
                }

                self.repo.add_tech_to_system(*system_id, tech_id)?;
                added_count += 1;
            }

            self.refresh_systems()?;
            if let Some(selected_system_id) = self.selected_system_id {
                self.load_selected_data(selected_system_id)?;
            }

            Ok(())
        })();

        match result {
            Ok(_) => {
                if added_count > 0 {
                    self.status_message = format!(
                        "Added '{}' to {} system(s) in subtree",
                        self.tech_name_by_id(tech_id),
                        added_count
                    );
                }
            }
            Err(error) => {
                self.status_message = format!("Failed to apply tech to subtree: {error}");
            }
        }
    }
    pub(super) fn create_link(&mut self) {
        let Some(source_id) = self.selected_system_id else {
            self.status_message = "Select a system to create an interaction".to_owned();
            return;
        };

        let Some(target_id) = self.new_link_target_id else {
            self.status_message = "Select a target system".to_owned();
            return;
        };

        let label = self.new_link_label.trim().to_string();
        self.create_link_between_kind(source_id, target_id, &label, InteractionKind::Standard);
    }

    pub(super) fn create_link_between(&mut self, source_id: i64, target_id: i64, label: &str) {
        self.create_link_between_kind(source_id, target_id, label, InteractionKind::Standard);
    }

    pub(super) fn create_link_between_kind(
        &mut self,
        source_id: i64,
        target_id: i64,
        label: &str,
        kind: InteractionKind,
    ) {
        if source_id == target_id {
            self.status_message = "A system cannot link to itself".to_owned();
            return;
        }

        let result = self
            .repo
            .create_link(
                source_id,
                target_id,
                label,
                Self::interaction_kind_to_setting_value(kind),
            )
            .and_then(|_| self.refresh_systems())
            .and_then(|_| self.load_selected_data(source_id));

        match result {
            Ok(_) => {
                self.new_link_label.clear();
                self.new_link_target_id = None;
                self.selected_system_id = Some(source_id);
                self.status_message = "Interaction saved".to_owned();
            }
            Err(error) => {
                self.status_message = format!("Failed to create interaction: {error}");
            }
        }
    }

    pub(super) fn assign_parent_between(&mut self, child_id: i64, parent_id: i64) {
        if child_id == parent_id {
            self.status_message = "A system cannot be its own parent".to_owned();
            return;
        }

        if self.would_create_parent_cycle(child_id, parent_id) {
            self.status_message = "Invalid parent: this would create a cycle".to_owned();
            return;
        }

        let child_name = self.system_name_by_id(child_id);
        let parent_name = self.system_name_by_id(parent_id);

        let result = self
            .repo
            .update_system_parent(child_id, Some(parent_id))
            .and_then(|_| self.refresh_systems())
            .and_then(|_| {
                if self.selected_system_id == Some(child_id) {
                    self.load_selected_data(child_id)?;
                }
                Ok(())
            });

        match result {
            Ok(_) => {
                self.status_message =
                    format!("Assigned parent: '{parent_name}' <- '{child_name}'");
            }
            Err(error) => {
                self.status_message = format!("Failed to assign parent: {error}");
            }
        }
    }
}
