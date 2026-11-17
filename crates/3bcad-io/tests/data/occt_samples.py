"""Writes OCCT-made STEP samples for 3BCAD's importer tests and prints their exact volumes."""
import sys
from OCP.BRepPrimAPI import BRepPrimAPI_MakeBox, BRepPrimAPI_MakeCylinder, BRepPrimAPI_MakeSphere, BRepPrimAPI_MakeCone
from OCP.BRepAlgoAPI import BRepAlgoAPI_Cut
from OCP.BRepFilletAPI import BRepFilletAPI_MakeFillet
from OCP.TopExp import TopExp, TopExp_Explorer
from OCP.BRep import BRep_Tool
from OCP.TopAbs import TopAbs_EDGE
from OCP.TopoDS import TopoDS
from OCP.gp import gp_Ax2, gp_Pnt, gp_Dir
from OCP.STEPControl import STEPControl_Writer, STEPControl_AsIs
from OCP.Interface import Interface_Static
from OCP.GProp import GProp_GProps
from OCP.BRepGProp import BRepGProp

out = sys.argv[1]

def volume(s):
    p = GProp_GProps(); BRepGProp.VolumeProperties_s(s, p); return p.Mass()

def write(name, shape, unit="MM", schema="AP214IS"):
    Interface_Static.SetCVal_s("write.step.schema", schema)
    Interface_Static.SetCVal_s("write.step.unit", unit)
    w = STEPControl_Writer(); w.Transfer(shape, STEPControl_AsIs); w.Write(f"{out}/{name}.step")
    print(name, f"{volume(shape):.4f}")

box = BRepPrimAPI_MakeBox(40.0, 30.0, 10.0).Shape()
hole = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(20, 15, -1), gp_Dir(0, 0, 1)), 5.0, 12.0).Shape()
write("occt_block_hole", BRepAlgoAPI_Cut(box, hole).Shape())
write("occt_sphere", BRepPrimAPI_MakeSphere(gp_Pnt(5, 5, 5), 8.0).Shape(), schema="AP203")
rod = BRepPrimAPI_MakeCylinder(10.0, 20.0).Shape()
rf = BRepFilletAPI_MakeFillet(rod)
ex = TopExp_Explorer(rod, TopAbs_EDGE)
while ex.More():
    edge = TopoDS.Edge(ex.Current())
    if BRep_Tool.Pnt_s(TopExp.FirstVertex_s(edge)).Z() > 19.0:
        rf.Add(2.0, edge)
        break
    ex.Next()
write("occt_rounded_rod", rf.Shape(), schema="AP242DIS")
write("occt_cone", BRepPrimAPI_MakeCone(10.0, 4.0, 15.0).Shape())
b = BRepPrimAPI_MakeBox(20.0, 20.0, 20.0).Shape()
f = BRepFilletAPI_MakeFillet(b)
e = TopExp_Explorer(b, TopAbs_EDGE)
f.Add(3.0, TopoDS.Edge(e.Current()))
write("occt_fillet", f.Shape())

# Cases that tripped the importer on downloaded files (v1.0).
from OCP.BRepAlgoAPI import BRepAlgoAPI_Fuse

def all_edges(shape):
    e = TopExp_Explorer(shape, TopAbs_EDGE)
    while e.More():
        yield TopoDS.Edge(e.Current())
        e.Next()

# Onshape writes metres: a 2 mm hole is 0.002 in the file.
plate = BRepPrimAPI_MakeBox(6.0, 6.0, 1.0).Shape()
pin = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(3, 3, -1), gp_Dir(0, 0, 1)), 1.0, 3.0).Shape()
write("occt_tiny_hole_m", BRepAlgoAPI_Cut(plate, pin).Shape(), unit="M")
# Every edge rounded: cylinders along the edges, sphere octants at the corners.
box2 =BRepPrimAPI_MakeBox(20.0, 14.0, 10.0).Shape()
rb = BRepFilletAPI_MakeFillet(box2)
for edge in all_edges(box2):
    rb.Add(2.0, edge)
