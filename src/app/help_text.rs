/// Help text content for the Systems Catalog application.
/// Organized by topic for easy editing and maintenance.

pub struct HelpText;

impl HelpText {
    pub fn getting_started() -> &'static str {
        r#"Getting Started with Systems Catalog

1. CREATE YOUR FIRST SYSTEM
   • Click "File" → "New Catalog" to start fresh
   • Use Ctrl+N to open the "Add System" dialog
   • Enter a system name (e.g., "API Server", "Database")
   • Optionally add a description
   • Click "Create System"

2. BUILD A HIERARCHY
   • Systems can have parent-child relationships
   • Right-click a system or use the details panel to add child systems
   • This creates a natural organizational structure
   • Collapse/expand parents by clicking the arrow icon in the Systems list

3. ASSIGN TECHNOLOGY
   • First, create tech items in the Tech Catalog (Alt+N)
   • Select a system and assign tech from the right details panel
   • View all cumulative tech used by a system and its children
   • Tech is color-coded for easy visualization

4. CREATE INTERACTIONS
   • In the map canvas, Shift+click on one system then another to create a link
   • Or use the details panel → "Interactions" section
   • Add labels and notes to describe the relationship
   • Different interaction types: Standard, Pull, Push, Bidirectional

5. ORGANIZE WITH ZONES
   • Draw zones on the map using the draw tool
   • Zone system nodes by grouping related systems together
   • Collapse zones to hide complexity while preserving connections
   • Useful for separating concerns (frontend, backend, infra, etc.)

6. SAVE YOUR WORK
   • Changes are automatically saved to systems_catalog.db
   • Use "File" → "Save Catalog" to export your catalog as a backup
   • Use "File" → "Load Catalog" to import a previously saved catalog

Pro Tips:
   • Press F1 to see keyboard shortcuts
   • Use View menu to zoom/pan map efficiently
   • Alt+V to paste copied systems onto the map
   • Use Flow Inspector (Tools menu) to trace data paths
"#
    }

    pub fn creating_interactions() -> &'static str {
        r#"Creating & Managing Interactions

WHAT ARE INTERACTIONS?
Interactions represent how systems communicate or depend on each other.

HOW TO CREATE
Method 1: Visual Map Editor
   • Hold Shift and click on source system
   • Click on target system
   • A line appears connecting them

Method 2: Details Panel
   • Select a system
   • Scroll to "Interactions" section
   • Click "New Interaction"
   • Choose target system from dropdown
   • Click "Create"

INTERACTION TYPES
   Standard  → One-way dependency (A calls B)
   Pull      → B pulls data from A
   Push      → A pushes data to B
   Bidirectional → A and B communicate both ways

EDITING INTERACTIONS
   • Click an interaction line in the map to select it
   • Details panel shows interaction properties
   • Add descriptive labels ("REST API call", "Database query", etc.)
   • Add implementation notes
   • Change interaction type

DELETING INTERACTIONS
   • Select the interaction in the map
   • Press Delete key
   • Or use the details panel "Delete" button

VIEWING INTERACTION NOTES
   • Hover over an interaction line in the map
   • A popup shows the interaction note if one exists
   • Useful for tracking implementation details

FLOW INSPECTOR
   • Tools menu → Flow Inspector
   • Traces data paths between systems
   • Shows incoming and outgoing dependencies
   • Helps validate your architecture
"#
    }

    pub fn managing_technology() -> &'static str {
        r#"Managing Your Tech Catalog

BUILD YOUR TECH CATALOG
   1. Press Alt+N or use Tools → "Add Technology"
   2. Enter tech name (e.g., "PostgreSQL", "React", "Docker")
   3. Add optional description
   4. Add optional documentation link (URL)
   5. Assign a color (optional)
   6. Click "Create"

ORGANIZE TECH BY SYSTEM
   • Select a system in the Systems list
   • Right panel shows "Tech Stack"
   • Click "+" to add tech items
   • Remove tech by clicking "x"
   • Order is preserved

VIEW CUMULATIVE TECH
   • Select a parent system
   • "Child Tech" section shows all tech used by descendants
   • Automatically deduplicated
   • Helpful for understanding overall system dependencies

EDIT TECHNOLOGY
   • Switch to Tech Catalog tab (Systems list top)
   • Select a tech item
   • Modify name, description, link, color, priority
   • Changes apply to all systems using it

DELETE TECHNOLOGY
   • Select tech in Tech Catalog
   • Click "Delete Tech"
   • Removes from catalog AND all system assignments

COLOR-CODED TECH
   • Each tech can have a color
   • Systems show tech borders in assigned colors
   • Enable "Tech Border Colors" in View menu to see them
   • Useful for visually grouping tech domains

TECH PRIORITY
   • Higher priority tech shows first in lists
   • Useful for highlighting critical technologies
   • Configurable per tech item
"#
    }

    pub fn understanding_the_map() -> &'static str {
        r#"Understanding the Visual Map

THE CANVAS
   • Central area shows your systems as cards
   • Lines connect systems showing interactions
   • Zones are rectangular groups of systems
   • Dark background = world coordinates

SYSTEM CARDS
   • System name at top
   • Description below
   • Tech items listed at bottom with colors
   • Click to select & view details
   • Drag to move around
   • Highlighted when selected

LINES & CONNECTIONS
   Parent Lines (gray, thinner)
      • Shows hierarchy relationships
      • Can be toggled in Connection Style menu
      • Thin arrows indicate parent-child

   Interaction Lines (thicker, colored)
      • Shows how systems communicate
      • Arrow type indicates interaction style
      • Different colors per interaction type
      • Hover to see notes

