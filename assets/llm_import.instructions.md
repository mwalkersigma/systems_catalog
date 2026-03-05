# LLM Systems Import Instructions

Use this guide to produce a JSON file that can be imported via **File → Import LLM Systems File**.

## Goal

Map a codebase into high-level systems and optional nested subsystems.

Examples:
- Django/Flask/FastAPI: map routes/endpoints
- Next.js: map app/pages routes
- Vanilla HTML: map pages by HTML files

## Output format

Write one JSON file matching `assets/llm_import.schema.json`.

Top-level shape can be either:
1. Object with `systems` array, or
2. Direct array of systems

## Required behavior

- Each item should include a `name` and a stable `key`.
- For nested systems, set `parentKey` to the parent system key.
- If path-like structure exists, set `path` (for example `/users/create`).
- For API systems, include `systemType: "api"` and route verbs in `routeMethods`.
- Include concise descriptions.

## Route hierarchy convention

Treat each slash level as a subsystem level.

Example routes:
- `/users`
- `/users/create`
- `/users/stats`

Recommended systems:
- `users` (root)
- `create` (child of users)
- `stats` (child of users)

## Suggested model workflow

1. Inventory project structure (framework, route folders, entrypoints, templates/pages).
2. Build a unique list of systems with parent-child relationships.
3. Add short business descriptions.
4. Emit JSON conforming to schema.

## Minimal example

```json
{
  "schemaVersion": 1,
  "systems": [
    {
      "key": "/users",
      "name": "users",
      "path": "/users",
      "description": "User account landing and overview",
      "systemType": "api",
      "routeMethods": ["GET", "POST"]
    },
    {
      "key": "/users/create",
      "parentKey": "/users",
      "name": "create",
      "path": "/users/create",
      "description": "Create a new user",
      "systemType": "api",
      "routeMethods": ["GET", "POST", "PUT"]
    }
  ]
}
```
