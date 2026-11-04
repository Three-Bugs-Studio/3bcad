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
