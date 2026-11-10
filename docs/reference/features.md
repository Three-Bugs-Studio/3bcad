# Part features

Features are the steps that build a part. Each command on the **Features** tab makes its feature from
what you have selected, with a starting value, and selects it so you can change its values in the
Properties panel. Hover over a command to see what it needs. If a feature fails, it turns red in the
feature tree and the Properties panel says why, with a suggested fix.

To select more than one item, Ctrl+click. To change what a feature uses after you made it, select the new
edges or faces and click **Use selection** in its Properties panel.

## Adding and cutting material

**Extruded Boss/Base** pushes the closed loops of the selected sketch out of its plane, 10 mm to start
with. **Extruded Cut** does the same but removes material. In the Properties panel:

- **Direction 1** sets the end condition: **Blind** (a **Depth**), **Through All**, **Up To Next** or
  **Mid Plane** (the depth split evenly on both sides). With a vertex or face selected you also get **Up
  To Vertex**, **Up To Surface** and **Offset From Surface**.
- **Reverse direction** flips the side. **Direction 2** adds a second end condition the other way.
- **Draft** tapers the sides by an **Angle**; tick **Draft outward** to widen instead.
- **Thin feature** makes a wall along the profile instead of a solid: **One**, **Mid** or **Two**
  directions, with **Thickness**. It works with open sketches too.
- **Merge result** (boss): join the bodies the new material touches. Untick it to keep a separate body.
- **Flip side to cut** (cut): keep what is inside the profile and remove the rest.

**Revolved Boss/Base** and **Revolved Cut** turn the profile about the sketch's first centerline. Under
**Axis** you can pick **Centerline**, the world **X**, **Y** or **Z** axis, or **Use selection** for an
edge or reference axis. **Angle** is 360° to start with; **Direction 2 angle** turns the other way too.

**Swept Boss/Base** and **Swept Cut** carry a profile along a path. Select the closed profile sketch, then
Ctrl+click the path: an open sketch chain or a helix. If you select only a path, a circle (5 mm across to
start with) is swept; choose **Sketch profile** or **Circular profile** and its **Diameter**. **Twist**
turns the profile along the path.

**Lofted Boss/Base** and **Lofted Cut** blend between two or more profile sketches. Select them in order
(Ctrl+click), then any guide curves. The Properties panel lists **Profiles, in order** (reorder with the
arrows) and **Guide curves**, and sets **Start tangency** and **End tangency**.

**Helix/Spiral** makes a curve, not material: select a sketch with a circle, and the helix starts on it
(5 mm pitch, 4 revolutions to start with). Choose **Pitch and Revolution**, **Height and Pitch**,
**Height and Revolution** or **Spiral**, plus **Taper angle**, **Start angle**, **Clockwise** or
**Counterclockwise**. Use a helix as a sweep path (a spring) or a curve-pattern path.

**Box** and **Cylinder** add a 20 mm cube or a Ø20 × 20 mm cylinder on the Top Plane. Edit their sketch
and extrude like any other.

## Holes and threads

**Hole Wizard** puts a hole at every point of the selected sketch. If you select a flat face instead, it
puts one hole at the face's center (an ISO M6 simple hole, **Through All**); edit the hole's sketch to
move it or add more points. In the Properties panel choose:

- the type: **Hole** (clearance size), **Counterbore**, **Countersink** or **Tapped Hole** (tap drill size,
  with the thread noted for drawings);
