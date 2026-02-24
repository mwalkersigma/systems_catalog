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
- `src/app.rs` — shared app state, synchronization logic, and helpers
- `src/app/actions.rs` — user actions (create system, assign tech, save notes, create links)
- `src/app/ui.rs` — rendering for list/details and visual map-link mode
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
- Add optional technology description and documentation link
- Assign tech stack items to each system
- Show cumulative deduplicated tech used by all child/descendant systems
- Visual map-link mode (mind-map style): drag system nodes, Shift+drag between nodes to create links
- Two-way sync: map and list view reflect the same underlying systems and links
- Create and manage multiple notes per system
- Save catalog snapshots to a database file and load them back
- Restore previous window size/position on startup

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
  - `id` (PK)
  - `system_id` (FK)
  - `body`
  - `updated_at`
- `tech_catalog`
  - `id` (PK)
  - `name` (unique)
  - `description` (nullable)
  - `documentation_link` (nullable)
- `system_tech`
  - `system_id` (FK)
  - `tech_id` (FK)
  - primary key (`system_id`, `tech_id`)

## Run

```bash
cargo run
```

This creates/uses `systems_catalog.db` in the project root.

## Compile / Build

Debug build:

```bash
cargo build
```

Release build:

```bash
cargo build --release
```

Release binary output:

- Windows: `target/release/systems_catalog.exe`
- macOS/Linux: `target/release/systems_catalog`

Windows note:

- The release executable is built as a GUI app (no extra console window).
- An application icon is embedded into the `.exe`.

## Quality checks

```bash
cargo check
cargo fmt --check
```
