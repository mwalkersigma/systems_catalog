/// A single system in the catalog.
/// TypeScript analogy: this is similar to a simple `interface` used as a data transfer shape.
#[derive(Debug, Clone)]
pub struct SystemRecord {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub parent_id: Option<i64>,
    pub map_x: Option<f32>,
    pub map_y: Option<f32>,
    pub line_color_override: Option<String>,
    pub naming_root: bool,
    pub naming_delimiter: String,
}

/// Directed interaction from one system to another.
/// TypeScript analogy: imagine an edge in a graph where `source_system_id -> target_system_id`.
#[derive(Debug, Clone)]
pub struct SystemLink {
    pub id: i64,
    pub source_system_id: i64,
    pub target_system_id: i64,
    pub label: String,
    pub note: String,
    pub kind: String,
}

/// Notes attached to one system.
/// TypeScript analogy: a one-to-one relation (`system_id`) that stores freeform text.
#[derive(Debug, Clone)]
pub struct SystemNote {
    pub id: i64,
    pub body: String,
    pub updated_at: String,
}

/// Reusable technology catalog item (e.g. Rust, PostgreSQL, Redis).
/// TypeScript analogy: a shared lookup table that systems can reference by `tech_id`.
#[derive(Debug, Clone)]
pub struct TechItem {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub documentation_link: Option<String>,
    pub color: Option<String>,
    pub display_priority: i64,
}
