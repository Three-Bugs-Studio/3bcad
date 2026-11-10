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
