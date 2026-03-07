use std::path::Path;

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};

use crate::models::{
    DatabaseColumnInput, DatabaseColumnRecord, SystemLink, SystemNote, SystemRecord, TechItem,
    ZoneRecord, ZoneSystemOffset,
};

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
                display_name TEXT NULL,
                description TEXT NOT NULL DEFAULT '',
                parent_id INTEGER NULL,
                map_x REAL NULL,
                map_y REAL NULL,
                line_color_override TEXT NULL,
                naming_root INTEGER NOT NULL DEFAULT 0,
                naming_delimiter TEXT NOT NULL DEFAULT '/',
                system_type TEXT NOT NULL DEFAULT 'service',
                route_methods TEXT NULL,
                FOREIGN KEY(parent_id) REFERENCES systems(id) ON DELETE SET NULL
            );

            CREATE TABLE IF NOT EXISTS links (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_system_id INTEGER NOT NULL,
                target_system_id INTEGER NOT NULL,
                label TEXT NOT NULL DEFAULT '',
                note TEXT NOT NULL DEFAULT '',
                kind TEXT NOT NULL DEFAULT 'standard',
                source_column_name TEXT NULL,
                target_column_name TEXT NULL,
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
                documentation_link TEXT NULL,
                color TEXT NULL,
                display_priority INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS system_tech (
                system_id INTEGER NOT NULL,
                tech_id INTEGER NOT NULL,
                PRIMARY KEY(system_id, tech_id),
                FOREIGN KEY(system_id) REFERENCES systems(id) ON DELETE CASCADE,
                FOREIGN KEY(tech_id) REFERENCES tech_catalog(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS database_columns (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                system_id INTEGER NOT NULL,
                position INTEGER NOT NULL DEFAULT 0,
                column_name TEXT NOT NULL,
                column_type TEXT NOT NULL,
                constraints TEXT NULL,
                FOREIGN KEY(system_id) REFERENCES systems(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS zones (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                x REAL NOT NULL,
                y REAL NOT NULL,
                width REAL NOT NULL,
                height REAL NOT NULL,
                color TEXT NULL,
                render_priority INTEGER NOT NULL DEFAULT 1,
                parent_zone_id INTEGER NULL,
                minimized INTEGER NOT NULL DEFAULT 0,
                representative_system_id INTEGER NULL,
                FOREIGN KEY(parent_zone_id) REFERENCES zones(id) ON DELETE SET NULL,
                FOREIGN KEY(representative_system_id) REFERENCES systems(id) ON DELETE SET NULL
            );

            CREATE TABLE IF NOT EXISTS zone_system_offsets (
                zone_id INTEGER NOT NULL,
                system_id INTEGER NOT NULL,
                offset_x REAL NOT NULL,
                offset_y REAL NOT NULL,
                PRIMARY KEY(system_id),
                FOREIGN KEY(zone_id) REFERENCES zones(id) ON DELETE CASCADE,
                FOREIGN KEY(system_id) REFERENCES systems(id) ON DELETE CASCADE
            );
            "#,
        )?;

        self.ensure_systems_position_columns()?;
        self.ensure_system_line_color_override_column()?;
        self.ensure_system_naming_columns()?;
        self.ensure_system_type_columns()?;
        self.ensure_links_note_column()?;
        self.ensure_links_kind_column()?;
        self.ensure_links_column_reference_columns()?;
        self.ensure_tech_catalog_columns()?;
        self.ensure_tech_catalog_visual_columns()?;
        self.ensure_default_tech_catalog()?;
        self.ensure_notes_table_shape()?;
        self.ensure_zones_columns()?;

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

    fn ensure_system_naming_columns(&self) -> Result<()> {
        if !self.table_has_column("systems", "display_name")? {
            self.conn
                .execute("ALTER TABLE systems ADD COLUMN display_name TEXT NULL", [])?;
        }

        if !self.table_has_column("systems", "naming_root")? {
            self.conn.execute(
                "ALTER TABLE systems ADD COLUMN naming_root INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }

        if !self.table_has_column("systems", "naming_delimiter")? {
            self.conn.execute(
                "ALTER TABLE systems ADD COLUMN naming_delimiter TEXT NOT NULL DEFAULT '/'",
                [],
            )?;
        }

        self.conn.execute(
            "UPDATE systems SET display_name = name WHERE display_name IS NULL OR TRIM(display_name) = ''",
            [],
        )?;

        Ok(())
    }

    fn ensure_system_type_columns(&self) -> Result<()> {
        if !self.table_has_column("systems", "system_type")? {
            self.conn.execute(
                "ALTER TABLE systems ADD COLUMN system_type TEXT NOT NULL DEFAULT 'service'",
                [],
            )?;
        }

        if !self.table_has_column("systems", "route_methods")? {
            self.conn
                .execute("ALTER TABLE systems ADD COLUMN route_methods TEXT NULL", [])?;
        }

        Ok(())
    }

    fn ensure_links_note_column(&self) -> Result<()> {
        if !self.table_has_column("links", "note")? {
            self.conn
                .execute("ALTER TABLE links ADD COLUMN note TEXT NOT NULL DEFAULT ''", [])?;
        }

        Ok(())
    }

    fn ensure_links_kind_column(&self) -> Result<()> {
        if !self.table_has_column("links", "kind")? {
            self.conn.execute(
                "ALTER TABLE links ADD COLUMN kind TEXT NOT NULL DEFAULT 'standard'",
                [],
            )?;
        }

        Ok(())
    }

    fn ensure_links_column_reference_columns(&self) -> Result<()> {
        if !self.table_has_column("links", "source_column_name")? {
            self.conn
                .execute("ALTER TABLE links ADD COLUMN source_column_name TEXT NULL", [])?;
        }

        if !self.table_has_column("links", "target_column_name")? {
            self.conn
                .execute("ALTER TABLE links ADD COLUMN target_column_name TEXT NULL", [])?;
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

    fn ensure_tech_catalog_visual_columns(&self) -> Result<()> {
        if !self.table_has_column("tech_catalog", "color")? {
            self.conn
                .execute("ALTER TABLE tech_catalog ADD COLUMN color TEXT NULL", [])?;
        }

        if !self.table_has_column("tech_catalog", "display_priority")? {
            self.conn.execute(
                "ALTER TABLE tech_catalog ADD COLUMN display_priority INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }

        Ok(())
    }

    fn ensure_default_tech_catalog(&self) -> Result<()> {
        let defaults = [
            (
                "JavaScript",
                "Core web scripting language used across frontend and backend runtimes.",
                "https://developer.mozilla.org/en-US/docs/Web/JavaScript",
                "247,223,30,255",
                12,
            ),
            (
                "TypeScript",
                "Typed superset of JavaScript for safer large-scale application development.",
                "https://www.typescriptlang.org/docs/",
                "49,120,198,255",
                12,
            ),
            (
                "TSX",
                "TypeScript syntax extension for authoring typed React UI components.",
                "https://www.typescriptlang.org/docs/handbook/jsx.html",
                "49,120,198,255",
                10,
            ),
            (
                "JSX",
                "JavaScript syntax extension commonly used to compose React component trees.",
                "https://react.dev/learn/writing-markup-with-jsx",
                "97,218,251,255",
                10,
            ),
            (
                "React",
                "Component-driven UI library for building interactive frontend applications.",
                "https://react.dev/",
                "97,218,251,255",
                11,
            ),
            (
                "GoLand",
                "JetBrains IDE tailored for Go development, debugging, and refactoring.",
                "https://www.jetbrains.com/go/",
                "0,189,165,255",
                7,
            ),
            (
                "Postgres",
                "Robust relational database with strong SQL support and JSON capabilities.",
                "https://www.postgresql.org/docs/",
                "51,103,145,255",
                11,
            ),
            (
                "Rust",
                "Systems programming language focused on memory safety and high performance.",
                "https://www.rust-lang.org/learn",
                "222,165,132,255",
                10,
            ),
            (
                "Go",
                "Compiled language optimized for backend services, tooling, and concurrency.",
                "https://go.dev/doc/",
                "0,173,216,255",
                10,
            ),
            (
                "Python",
                "General-purpose language popular for automation, APIs, and data workflows.",
                "https://docs.python.org/3/",
                "55,118,171,255",
                10,
            ),
            (
                "Node.js",
                "JavaScript runtime for server-side development and tooling ecosystems.",
                "https://nodejs.org/docs/latest/api/",
                "104,160,99,255",
                10,
            ),
            (
                "Bun",
                "Fast JavaScript and TypeScript runtime with integrated bundling and tooling.",
                "https://bun.sh/docs",
                "251,248,232,255",
                8,
            ),
            (
                "Deno",
                "Secure JavaScript and TypeScript runtime with modern built-in web APIs.",
                "https://docs.deno.com/runtime/manual/",
                "0,0,0,255",
                8,
            ),
            (
                "C#",
                "Modern language on .NET for APIs, desktop apps, and cloud services.",
                "https://learn.microsoft.com/dotnet/csharp/",
                "104,33,122,255",
                9,
            ),
            (
                "Java",
                "Widely used JVM language for enterprise services and Android foundations.",
                "https://docs.oracle.com/en/java/",
                "237,139,0,255",
                9,
            ),
            (
                "Kotlin",
                "Concise JVM language used for Android and server-side applications.",
                "https://kotlinlang.org/docs/home.html",
                "127,82,255,255",
                8,
            ),
            (
                "Swift",
                "Primary language for Apple platform application development.",
                "https://docs.swift.org/swift-book/",
                "250,105,45,255",
                8,
            ),
            (
                "SQL",
                "Declarative query language for relational data modeling and retrieval.",
                "https://www.postgresql.org/docs/current/sql.html",
                "51,103,145,255",
                9,
            ),
            (
                "GraphQL",
                "API query language for flexible, client-driven data fetching.",
                "https://graphql.org/learn/",
                "225,0,152,255",
                8,
            ),
            (
                "REST",
                "HTTP architectural style for resource-oriented service communication.",
                "https://restfulapi.net/",
                "0,122,204,255",
                8,
            ),
        ];

        for (name, description, documentation_link, color, display_priority) in defaults {
            self.conn.execute(
                r#"
                INSERT OR IGNORE INTO tech_catalog (name, description, documentation_link, color, display_priority)
                VALUES (?1, ?2, ?3, ?4, ?5)
                "#,
                params![name, description, documentation_link, color, display_priority],
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

    fn ensure_zones_columns(&self) -> Result<()> {
        if !self.table_has_column("zones", "render_priority")? {
            self.conn.execute(
                "ALTER TABLE zones ADD COLUMN render_priority INTEGER NOT NULL DEFAULT 1",
                [],
            )?;
        }

        if !self.table_has_column("zones", "parent_zone_id")? {
            self.conn
                .execute("ALTER TABLE zones ADD COLUMN parent_zone_id INTEGER NULL", [])?;
        }

        if !self.table_has_column("zones", "minimized")? {
            self.conn.execute(
                "ALTER TABLE zones ADD COLUMN minimized INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }

        if !self.table_has_column("zones", "representative_system_id")? {
            self.conn.execute(
                "ALTER TABLE zones ADD COLUMN representative_system_id INTEGER NULL",
                [],
            )?;
        }

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

    fn attached_table_has_column(
        &self,
        schema_name: &str,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info(?1, ?2) WHERE name = ?3",
            params![table_name, schema_name, column_name],
            |row| row.get(0),
        )?;

        Ok(count > 0)
    }

    pub fn list_systems(&self) -> Result<Vec<SystemRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                id,
                COALESCE(NULLIF(display_name, ''), name) AS display_name,
                description,
                parent_id,
                map_x,
                map_y,
                line_color_override,
                naming_root,
                naming_delimiter,
                system_type,
                route_methods
            FROM systems
            ORDER BY LOWER(COALESCE(NULLIF(display_name, ''), name));
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
                    naming_root: row.get::<_, i64>(7)? != 0,
                    naming_delimiter: row.get(8)?,
                    system_type: row.get(9)?,
                    route_methods: row.get(10)?,
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
        system_type: &str,
        route_methods: Option<&str>,
    ) -> Result<i64> {
        let mut unique_internal_name = name.trim().to_owned();
        if unique_internal_name.is_empty() {
            unique_internal_name = "system".to_owned();
        }

        let mut dedupe_suffix: i64 = 2;
        while self
            .conn
            .query_row(
                "SELECT 1 FROM systems WHERE name = ?1 LIMIT 1",
                params![unique_internal_name.as_str()],
                |_| Ok(()),
            )
            .optional()?
            .is_some()
        {
            unique_internal_name = format!("{}-{}", name.trim(), dedupe_suffix);
            dedupe_suffix += 1;
        }

        self.conn.execute(
            r#"
            INSERT INTO systems (
                name,
                display_name,
                description,
                parent_id,
                naming_root,
                naming_delimiter,
                system_type,
                route_methods
            )
            VALUES (?1, ?2, ?3, ?4, 0, '/', ?5, ?6)
            "#,
            params![
                unique_internal_name,
                name,
                description,
                parent_id,
                system_type,
                route_methods
            ],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    pub fn insert_system_with_id(
        &self,
        id: i64,
        name: &str,
        description: &str,
        parent_id: Option<i64>,
        map_x: Option<f32>,
        map_y: Option<f32>,
        line_color_override: Option<&str>,
        naming_root: bool,
        naming_delimiter: &str,
        system_type: &str,
        route_methods: Option<&str>,
    ) -> Result<()> {
        let display_name = {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                format!("System {id}")
            } else {
                trimmed.to_owned()
            }
        };
        let internal_name = format!("sys_{id}");

        self.conn.execute(
            r#"
            INSERT INTO systems (
                id,
                name,
                display_name,
                description,
                parent_id,
                map_x,
                map_y,
                line_color_override,
                naming_root,
                naming_delimiter,
                system_type,
                route_methods
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                id,
                internal_name,
                display_name,
                description,
                parent_id,
                map_x,
                map_y,
                line_color_override,
                if naming_root { 1 } else { 0 },
                naming_delimiter,
                system_type,
                route_methods
            ],
        )?;

        Ok(())
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

    pub fn update_system_details(
        &self,
        system_id: i64,
        display_name: &str,
        description: &str,
        naming_root: bool,
        naming_delimiter: &str,
        system_type: &str,
        route_methods: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            UPDATE systems
            SET display_name = ?2,
                description = ?3,
                naming_root = ?4,
                naming_delimiter = ?5,
                system_type = ?6,
                route_methods = ?7
            WHERE id = ?1
            "#,
            params![
                system_id,
                display_name,
                description,
                if naming_root { 1 } else { 0 },
                naming_delimiter,
                system_type,
                route_methods
            ],
        )?;

        Ok(())
    }

    pub fn list_database_columns(&self) -> Result<Vec<DatabaseColumnRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, system_id, position, column_name, column_type, constraints
            FROM database_columns
            ORDER BY system_id ASC, position ASC, id ASC
            "#,
        )?;

        let columns = stmt
            .query_map([], |row| {
                Ok(DatabaseColumnRecord {
                    system_id: row.get(1)?,
                    position: row.get(2)?,
                    column_name: row.get(3)?,
                    column_type: row.get(4)?,
                    constraints: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(columns)
    }

    pub fn replace_database_columns_for_system(
        &mut self,
        system_id: i64,
        columns: &[DatabaseColumnInput],
    ) -> Result<()> {
        let tx = self.conn.transaction()?;

        tx.execute(
            "DELETE FROM database_columns WHERE system_id = ?1",
            params![system_id],
        )?;

        for column in columns {
            tx.execute(
                r#"
                INSERT INTO database_columns (system_id, position, column_name, column_type, constraints)
                VALUES (?1, ?2, ?3, ?4, ?5)
                "#,
                params![
                    system_id,
                    column.position,
                    column.column_name,
                    column.column_type,
                    column.constraints,
                ],
            )?;
        }

        tx.commit()?;
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

    pub fn update_system_position_optional(
        &self,
        system_id: i64,
        map_x: Option<f32>,
        map_y: Option<f32>,
    ) -> Result<()> {
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

    #[allow(dead_code)]
    pub fn list_links_for_system(&self, system_id: i64) -> Result<Vec<SystemLink>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, source_system_id, target_system_id, label, note, kind, source_column_name, target_column_name
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
                    note: row.get(4)?,
                    kind: row.get(5)?,
                    source_column_name: row.get(6)?,
                    target_column_name: row.get(7)?,
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
        kind: &str,
        source_column_name: Option<&str>,
        target_column_name: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO links (source_system_id, target_system_id, label, note, kind, source_column_name, target_column_name)
            VALUES (?1, ?2, ?3, '', ?4, ?5, ?6)
            "#,
            params![
                source_system_id,
                target_system_id,
                label,
                kind,
                source_column_name,
                target_column_name
            ],
        )?;
        Ok(())
    }

    pub fn insert_link_with_id(
        &self,
        id: i64,
        source_system_id: i64,
        target_system_id: i64,
        label: &str,
        note: &str,
        kind: &str,
        source_column_name: Option<&str>,
        target_column_name: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO links (
                id,
                source_system_id,
                target_system_id,
                label,
                note,
                kind,
                source_column_name,
                target_column_name
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                id,
                source_system_id,
                target_system_id,
                label,
                note,
                kind,
                source_column_name,
                target_column_name
            ],
        )?;
        Ok(())
    }

    pub fn update_link_details(
        &self,
        link_id: i64,
        label: &str,
        note: &str,
        kind: &str,
        source_column_name: Option<&str>,
        target_column_name: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            UPDATE links
            SET label = ?2,
                note = ?3,
                kind = ?4,
                source_column_name = ?5,
                target_column_name = ?6
            WHERE id = ?1
            "#,
            params![
                link_id,
                label,
                note,
                kind,
                source_column_name,
                target_column_name
            ],
        )?;

        Ok(())
    }

    pub fn update_link_endpoints(
        &self,
        link_id: i64,
        source_system_id: i64,
        target_system_id: i64,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            UPDATE links
            SET source_system_id = ?2,
                target_system_id = ?3
            WHERE id = ?1
            "#,
            params![link_id, source_system_id, target_system_id],
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
            SELECT id, source_system_id, target_system_id, label, note, kind, source_column_name, target_column_name
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
                    note: row.get(4)?,
                    kind: row.get(5)?,
                    source_column_name: row.get(6)?,
                    target_column_name: row.get(7)?,
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

    pub fn insert_note_with_id(
        &self,
        id: i64,
        system_id: i64,
        body: &str,
        updated_at: &str,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO notes (id, system_id, body, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            "#,
            params![id, system_id, body, updated_at],
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

    pub fn delete_notes_for_system(&self, system_id: i64) -> Result<()> {
        self.conn.execute(
            r#"
            DELETE FROM notes
            WHERE system_id = ?1
            "#,
            params![system_id],
        )?;

        Ok(())
    }

    pub fn replace_system_tech_assignments(&self, system_id: i64, tech_ids: &[i64]) -> Result<()> {
        self.conn.execute(
            r#"
            DELETE FROM system_tech
            WHERE system_id = ?1
            "#,
            params![system_id],
        )?;

        for tech_id in tech_ids {
            self.conn.execute(
                r#"
                INSERT INTO system_tech (system_id, tech_id)
                VALUES (?1, ?2)
                "#,
                params![system_id, tech_id],
            )?;
        }

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
            self.conn.execute("DELETE FROM database_columns", [])?;
            self.conn.execute("DELETE FROM systems", [])?;
            self.conn.execute("DELETE FROM tech_catalog", [])?;
            self.conn.execute("DELETE FROM app_settings", [])?;
            self.conn.execute("DELETE FROM zones", [])?;
            self.conn.execute("DELETE FROM zone_system_offsets", [])?;

            let imported_has_systems_display_name =
                self.attached_table_has_column("imported", "systems", "display_name")?;
            let imported_has_systems_map_x =
                self.attached_table_has_column("imported", "systems", "map_x")?;
            let imported_has_systems_map_y =
                self.attached_table_has_column("imported", "systems", "map_y")?;
            let imported_has_systems_line_color_override =
                self.attached_table_has_column("imported", "systems", "line_color_override")?;
            let imported_has_systems_naming_root =
                self.attached_table_has_column("imported", "systems", "naming_root")?;
            let imported_has_systems_naming_delimiter =
                self.attached_table_has_column("imported", "systems", "naming_delimiter")?;
            let imported_has_systems_system_type =
                self.attached_table_has_column("imported", "systems", "system_type")?;
            let imported_has_systems_route_methods =
                self.attached_table_has_column("imported", "systems", "route_methods")?;

            let display_name_select = if imported_has_systems_display_name {
                "display_name"
            } else {
                "name"
            };
            let map_x_select = if imported_has_systems_map_x { "map_x" } else { "NULL" };
            let map_y_select = if imported_has_systems_map_y { "map_y" } else { "NULL" };
            let line_color_override_select = if imported_has_systems_line_color_override {
                "line_color_override"
            } else {
                "NULL"
            };
            let naming_root_select = if imported_has_systems_naming_root {
                "naming_root"
            } else {
                "0"
            };
            let naming_delimiter_select = if imported_has_systems_naming_delimiter {
                "naming_delimiter"
            } else {
                "'/'"
            };
            let system_type_select = if imported_has_systems_system_type {
                "system_type"
            } else {
                "'service'"
            };
            let route_methods_select = if imported_has_systems_route_methods {
                "route_methods"
            } else {
                "NULL"
            };

            let systems_insert_sql = format!(
                "INSERT INTO systems (id, name, display_name, description, parent_id, map_x, map_y, line_color_override, naming_root, naming_delimiter, system_type, route_methods) SELECT id, name, {display_name_select}, description, parent_id, {map_x_select}, {map_y_select}, {line_color_override_select}, {naming_root_select}, {naming_delimiter_select}, {system_type_select}, {route_methods_select} FROM imported.systems"
            );
            self.conn.execute(systems_insert_sql.as_str(), [])?;

            let imported_has_links_note =
                self.attached_table_has_column("imported", "links", "note")?;
            let imported_has_links_kind =
                self.attached_table_has_column("imported", "links", "kind")?;
            let imported_has_links_source_column_name =
                self.attached_table_has_column("imported", "links", "source_column_name")?;
            let imported_has_links_target_column_name =
                self.attached_table_has_column("imported", "links", "target_column_name")?;
            let note_select = if imported_has_links_note { "note" } else { "''" };
            let kind_select = if imported_has_links_kind {
                "kind"
            } else {
                "'standard'"
            };
            let source_column_select = if imported_has_links_source_column_name {
                "source_column_name"
            } else {
                "NULL"
            };
            let target_column_select = if imported_has_links_target_column_name {
                "target_column_name"
            } else {
                "NULL"
            };

            let links_insert_sql = format!(
                "INSERT INTO links (id, source_system_id, target_system_id, label, note, kind, source_column_name, target_column_name) SELECT id, source_system_id, target_system_id, label, {note_select}, {kind_select}, {source_column_select}, {target_column_select} FROM imported.links"
            );
            self.conn.execute(links_insert_sql.as_str(), [])?;
            self.conn.execute(
                "INSERT INTO notes (id, system_id, body, updated_at) SELECT id, system_id, body, updated_at FROM imported.notes",
                [],
            )?;
            let imported_has_tech_description =
                self.attached_table_has_column("imported", "tech_catalog", "description")?;
            let imported_has_tech_documentation_link = self.attached_table_has_column(
                "imported",
                "tech_catalog",
                "documentation_link",
            )?;
            let imported_has_tech_color =
                self.attached_table_has_column("imported", "tech_catalog", "color")?;
            let imported_has_tech_display_priority = self.attached_table_has_column(
                "imported",
                "tech_catalog",
                "display_priority",
            )?;

            let tech_description_select = if imported_has_tech_description {
                "description"
            } else {
                "NULL"
            };
            let tech_documentation_link_select = if imported_has_tech_documentation_link {
                "documentation_link"
            } else {
                "NULL"
            };
            let tech_color_select = if imported_has_tech_color {
                "color"
            } else {
                "NULL"
            };
            let tech_display_priority_select = if imported_has_tech_display_priority {
                "display_priority"
            } else {
                "0"
            };

            let tech_insert_sql = format!(
                "INSERT INTO tech_catalog (id, name, description, documentation_link, color, display_priority) SELECT id, name, {tech_description_select}, {tech_documentation_link_select}, {tech_color_select}, {tech_display_priority_select} FROM imported.tech_catalog"
            );
            self.conn.execute(tech_insert_sql.as_str(), [])?;
            self.conn.execute(
                "INSERT INTO system_tech (system_id, tech_id) SELECT system_id, tech_id FROM imported.system_tech",
                [],
            )?;

            let mut database_columns_table_stmt = self.conn.prepare(
                "SELECT COUNT(*) FROM imported.sqlite_master WHERE type = 'table' AND name = 'database_columns'",
            )?;
            let imported_has_database_columns: i64 =
                database_columns_table_stmt.query_row([], |row| row.get(0))?;

            if imported_has_database_columns > 0 {
                self.conn.execute(
                    "INSERT INTO database_columns (id, system_id, position, column_name, column_type, constraints) SELECT id, system_id, position, column_name, column_type, constraints FROM imported.database_columns",
                    [],
                )?;
            }

            self.conn.execute(
                "INSERT INTO app_settings (key, value) SELECT key, value FROM imported.app_settings",
                [],
            )?;

            let mut zone_table_stmt = self
                .conn
                .prepare("SELECT COUNT(*) FROM imported.sqlite_master WHERE type = 'table' AND name = 'zones'")?;
            let imported_has_zones: i64 = zone_table_stmt.query_row([], |row| row.get(0))?;

            if imported_has_zones > 0 {
                let imported_has_zone_priority =
                    self.attached_table_has_column("imported", "zones", "render_priority")?;
                let imported_has_zone_minimized =
                    self.attached_table_has_column("imported", "zones", "minimized")?;
                let imported_has_zone_parent =
                    self.attached_table_has_column("imported", "zones", "parent_zone_id")?;
                let imported_has_zone_representative = self.attached_table_has_column(
                    "imported",
                    "zones",
                    "representative_system_id",
                )?;

                let render_priority_select = if imported_has_zone_priority {
                    "render_priority"
                } else {
                    "1"
                };
                let minimized_select = if imported_has_zone_minimized {
                    "minimized"
                } else {
                    "0"
                };
                let parent_select = if imported_has_zone_parent {
                    "parent_zone_id"
                } else {
                    "NULL"
                };
                let representative_select = if imported_has_zone_representative {
                    "representative_system_id"
                } else {
                    "NULL"
                };

                let zone_insert_sql = format!(
                    "INSERT INTO zones (id, name, x, y, width, height, color, render_priority, parent_zone_id, minimized, representative_system_id) SELECT id, name, x, y, width, height, color, {render_priority_select}, {parent_select}, {minimized_select}, {representative_select} FROM imported.zones"
                );
                self.conn.execute(zone_insert_sql.as_str(), [])?;
            }

            let mut zone_offsets_table_stmt = self
                .conn
                .prepare("SELECT COUNT(*) FROM imported.sqlite_master WHERE type = 'table' AND name = 'zone_system_offsets'")?;
            let imported_has_zone_offsets: i64 = zone_offsets_table_stmt.query_row([], |row| row.get(0))?;

            if imported_has_zone_offsets > 0 {
                self.conn.execute(
                    "INSERT INTO zone_system_offsets (zone_id, system_id, offset_x, offset_y) SELECT zone_id, system_id, offset_x, offset_y FROM imported.zone_system_offsets",
                    [],
                )?;
            }

            Ok(())
        })();

        let _ = self.conn.execute("DETACH DATABASE imported", []);
        self.conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        result
    }

    pub fn clear_catalog_data(&self) -> Result<()> {
        self.conn.execute("DELETE FROM system_tech", [])?;
        self.conn.execute("DELETE FROM links", [])?;
        self.conn.execute("DELETE FROM notes", [])?;
        self.conn.execute("DELETE FROM database_columns", [])?;
        self.conn.execute("DELETE FROM systems", [])?;
        self.conn.execute("DELETE FROM tech_catalog", [])?;
        self.conn.execute("DELETE FROM zones", [])?;
        self.conn.execute("DELETE FROM zone_system_offsets", [])?;
        Ok(())
    }

    pub fn clear_non_system_catalog_data(&self) -> Result<()> {
        self.conn.execute("DELETE FROM system_tech", [])?;
        self.conn.execute("DELETE FROM links", [])?;
        self.conn.execute("DELETE FROM notes", [])?;
        self.conn.execute("DELETE FROM database_columns", [])?;
        self.conn.execute("DELETE FROM tech_catalog", [])?;
        self.conn.execute("DELETE FROM zones", [])?;
        self.conn.execute("DELETE FROM zone_system_offsets", [])?;
        Ok(())
    }

    pub fn list_zone_system_offsets(&self) -> Result<Vec<ZoneSystemOffset>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT zone_id, system_id, offset_x, offset_y
            FROM zone_system_offsets
            ORDER BY zone_id ASC, system_id ASC
            "#,
        )?;

        let offsets = stmt
            .query_map([], |row| {
                Ok(ZoneSystemOffset {
                    zone_id: row.get(0)?,
                    system_id: row.get(1)?,
                    offset_x: row.get(2)?,
                    offset_y: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(offsets)
    }

    pub fn upsert_zone_system_offset(
        &self,
        zone_id: i64,
        system_id: i64,
        offset_x: f32,
        offset_y: f32,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO zone_system_offsets (zone_id, system_id, offset_x, offset_y)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(system_id) DO UPDATE SET
                zone_id = excluded.zone_id,
                offset_x = excluded.offset_x,
                offset_y = excluded.offset_y
            "#,
            params![zone_id, system_id, offset_x, offset_y],
        )?;

        Ok(())
    }

    pub fn list_zones(&self) -> Result<Vec<ZoneRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, x, y, width, height, color, render_priority, parent_zone_id, minimized, representative_system_id
            FROM zones
            ORDER BY render_priority ASC, id ASC
            "#,
        )?;

        let zones = stmt
            .query_map([], |row| {
                Ok(ZoneRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    x: row.get(2)?,
                    y: row.get(3)?,
                    width: row.get(4)?,
                    height: row.get(5)?,
                    color: row.get(6)?,
                    render_priority: row.get(7)?,
                    parent_zone_id: row.get(8)?,
                    minimized: row.get::<_, i64>(9)? != 0,
                    representative_system_id: row.get(10)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(zones)
    }

    pub fn create_zone(
        &self,
        name: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Option<&str>,
        render_priority: i64,
        parent_zone_id: Option<i64>,
        minimized: bool,
        representative_system_id: Option<i64>,
    ) -> Result<i64> {
        self.conn.execute(
            r#"
            INSERT INTO zones (name, x, y, width, height, color, render_priority, parent_zone_id, minimized, representative_system_id)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            params![name, x, y, width, height, color, render_priority, parent_zone_id, if minimized { 1 } else { 0 }, representative_system_id],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    pub fn insert_zone_with_id(
        &self,
        id: i64,
        name: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Option<&str>,
        render_priority: i64,
        parent_zone_id: Option<i64>,
        minimized: bool,
        representative_system_id: Option<i64>,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO zones (
                id,
                name,
                x,
                y,
                width,
                height,
                color,
                render_priority,
                parent_zone_id,
                minimized,
                representative_system_id
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                id,
                name,
                x,
                y,
                width,
                height,
                color,
                render_priority,
                parent_zone_id,
                if minimized { 1 } else { 0 },
                representative_system_id
            ],
        )?;

        Ok(())
    }

    pub fn update_zone(
        &self,
        zone_id: i64,
        name: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Option<&str>,
        render_priority: i64,
        parent_zone_id: Option<i64>,
        minimized: bool,
        representative_system_id: Option<i64>,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            UPDATE zones
            SET name = ?2,
                x = ?3,
                y = ?4,
                width = ?5,
                height = ?6,
                color = ?7,
                render_priority = ?8,
                parent_zone_id = ?9,
                minimized = ?10,
                representative_system_id = ?11
            WHERE id = ?1
            "#,
            params![zone_id, name, x, y, width, height, color, render_priority, parent_zone_id, if minimized { 1 } else { 0 }, representative_system_id],
        )?;

        Ok(())
    }

    pub fn delete_zone(&self, zone_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM zones WHERE id = ?1", params![zone_id])?;
        Ok(())
    }

    pub fn list_tech_catalog(&self) -> Result<Vec<TechItem>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, description, documentation_link, color, display_priority
            FROM tech_catalog
            ORDER BY display_priority DESC, LOWER(name)
            "#,
        )?;

        let technologies = stmt
            .query_map([], |row| {
                Ok(TechItem {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    documentation_link: row.get(3)?,
                    color: row.get(4)?,
                    display_priority: row.get(5)?,
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
        color: Option<&str>,
        display_priority: i64,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO tech_catalog (name, description, documentation_link, color, display_priority)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![name, description, documentation_link, color, display_priority],
        )?;

        Ok(())
    }

    pub fn insert_tech_item_with_id(
        &self,
        id: i64,
        name: &str,
        description: Option<&str>,
        documentation_link: Option<&str>,
        color: Option<&str>,
        display_priority: i64,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO tech_catalog (
                id,
                name,
                description,
                documentation_link,
                color,
                display_priority
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                id,
                name,
                description,
                documentation_link,
                color,
                display_priority
            ],
        )?;

        Ok(())
    }

    pub fn update_tech_item(
        &self,
        tech_id: i64,
        name: &str,
        description: Option<&str>,
        documentation_link: Option<&str>,
        color: Option<&str>,
        display_priority: i64,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            UPDATE tech_catalog
            SET name = ?2,
                description = ?3,
                documentation_link = ?4,
                color = ?5,
                display_priority = ?6
            WHERE id = ?1
            "#,
            params![
                tech_id,
                name,
                description,
                documentation_link,
                color,
                display_priority
            ],
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
            SELECT tc.id, tc.name, tc.description, tc.documentation_link, tc.color, tc.display_priority
            FROM system_tech st
            JOIN tech_catalog tc ON tc.id = st.tech_id
            WHERE st.system_id = ?1
            ORDER BY tc.display_priority DESC, LOWER(tc.name)
            "#,
        )?;

        let technologies = stmt
            .query_map(params![system_id], |row| {
                Ok(TechItem {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    documentation_link: row.get(3)?,
                    color: row.get(4)?,
                    display_priority: row.get(5)?,
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

    pub fn list_system_tech_assignments(&self) -> Result<Vec<(i64, i64)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT system_id, tech_id
            FROM system_tech
            "#,
        )?;

        let assignments = stmt
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(assignments)
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

    pub fn delete_settings(&self, keys: &[&str]) -> Result<()> {
        for key in keys {
            self.conn.execute(
                r#"
                DELETE FROM app_settings
                WHERE key = ?1
                "#,
                params![key],
            )?;
        }

        Ok(())
    }
}
