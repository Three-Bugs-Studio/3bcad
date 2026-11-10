# Files, import and export

## The File menu

- **New Part** (`Ctrl+N`), **New Assembly**, **New Drawing**.
- **Open…** (`Ctrl+O`) and **Open Recent**.
- **Save** (`Ctrl+S`) and **Save As…** (`Ctrl+Shift+S`).
- **Properties…**: part number, description, author and your own properties, used by drawings and the
  bill of materials (see [Equations, configurations, appearances and properties](design-intent.md)).
- **Import…** and **Export DXF…** (parts only).
- **Units** and **Material** (parts only).
- **Settings…** (`Ctrl+,`): language, theme, interface size, autosave, updates, shortcuts and more (see
  [View controls, display styles and settings](../guide/viewport-and-settings.md#settings)).

The **Help** menu has the keyboard shortcuts, this guide, the bug report form, **Open Log Folder**,
**Check for Updates…** and **About 3BCAD…**.

## 3BCAD's own files

| File | What it holds |
| --- | --- |
| `.3bcad` | A part: its feature history, equations, configurations, colours, units, material, print orientation and properties. The geometry is rebuilt when you open it. |
| `.3bcasm` | An assembly: the part files it uses (relative to the assembly), where each component sits, its mates and exploded view. |
| `.3bcdrw` | A drawing: the part it shows (relative to the drawing), its sheets, views, dimensions, notes and title block. |

The files are plain, readable JSON, so they work well with version control. A save writes a temporary
file first, so a failed save never destroys the previous one. A damaged file, or a file saved by a newer
3BCAD, is refused with a message instead of being opened half-read.

The title row shows `*` after the name when there are unsaved changes. **New**, **Open** and closing the
window ask **Save changes?** first: **Save**, **Don't Save** or **Cancel**.

### Autosave and recovery

Every 10 minutes (set in the File menu), 3BCAD saves a recovery copy of unsaved work. If 3BCAD closes
without saving, for example after a crash, it asks **Recover your work?** at the next start: **Recover**
opens the copy, which you then save yourself, and **Discard** deletes it. Recovery copies are removed
after a normal save or exit.

## Opening other formats

**File > Open…**, dragging a file onto the window, and starting 3BCAD with a file (`3bcad <file>`, which
is what "Open with" and file associations do) all open STEP (`.step`, `.stp`), STL, 3MF, OBJ and DXF files
as well as 3BCAD files. A file from another program opens as a **new part**, named after the file, so the
part you had open is kept if the file cannot be read. While you drag a file over the window, it shows
which files it can open; if you drop several, only the first is opened.

## Importing into the open part

**File > Import…** adds a file to the part you have open, as one feature:

- **STEP** (AP203, AP214 and AP242): each closed shell becomes a solid body under an **Imported**
  feature. Inch and metre files are scaled to millimetres. Shells that cannot be read are counted in the
  status bar instead of failing the whole file. Imported bodies take features like any other body:
  fillets, cuts, holes, shell.
- **STL, 3MF and OBJ**: the mesh becomes an **Imported Mesh**. It is drawn, measured, counted in mass
  properties and written to STL and 3MF exports, but it is a reference, not a solid: features cannot
  change it.
- **DXF**: lines, arcs, circles, polylines and splines become a new sketch, on the selected plane or flat
  face, or on the Top Plane when nothing is selected. The file's units are respected.

## Exporting

| What | Where | Notes |
| --- | --- | --- |
| STL, 3MF | **3D Print** tab: **Export STL**, **Export 3MF** | As the part sits on the printer's bed, meshed at the chosen quality. 3MF keeps one named object per body, the material and colours. |
| STEP | **3D Print** tab: **Export STEP** | The exact geometry in millimetres, in its model position, named after the part. Colours are written as AP214 face styles. |
| DXF of a sketch or face | **File > Export DXF…** | Select a sketch, or a flat face, first. |
| Send to Slicer | **3D Print** tab | Opens the part in OrcaSlicer, Bambu Studio or PrusaSlicer. |
| Assembly STEP or 3MF | **Assembly** tab: **STEP**, **3MF** | See [Assemblies](assemblies.md). |
| Drawing PDF, SVG or DXF | **Drawing** tab: **PDF**, **SVG**, **DXF** | See [Drawings](drawings.md). |
| Bill of materials | **Assembly** tab: **BOM**, **Export CSV…** | See [Assemblies](assemblies.md). |

For a part with several configurations, the 3D Print tab has an **Export configuration** box that picks
which one STL, 3MF, STEP and Send to Slicer use.

## Units

Each part has its own unit system: **MMGS** (millimetre), **CGS** (centimetre) or **IPS** (inch), and a
number of **Decimals**. Change them by clicking the unit system in the status bar, or in **File >
Units**. Units change what is shown and how typed values are read; the geometry is stored in millimetres.
You can always type another unit: `1in` in a millimetre part becomes 25.4 mm.

## Material

**File > Material** picks the part's material from the library (PLA, PETG, ABS, ASA, TPU, nylon, PC,
resin, aluminium, steels, brass, copper, titanium, oak) or lets you set your own **Name**, **Density** and
**Colour**. The material sets the mass in **Mass Properties**, the model's colour, the hatching in drawing
sections, the material written to 3MF, and the clearance **Fit** starts with.

## Where settings are kept

Preferences (recent files, autosave, slicer and its location, printer, check and export settings) are in
`settings.txt`, in `%APPDATA%\3BCAD` on Windows, `~/Library/Application Support/3BCAD` on macOS and
`~/.config/3bcad` on Linux. Recovery copies are in the `recovery` folder next to it.
