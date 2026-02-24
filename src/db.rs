use std::path::Path;

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};

use crate::models::{SystemLink, SystemNote, SystemRecord, TechItem};

/// Data access layer that owns the SQLite connection.
///
/// TypeScript analogy: this is similar to a repository/service class that wraps SQL queries,
/// keeping UI code focused on state + rendering instead of persistence details.
pub struct Repository {
    conn: Connection,
}

impl Repository {
    /// Open/create a SQLite database and ensure schema is present.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let repository = Self { conn };
        repository.init_schema()?;
        Ok(repository)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS systems (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                description TEXT NOT NULL DEFAULT '',
                parent_id INTEGER NULL,
                map_x REAL NULL,
                map_y REAL NULL,
                line_color_override TEXT NULL,
                FOREIGN KEY(parent_id) REFERENCES systems(id) ON DELETE SET NULL
            );

            CREATE TABLE IF NOT EXISTS links (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_system_id INTEGER NOT NULL,
                target_system_id INTEGER NOT NULL,
                label TEXT NOT NULL DEFAULT '',
                UNIQUE(source_system_id, target_system_id),
                FOREIGN KEY(source_system_id) REFERENCES systems(id) ON DELETE CASCADE,
                FOREIGN KEY(target_system_id) REFERENCES systems(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS notes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                system_id INTEGER NOT NULL,
                body TEXT NOT NULL DEFAULT '',
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY(system_id) REFERENCES systems(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS tech_catalog (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                description TEXT NULL,
                documentation_link TEXT NULL
            );

            CREATE TABLE IF NOT EXISTS system_tech (
                system_id INTEGER NOT NULL,
                tech_id INTEGER NOT NULL,
                PRIMARY KEY(system_id, tech_id),
                FOREIGN KEY(system_id) REFERENCES systems(id) ON DELETE CASCADE,
                FOREIGN KEY(tech_id) REFERENCES tech_catalog(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )?;

        self.ensure_systems_position_columns()?;
        self.ensure_system_line_color_override_column()?;
        self.ensure_tech_catalog_columns()?;
        self.ensure_notes_table_shape()?;

        Ok(())
    }

    fn ensure_systems_position_columns(&self) -> Result<()> {
        if !self.table_has_column("systems", "map_x")? {
            self.conn
                .execute("ALTER TABLE systems ADD COLUMN map_x REAL NULL", [])?;
        }

        if !self.table_has_column("systems", "map_y")? {
            self.conn
                .execute("ALTER TABLE systems ADD COLUMN map_y REAL NULL", [])?;
        }

        Ok(())
    }

    fn ensure_system_line_color_override_column(&self) -> Result<()> {
        if !self.table_has_column("systems", "line_color_override")? {
            self.conn.execute(
                "ALTER TABLE systems ADD COLUMN line_color_override TEXT NULL",
                [],
            )?;
        }

        Ok(())
    }

    fn ensure_tech_catalog_columns(&self) -> Result<()> {
        if !self.table_has_column("tech_catalog", "description")? {
            self.conn.execute(
                "ALTER TABLE tech_catalog ADD COLUMN description TEXT NULL",
                [],
            )?;
        }

        if !self.table_has_column("tech_catalog", "documentation_link")? {
            self.conn.execute(
                "ALTER TABLE tech_catalog ADD COLUMN documentation_link TEXT NULL",
                [],
            )?;
        }

        Ok(())
    }

    fn ensure_notes_table_shape(&self) -> Result<()> {
        if self.table_has_column("notes", "id")? {
            return Ok(());
        }

        self.conn.execute_batch(
            r#"
            ALTER TABLE notes RENAME TO notes_legacy;

            CREATE TABLE notes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                system_id INTEGER NOT NULL,
                body TEXT NOT NULL DEFAULT '',
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY(system_id) REFERENCES systems(id) ON DELETE CASCADE
            );

            INSERT INTO notes (system_id, body, updated_at)
            SELECT system_id, body, updated_at
            FROM notes_legacy;

            DROP TABLE notes_legacy;
            "#,
        )?;

        Ok(())
    }

    fn table_has_column(&self, table_name: &str, column_name: &str) -> Result<bool> {
        let query = format!("PRAGMA table_info({table_name})");
        let mut stmt = self.conn.prepare(&query)?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(columns.iter().any(|column| column == column_name))
    }

    pub fn list_systems(&self) -> Result<Vec<SystemRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, description, parent_id, map_x, map_y, line_color_override
            FROM systems
            ORDER BY LOWER(name);
            "#,
        )?;

        let systems = stmt
            .query_map([], |row| {
                Ok(SystemRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    parent_id: row.get(3)?,
                    map_x: row.get(4)?,
                    map_y: row.get(5)?,
                    line_color_override: row.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(systems)
    }

    pub fn create_system(
        &self,
        name: &str,
        description: &str,
        parent_id: Option<i64>,
    ) -> Result<i64> {
        self.conn.execute(
            r#"
            INSERT INTO systems (name, description, parent_id)
            VALUES (?1, ?2, ?3)
            "#,
            params![name, description, parent_id],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_system_parent(&self, system_id: i64, parent_id: Option<i64>) -> Result<()> {
        self.conn.execute(
            r#"
            UPDATE systems
            SET parent_id = ?2
            WHERE id = ?1
            "#,
            params![system_id, parent_id],
        )?;

        Ok(())
    }

    pub fn delete_system(&self, system_id: i64) -> Result<()> {
        self.conn.execute(
            r#"
            DELETE FROM systems
            WHERE id = ?1
            "#,
            params![system_id],
        )?;

        Ok(())
    }

    pub fn update_system_position(&self, system_id: i64, map_x: f64, map_y: f64) -> Result<()> {
        self.conn.execute(
            r#"
            UPDATE systems
            SET map_x = ?2, map_y = ?3
            WHERE id = ?1
            "#,
            params![system_id, map_x, map_y],
        )?;

        Ok(())
    }

    pub fn clear_system_positions(&self) -> Result<()> {
        self.conn.execute(
            r#"
            UPDATE systems
            SET map_x = NULL, map_y = NULL
            "#,
            [],
        )?;

        Ok(())
    }

    pub fn update_system_line_color_override(
        &self,
        system_id: i64,
        line_color_override: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            UPDATE systems
            SET line_color_override = ?2
            WHERE id = ?1
            "#,
            params![system_id, line_color_override],
        )?;

        Ok(())
    }

    pub fn list_links_for_system(&self, system_id: i64) -> Result<Vec<SystemLink>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, source_system_id, target_system_id, label
            FROM links
            WHERE source_system_id = ?1 OR target_system_id = ?1
            ORDER BY id DESC
            "#,
        )?;

        let links = stmt
            .query_map(params![system_id], |row| {
                Ok(SystemLink {
                    id: row.get(0)?,
                    source_system_id: row.get(1)?,
                    target_system_id: row.get(2)?,
                    label: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(links)
    }

    pub fn create_link(
        &self,
        source_system_id: i64,
        target_system_id: i64,
        label: &str,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO links (source_system_id, target_system_id, label)
            VALUES (?1, ?2, ?3)
            "#,
            params![source_system_id, target_system_id, label],
        )?;
        Ok(())
    }

    pub fn update_link_label(&self, link_id: i64, label: &str) -> Result<()> {
        self.conn.execute(
            r#"
            UPDATE links
            SET label = ?2
            WHERE id = ?1
            "#,
            params![link_id, label],
        )?;

        Ok(())
    }

    pub fn delete_link(&self, link_id: i64) -> Result<()> {
        self.conn.execute(
            r#"
            DELETE FROM links
            WHERE id = ?1
            "#,
            params![link_id],
        )?;

        Ok(())
    }

    pub fn list_links(&self) -> Result<Vec<SystemLink>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, source_system_id, target_system_id, label
            FROM links
            ORDER BY id DESC
            "#,
        )?;

        let links = stmt
            .query_map([], |row| {
                Ok(SystemLink {
                    id: row.get(0)?,
                    source_system_id: row.get(1)?,
                    target_system_id: row.get(2)?,
                    label: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(links)
    }

    pub fn list_notes_for_system(&self, system_id: i64) -> Result<Vec<SystemNote>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, body, updated_at
            FROM notes
            WHERE system_id = ?1
            ORDER BY updated_at DESC, id DESC
            "#,
        )?;

        let notes = stmt
            .query_map(params![system_id], |row| {
                Ok(SystemNote {
                    id: row.get(0)?,
                    body: row.get(1)?,
                    updated_at: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(notes)
    }

    pub fn create_note(&self, system_id: i64, body: &str) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO notes (system_id, body, updated_at)
            VALUES (?1, ?2, CURRENT_TIMESTAMP)
            "#,
            params![system_id, body],
        )?;

        Ok(())
    }

    pub fn update_note(&self, note_id: i64, body: &str) -> Result<()> {
        self.conn.execute(
            r#"
            UPDATE notes
            SET body = ?2,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?1
            "#,
            params![note_id, body],
        )?;

        Ok(())
    }

    pub fn delete_note(&self, note_id: i64) -> Result<()> {
        self.conn.execute(
            r#"
            DELETE FROM notes
            WHERE id = ?1
            "#,
            params![note_id],
        )?;

        Ok(())
    }

    pub fn export_catalog_to_path(&self, path: &str) -> Result<()> {
        self.conn.execute("VACUUM INTO ?1", params![path])?;
        Ok(())
    }

    pub fn import_catalog_from_path(&self, path: &str) -> Result<()> {
        self.conn.execute_batch("PRAGMA foreign_keys = OFF;")?;

        let result = (|| -> Result<()> {
            self.conn
                .execute("ATTACH DATABASE ?1 AS imported", params![path])?;

            self.conn.execute("DELETE FROM system_tech", [])?;
            self.conn.execute("DELETE FROM links", [])?;
            self.conn.execute("DELETE FROM notes", [])?;
            self.conn.execute("DELETE FROM systems", [])?;
            self.conn.execute("DELETE FROM tech_catalog", [])?;
            self.conn.execute("DELETE FROM app_settings", [])?;

            self.conn.execute(
                "INSERT INTO systems (id, name, description, parent_id, map_x, map_y, line_color_override) SELECT id, name, description, parent_id, map_x, map_y, line_color_override FROM imported.systems",
                [],
            )?;
            self.conn.execute(
                "INSERT INTO links (id, source_system_id, target_system_id, label) SELECT id, source_system_id, target_system_id, label FROM imported.links",
                [],
            )?;
            self.conn.execute(
                "INSERT INTO notes (id, system_id, body, updated_at) SELECT id, system_id, body, updated_at FROM imported.notes",
                [],
            )?;
            self.conn.execute(
                "INSERT INTO tech_catalog (id, name, description, documentation_link) SELECT id, name, description, documentation_link FROM imported.tech_catalog",
                [],
            )?;
            self.conn.execute(
                "INSERT INTO system_tech (system_id, tech_id) SELECT system_id, tech_id FROM imported.system_tech",
                [],
            )?;
            self.conn.execute(
                "INSERT INTO app_settings (key, value) SELECT key, value FROM imported.app_settings",
                [],
            )?;

            self.conn.execute("DETACH DATABASE imported", [])?;
            Ok(())
        })();

        self.conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        result
    }

    pub fn clear_catalog_data(&self) -> Result<()> {
        self.conn.execute("DELETE FROM system_tech", [])?;
        self.conn.execute("DELETE FROM links", [])?;
        self.conn.execute("DELETE FROM notes", [])?;
        self.conn.execute("DELETE FROM systems", [])?;
        self.conn.execute("DELETE FROM tech_catalog", [])?;
        Ok(())
    }

    pub fn list_tech_catalog(&self) -> Result<Vec<TechItem>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, description, documentation_link
            FROM tech_catalog
            ORDER BY LOWER(name)
            "#,
        )?;

        let technologies = stmt
            .query_map([], |row| {
                Ok(TechItem {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    documentation_link: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(technologies)
    }

    pub fn create_tech_item(
        &self,
        name: &str,
        description: Option<&str>,
        documentation_link: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO tech_catalog (name, description, documentation_link)
            VALUES (?1, ?2, ?3)
            "#,
            params![name, description, documentation_link],
        )?;

        Ok(())
    }

    pub fn update_tech_item(
        &self,
        tech_id: i64,
        name: &str,
        description: Option<&str>,
        documentation_link: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            UPDATE tech_catalog
            SET name = ?2,
                description = ?3,
                documentation_link = ?4
            WHERE id = ?1
            "#,
            params![tech_id, name, description, documentation_link],
        )?;

        Ok(())
    }

    pub fn delete_tech_item(&self, tech_id: i64) -> Result<()> {
        self.conn.execute(
            r#"
            DELETE FROM tech_catalog
            WHERE id = ?1
            "#,
            params![tech_id],
        )?;

        Ok(())
    }

    pub fn add_tech_to_system(&self, system_id: i64, tech_id: i64) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO system_tech (system_id, tech_id)
            VALUES (?1, ?2)
            "#,
            params![system_id, tech_id],
        )?;

        Ok(())
    }

    pub fn remove_tech_from_system(&self, system_id: i64, tech_id: i64) -> Result<()> {
        self.conn.execute(
            r#"
            DELETE FROM system_tech
            WHERE system_id = ?1 AND tech_id = ?2
            "#,
            params![system_id, tech_id],
        )?;

        Ok(())
    }

    pub fn list_tech_for_system(&self, system_id: i64) -> Result<Vec<TechItem>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT tc.id, tc.name, tc.description, tc.documentation_link
            FROM system_tech st
            JOIN tech_catalog tc ON tc.id = st.tech_id
            WHERE st.system_id = ?1
            ORDER BY LOWER(tc.name)
            "#,
        )?;

        let technologies = stmt
            .query_map(params![system_id], |row| {
                Ok(TechItem {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    documentation_link: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(technologies)
    }

    pub fn list_system_ids_for_tech(&self, tech_id: i64) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT system_id
            FROM system_tech
            WHERE tech_id = ?1
            "#,
        )?;

        let system_ids = stmt
            .query_map(params![tech_id], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(system_ids)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO app_settings (key, value)
            VALUES (?1, ?2)
            ON CONFLICT(key)
            DO UPDATE SET value = excluded.value
            "#,
            params![key, value],
        )?;

        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT value
            FROM app_settings
            WHERE key = ?1
            "#,
        )?;

        let value = stmt
            .query_row(params![key], |row| row.get::<_, String>(0))
            .optional()?;

        Ok(value)
    }
}