write("occt_rounded_box", rb.Shape())
# A drilled hole with a 118 degree point and a countersink: cones that end at their apex.
blk = BRepPrimAPI_MakeBox(gp_Pnt(-10, -10, 0), 20.0, 20.0, 15.0).Shape()
drill = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 5), gp_Dir(0, 0, 1)), 3.0, 11.0).Shape()
tip = BRepPrimAPI_MakeCone(gp_Ax2(gp_Pnt(0, 0, 5), gp_Dir(0, 0, -1)), 3.0, 0.0, 3.0 / 1.6642794823505).Shape()
sink = BRepPrimAPI_MakeCone(gp_Ax2(gp_Pnt(0, 0, 12), gp_Dir(0, 0, 1)), 3.0, 6.0, 3.0).Shape()
tool = BRepAlgoAPI_Fuse(BRepAlgoAPI_Fuse(drill, tip).Shape(), sink).Shape()
write("occt_drill_point", BRepAlgoAPI_Cut(blk, tool).Shape())
# SolidWorks and Parasolid bound a tube's cylinders by two full circles, with no seam edge.
tube = BRepAlgoAPI_Cut(
    BRepPrimAPI_MakeCylinder(5.5, 30.0).Shape(), BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -1), gp_Dir(0, 0, 1)), 4.0, 32.0).Shape()
).Shape()
write("occt_tube", tube)


def without_seams(name):
    """Rewrites each cylindrical face's one loop (circle, seam, circle, seam) as two circle loops."""
    path = f"{out}/{name}.step"
    text = open(path).read()
    body = {}
    for stmt in text.split(";"):
        s = stmt.strip()
        if s.startswith("#") and "=" in s:
            k, v = s.split("=", 1)
            body[k.strip()] = v.strip()
    refs = lambda v: ["#" + r.split(")")[0].split(",")[0].strip() for r in v.split("#")[1:]]
    top = max(int(k[1:]) for k in body) + 1
    extra = []
    for face, v in list(body.items()):
        if not v.startswith("ADVANCED_FACE") or "CYLINDRICAL_SURFACE" not in body[refs(v)[-1]]:
            continue
        bound = refs(v)[0]
        loop = refs(body[bound])[0]
        oriented = refs(body[loop])
        curves = [refs(body[o])[-1] for o in oriented]
        seam = next(c for c in curves if curves.count(c) == 2)
        circles = [o for o, c in zip(oriented, curves) if c != seam]
        sense = body[bound].split(",")[-1].rstrip(")")
        new_bounds = []
        for o in circles:
            extra.append(f"#{top} = EDGE_LOOP('',({o}));")
            extra.append(f"#{top + 1} = FACE_BOUND('',#{top},{sense});")
            new_bounds.append(f"#{top + 1}")
            top += 2
        text = text.replace(f"{face} = {v};", f"{face} = ADVANCED_FACE('',({','.join(new_bounds)}),{v.rsplit(',', 2)[-2]},{v.rsplit(',', 1)[-1]};")
    text = text.replace("ENDSEC;\nEND-ISO", "\n".join(extra) + "\nENDSEC;\nEND-ISO", 1)
    open(f"{out}/{name}_two_loops.step", "w").write(text)


without_seams("occt_tube")

# Cases that tripped the importer on downloaded files (v1.2). The rewrites stand in for other
# writers' habits; each prints the volume OCCT reads back from the rewritten file.
import math
import re
from OCP.BRepAlgoAPI import BRepAlgoAPI_Common
from OCP.BRepBuilderAPI import BRepBuilderAPI_MakeFace, BRepBuilderAPI_NurbsConvert
from OCP.BRepFilletAPI import BRepFilletAPI_MakeChamfer
from OCP.BRepPrimAPI import BRepPrimAPI_MakeHalfSpace
from OCP.STEPControl import STEPControl_Reader
from OCP.gp import gp_Ax3, gp_Torus


def read_back(name):
    r = STEPControl_Reader()
    r.ReadFile(f"{out}/{name}.step")
    r.TransferRoots()
    print(f"  {name} read back: {volume(r.OneShape()):.4f}")


