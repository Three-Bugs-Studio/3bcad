# The 3D Print tab

The **3D Print** tab gets a part ready for your printer: it shows the part on your printer's bed,
colours what needs support or is too thin, exports STL and 3MF, and opens the part in your slicer. The
settings on this tab are remembered between sessions.

## Printer and bed

Pick your printer from the list: Bambu Lab X1 / P1, A1 or A1 mini, Prusa MK4, MINI or XL, Creality
Ender 3, or Voron 2.4 (350). The three boxes under it are the build volume in X, Y and Z; change one and
the printer becomes **Custom**. Below them, **✔ Fits** shows the part's size on the bed, or a warning says
how much it is too long on which axis.

The part stands on its bottom to start with. To print it on another face:

1. Click the face that should go on the bed.
2. Click **Lay Flat**.

You can also select a straight edge or an axis: **Lay Flat** then points it straight up. **Reset** puts
the part back on its bottom. The orientation is saved with the part, and STL and 3MF exports are written
as the part sits on the bed.

## Checks

- **Overhangs** colours the faces that need support when printed the way the part is laid out: faces
  that lean out further than the **Overhang** angle (45° from vertical to start with). Faces resting on
  the bed do not count. Turn the part with **Lay Flat** until the coloured area is small.
- **Thin Walls** colours walls thinner than the **Nozzle** in red and walls thinner than **Min wall** in
  orange. Set **Nozzle** (0.4 mm to start with) and **Min wall** (0.8 mm; usually two or three nozzle
  widths) for your printer.

The legend next to the buttons gives the coloured area. The colours update as you edit the part.

## Export

**Export STL**, **Export 3MF** and **Export STEP** save the part. STL and 3MF are meshed at the
**Quality** next to them:

- **Coarse**: chord 0.1 mm, angle 30°;
- **Fine** (the default): chord 0.01 mm, angle 10°;
- **Custom**: your own **Chord** (how far a triangle may lie from the true surface) and **Angle** (how
  much a curved surface may turn within one triangle).

The triangle count of the export at that quality is shown before you export. A 3MF file holds one named
object per body, with the part's material and colours, and the printer and nozzle it was prepared for.
STEP is the exact geometry in its model position, for other CAD programs. If the part has several
configurations, **Export configuration** picks the one to export.

## Send to Slicer

Choose **OrcaSlicer**, **Bambu Studio** or **PrusaSlicer** in the list and click **Send to Slicer**. 3BCAD
writes the part on its bed and opens it in that slicer. It looks in the slicer's usual install folder and
on the `PATH`. If the slicer is somewhere else:

- click **Locate…** under the list and pick the slicer's program; **Default** forgets it again; or
- click **Send to Slicer** anyway: 3BCAD says the slicer was not found and offers **Locate…**.

## Split for printing

A part too big for the bed can be printed in pieces and glued together. **Split for Printing** cuts the
part by planes and adds alignment pins across each cut, so the pieces line up again.

1. Make a plane where each cut should go, if the standard planes are not in the right place: **Plane**
   on the **Features** tab, offset from the Right, Front or Top Plane. The **✔ Fits** line under the
   printer shows how long the part is.
2. Select the planes (Ctrl+click for more), then click **Split for Printing**. With nothing selected, it
   cuts at the Right Plane.
3. The Properties panel of the new **Split** feature sets the pins:
   - **Diameter** and **Length** of the pins (5 mm and 10 mm to start with); half of each pin goes into
     each piece;
   - **Pins per plane** (two to start with; two stop the pieces turning about a pin);
   - **Clearance** per side, which starts at the slip fit for the part's material, with the same presets
     as Fit (below);
   - **Pins on one piece**: at each cut one piece carries the pins and the other has the holes, or
     **Separate dowels**: both pieces get holes and the pins become bodies of their own, to print
     separately (or use metal dowels of that size).

The pins go where the cut face has room for the hole plus a wall of material round it (0.3 × the
diameter, at least 1 mm), spread as far apart as the face allows, and only where the material behind the
face is deep enough for the hole. If a cut face has room for fewer pins than asked for, the split still
happens and the What's Wrong list says how many fitted. Holes are wider than the pins by the clearance
on each side and deeper by the clearance.

**Export 3MF** then writes each piece (and each dowel) as an object of its own, so the slicer can lay
them out on the bed; turn each piece onto its cut face in the slicer. More planes can be added to the
Split later, under **Add the selected plane**.

## Fit clearances

Two printed parts made to the same size do not slide together: plastic spreads a little. **Fit**, on
the **Features** tab, moves faces into the part by a clearance. Holes grow; pins, tabs and the walls of
slots shrink. Fit only one part of each pair.

1. Select the faces to move. One click on a round face takes the whole hole or pin; for a tab or slot,
   Ctrl+click each flat face.
2. Click **Fit**. The clearance starts at the slip fit for the part's material.
3. In the Properties panel, type a **Clearance**, or click a preset. The presets are per side, in mm:

| Process | Press | Slip | Loose |
| --- | --- | --- | --- |
| FDM: PLA, PETG | 0.05 | 0.15 | 0.3 |
| FDM: ABS, ASA, nylon | 0.1 | 0.2 | 0.35 |
| Resin (SLA, DLP) | 0.025 | 0.075 | 0.15 |
| Powder (SLS, MJF) | 0.1 | 0.2 | 0.3 |

Press fits hold by friction, slip fits slide by hand, and loose fits move freely. Printers differ, so
print a small test pair before a big part. The [enclosure tutorial](../tutorials/enclosure.md) uses a
slip fit on a lid.