VIEW CONTROLS
   Zoom        : Scroll wheel or View menu
   Pan         : Right-click drag or arrow keys
   Center      : View → "Reset Pan"
   Fit All     : View → "Show All"

SNAP TO GRID
   • View menu → Snap to Grid toggle
   • Helps align cards neatly
   • Useful for organized layouts

SELECTION
   • Click a card to select it
   • Alt+click to multi-select
   • Drag selection box to select multiple
   • Undo with Ctrl+Z

UNDO
   • Only works for map position changes
   • Ctrl+Z to undo last move/layout change
   • Up to 100 snapshots retained

ZONES
   • Draw zones to group related systems
   • Drag corner/edges to resize
   • Collapse to hide complexity
   • Parent-child zone relationships possible
"#
    }

    pub fn zones_and_organization() -> &'static str {
        r#"Zones: Organizing Complex Architectures

WHAT ARE ZONES?
Zones are rectangular groups on the map that help organize systems into domains.
Useful for: frontend/backend separation, AWS regions, service boundaries, etc.

DRAWING ZONES
   • Anything on the map by default starts drawing a zone
   • Click and drag to create a rectangle
   • Release to confirm
   • Systems inside the zone are now "grouped"

ZONE PROPERTIES
   • Name: Identify the zone purpose
   • Color: Customize appearance
   • Render Priority: Controls overlaying
   • Parent Zone: Create zone hierarchies
   • Minimized: Collapse to hide contents

USING ZONES
   • Move systems within zones
   • Collapse zones to reduce clutter
   • Collapsed zones still show connections through representative system
   • Nested zones supported

COLLAPSING ZONES
   • Click minimize button on zone card
   • Hidden systems still participate in interactions
   • Cleaner view of high-level architecture
   • Expand by clicking card again

ZONE HIERARCHY
   • Zones can contain other zones
   • Parent zone becomes visual container
   • Child zones show within bounds
   • Useful for multi-level organization

DELETING ZONES
   • Right-click zone or use details panel
   • Delete Zone button
   • Systems inside are NOT deleted
   • Just removes the grouping
"#
    }

    pub fn keyboard_shortcuts() -> &'static str {
        r#"Keyboard Shortcuts

GLOBAL
   F1              Open this help
   Escape          Close current dialog/modal
   Ctrl+Z          Undo last map position change
   Delete          Delete selected system or interaction

EDITING
   Ctrl+N          Add system (with current selection as parent)
   Ctrl+Shift+N    Bulk add systems
   Alt+N           Add technology

CLIPBOARD
   Alt+C           Copy selected systems
   Alt+V           Paste copied systems onto map

NAVIGATION & VIEW
   Scroll          Zoom in/out
   Right-click + drag    Pan around map
   Arrow keys      Navigate through systems (in list)

FILE OPERATIONS
   From File menu:
      • New Catalog        Start fresh
      • Save Catalog       Export snapshot
      • Load Catalog       Import backup

MAP CANVAS
   Ctrl+R/B/F/D + click   Select interaction source, then click target
   Ctrl+R/B/F/D + drag    Drag interaction line source -> target
   Ctrl+Shift+R/B/F/D     Open style modal for interaction type
   Shift+drag             Assign parent (child -> parent)
   Alt+click     Multi-select systems
   Click+drag    Select multiple systems (box selection)
"#
    }

    pub fn troubleshooting() -> &'static str {
        r#"Troubleshooting & FAQ

CONNECTION ISSUES
Q: Why can't I create a link between systems?
A: Ensure both systems exist and aren't the same system.
   Some link types have directionality restrictions.

Q: Lines are hard to see. How do I fix this?
A: Use "Connection Style" → "Interaction Lines" to change colors/widths.
   Or toggle "Show Interaction Lines" to highlight them.

DATA & SAVING
Q: Where is my data stored?
A: In systems_catalog.db (same directory as the app)
   This file contains everything: systems, links, tech, notes, zones.

Q: Can I backup my data?
A: Yes! Use File → "Save Catalog" to export a snapshot.
   Create multiple exports for safety.

Q: Can I restore from a backup?
A: Yes, File → "Load Catalog" to import a saved export.
   Choose "Replace mode" to completely restore.

VISUALIZATION
Q: The map is too cluttered. How do I simplify?
A: 1. Collapse systems (arrow icon in list)
   2. Use zones to group related systems
   3. Collapse zones to hide implementation details
   4. Use Flow Inspector to focus on specific paths

Q: Can I hide certain systems temporarily?
A: Select systems and use View → "Clear Selection Set"
   Or close their parent to collapse the tree.

TECH CATALOG
Q: Why is tech showing on systems I didn't assign it to?
A: You might be viewing "Cumulative Tech"
   This shows all tech used by descendants too.
   Each system can have its own independent tech list.

Q: How do I reorder my tech list?
A: Adjust "Display Priority" in Tech Catalog
   Higher priorities appear first.

PERFORMANCE
Q: App is slow with many systems.
A: This is normal. Consider:
   • Zooming out to reduce rendered systems
   • Collapsing large subsystems
   • Using zones to organize into logical groups
   • Closing the app and relaunching

GENERAL
Q: Can I share my catalog with others?
A: Yes! Use File → "Save Catalog" to export
   Send the .db file to colleagues
   They can load it with File → "Load Catalog"

Q: Is there a redo function?
A: Not currently. Only undo for map position changes.
   Use exports to preserve versions.
"#
    }
}
