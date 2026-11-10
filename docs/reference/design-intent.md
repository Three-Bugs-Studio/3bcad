# Equations, configurations, appearances and properties

These tools record what a part means, not only its shape: values that depend on each other, variants of
the same part, colours, and the facts a drawing or a bill of materials needs.

## Variables and equations

Click **Equations** on the **Evaluate** tab. Each row has a **Name**, an **Expression** and the resulting
**Value**.

- A **variable** is a row with a new name: `wall` = `2`.
- An **equation** sets a dimension: `D1@Boss-Extrude1` = `wall * 10`. Click **+ Dimension** for a list of
  every dimension in the part with its name and value; picking one adds a row for it.

Click **+ Variable** to add a row, **✖** to remove one and **Apply** to rebuild the part with the
results. Expressions can use:

- units: `mm`, `cm`, `m`, `in` (or `"`), `ft`, `deg`, `rad`; for example `5 mm`, `1in`, `30deg`;
- `+ - * / ^`, brackets and `pi`;
- functions: `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `sqrt`, `abs`, `min`, `max`, `round`, `floor`,
  `ceil` (angles in degrees).

Equations are solved in dependency order, whatever order they are listed in. A circular reference, an
unknown name or a typing mistake is shown on the equation that has it, and the other equations still
apply.

Sketch dimensions are named `D1@Sketch1`, `D2@Sketch1` and so on. A dimension keeps its number when
others are deleted, and the sketch's dimension box shows it. Feature values are named after the feature,
like `D1@Boss-Extrude1` for an extrusion's depth. Length boxes everywhere take arithmetic too, such as
`10 + 2in`.

## Configurations

A configuration is a variant of the same part: for example a short and a long version of a bracket.
Each configuration has its own suppressed features and can have its own equations.

- Switch configuration with the **Configuration** box at the top of the feature tree.
- **New / Edit Configurations…** at the end of that list, or the **Configuration** row of the Equations
  window, adds, renames and deletes configurations (**+ New**, **Delete**).
- To leave a feature out of the active configuration, right-click it in the tree and choose
  **Suppress**.
- To change a dimension in one configuration only, add its equation in the Equations window and tick
  **This configuration only**.

On the 3D Print tab, **Export configuration** picks the configuration that STL, 3MF and STEP exports and
Send to Slicer use.

## Appearances

Click **Appearance** on the **Evaluate** tab after picking a face, or right-click a feature or a body in
the tree and choose **Appearance…**. Choose what to colour: the **Face**, its **Body** or the **Feature**
that made it, then the **Colour**. **Remove colour** takes it off again.

A face shows its own colour if it has one, else its body's, else its feature's, else the material's.
Colours follow their faces through rebuilds, show in assemblies, and are written to 3MF and STEP exports.

## Properties

**File > Properties…** lists name and value rows for the open part, assembly or drawing. Buttons add the
usual ones (**+ Part number**, **+ Description**, **+ Author**) and **+ Other** adds your own. Click
**Apply** to keep them; they are saved with the file.

Properties fill in drawing title blocks and notes, where `{Part number}` or any other property name in
braces is replaced by its value, and they are the columns of an assembly's bill of materials.