- **ISO** (M2 to M20) or **ANSI Inch** (#2 to 1/2) and the size;
- **Through All**, or untick it for a **Depth**.

The panel shows the drill, counterbore and countersink sizes it uses.

**Thread** models a real ISO or ANSI thread. Select a cylindrical face: a rod gets an outside thread, a
hole an inside one, and the nearest standard size is picked. The panel shows the major diameter, pitch and
tap drill. Untick **Full length of the face** to set a **Length**; **Offset from the start**, **Start from
the other end** and **Right-handed** are there too. For an inside thread, make the hole the tap drill
size. The [bottle cap tutorial](../tutorials/bottle-cap.md) shows a complete example, and
[Troubleshooting](../guide/troubleshooting.md#known-problems) lists what threads cannot do yet.

## Edges and faces

- **Fillet**: rounds the selected edges, or every edge of the selected faces (2 mm to start with).
  **Tangent propagation** also rounds edges that continue a selected edge smoothly. Round edges, such as
  a rod's end or a hole's rim, can be filleted too.
- **Chamfer**: bevels the selected edges (1 mm to start with): **Equal distance**,
  **Distance-distance** or **Angle-distance**, and **Flip direction**.
- **Variable Fillet**: select one straight edge; the radius runs from 1 to 3 mm along it. Add points with
  **Add a point** and set the radius at each (0 is the edge's start, 1 its end).
- **Face Fillet**: select faces; the first is set 1 and the rest set 2. It rounds where the sets meet.
- **Shell**: hollows the body and removes the selected faces (1 mm wall to start with). Set
  **Thickness**, or tick **Shell outward** to add the wall outside.
- **Draft**: select side faces to tilt them by an angle (3° to start with) about the Top Plane, or about a
  plane you also selected.
- **Rib**: select a sketch with an open line. The rib grows from it into the part (2 mm thick, on both
  sides of the sketch plane), runs on to the nearest wall and stops at the floor. **Flip material side**
  changes which way it grows.
- **Dome**: select a flat face to raise it into a smooth cap (2 mm to start with), or sink it.
- **Wrap**: select a sketch on a plane parallel to a cylinder's axis, then Ctrl+click the cylindrical
  face. **Emboss**, **Deboss** or **Scribe** rolls the sketch onto the surface without stretching it.
- **Fit**: moves the faces of a hole, pin, tab or slot into the part by a clearance, so printed parts
  slide together. See [The 3D Print tab](printing.md#fit-clearances).

## Patterns and mirror

These repeat the feature selected in the tree. With nothing selected they repeat every body.

- **Linear Pattern**: along a selected edge or axis (X to start with), 3 instances 20 mm apart. Set
  **Spacing**, **Instances**, **Reverse**, and **Direction 2** for a grid. List instances to leave out as
  row,column pairs separated by `;`.
- **Circular Pattern**: around a selected axis (Z to start with), 4 instances over 360°. Untick **Equal
  spacing** to set the angle between instances. List instances to leave out; 1 is the seed.
- **Mirror**: about a selected plane (the Right Plane to start with). **Merge solids** joins the mirrored
  copy to what it touches.
- **Sketch Pattern**: select the feature, then Ctrl+click a sketch of points. A copy goes at each point.
- **Curve Pattern**: select the feature, then Ctrl+click a sketch curve or helix. Copies are spread over
  the whole curve, or a set **Spacing** apart; **Turn with the curve** rotates them.

## Bodies

A part can hold several bodies, listed in the **Solid Bodies** folder of the feature tree. Click a body
there to select it; Ctrl+click to add more. A boss with **Merge result** unticked makes a new body.

- **Combine**: select a face of the main body, then Ctrl+click a face of each other body. Choose **Add**,
  **Subtract** or **Common**.
- **Split**: cuts the bodies with a selected plane or sketch (the Right Plane to start with).
- **Delete/Keep Body**: select a face of each body; choose **Delete these** or **Keep only these**.
- **Move/Copy Body**: moves the selected bodies (or all) by **ΔX**, **ΔY**, **ΔZ**, then optionally
  **Rotate** about an axis, or **Mate a face against a plane**. Tick **Copy (keep the original)** to copy.
- **Scale**: scales the selected bodies (or all) about the part origin or a selected point, **Uniform** or
  per axis.

## Reference geometry

- **Plane**: what you select decides the kind. Nothing or one plane or face: an offset plane (10 mm from
  the Top Plane to start with; set **Distance**, **Flip normal**). Three vertices: a plane through them.
  Two planes: the plane midway. A plane and an edge: a plane through the edge at an angle.
- **Axis**: from an edge, a cylindrical face, two vertices or two planes.
- **Point**: at a vertex, the center of a face or the center of an arc.
- **Coordinate System**: at a selected vertex, or at the origin.

New reference planes appear on the **Sketch** tab, so you can sketch on them in one click.

## Checking the part: the Evaluate tab

- **Measure**: distances, angles, radii and lengths of the selection. **Copy** puts the results on the
  clipboard.
- **Mass Properties**: volume, area, mass, center of mass and moments of inertia, for the part's material.
- **What's Wrong**: every rebuild error and warning, with a suggested fix. It opens by itself when a
  rebuild fails.
- **Equations** and **Appearance**: see
  [Equations, configurations, appearances and properties](design-intent.md).
- **Rebuild** (`Ctrl+Q`): rebuilds every feature from scratch. Normally only the features after a change
  are rebuilt.
