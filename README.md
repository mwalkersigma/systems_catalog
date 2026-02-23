# Systems Catalog (Rust Desktop App)

Systems Catalog is a desktop GUI app for documenting the systems you maintain, linking systems together, and storing notes per system.

## Stack

- **GUI:** `eframe` / `egui`
- **Database:** SQLite via `rusqlite` (bundled SQLite)
- **Error handling:** `anyhow`

## Why this structure (learning-first)

The code is split into small modules with clear responsibilities:

- `src/models.rs` — data shapes (`SystemRecord`, `SystemLink`, `SystemNote`)
- `src/db.rs` — repository/data access layer and schema initialization
- `src/app.rs` — GUI state + rendering logic
- `src/main.rs` — startup/bootstrap

### TypeScript mental model

- `models.rs` ≈ TypeScript `interface`/`type` definitions
- `db.rs` ≈ service/repository class (e.g. `SystemsRepository`)
- `app.rs` ≈ stateful UI component (like a React container) but explicit and strongly typed

## Features implemented

- Dark theme by default
- Create systems with optional parent system
- Browse systems in hierarchy view
- Select a system and view details
- Create directed interactions (links) between systems
- Maintain a reusable tech catalog (create once, reuse across systems)
- Assign tech stack items to each system
- Show cumulative deduplicated tech used by all child/descendant systems
- Write and save notes for each system

## Data model

SQLite tables:

- `systems`
  - `id` (PK)
  - `name` (unique)
  - `description`
  - `parent_id` (nullable FK to `systems.id`)
- `links`
  - `id` (PK)
  - `source_system_id` (FK)
  - `target_system_id` (FK)
  - `label`
  - unique pair (`source_system_id`, `target_system_id`)
- `notes`
  - `system_id` (PK + FK)
  - `body`
  - `updated_at`
- `tech_catalog`
  - `id` (PK)
  - `name` (unique)
- `system_tech`
  - `system_id` (FK)
  - `tech_id` (FK)
  - primary key (`system_id`, `tech_id`)

## Run

```bash
cargo run
```

This creates/uses `systems_catalog.db` in the project root.

## Quality checks

```bash
cargo check
cargo fmt --check
```
