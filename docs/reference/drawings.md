# Drawings

A drawing (`.3bcdrw`) shows a saved part on one or more sheets, with views, dimensions and notes, for a
workshop or a supplier. The drawing reads the part from its file, so its views follow every change you
save to the part.

## Start a drawing

1. Save the part.
2. Click **File > New Drawing**. The ribbon shows the **Drawing** tab.
3. Click **Model** and pick the part, or pick it from **Recent**. The front view and the two views
   projected from it appear on the sheet.

The **Drawing Tree** lists the sheets, views, dimensions and notes. Click an item to edit it in the
Properties panel; drag views, dimensions and notes on the sheet to tidy it.

## Sheets and the title block

Click the sheet in the tree to set:

- its **Name**, **Size** (ISO A4 to A0, ANSI A to E) and **Scale**;
- the **Projection**: **First angle (ISO)** or **Third angle (ANSI)**;
- the **Title block** cells: a label and a text each. **+ Cell** adds one and **✖** removes one. Text in
  braces fills in by itself: `{Part number}`, `{Material}` or any other name from **File >
  Properties…** of the drawing or the part, and `{Sheet}` (as "1 of 2"), `{Scale}`, `{Projection}` and
  `{Model}`.

**Add Sheet** adds a sheet with the same size, scale and title block. **Delete Sheet** removes the
selected sheet when there is more than one.

## Views

- **Standard Views**: the front view with the top and side views projected from it.
- **Model View**: a view from one side (**Front**, **Back**, **Top**, **Bottom**, **Left**, **Right**) or
  **Isometric**.
- **Projected**: select a view and click an arrow (⬅ ⬆ ⬇ ➡) to place a view projected from it on that side.
  Projected views stay lined up with their parent when you drag them.
- **Section**, then click a view where the vertical cut goes; **Section —** does the same with a
  horizontal cut. The section view lands beside it, labelled A-A and hatched for the part's material.
- **Detail**, then click the middle of the area to enlarge: the detail view shows a circle of the view at
  twice the scale.

Select a view to set its own scale, its label, a section's line position (tick **Look the other way** to
reverse it) or a detail's radius and center. **Hidden lines (dashed)** shows hidden edges, and **Centre
marks and centrelines** turns those on for the view. **Delete View** removes it.

## Dimensions and notes

- **Model Items** brings in the dimensions of the part's sketches and extrusions. Change those in the
  part; the drawing follows.
- **Smart Dimension**, then click an edge, a circle or two vertices of a view, to measure it. These
  reference dimensions follow the part's changes too.
- **Note**, then click what it points at, or empty sheet for a note without a leader. Text in braces fills
  in like the title block. Select a note to edit it or click **Remove Leader**.

Press `Esc` to stop a tool, and `Delete` (or **Delete** on the ribbon) to delete the selected view,
dimension or note.

## Editing the part

Click **Edit Part** to open the part. Change it, then click **◀ Back to** *drawing name*: the part is
saved and the views and dimensions follow.

## Export

- **PDF**: every sheet in one file, at true scale. Print it at 100 %.
- **SVG**: one file per sheet.
- **DXF**: one file per sheet, with a layer and line type for visible, hidden and centre lines, hatching,
  dimensions, notes and the sheet format.