def entities(text):
    return {int(m.group(1)): m.group(2) for m in re.finditer(r"#(\d+)\s*=\s*([^;]*);", text)}


ids = lambda body: [int(r) for r in re.findall(r"#(\d+)", body)]
numbers = lambda body: [float(x) for x in re.sub(r"\s", "", body).split("(")[-1].rstrip(")").split(",")]

# A boss rounded into its plate: a concave torus blend bounded by two full circles, a reversed face
# whose loops OCCT writes the way its surface runs (the shell did not close).
plate = BRepPrimAPI_MakeBox(gp_Pnt(-15, -15, 0), 30.0, 30.0, 5.0).Shape()
boss = BRepAlgoAPI_Fuse(plate, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 5), gp_Dir(0, 0, 1)), 6.0, 8.0).Shape()).Shape()
bf = BRepFilletAPI_MakeFillet(boss)
for edge in all_edges(boss):
    a, b = BRep_Tool.Pnt_s(TopExp.FirstVertex_s(edge)), BRep_Tool.Pnt_s(TopExp.LastVertex_s(edge))
    if abs(a.Z() - 5) < 1e-6 and abs(b.Z() - 5) < 1e-6 and abs(math.hypot(a.X(), a.Y()) - 6) < 1e-6:
        bf.Add(2.0, edge)
write("occt_boss_fillet", bf.Shape())

# Holes along x whose circles are placed with no reference direction (NIST's STC-07 has 72): truck
# takes (1,0,0), along the axis, and the arcs came out NaN. Only circles lose theirs: a surface's
# reference direction is what its pcurves are measured from.
xb = BRepPrimAPI_MakeBox(30.0, 20.0, 20.0).Shape()
for y, z, r in [(6, 6, 3.0), (14, 13, 4.0)]:
    xb = BRepAlgoAPI_Cut(xb, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-1, y, z), gp_Dir(1, 0, 0)), r, 32.0).Shape()).Shape()
write("occt_x_holes", xb)
text = open(f"{out}/occt_x_holes.step").read()
ents = entities(text)
for k in {ids(b)[0] for b in ents.values() if b.startswith("CIRCLE(")}:
    if len(ids(ents[k])) == 3 and abs(numbers(ents[ids(ents[k])[1]])[0]) == 1.0:
        text = re.sub(rf"(#{k}\s*=\s*AXIS2_PLACEMENT_3D\('',#\d+,#\d+,)#\d+\)", r"\1$)", text)
open(f"{out}/occt_x_holes.step", "w").write(text)
read_back("occt_x_holes")

# Blind holes drilled every which way, each ending in a 118 degree point placed at its apex with
# radius 0 (as CATIA writes NIST CTC-02's): the apex vertex lands a rounding error either side of
# the apex, and on the wrong side the cone was built the other way.
db = BRepPrimAPI_MakeBox(gp_Pnt(-20, -20, -20), 40.0, 40.0, 40.0).Shape()
tip = 2.0 / math.tan(math.radians(59.0))
for i in range(8):
    a, b = 2 * math.pi * i / 8, math.radians(25 + 5 * (i % 4))
    d = gp_Dir(math.cos(a) * math.sin(b), math.sin(a) * math.sin(b), math.cos(b))
    at = lambda t: gp_Pnt(d.X() * t, d.Y() * t, d.Z() * t)
    drill = BRepPrimAPI_MakeCylinder(gp_Ax2(at(30), d.Reversed()), 2.0, 22.0).Shape()
    point = BRepPrimAPI_MakeCone(gp_Ax2(at(8 - tip), d), 0.0, 2.0, tip).Shape()
    db = BRepAlgoAPI_Cut(db, BRepAlgoAPI_Fuse(drill, point).Shape()).Shape()
write("occt_drill_points", db)

