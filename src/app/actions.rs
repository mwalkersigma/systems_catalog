use crate::app::SystemsCatalogApp;

impl SystemsCatalogApp {
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

        let result = self.repo.update_tech_item(tech_id, name).and_then(|_| {
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

    pub(super) fn save_note(&mut self) {
        let Some(system_id) = self.selected_system_id else {
            self.status_message = "Select a system first".to_owned();
            return;
        };

        match self.repo.upsert_note(system_id, self.note_text.trim()) {
            Ok(_) => self.status_message = "Notes saved".to_owned(),
            Err(error) => self.status_message = format!("Failed to save notes: {error}"),
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
