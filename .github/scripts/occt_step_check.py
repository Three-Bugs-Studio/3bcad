"""Reads 3BCAD's STEP exports with OpenCASCADE and checks each solid's volume, units and name, and
for an assembly (a line with a bounding box after the volume) that its components are placed, and
(colors.txt: file, sRGB colour, centre) that a face of that colour has its centre there.

Usage: python occt_step_check.py <folder with *.step, volumes.txt and colors.txt>
"""
import pathlib
import sys

from OCP.Bnd import Bnd_Box
from OCP.BRepBndLib import BRepBndLib
from OCP.BRepGProp import BRepGProp
from OCP.GProp import GProp_GProps
from OCP.IFSelect import IFSelect_RetDone
from OCP.Quantity import Quantity_Color, Quantity_TypeOfColor
from OCP.STEPCAFControl import STEPCAFControl_Reader
from OCP.STEPControl import STEPControl_Reader
from OCP.TCollection import TCollection_ExtendedString
from OCP.TDocStd import TDocStd_Document
from OCP.TopAbs import TopAbs_FACE
from OCP.TopExp import TopExp_Explorer
from OCP.XCAFDoc import XCAFDoc_ColorType, XCAFDoc_DocumentTool

folder = pathlib.Path(sys.argv[1])
failed = False
for line in (folder / "volumes.txt").read_text().splitlines():
    name, expected, *bounds = line.split()
    expected = float(expected)
    path = folder / f"{name}.step"
    reader = STEPControl_Reader()
    if reader.ReadFile(str(path)) != IFSelect_RetDone:
        print(f"FAIL {name}: OpenCASCADE could not read the file")
        failed = True
        continue
    reader.TransferRoots()
    props = GProp_GProps()
    BRepGProp.VolumeProperties_s(reader.OneShape(), props)
    volume = props.Mass()
    # Our volume comes from a fine mesh, OCCT's is exact: they agree to a fraction of a percent.
    ok = abs(volume - expected) < 2e-3 * expected and f"PRODUCT('{name}'" in path.read_text()
    print(f"{'ok  ' if ok else 'FAIL'} {name}: OCCT volume {volume:.3f}, 3BCAD {expected:.3f}")
    if bounds:
        box = Bnd_Box()
        BRepBndLib.AddOptimal_s(reader.OneShape(), box, False, False)
        lo, hi = box.CornerMin(), box.CornerMax()
        found = (lo.X(), lo.Y(), lo.Z(), hi.X(), hi.Y(), hi.Z())
        placed = all(abs(a - float(b)) < 0.05 for a, b in zip(found, bounds))
        print(f"{'ok  ' if placed else 'FAIL'} {name}: OCCT box {[round(v, 3) for v in found]}, 3BCAD {bounds}")
        ok &= placed
    failed |= not ok


def colored_faces(path):
    """(sRGB colour 0-255, centre) of every coloured face OpenCASCADE reads from the file."""
    doc = TDocStd_Document(TCollection_ExtendedString("XmlOcaf"))
    reader = STEPCAFControl_Reader()
    reader.SetColorMode(True)
    if reader.ReadFile(str(path)) != IFSelect_RetDone or not reader.Transfer(doc):
        return None
    colors = XCAFDoc_DocumentTool.ColorTool_s(doc.Main())
    out = []
    explorer = TopExp_Explorer(reader.Reader().OneShape(), TopAbs_FACE)
    while explorer.More():
        face = explorer.Current()
        color = Quantity_Color()
        if colors.GetColor(face, XCAFDoc_ColorType.XCAFDoc_ColorSurf, color):
            props = GProp_GProps()
            BRepGProp.SurfaceProperties_s(face, props)
            c = props.CentreOfMass()
            rgb = color.Values(Quantity_TypeOfColor.Quantity_TOC_sRGB)
            out.append(([round(v * 255) for v in rgb], (round(c.X(), 3), round(c.Y(), 3), round(c.Z(), 3))))
        explorer.Next()
    return out


colors_file = folder / "colors.txt"
for line in colors_file.read_text().splitlines() if colors_file.exists() else []:
    name, *numbers = line.split()
    want_rgb, want_at = [int(v) for v in numbers[:3]], [float(v) for v in numbers[3:]]
    faces = colored_faces(folder / f"{name}.step")
    ok = faces is not None and any(
        rgb == want_rgb and all(abs(a - b) < 0.1 for a, b in zip(at, want_at)) for rgb, at in faces
    )
    print(f"{'ok  ' if ok else 'FAIL'} {name}: colour {want_rgb} at {want_at}, OCCT found {faces}")
    failed |= not ok
sys.exit(1 if failed else 0)
