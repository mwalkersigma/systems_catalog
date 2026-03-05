# Detailed LLM Import Instructions (Systems + Interactions)

Use this when mapping large codebases (especially C#/.NET MVC, Django, Next.js, or mixed stacks) into Systems Catalog.

## Objective

Produce one JSON file that includes:
1. Systems (with hierarchy), and
2. Interactions between systems (with labels + notes)

The importer will prompt for a **root name** and create a root parent system. Any system without `parentKey` becomes a direct child of that root.

## Required output files

You may create helper files at repository root while analyzing (for example `memory.md`, `inventory.md`), but final import requires one JSON file conforming to:
- `assets/llm_detailed_import/schema.json`

## Hierarchy guidance

- Derive subsystem trees from:
  - URL/route depth (`/users`, `/users/create`, `/users/stats`)
  - Controller -> action methods
  - Class -> method trees where it represents feature boundaries
- Prefer feature semantics over folder-only grouping.

## Interaction guidance

For each dependency/use path, add an interaction:
- UI/view calling controller/service/repository => usually `pull`
- Service writing/triggering another system => usually `push`
- If both pull and push exist between same pair, use `bidirectional`
- If unclear, use `standard`

Use detailed `label` and `note`:
- Label: concise functional summary (e.g., `Fetch user list for dashboard grid`)
- Note: implementation-level evidence (methods/classes/files)

## C#/.NET specific checklist

- Map areas/modules, controllers, services, repositories, integration clients.
- Parse controller action methods and service method calls.
- Identify interactions from:
  - constructor-injected dependencies
  - HTTP client wrappers
  - EF/DB repositories
  - message bus / background job calls
- Include subsystem children for methods when useful (large controllers/services).

## Generic framework checklist

- Next.js: inspect `app/` and `pages/`; use route segments as hierarchy.
- Django: inspect `urls.py`, views, service modules.
- Static HTML apps: map each HTML page and its JS modules.

## JSON quality rules

- `key` must be unique per system.
- `parentKey` must reference an existing `key` if present.
- Keep names concise and human-readable.
- Keep descriptions 1-2 sentences, concrete and useful.
- Avoid duplicate systems that represent same boundary.

## Example skeleton

```json
{
  "schemaVersion": 1,
  "inventory": {
    "source": "solution scan",
    "framework": "ASP.NET MVC",
    "notes": "Controllers/actions mapped to subsystem tree"
  },
  "systems": [
    {
      "key": "users-controller",
      "name": "Users Controller",
      "description": "Handles user list and detail workflows",
      "systemType": "service"
    },
    {
      "key": "users-controller/get-users",
      "parentKey": "users-controller",
      "name": "GetUsers",
      "description": "Returns users for grid",
      "systemType": "api",
      "routeMethods": ["GET"]
    }
  ],
  "interactions": [
    {
      "sourceKey": "users-controller/get-users",
      "targetKey": "user-service/get-all",
      "kind": "pull",
      "label": "Load users for UI grid",
      "note": "UsersController.GetUsers calls IUserService.GetAll"
    }
  ]
}
```
