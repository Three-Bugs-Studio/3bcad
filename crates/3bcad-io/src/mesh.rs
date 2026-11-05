//! Mesh import (STL, OBJ, 3MF). Every input is untrusted: bad data is an error, never a panic.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Cursor, Read};
use std::path::Path;

use threebcad_kernel::mesher::PrintMesh;

const MAX_FILE: u64 = 1 << 30;

/// Reads an STL, OBJ or 3MF file (by extension) into millimetres.
pub fn read_mesh(path: &Path) -> io::Result<PrintMesh> {
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    let parse = match ext.as_deref() {
        Some("stl") => parse_stl,
        Some("obj") => parse_obj,
        Some("3mf") => parse_3mf,
        _ => return Err(bad("unsupported mesh format (use STL, OBJ or 3MF)")),
    };
    let bytes = read_capped(File::open(path)?, MAX_FILE)?;
    let mesh = parse(&bytes)?;
    if mesh.triangles.is_empty() {
        return Err(bad("no triangles"));
    }
    Ok(mesh)
}

pub fn parse_stl(bytes: &[u8]) -> io::Result<PrintMesh> {
    let count = bytes
        .get(80..84)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()) as u64);
    let sized = count.is_some_and(|n| 84 + 50 * n == bytes.len() as u64);
    if bytes.starts_with(b"solid") && !sized {
        return parse_ascii_stl(bytes);
    }
    let n = count.ok_or_else(|| bad("STL too short"))?;
    if 84 + 50 * n > bytes.len() as u64 {
        return Err(bad("truncated binary STL"));
    }
    let mut w = Welder::default();
    for t in bytes[84..].chunks_exact(50).take(n as usize) {
        let f = |i: usize| f32::from_le_bytes(t[i..i + 4].try_into().unwrap());
        let mut tri = [0; 3];
        for (k, v) in tri.iter_mut().enumerate() {
            let o = 12 + 12 * k;
            *v = w.vertex([f(o), f(o + 4), f(o + 8)])?;
        }
        w.mesh.triangles.push(tri);
    }
    Ok(w.mesh)
}

fn parse_ascii_stl(bytes: &[u8]) -> io::Result<PrintMesh> {
    let mut w = Welder::default();
    let mut lp = Vec::new();
    let mut toks = bytes.split(u8::is_ascii_whitespace).filter(|t| !t.is_empty());
    while let Some(t) = toks.next() {
        match t {
            b"vertex" => {
                let mut c = [0f32; 3];
                for c in &mut c {
                    *c = toks
                        .next()
                        .and_then(|t| std::str::from_utf8(t).ok()?.parse().ok())
                        .ok_or_else(|| bad("bad STL vertex"))?;
                }
                lp.push(w.vertex(c)?);
            }
            b"endloop" => {
                fan(&mut w.mesh, &lp)?;
                lp.clear();
            }
            _ => {}
        }
    }
    Ok(w.mesh)
}

pub fn parse_obj(bytes: &[u8]) -> io::Result<PrintMesh> {
    let mut mesh = PrintMesh::default();
    let mut face = Vec::new();
    for line in bytes.split(|&b| b == b'\n') {
        let line = std::str::from_utf8(line).unwrap_or("");
        let mut toks = line.split_whitespace();
        match toks.next() {
            Some("v") => {
                let mut p = [0.0; 3];
                for c in &mut p {
                    *c = toks
                        .next()
                        .and_then(|t| t.parse().ok())
                        .ok_or_else(|| bad("bad OBJ vertex"))?;
                }
                mesh.vertices.push(finite(p)?);
            }
            Some("f") => {
                face.clear();
                let n = mesh.vertices.len() as i64;
                for t in toks {
                    let i: i64 = t
                        .split('/')
                        .next()
                        .unwrap_or("")
                        .parse()
                        .map_err(|_| bad("bad OBJ face"))?;
                    let i = if i < 0 { n + i } else { i - 1 };
                    face.push(
                        usize::try_from(i)
                            .ok()
                            .filter(|&i| i < mesh.vertices.len())
                            .ok_or_else(|| bad("OBJ face index out of range"))?,
                    );
                }
                fan(&mut mesh, &face)?;
            }
            _ => {}
        }
    }
    Ok(mesh)
}

