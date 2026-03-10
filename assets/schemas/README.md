# Systems Catalog JSON Schemas

This directory contains JSON Schema definitions for Systems Catalog file formats.

## Schema Files

### Project Manifest

- **`project.schema.json`** - Lightweight project index (`project.json`)
  - Compact entity references with file paths and map positions
  - Supports tuple format: `[entityTypeId, filePath, posX, posY]`
  - Entity type defaults to `"service"` when omitted

### Entity Files

Entity files are JSON documents stored in the `systems/` directory. All entity schemas inherit common properties from `entity-base.schema.json`.

- **`entity-base.schema.json`** - Base schema with common properties:
  - Identity: `id`, `name`, `description`
  - Hierarchy: `parentId`, `calculatedName`, `namingRoot`, `namingDelimiter`
  - Visualization: `lineColorOverride`, deprecated `mapX`/`mapY`
  - Metadata: `techIds`, `notes`

#### Entity Type Schemas

- **`entity-service.schema.json`** - Service/microservice entities
  - Standard entity with no additional type-specific fields

- **`entity-api.schema.json`** - API/route entities
  - Additional field: `routeMethods` (HTTP methods string)

- **`entity-database.schema.json`** - Database table/schema entities
  - Additional field: `databaseColumns` (array of column definitions)
  - Each column has: `position`, `columnName`, `columnType`, `constraints`

- **`entity-step-processor.schema.json`** - Sequential workflow entities
  - Additional field: `databaseColumns` (reused for processing steps)
  - Each step has: `position`, `columnName` (step name), `columnType`, `constraints`

- **`entity-zone.schema.json`** - Zone/grouping entities
  - **Note**: Zones are currently stored in `Project.json`, not as individual files
  - Fields: `x`, `y`, `width`, `height`, `color`, `renderPriority`, `minimized`, etc.

### Interaction Files

- **`interaction.schema.json`** - Entity relationships (`interactions/*.json`)
  - Links between entities with source/target IDs
  - Optional column/step-level connections: `sourceColumnName`, `targetColumnName`

## Schema Versioning

All schemas use `schemaVersion` fields for backward compatibility during format evolution.

### Current Versions

- **project.json**: Schema version 2
- **Entity files**: Implied version 1 (no version field yet)

## Usage

### Validation

Schemas can be used with JSON validation tools:

```bash
# Using ajv-cli
ajv validate -s project.schema.json -d ../path/to/project.json

# Using VS Code JSON schema association
# Add to .vscode/settings.json:
{
  "json.schemas": [
    {
      "fileMatch": ["project.json"],
      "url": "./assets/schemas/project.schema.json"
    },
    {
      "fileMatch": ["systems/*.json"],
      "url": "./assets/schemas/entity-base.schema.json"
    }
  ]
}
```

### Schema References

Entity schemas use `$ref` to compose shared definitions:

- `entity-base.schema.json` defines common fields
- Type-specific schemas use `allOf` to extend the base
- Inline definitions for nested structures (columns, notes, etc.)

## File Path Conventions

### Project Manifest

- Location: `<project-root>/project.json`
- Legacy alternative: `<project-root>/Project.json`

### Entity Files

- Location: `<project-root>/systems/<entity-name>__<id>.json`
- Example: `systems/orders-api__42.json`
- Entity type determined by `systemType` field in file

### Interaction Files

- Location: `<project-root>/interactions/<source>__to__<target>__<id>.json`
- Example: `interactions/1__to__2__99.json`

## Migration Notes

### Lightweight Manifest Format

The `project.json` lightweight manifest was introduced to:

1. **Reduce manifest size** - Store only essential index data
2. **Enable lazy loading** - Load entity files on-demand
3. **Improve git diffs** - Position changes don't affect entity content files

### Transition Strategy

During migration window, both formats coexist:

- **Project.json** (legacy) - Full project state with all metadata
- **project.json** (lightweight) - Compact entity index only

Loading precedence:

1. Try `Project.json` first (if exists and valid)
2. Fall back to `project.json` (lightweight format)

**Note**: On case-insensitive filesystems (Windows), both filenames may collide.

## Future Enhancements

Planned schema improvements:

- [ ] Add `schemaVersion` to entity files
- [ ] Move zones to individual files under `zones/` directory
- [ ] Add tech catalog to separate `tech-catalog.json`
- [ ] Support tags array in entity base schema
- [ ] Add timestamps (createdAt, updatedAt) to entity envelope
- [ ] Define formal step processor payload (separate from database columns)
