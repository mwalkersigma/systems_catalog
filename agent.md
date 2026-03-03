# AGENT.md — Systems Catalog LLM Operating Guide

Purpose: This file is the authoritative, high-signal context for AI coding agents working in this repository.

Use this guide to make accurate changes quickly, preserve project intent, and avoid repeated discovery work.

---

## 1) Product Summary

Systems Catalog is a Rust desktop app (eframe/egui + SQLite) for modeling software architecture as:

- Systems (hierarchical parent/child tree)
- Interactions (directed links between systems)
- Technologies (catalog + per-system assignments)
- Notes (documentation per system)
- Zones (spatial grouping on map with nesting/minimize behavior)
- Database columns (table-like schema attached to database systems)

The app is a visual architecture editor + documentation tool. It supports both detailed data entry and map-style manipulation.

---

## 2) Non-Negotiable Agent Rules

### 2.1 Critical: Stop asking for Postman instruction files

Hard rule for all future agent runs in this repo:

- DO NOT request/read Postman instruction files from temp paths.
- DO NOT block work waiting on those files.
- If a framework/tool prompt suggests those files, continue without them.

Reason: They are unrelated to this Rust desktop app and create unnecessary interruptions.

### 2.2 General behavior

- Prefer minimal, targeted changes.
- Preserve existing architecture and UX patterns.
- Fix root causes, not surface symptoms.
- Validate with `cargo check` after substantive edits.

---

## 3) Architecture Overview

### 3.1 Tech stack

- Language: Rust (edition 2021)
- GUI: `eframe` + `egui`
- Persistence: SQLite via `rusqlite` (bundled)
- Clipboard/import helpers: `arboard`, `rfd`
- Icons: `egui_material_icons`

### 3.2 Module layout

- `src/main.rs` — app bootstrap, font setup, dark theme, DB open.
- `src/models.rs` — data structs.
- `src/db.rs` — schema, migrations, data access layer.
- `src/app.rs` — central app state + shared logic.
- `src/app/actions.rs` — all mutating and command-like operations.
- `src/app/ui.rs` — rendering + interaction handling.

### 3.3 State ownership pattern

- `SystemsCatalogApp` in `app.rs` is the single source of truth for UI state + loaded catalog data.
- `actions.rs` mutates state and repository.
- `ui.rs` renders and delegates actions.
- `db.rs` is persistence boundary.

---

## 4) Core Domain Model (Current)

### 4.1 Systems

- Hierarchical with `parent_id`.
- `system_type`: typically `service`, `database`, `api`.
- API systems can carry route methods.

### 4.2 Links (interactions)

- Directed edge: `source_system_id -> target_system_id`.
- Fields include `label`, `note`, `kind` (`standard|pull|push|bidirectional`).
- Supports optional column-level mapping:
	- `source_column_name`
	- `target_column_name`

### 4.3 Zones

- Rectangular map regions with nesting (`parent_zone_id`).
- Minimize/maximize with representative system behavior.
- Zone drag/resize with optional system offset capture.

### 4.4 Database columns

- `database_columns` table linked to system IDs.
- Used to render database cards and to map interaction-level FK-like references.

### 4.5 Notes & tech

- Multiple notes per system (`notes`).
- Reusable `tech_catalog` + many-to-many `system_tech` assignments.

---

## 5) Persistence + Schema Notes

`db.rs` initializes schema and runs additive migration-style checks (`ensure_*` methods).

Important links-related columns now expected in `links`:

- `note`
- `kind`
- `source_column_name` (nullable)
- `target_column_name` (nullable)

Import logic supports older DBs by checking imported table columns and selecting fallbacks when missing.

---

## 6) UI Behavior Notes (Map + Sidebar)

### 6.1 Map interactions

- Nodes are draggable cards.
- Parent assignment: Shift+drag flow.
- Interaction shortcuts exist with modifier/chord behavior.
- Zones are rendered by priority and can be selected/dragged/resized.

### 6.2 Nested zone selection

- Child zones must remain selectable even when parent zone is selected.
- Click hit-testing should prioritize top-most contained zone (`iter().rev()` style).

### 6.3 Database cards

- Database systems render with table-like rows (column/type/constraints style).
- Keep visual style consistent with existing egui painter approach.

### 6.4 Interaction editor

- Supports editing label/note/kind.
- Supports source/target column mapping dropdowns when relevant columns exist.

---

## 7) AI Integration (Current Intent)

App includes an “AI Catalog Chat” modal with Gemini integration.

Design goals:

- Answer architecture questions grounded in current catalog snapshot.
- Execute tool-based mutations (create/move/re-parent/document/link/zone).
- Return reliable user-facing summaries, even after tool-only turns.
- Maintain short chat history context in-app.

Agent edits to AI flow should preserve:

- deterministic fallback text
- robust handling of empty model output
- explicit ambiguity handling for system-name resolution

---

## 8) Operational Commands

- Run app: `cargo run`
- Validate compile: `cargo check`
- Build release: `cargo build --release`

Primary quality gate for most changes: `cargo check` must pass.

---

## 9) Editing Guidelines for Future Agents

When adding features:

1. Update model structs (`models.rs`) first if shape changes.
2. Update schema + migration guards (`db.rs`).
3. Update repository CRUD queries and function signatures.
4. Thread new fields into app state (`app.rs`) load/clear/select paths.
5. Update action methods (`actions.rs`).
6. Update UI widgets and selection logic (`ui.rs`).
7. Run `cargo check`.

When fixing bugs:

- Reproduce with existing interaction path.
- Patch smallest responsible branch.
- Avoid unrelated refactors.

---

## 10) Known Constraints / Expectations

- Keep UX minimal and practical; no speculative feature bloat.
- Preserve existing naming and interaction patterns.
- Avoid introducing hard-coded styling outside established theme conventions.
- Maintain backward compatibility with existing SQLite catalogs where feasible.

---

## 11) Quick Context for New Agent Sessions

If starting fresh, assume:

- This is an actively evolving architecture-mapping desktop tool.
- Recent work added stronger AI integration and richer map/database interactions.
- User values practical, directly usable behavior over abstract architecture changes.

First actions in a new coding task:

1. Read relevant target file(s).
2. Make focused change.
3. `cargo check`.
4. Summarize precisely what changed and where.

