# An enclosure with a lid

You will make a small project box, 80 × 60 × 30 mm with 2 mm walls and a cable hole, and a lid with a
lip that fits inside it. The lid gets a slip fit from the **Fit** presets so it goes on by hand and stays
on, and a hole for a 5 mm LED. The box and the lid are two parts, each in its own file.

## The box

### 1. Block

1. Click **File > New Part**.
2. On the **Sketch** tab, click **Top Plane**.
3. Click **Center Rectangle**. Click the origin, then click up and to the right.
4. Click **Smart Dimension**. Click the bottom line, place the dimension, type `80` and press `Enter`.
   Click the right-hand line, place it, type `60` and press `Enter`.
5. Click **✔ Exit Sketch**.
6. Click **Extruded Boss/Base** and set **Depth** to `30`.

### 2. Hollow it out

1. Click the top face of the block.
2. Click **Shell**. The top face is removed and the block is hollowed.
3. In the Properties panel, set **Thickness** to `2`.

The box now has 2 mm walls and a 2 mm floor. Inside, it is 76 × 56 mm and 28 mm deep.

### 3. Cable hole

1. Turn the view so you see the front face of the box, the one facing the -Y direction. Click it.
2. On the **Sketch** tab, click **Sketch**. The view turns to face it. The part origin is at the middle of
   the face's bottom edge.
3. Click **Circle**. Click a point to the right of the middle, then click again to set a size.
4. Click **Smart Dimension**:
   - click the circle, place the dimension, type `11` and press `Enter`;
   - click the origin, then the circle's center, and place the dimension to the side so it measures up:
     type `15` and press `Enter`;
   - click the origin and the circle's center again, and place the dimension above them so it measures
     across: type `20` and press `Enter`.
5. Click **✔ Exit Sketch**.
6. Click **Extruded Cut**. Keep the **Blind** depth of `10`: it goes through the 2 mm wall and stops in the
   empty box. (**Through All** would drill the back wall too.)

### 4. Save the box

Press `Ctrl+S` and save it as `enclosure.3bcad`. **Evaluate > Mass Properties** gives a volume of about
24 642 mm³.

## The lid

### 5. Plate

1. Click **File > New Part**.
2. On the **Sketch** tab, click **Top Plane**. Draw a **Center Rectangle** on the origin and dimension it
   `80` by `60`, as for the box.
3. Click **✔ Exit Sketch**, then **Extruded Boss/Base** with **Depth** `2`.

### 6. Lip

The lip is a 2 mm wall that fits inside the box's 76 × 56 mm opening.

1. Click the top face of the plate. On the **Sketch** tab, click **Sketch**.
2. Draw a **Center Rectangle** on the origin and dimension it `76` by `56`.
3. Click **✔ Exit Sketch**, then **Extruded Boss/Base** with **Depth** `4`.
4. Click the top face of the block you just made. On the **Sketch** tab, click **Sketch**.
5. Draw a **Center Rectangle** on the origin and dimension it `72` by `52`.
6. Click **✔ Exit Sketch**, then **Extruded Cut** with **Depth** `4`.

The block is now a hollow lip, 4 mm tall, standing on the plate.

### 7. LED hole

1. Click the flat face inside the lip. On the **Sketch** tab, click **Sketch**.
2. Click **Circle**, click the origin, then click to set a size. Dimension it `5.5` with **Smart
   Dimension**.
3. Click **✔ Exit Sketch**, then **Extruded Cut**. Set the end condition to **Through All**.

### 8. Fit the lip

The lip is exactly as big as the box's opening, so it would not go in. **Fit** moves its outside faces in
by a clearance.

1. Click one of the four outside faces of the lip, then Ctrl+click the other three.
2. On the **Features** tab, click **Fit**.
3. The clearance starts at the slip fit for the part's material, `0.15` mm for PLA. To change it, click a
   preset in the Properties panel: on the **FDM: PLA, PETG** row, **Press** is 0.05, **Slip** 0.15 and
   **Loose** 0.3 mm.

With the slip fit, the lip is 75.7 × 55.7 mm: 0.15 mm smaller than the opening on every side. If you
print in another material, set it first with **File > Material**; the presets are listed in
[The 3D Print tab](../reference/printing.md#fit-clearances).

### 9. Save the lid

Save it as `enclosure-lid.3bcad`. Its volume is about 11 442 mm³.

## 10. Export STL/3MF / Send to Slicer

Both parts print the way they are modelled: the box with its open side up, the lid with its lip up. The
flat outside of the lid is on the bed, so it comes out smooth. For each part:

1. Open the **3D Print** tab and pick your printer. **✔ Fits** shows the part's size.
2. Keep **Quality: Fine**.
3. Click **Export 3MF** (or **Export STL**), or pick your slicer and click **Send to Slicer**.

To print both in one go, put them in an assembly (**File > New Assembly**, then **Insert** each part) and
click **3MF** or **Send to Slicer** on the **Assembly** tab. See [Assemblies](../reference/assemblies.md).

When the parts are printed, turn the lid over and press it into the box.
