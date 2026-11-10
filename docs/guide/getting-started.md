# Getting started

This page shows you around the window, then walks you through a first part: a plate with a hole and
rounded corners.

## The window

When 3BCAD starts, it shows a starter part: a 30 mm cube and a 20 mm cylinder.

- **Top row**: the **3BCAD** name (click it for **About 3BCAD**), the version (a link to its release
  notes), the **File** menu and the name of the open document. A `*` after the name means unsaved
  changes. On the right are **Undo**, **Redo** and the **Shortcuts** button.
- **Ribbon**: the commands of the current tab. The tabs are under the commands: **Features**,
  **Sketch**, **Evaluate** and **3D Print** for a part. An assembly shows the **Assembly** tab and a
  drawing the **Drawing** tab instead. Hover over a command to see what it needs selected.
- **Feature tree** (left): the part's name, the **Configuration** box, the **Front Plane**, **Top
  Plane** and **Right Plane** (tick or untick to show or hide them), the **Origin**, the
  **Solid Bodies** folder and then every feature in build order. The blue bar at the end is the rollback
  bar.
- **Properties panel** (right): the values of the selected feature. Every value you see there stays
  editable, and the part rebuilds as you change it. While you sketch, it shows the sketch's status, the
  active tool's options and the relations of the selected sketch geometry.
- **Graphics area** (middle): the model, the ground grid and the XYZ triad.
- **Status bar** (bottom): the last message, the unit system (for example **MMGS (mm)**; click it to
  change units), the number of bodies and, when a feature fails, a **⚠ errors, warnings** button that
  opens **What's Wrong**.

### Mouse and keys

| Action | Input |
| --- | --- |
| Orbit | Left drag, or middle drag |
| Pan | Right drag, or Ctrl + middle drag |
| Zoom | Mouse wheel |
| Zoom to fit | `F`, or double-click empty space |
| Select a face, edge, vertex or plane | Click |
| Add to or remove from the selection | Ctrl + click |
| Clear the selection | `Esc` |
| Undo / Redo | `Ctrl+Z` / `Ctrl+Y` |
| Rename the selected feature | `F2` |
| Rebuild every feature | `Ctrl+Q` |
| Keyboard Shortcuts window | `F1` |

[View controls, display styles and settings](viewport-and-settings.md) covers the rest.

### The feature tree

- Click a feature to select it and edit its values in the Properties panel.
- Double-click a sketch to edit it. Double-click a plane to start a sketch on it.
- Right-click a feature for **Edit Feature**, **Edit Sketch**, **Rename (F2)**, **Appearance…**,
  **Suppress**, **Move Up**, **Move Down**, **Roll Back to Here** and **Delete (with children)**.
- Drag a feature to reorder it. A move that would put a feature above the features it is built from is
  refused.
- Drag the blue rollback bar up to see the part as it was at that point. Features below the bar are
  greyed out and left out of the part.
- A sketch marked `(-)` is under defined and `(+)` over defined (see [Sketching](../reference/sketching.md)).
  A feature in red failed to rebuild; hover over it to see why.

## Your first part

You will make a 60 × 40 × 8 mm plate with a counterbored hole in the middle and 5 mm rounded corners.

### Sketch the outline

1. Click **File > New Part** (`Ctrl+N`). If 3BCAD asks to save the starter part, click **Don't Save**.
2. On the **Sketch** tab, click **Top Plane**. The view turns to look straight down at the plane.
3. Click **Corner Rectangle**. Click the origin (the cursor snaps to it), then click up and to the right
   of it.
4. Click **Smart Dimension**. Click the bottom line, click below it where the dimension goes, type `60`
   and press `Enter`. Click the right-hand line, click to its right, type `40` and press `Enter`.
5. The lines turn black and the status bar shows **Fully Defined**: nothing in the sketch can move.
6. Click **✔ Exit Sketch** in the top right corner of the graphics area. The **Features** tab opens with
   the new sketch, **Sketch1**, selected.

### Extrude it

1. Click **Extruded Boss/Base**.
2. In the Properties panel, set **Depth** to `8`. The plate grows as you type.

### Add a hole

1. Click the plate's top face.
2. Click **Hole Wizard**. A hole goes at the center of the face. It starts as an ISO M6 simple hole,
   **Through All**.
3. In the Properties panel, change the hole type from **Hole** to **Counterbore** and the size to `M5`.
   The panel shows the drill and counterbore diameters.

### Round the corners

1. Click one of the four vertical corner edges, then Ctrl+click the other three.
2. Click **Fillet**, and set **Radius** to `5`.

### Change your mind

1. Double-click **Sketch1** in the feature tree.
2. Double-click the `60` dimension, type `80` and press `Enter`.
3. Click **✔ Exit Sketch**. The plate is now 80 mm long and the fillets follow the corners.

The hole stays where it was, 30 mm from the left end, because its position is a point in its own sketch
on the top face. To move it, double-click that sketch in the tree and drag or dimension the point.

### Save

Press `Ctrl+S` and choose a file name. Parts are saved as `.3bcad` files. See
[Files, import and export](../reference/files.md) for what else you can open and export, and the
[tutorials](../tutorials/phone-stand.md) for complete printable parts.