pub fn parse_3mf(bytes: &[u8]) -> io::Result<PrintMesh> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut names = Vec::new();
    if let Ok(rels) = entry(&mut zip, "_rels/.rels") {
        let rels = String::from_utf8_lossy(&rels).into_owned();
        names.extend(
            tags(&rels)
                .filter(|(n, a)| *n == "Relationship" && attr(a, "Type").is_some_and(|t| t.ends_with("/3dmodel")))
                .filter_map(|(_, a)| attr(a, "Target"))
                .map(|t| t.trim_start_matches('/').to_string()),
        );
    }
    names.push("3D/3dmodel.model".into());
    names.extend(zip.file_names().filter(|n| n.ends_with(".model")).map(str::to_string));
    let name = names
        .into_iter()
        .find(|n| zip.index_for_name(n).is_some())
        .ok_or_else(|| bad("no 3D model in the 3MF"))?;
    let xml = entry(&mut zip, &name)?;
    parse_model(std::str::from_utf8(&xml).map_err(|_| bad("3MF model is not UTF-8"))?)
}

// shortcut: build-item and component transforms are ignored (meshes keep their object coordinates),
// upgrade when we import multi-part 3MF files from slicers that place objects with transforms.
fn parse_model(xml: &str) -> io::Result<PrintMesh> {
    let mut mesh = PrintMesh::default();
    let (mut offset, mut scale) = (0, 1.0);
    for (name, a) in tags(xml) {
        let num = |k: &str| attr(a, k).and_then(|v| v.trim().parse::<f64>().ok());
        match name {
            "model" => {
                scale = match attr(a, "unit").unwrap_or("millimeter") {
                    "micron" => 0.001,
                    "millimeter" => 1.0,
                    "centimeter" => 10.0,
                    "inch" => 25.4,
                    "foot" => 304.8,
                    "meter" => 1000.0,
                    _ => return Err(bad("unknown 3MF unit")),
                }
            }
            "mesh" => offset = mesh.vertices.len(),
            "vertex" => {
                let p = [num("x"), num("y"), num("z")];
                mesh.vertices.push(finite(p.map(|c| c.unwrap_or(f64::NAN) * scale))?);
            }
            "triangle" => {
                let local = mesh.vertices.len() - offset;
                let mut t = [0; 3];
                for (v, k) in t.iter_mut().zip(["v1", "v2", "v3"]) {
                    let i = attr(a, k)
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .filter(|&i| i < local);
                    *v = offset + i.ok_or_else(|| bad("3MF triangle index out of range"))?;
                }
                mesh.triangles.push(t);
            }
            _ => {}
        }
    }
    Ok(mesh)
}

/// `(local name, attribute text)` of every start tag; good enough for 3MF's flat model XML.
fn tags(xml: &str) -> impl Iterator<Item = (&str, &str)> {
    xml.split('<').skip(1).map(|t| {
        let t = t.split('>').next().unwrap_or("");
        let end = t.find(|c: char| c.is_whitespace() || c == '/').unwrap_or(t.len());
        (t[..end].rsplit(':').next().unwrap_or(""), &t[end..])
    })
}

fn attr<'a>(mut s: &'a str, key: &str) -> Option<&'a str> {
    loop {
        let eq = s.find('=')?;
        let name = s[..eq].trim();
        let rest = s[eq + 1..].trim_start();
        let q = rest.chars().next().filter(|&c| c == '"' || c == '\'')?;
        let end = rest[1..].find(q)? + 1;
        if name == key {
            return Some(&rest[1..end]);
        }
        s = &rest[end + 1..];
    }
}

fn entry(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> io::Result<Vec<u8>> {
    read_capped(zip.by_name(name)?, MAX_FILE)
}

fn read_capped(r: impl Read, cap: u64) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    r.take(cap + 1).read_to_end(&mut out)?;
    if out.len() as u64 > cap {
        return Err(bad("file larger than 1 GiB"));
    }
    Ok(out)
}

