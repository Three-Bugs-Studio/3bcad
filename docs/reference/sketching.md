# Sketching

Every solid feature starts from a 2D sketch: lines, arcs and circles held in place by relations and
dimensions. A sketch lies on a plane or on a flat face of the model.

## Start and finish a sketch

To start a sketch, do one of these:

- On the **Sketch** tab, click **Front Plane**, **Top Plane** or **Right Plane**. Reference planes you
  made with **Plane** are listed next to them.
- Click a flat face of the model (or a plane), then click **Sketch** on the **Sketch** tab. A sketch on a
  face follows that face when the model changes.
- Double-click a plane in the feature tree, or right-click it and choose **✏ Sketch on it**.

The view turns to look straight at the sketch plane. To edit a sketch later, double-click it in the
feature tree or the graphics area, select it and click **Edit Sketch**, or click **✏ Edit Sketch** in
the Properties panel.

To finish, click **✔ Exit Sketch** in the top right corner of the graphics area, or **Exit Sketch** on
the ribbon. The **✖** next to it discards the changes. Inside a sketch, `Ctrl+Z` and `Ctrl+Y` undo and
redo sketch edits; the whole sketch session is one undo step in the part.

You can also start a feature straight from an open sketch: clicking **Extruded Boss/Base** (or another
sketch feature) on the **Features** tab closes the sketch first.

## Drawing tools

Pick a tool on the **Sketch** tab. The Properties panel shows the tool's options and what to click.

- **Line**: click points to chain lines. Double-click, right-click or press `Esc` to end the chain.
- **Centerline**: click two points. Centerlines are construction lines; **Revolved Boss/Base** turns the
  profile about the first one.
- **Point**: click to place a point. The **Hole Wizard** puts a hole at every point of a sketch.
- **Corner Rectangle**: click two opposite corners. **Center Rectangle**: the center, then a corner.
  **3 Point Rectangle**: two corners of one side, then the opposite side.
- **Circle**: click the center, then a point on the circle. Tick **Radius** in the Properties panel to
  type the radius before you click. **Perimeter Circle**: three points on the circle.
- **Polygon**: click the center, then a corner. Set **Sides**, and choose **Inscribed circle (click a
  vertex)** or **Circumscribed circle (click an edge midpoint)**. The polygon gets a construction circle
  that you can dimension.
- **Centerpoint Arc**: the center, the start, then the end. **Tangent Arc**: the end of a line or arc,
  then the arc's end. **3 Point Arc**: the start, the end, then a point on the arc.
- **Spline**: click points; double-click or press `Enter` to finish.
- **Straight Slot**: both arc centers, then the width. **Centerpoint Slot**: the slot's center, an arc
  center, then the width.

## Editing tools

- **Sketch Fillet** and **Sketch Chamfer**: click a corner where two lines meet. Set the **Radius**, or
  the chamfer's **Distance-distance** or **Angle-distance** values, first.
- **Trim Entities**: click the piece of an entity to remove. **Extend Entities**: click near the end of a
  line to extend it.
- **Offset Entities**: select entities, set **Distance** (tick **Reverse** for the other side), then
  click **Apply**.
- **Mirror Entities**: select entities and then a centerline (picked last), then click **Apply**.
- **Linear Pattern**: select entities, set **Instances**, **Spacing** and **Direction**, then **Apply**.
  **Circular Pattern**: select entities and then the center point (picked last), set **Instances** and
  **Total angle**, then **Apply**. A sketch pattern's instance count can be changed later in the sketch's
  Properties panel.
- **Convert Entities**: in a sketch on a face, copies the face's edges into the sketch.
- **Select**: click to select, drag geometry to move it (the sketch solves as you drag), double-click a
  dimension to change it. Press `Delete`, or click **Delete selected**, to delete the selection.

## Snapping and automatic relations

While you draw, the cursor snaps to points, midpoints, curves and the part origin, and dotted lines show
horizontal and vertical alignment. Snapping adds the matching relation: a line drawn nearly level gets
**Horizontal**, an end dropped on a point gets **Coincident**. A point placed on the origin is fixed
there. Hold `Ctrl` while you draw to turn snapping and the automatic relations off.

## Dimensions

Click **Smart Dimension**, click one or two items, then click where the dimension goes. Type the value
and press `Enter`. What you get depends on what you click:

- one line: its length, horizontal, vertical or aligned depending on where you place the dimension;
- a circle: its diameter; an arc: its radius;
- two points: their distance, horizontal, vertical or aligned by placement;
- a point and a line: the distance from the point to the line;
- a point and a circle, or two circles: the distance between the centers;
- two lines: the angle between them, or their distance when they are parallel;
- the part origin and another item: clicking the origin adds a point fixed there.

Double-click a dimension to change it. Value boxes take units and arithmetic, for example `1in`,
`2.5 cm` or `10 + 2in`; the result is converted to the part's units. If a dimension over-defines the
sketch, the Properties panel says so: delete it or another relation.

Sketch dimensions are named `D1@Sketch1`, `D2@Sketch1` and so on, and can be driven by
[equations](design-intent.md).

## Relations

Select sketch geometry (Ctrl+click for more) to see its relations in the Properties panel:

- **Existing relations** lists them; click **✖** to delete one.
- **Add relations** offers the ones that fit the selection: **Fix**, **Coincident**, **Horizontal**,
  **Vertical**, **Parallel**, **Perpendicular**, **Collinear**, **Equal**, **Midpoint**, **Concentric**,
  **Symmetric**, **Tangent** and **Coincident (on curve)**.
- **For construction** turns the selected entities into construction geometry, which is
  not part of any profile.

Tick **Display relations** on the ribbon to show relation symbols next to the geometry.

## Sketch status

The status bar, the Properties panel and the colours show how far the sketch is pinned down:

- **Under Defined** (blue): some geometry can still move. The status bar shows how many degrees of
  freedom are left, and the feature tree marks the sketch `(-)`.
- **Fully Defined** (black): nothing can move.
- **Over Defined** (red): relations conflict or repeat each other, and the tree marks the sketch `(+)`.
  Click **Diagnose** in the Properties panel to list the relations whose deletion fixes it.

An under-defined sketch still makes features. Fully defining it means a later change to one dimension
moves only what you expect.

## Profiles

Extrudes, revolves, cuts and sweeps use the closed loops of a sketch. Loops can be drawn in any order and
can contain holes and islands: a rectangle with a circle inside extrudes as a plate with a hole.
Construction geometry and open chains are left out of profiles. **Rib**, sweep paths and curve patterns
use open chains instead.