# A slotted, chamfered screw head domed by a spindle torus (minor radius 54.87 over major 8.25, as
# OCCT's screw.step): truck's torus normal is backwards on the inner, lemon part and its search
# misses there, as on the chamfer cone past its unit line; the torus seam splits one slot edge, whose
# halves share one B-spline. OCCT now writes such a torus as a surface of revolution; the file gets
# the TOROIDAL_SURFACE older writers gave, its edges on their 3D curves.
R, r = 8.25, 54.873718663856
dome = BRepBuilderAPI_MakeFace(
    gp_Torus(gp_Ax3(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1), gp_Dir(0, 1, 0)), R, r),
    0.0, 2 * math.pi, math.acos(-R / r), math.acos(-(R + 12.0) / r),
).Face()
head = BRepAlgoAPI_Common(BRepPrimAPI_MakeCylinder(10.0, 60.0).Shape(), BRepPrimAPI_MakeHalfSpace(dome, gp_Pnt(0, 0, 0)).Solid()).Shape()
ch = BRepFilletAPI_MakeChamfer(head)
for edge in all_edges(head):
    if abs(BRep_Tool.Pnt_s(TopExp.FirstVertex_s(edge)).Z()) < 1e-6:
        ch.Add(1.5, edge)
slot = BRepPrimAPI_MakeBox(gp_Pnt(-20, -1.25, 50), 40.0, 2.5, 10.0).Shape()
write("occt_screw_head", BRepAlgoAPI_Cut(ch.Shape(), slot).Shape())
text = open(f"{out}/occt_screw_head.step").read()
ents = entities(text)
top = max(ents)
for k, b in ents.items():
    if b.startswith("SURFACE_OF_REVOLUTION("):
        loc, axis = ids(ents[ids(b)[1]])
        torus = f"#{k} = TOROIDAL_SURFACE('',#{top + 1},{R},{r});\n#{top + 1} = AXIS2_PLACEMENT_3D('',#{loc},#{axis},#{top + 2});\n"
        text = re.sub(rf"#{k}\s*=\s*SURFACE_OF_REVOLUTION\([^;]*\);", torus + f"#{top + 2} = DIRECTION('',(0.,1.,0.));", text)
        top += 2
on_curve = {k: ids(b)[0] for k, b in ents.items() if b.startswith(("SURFACE_CURVE(", "SEAM_CURVE("))}


def edge_on_3d_curve(m):
    name, c, sense = m.group(2).rsplit(",", 2)
    return f"{m.group(1)}EDGE_CURVE({name},#{on_curve.get(int(c.strip()[1:]), c.strip()[1:])},{sense})"


text = re.sub(r"(#\d+\s*=\s*)EDGE_CURVE\(([^;]*)\)", edge_on_3d_curve, text)
open(f"{out}/occt_screw_head.step", "w").write(text)
read_back("occt_screw_head")

# A tube whose walls are periodic B-splines, written unclamped as NIST's CTC-02 AP203 writes its
# curved wall: truck sees no period in a B-spline and its mesher never finishes such a face, so
# the import must refuse it, saying why.
write("occt_tube_bspline", BRepBuilderAPI_NurbsConvert(tube, True).Shape())

# A box of B-spline faces, its top's surface lifted 0.1 mm off its edges (where truck's search
# placed linkrods.step's corner blends): the import must warn.
write("occt_lifted_face", BRepBuilderAPI_NurbsConvert(BRepPrimAPI_MakeBox(20.0, 20.0, 20.0).Shape(), True).Shape())
text = open(f"{out}/occt_lifted_face.step").read()
ents = entities(text)
for k, b in ents.items():
    points = [p for p in ids(ents[ids(b)[-1]]) if ents[p].startswith("CARTESIAN_POINT")] if b.startswith("ADVANCED_FACE") else []
    if points and all(abs(numbers(ents[p])[2] - 20) < 1e-9 for p in points):
        for p in points:
            x, y, z = numbers(ents[p])
            text = re.sub(rf"#{p}\s*=\s*CARTESIAN_POINT\([^;]*\);", f"#{p} = CARTESIAN_POINT('',({x},{y},{z + 0.1}));", text)
open(f"{out}/occt_lifted_face.step", "w").write(text)
