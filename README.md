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

## Publish Releases

This repo includes a release workflow and publish helpers that manage semantic version bumps and tagging.

### One-command publish

On macOS/Linux/Git Bash:

```bash
./publish --type major
./publish --type minor
./publish --type bugfix
```

On PowerShell:

```powershell
./publish.ps1 -Type major
./publish.ps1 -Type minor
./publish.ps1 -Type bugfix
```

What it does:

- Validates a clean git working tree
- Bumps `Cargo.toml` version
- Runs `cargo check`
- Commits version bump as `chore(release): vX.Y.Z`
- Creates and pushes git tag `vX.Y.Z`

### GitHub Actions release automation

`.github/workflows/release.yml` listens for pushed tags matching `v*` and builds release binaries for:

- Windows (`systems_catalog-windows-x86_64.exe`)
- Linux (`systems_catalog-linux-x86_64`)
- macOS Apple Silicon (`systems_catalog-macos-aarch64`)

The workflow then creates or updates the GitHub Release for that tag and uploads all binaries.

## Auto-update behavior

The app now performs a lazy update check in the background after startup.

- On startup it loads normally, then checks `https://api.github.com/repos/<owner>/<repo>/releases/latest`
- If a newer version is available, an `Update <version>` badge appears in the top toolbar
- Clicking the badge asks for confirmation
- If confirmed, the app downloads the platform asset from the release, stages it, and schedules replacement
- The app closes and restarts into the updated binary

Current update source defaults to `mwalker/systems_catalog` in `src/app.rs` (`update_repo_owner` and `update_repo_name`).
If your repository uses a different owner or name, update those values.

## Quality checks

```bash
cargo check
cargo fmt --check
cargo test
```
