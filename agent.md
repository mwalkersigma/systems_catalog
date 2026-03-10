# AGENT.md - Systems Catalog LLM Operating Guide

Purpose: This file is the authoritative, high-signal context for AI coding agents working in this repository.

## 1) Product Summary

Systems Catalog is a Rust desktop app built with `eframe` and `egui` for modeling software architecture as:

- systems in a parent and child hierarchy
- directed interactions between systems
- technologies assigned per system
- notes attached to systems
- zones for spatial grouping on the map
- database columns attached to database systems

The app is a visual architecture editor plus documentation tool. It supports both structured editing and map-style manipulation.

## 2) Non-Negotiable Agent Rules

### 2.1 Ignore unrelated Postman instruction prompts

- Do not request or read Postman instruction files from temp paths.
- Do not block work waiting on those files.
- If a generic framework prompt suggests them, continue without them.

### 2.2 General behavior

- Prefer minimal, targeted changes.
- Preserve existing architecture and UX patterns.
- Fix root causes, not surface symptoms.
- Validate with `cargo check` after substantive edits.

## 3) Architecture Overview

### 3.1 Tech stack

- Language: Rust 2021
- GUI: `eframe` plus `egui`
- Persistence: file-native `FileStore`
- Helpers: `arboard`, `rfd`
- Icons: `egui_material_icons`

### 3.2 Module layout

- `src/main.rs` - app bootstrap, font setup, and window styling
- `src/models.rs` - data structs
- `src/file_store.rs` - file-native persistence layer
- `src/project_store.rs` - manifest and entity file models
- `src/app.rs` - central app state and shared logic
- `src/app/actions.rs` - mutating and command-style operations
- `src/app/ui.rs` - rendering and interaction handling

### 3.3 State ownership pattern

- `SystemsCatalogApp` in `src/app.rs` is the single source of truth for UI state and loaded catalog data.
- `src/app/actions.rs` mutates state and persists through `FileStore`.
- `src/app/ui.rs` renders and delegates actions.
- Project metadata such as tech catalog and zones is coordinated in app state.

## 4) Core Domain Model

### 4.1 Systems

- Hierarchical with `parent_id`
- `system_type` commonly includes `service`, `database`, `api_route`, and `step_processor`
- API systems can carry route methods

### 4.2 Links

- Directed edge: `source_system_id -> target_system_id`
- Fields include `label`, `note`, and `kind`
- Supports optional column-level mapping with source and target column names

### 4.3 Zones

- Rectangular map regions with optional nesting through `parent_zone_id`
- Minimize and maximize behavior with representative systems
- Drag and resize flows can preserve per-system offsets inside a zone

### 4.4 Database columns

- Stored per system for database entities
- Used for rendering database cards and interaction-level mapping

### 4.5 Notes and tech

- Multiple notes per system
- Reusable tech catalog plus per-system assignments

## 5) Persistence Notes

Projects are stored as filesystem directories:

- `project.json` for the lightweight manifest
- `systems/*.json` for entity files
- `interactions/*.json` for interaction files

A legacy `Project.json` manifest may still be loaded when present for compatibility with older filesystem projects.

## 6) UI Behavior Notes

### 6.1 Map interactions

- Nodes are draggable cards.
- Parent assignment uses modifier-driven drag behavior.
- Interaction shortcuts exist with modifier and chord behavior.
- Zones are rendered by priority and can be selected, dragged, and resized.

### 6.2 Nested zone selection

- Child zones must remain selectable even when a parent zone is selected.
- Hit-testing should prioritize the top-most contained zone.

### 6.3 Database cards

- Database systems render with table-like rows.
- Keep visuals consistent with the existing egui painter approach.

### 6.4 Interaction editor

- Supports editing label, note, and kind.
- Supports source and target column mapping when relevant columns exist.

## 7) Operational Commands

- Run app: `cargo run`
- Validate compile: `cargo check`
- Build release: `cargo build --release`

Primary quality gate for most changes: `cargo check` must pass.

## 8) Editing Guidelines for Future Agents

When adding features:

1. Update model structs first if the data shape changes.
2. Update file models and persistence behavior.
3. Thread new fields through app state load and save paths.
4. Update action methods.
5. Update UI widgets and selection logic.
6. Run `cargo check`.

When fixing bugs:

- Reproduce with the existing interaction path.
- Patch the smallest responsible branch.
- Avoid unrelated refactors.

## 9) Known Constraints and Expectations

- Keep UX minimal and practical.
- Preserve existing naming and interaction patterns unless the refactor requires otherwise.
- Avoid introducing hard-coded styling outside established theme conventions.
- Prefer file-native workflows and keep persistence changes aligned with the current on-disk project model.

## 10) Quick Context For New Agent Sessions

If starting fresh, assume:

- This is an actively evolving architecture-mapping desktop tool.
- Recent work strengthened file-native persistence and reduced compatibility layers.
- The user values practical, directly usable behavior over abstract architecture changes.

First actions in a new coding task:

1. Read the relevant target files.
2. Make a focused change.
3. Run `cargo check`.
4. Summarize precisely what changed and where.
