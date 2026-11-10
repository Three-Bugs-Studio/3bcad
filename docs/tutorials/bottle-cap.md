# A threaded bottle cap

You will make a screw cap with an M20 × 2.5 thread inside, for a bottle or jar with an M20 neck.
The outside is a hexagon, so it is easy to grip. You will use a polygon sketch, **Shell**, a reference
plane and the **Thread** feature.

The finished cap is 32 mm across the corners (27.7 mm across the flats) and 18 mm tall. Inside, the thread
runs 10 mm. Below it is a 4 mm deep space for a seal disc up to about 21 mm across, and above it a 1 mm
lead-in that helps the neck start straight.

## 1. Hexagon

1. Click **File > New Part**.
2. On the **Sketch** tab, click **Top Plane**.
3. Click **Polygon**. In the Properties panel, set **Sides** to `6` and keep **Inscribed circle (click a
   vertex)**.
4. Click the origin, then click straight to the right of it to place a corner.
5. Click **Smart Dimension**, click the polygon's construction circle (through its corners), place the
   dimension, type `32` and press `Enter`.
6. Click the top edge of the hexagon and click **Horizontal** under **Add relations**. The sketch is now
   **Fully Defined**.
7. Click **✔ Exit Sketch**, then **Extruded Boss/Base** with **Depth** `18`.

## 2. Hollow it

1. Click the top face.
2. Click **Shell** and set **Thickness** to `3`.

The cap is now a hexagonal cup with 3 mm walls and a 3 mm floor.

## 3. A plane for the thread's ring

The thread goes in a ring inside the cup, from 7 mm to 17 mm above the bed. The ring starts on a
reference plane.

1. Press `Esc` so that nothing is selected.
2. On the **Features** tab, click **Plane**. It is made 10 mm above the Top Plane.
3. In the Properties panel, set **Distance** to `7`. The feature tree shows **Plane1**.

## 4. The ring

1. On the **Sketch** tab, click **Plane1**.
2. Click **Circle**. Click the origin, then click to set a size. Draw a second circle on the origin the
   same way.
3. Click **Smart Dimension**. Dimension the larger circle `24` and the smaller one `17.5`. 17.5 mm is the
   tap drill size for M20.
4. Click **✔ Exit Sketch**, then **Extruded Boss/Base** with **Depth** `10`.

The ring's outside runs into the cup's walls, so it joins them and the part stays one body (the body
count in the status bar stays at 1).

## 5. Thread

1. Click the round face inside the ring.
2. On the **Features** tab, click **Thread**. 3BCAD picks the standard size nearest the hole: **M20**.
3. The Properties panel shows **Major Ø20.00 mm, pitch 2.500 mm, tap drill Ø17.50 mm**. Keep **Full
   length of the face** and **Right-handed** ticked.

The thread takes a moment to build. It runs out at both ends of the ring, into the space below and the
lead-in above. **Evaluate > Mass Properties** gives a volume of about 7 195 mm³.

## 6. Export STL/3MF / Send to Slicer

The cap prints the way it is modelled, floor on the bed.

1. Open the **3D Print** tab and pick your printer.
2. Click **Overhangs**. The underside of the ring, a ledge 2 to 4 mm wide held by the wall, and the
   downward-facing sides of the thread are coloured. Most printers print both without support; if yours
   sags, turn on supports in the slicer.
3. Keep **Quality: Fine**. Threads need it: the coarse mesh can leave gaps.
4. Click **Export 3MF** (or **Export STL**), or pick your slicer and click **Send to Slicer**.
5. Save the part with `Ctrl+S`.

## Try it

The thread is a standard M20 × 2.5 (coarse) thread, so an M20 bolt screws in. That is a quick way to check
your print before you make a matching neck.

## Why the cap is built this way

3BCAD's thread feature needs the threaded face to be open at both ends, so the ring stops short of the
floor and of the rim. It also cannot yet join a ring to a round cup, which is why the cup is a hexagon. See
[Known problems](../guide/troubleshooting.md#known-problems).