/// Indexes STL's triangle soup by exact coordinates.
#[derive(Default)]
struct Welder {
    mesh: PrintMesh,
    index: HashMap<[u32; 3], usize>,
}

impl Welder {
    fn vertex(&mut self, p: [f32; 3]) -> io::Result<usize> {
        let v = finite(p.map(f64::from))?;
        // + 0.0 folds -0.0 into 0.0 so they weld.
        let key = p.map(|c| (c + 0.0).to_bits());
        let next = self.mesh.vertices.len();
        let i = *self.index.entry(key).or_insert(next);
        if i == next {
            self.mesh.vertices.push(v);
        }
        Ok(i)
    }
}

fn fan(mesh: &mut PrintMesh, poly: &[usize]) -> io::Result<()> {
    if poly.len() < 3 {
        return Err(bad("face with fewer than 3 vertices"));
    }
    mesh.triangles
        .extend((1..poly.len() - 1).map(|i| [poly[0], poly[i], poly[i + 1]]));
    Ok(())
}

fn finite(p: [f64; 3]) -> io::Result<[f64; 3]> {
    if p.iter().all(|c| c.is_finite()) {
        Ok(p)
    } else {
        Err(bad("non-finite coordinate"))
    }
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

/// Runs `parse` on every truncation and a few hundred byte mutations of `sample`; it must not panic.
#[cfg(test)]
pub(crate) fn shake<T>(sample: &[u8], parse: impl Fn(&[u8]) -> io::Result<T>) {
    for n in 0..sample.len() {
        let _ = parse(&sample[..n]);
    }
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut rand = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for _ in 0..400 {
        let mut b = sample.to_vec();
        for _ in 0..1 + rand() % 4 {
            let i = rand() as usize % b.len();
            b[i] = [b'0', b'-', b' ', b'\n', b'<', b'"', b'e', 0xFF, rand() as u8][rand() as usize % 9];
        }
        let _ = parse(&b);
    }
}

#[cfg(test)]
mod tests {
    use threebcad_kernel::mass::mass_properties;
    use threebcad_kernel::{mesher::print_mesh, operations::extrude_polygon};

    use super::*;

    fn cube(size: f64) -> PrintMesh {
        let s = size;
        print_mesh(
            &extrude_polygon(&[[0.0, 0.0], [s, 0.0], [s, s], [0.0, s]], s).unwrap(),
            0.01,
        )
    }

    fn volume(m: &PrintMesh) -> f64 {
        mass_properties(std::slice::from_ref(m)).volume
    }

    fn ascii_stl(m: &PrintMesh) -> Vec<u8> {
        let mut s = String::from("solid cube\n");
        for t in &m.triangles {
            s += "  facet normal 0 0 0\r\n    outer loop\n";
            for v in t.map(|i| m.vertices[i]) {
                s += &format!("      vertex {} {} {}\n", v[0], v[1], v[2]);
            }
            s += "    endloop\n  endfacet\n";
        }
        (s + "endsolid cube\n").into_bytes()
    }

    fn binary_stl(m: &PrintMesh) -> Vec<u8> {
        let path = std::env::temp_dir().join(format!("3bcad-mesh-{}.stl", std::process::id()));
        crate::write_stl(&path, &[("cube", m)]).unwrap();
        let mut b = std::fs::read(&path).unwrap();
        std::fs::remove_file(path).ok();
        b[..5].copy_from_slice(b"solid"); // some exporters do this; the size check must still win
        b
    }

    fn sample_3mf(m: &PrintMesh) -> Vec<u8> {
        let path = std::env::temp_dir().join(format!("3bcad-mesh-{}.3mf", std::process::id()));
        crate::write_3mf(&path, &[("cube", m)], &crate::threemf::Info::default()).unwrap();
        let b = std::fs::read(&path).unwrap();
        std::fs::remove_file(path).ok();
        b
    }

    const OBJ: &str = "# unit cube as quads\nv 0 0 0\nv 10 0 0\nv 10 10 0\nv 0 10 0\nv 0 0 10\nv 10 0 10\nv 10 10 10\nv 0 10 10\n\
                       vn 0 0 1\nf 1 4 3 2\nf 5/1 6/1 7/1 8/1\nf 1//1 2//1 6//1 5//1\nf -6 -5 -1 -2\nf 2 3 7 6\nf 4 1 5 8\n";

    #[test]
    fn stl_ascii_and_binary_weld_a_cube() {
        let m = cube(10.0);
        for bytes in [ascii_stl(&m), binary_stl(&m)] {
            let got = parse_stl(&bytes).unwrap();
            assert_eq!((got.vertices.len(), got.triangles.len()), (8, 12));
            assert!((volume(&got) - 1000.0).abs() < 1e-3, "{}", volume(&got));
        }
    }

    #[test]
    fn obj_quads_and_relative_indices() {
        let got = parse_obj(OBJ.as_bytes()).unwrap();
        assert_eq!(got.triangles.len(), 12);
        assert!((volume(&got) - 1000.0).abs() < 1e-9, "{}", volume(&got));
        assert!(parse_obj(b"v 0 0 0\nf 1 2 3\n").is_err());
    }

    #[test]
    fn threemf_round_trips_and_scales_units() {
        let m = cube(10.0);
        let bytes = sample_3mf(&m);
        let got = parse_3mf(&bytes).unwrap();
        assert_eq!(got.triangles.len(), m.triangles.len());
        assert!((volume(&got) - 1000.0).abs() < 1e-6);

        let xml = "<model unit=\"inch\"><resources><object id=\"1\"><mesh><vertices>\
                   <vertex x=\"0\" y=\"0\" z=\"0\"/><vertex x=\"1\" y=\"0\" z=\"0\"/><vertex x=\"0\" y=\"1\" z=\"0\"/>\
                   <vertex x=\"0\" y=\"0\" z=\"1\"/></vertices><triangles><triangle v1=\"0\" v2=\"2\" v3=\"1\"/>\
                   <triangle v1=\"0\" v2=\"1\" v3=\"3\"/><triangle v1=\"0\" v2=\"3\" v3=\"2\"/>\
                   <triangle v1=\"1\" v2=\"2\" v3=\"3\"/></triangles></mesh></object></resources></model>";
        let tet = parse_model(xml).unwrap();
        assert!((volume(&tet) - 25.4f64.powi(3) / 6.0).abs() < 1e-9, "{}", volume(&tet));
        assert!(parse_model(&xml.replace("v3=\"3\"/></tri", "v3=\"4\"/></tri")).is_err());
    }

    #[test]
    fn read_mesh_dispatches_and_rejects_empty() {
        let dir = std::env::temp_dir();
        let obj = dir.join(format!("3bcad-mesh-{}.OBJ", std::process::id()));
        std::fs::write(&obj, OBJ).unwrap();
        assert_eq!(read_mesh(&obj).unwrap().triangles.len(), 12);
        std::fs::write(&obj, "v 0 0 0\n").unwrap();
        assert!(read_mesh(&obj).is_err());
        std::fs::remove_file(obj).ok();
        assert!(read_mesh(Path::new("part.ply")).is_err());
    }

    #[test]
    fn parsers_survive_truncation_and_mutation() {
        let m = cube(10.0);
        shake(&ascii_stl(&m), parse_stl);
        shake(&binary_stl(&m), parse_stl);
        shake(OBJ.as_bytes(), parse_obj);
        shake(&sample_3mf(&m), parse_3mf);
        let xml = String::from_utf8(
            entry(
                &mut zip::ZipArchive::new(Cursor::new(&sample_3mf(&m)[..])).unwrap(),
                "3D/3dmodel.model",
            )
            .unwrap(),
        )
        .unwrap();
        shake(xml.as_bytes(), |b| parse_model(&String::from_utf8_lossy(b)));
    }
}
