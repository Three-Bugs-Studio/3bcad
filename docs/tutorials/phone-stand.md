# A phone stand

You will draw the stand's side profile, extrude it to 60 mm wide, round the inside corner where the phone
sits and take the sharp edges off. Printed on its side, it needs no support.

The finished stand is 50 mm deep, 60 mm wide and about 69 mm tall. The phone leans back at about 63°
and stands on a 14 mm ledge behind an 18 mm lip, so phones up to about 12 mm thick, case included, fit.

## 1. Sketch the profile

1. Click **File > New Part**.
2. On the **Sketch** tab, click **Front Plane**. You look at the XZ plane: X to the right, Z up.
3. Click **Line**. Starting at the origin, click these corners in turn. They only need to be roughly
   right; the dimensions fix them in step 5.
   - the origin, (0, 0);
   - straight to the right, about (50, 0);
   - straight up, about (50, 70);
   - down and to the left, about (19, 10);
   - straight to the left, about (5, 10);
   - straight up, about (5, 18);
   - straight to the left, about (0, 18);
   - the origin again, to close the outline.

   Press `Esc` to end the line chain.
4. Every line except the long sloping one should show a horizontal or vertical relation. If one does
   not, click it and click **Horizontal** or **Vertical** under **Add relations** in the Properties panel.
5. Click **Smart Dimension** and dimension the lines. For each one, click the line, click beside it to
   place the dimension, type the value and press `Enter`:
   - the bottom line: `50`;
   - the back (right-hand) line: `70`;
   - the front (left-hand) line: `18`;
   - the short top line of the lip: `5`;
   - the lip's back line, from the lip's top down to the ledge: `8`;
   - the ledge: `14`.
6. The status bar shows **Fully Defined**. Click **✔ Exit Sketch**.

## 2. Extrude it

1. With **Sketch1** selected, click **Extruded Boss/Base** on the **Features** tab.
2. In the Properties panel, set **Depth** to `60`.

## 3. Round the inside corner

The ledge meets the sloping face in a sharp inside corner. A fillet there makes the ledge stronger.

1. Click the edge along that inside corner, where the ledge meets the sloping face.
2. Click **Fillet** and set **Radius** to `3`.

## 4. Chamfer the sharp edges

1. Click the edge along the top of the stand (where the back meets the sloping face).
2. Ctrl+click the two edges along the top of the lip, front and back.
3. Click **Chamfer**. It starts as **Equal distance**, **Distance** `1`: keep that.

The feature tree now shows **Sketch1**, **Boss-Extrude1**, **Fillet1** and **Chamfer1**.
**Evaluate > Mass Properties** gives a volume of about 88 160 mm³.

## 5. Lay it on its side

Printed standing up, the layers would run across the thin lip and it could snap off. On its side, every
layer is the whole profile.

1. Open the **3D Print** tab and pick your printer.
2. Click one of the two large flat side faces, then click **Lay Flat**. The stand now lies on the bed.
3. Click **Overhangs**: nothing is coloured, so the stand needs no support.

## 6. Export STL/3MF / Send to Slicer

1. Keep **Quality: Fine**.
2. Click **Export 3MF** (or **Export STL**) and choose a file name, or pick your slicer and click **Send to
   Slicer**.
3. Save the part with `Ctrl+S`.

## Make it your own

Double-click **Sketch1** to change its dimensions. A longer back line makes a taller stand; a longer ledge
takes thicker phones. The fillet and chamfers follow their edges.
