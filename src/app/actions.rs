use crate::app::SystemsCatalogApp;

impl SystemsCatalogApp {
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
        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        let result = self
            .repo
            .update_link_label(link_id, label)
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
        self.create_link_between(source_id, target_id, &label);
    }

    pub(super) fn create_link_between(&mut self, source_id: i64, target_id: i64, label: &str) {
        if source_id == target_id {
            self.status_message = "A system cannot link to itself".to_owned();
            return;
        }

        let result = self
            .repo
            .create_link(source_id, target_id, label)
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
}
