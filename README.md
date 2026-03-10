# Systems Catalog

Systems Catalog is a Rust desktop app for documenting systems, their relationships, notes, and technology usage in a file-native project format.

## Stack

- GUI: `eframe` / `egui`
- Persistence: file-native `FileStore`
- Error handling: `anyhow`

## Project structure

- `src/models.rs` - core data shapes such as systems, links, notes, tech items, and zones
- `src/file_store.rs` - file-native persistence with atomic writes and lazy entity loading
- `src/app.rs` - shared app state, synchronization logic, and helpers
- `src/app/actions.rs` - write-side actions and workflows
- `src/app/ui.rs` - rendering and interaction surfaces
- `src/project_store.rs` - manifest and file format models
- `src/main.rs` - startup and window bootstrap

## Features

- Hierarchical system catalog with service, database, API route, and step processor entities
- Directed interactions between systems
- Reusable tech catalog with per-system assignments
- Multiple notes per system
- Database column editing for database entities
- Visual map editing with draggable nodes and relationship creation
- Zone and grouping support in project metadata
- YAML import and export flows
- File-based project save and load

## Storage model

Projects are stored on disk using a lightweight manifest plus per-entity and per-interaction files.

```text
project-root/
├── project.json
├── systems/
│   ├── service__users__2.json
│   └── api__orders__5.json
└── interactions/
    └── 2__to__5__99.json
```

`project.json` stores the lightweight graph index. Entity files in `systems/` store system-specific content such as notes, database columns, route methods, and assigned tech IDs. Interaction files in `interactions/` store link metadata.

When a legacy `Project.json` manifest is present, it is still loaded for compatibility. Otherwise the app uses `project.json` and discovers interactions from `interactions/*.json`.

## JSON schemas

Schema definitions for project and entity files are available under `assets/schemas/`.

- `project.schema.json` - lightweight project index
- `entity-base.schema.json` - shared entity fields
- `entity-service.schema.json` - service entities
- `entity-api.schema.json` - API route entities
- `entity-database.schema.json` - database entities
- `entity-step-processor.schema.json` - step processor entities
- `interaction.schema.json` - interaction files

See `assets/schemas/README.md` for schema details.

## Run

```bash
cargo run
```

## Build

```bash
cargo build
cargo build --release
```

Release binaries are written to `target/release/`.

## Quality checks

```bash
cargo check
cargo fmt --check
cargo test
```
