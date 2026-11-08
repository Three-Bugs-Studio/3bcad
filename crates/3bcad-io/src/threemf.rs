use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use threebcad_kernel::mesher::PrintMesh;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>
</Types>"#;

const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>
</Relationships>"#;

/// What a package says besides its meshes.
#[derive(Clone, Debug, Default)]
pub struct Info {
    /// The part's name (the package title).
    pub title: String,
    /// Material and sRGB colour of every object.
    pub material: String,
    pub color: [u8; 3],
    /// Per object, the sRGB colour of each B-Rep face (as `PrintMesh::face_of` numbers them),
    /// `None` or missing for the material colour.
    pub colors: Vec<Vec<Option<[u8; 3]>>>,
    /// Print settings the part was prepared for (printer, nozzle...), kept as 3BCAD metadata.
    pub settings: Vec<(String, String)>,
}

/// 3MF package (millimetres): one named object per body, with its material and display colour,
/// and per-triangle colours (core-spec base materials) where faces have their own.
/// The meshes are written where they are, so place them on the bed first.
// shortcut: Bambu Studio and OrcaSlicer read only geometry from 3MF files they did not write, so
// plates and per-object slicer settings need their (undocumented) project format; revisit if they
// publish it.
pub fn write_3mf(path: &Path, objects: &[(&str, &PrintMesh)], info: &Info) -> io::Result<()> {
    let mut zip = ZipWriter::new(File::create(path)?);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file("[Content_Types].xml", opts)?;
    zip.write_all(CONTENT_TYPES.as_bytes())?;
    zip.start_file("_rels/.rels", opts)?;
    zip.write_all(RELS.as_bytes())?;
    zip.start_file("3D/3dmodel.model", opts)?;
    zip.write_all(model_xml(objects, info).as_bytes())?;
    zip.finish()?;
    Ok(())
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

fn model_xml(objects: &[(&str, &PrintMesh)], info: &Info) -> String {
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<model unit=\"millimeter\" xml:lang=\"en-US\" \
         xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\" \
         xmlns:threebcad=\"https://github.com/Three-Bugs-Studio/3bcad\">\n",
    );
    let meta = |s: &mut String, name: &str, value: &str| {
        let _ = writeln!(s, "<metadata name=\"{name}\">{}</metadata>", escape(value));
    };
    meta(&mut s, "Title", &info.title);
    meta(&mut s, "Application", concat!("3BCAD ", env!("CARGO_PKG_VERSION")));
    for (k, v) in &info.settings {
        let key: String = k.chars().filter(char::is_ascii_alphanumeric).collect();
        meta(&mut s, &format!("threebcad:{key}"), v);
    }
    // Base 0 is the material; each other colour used by a face gets its own base.
    let mut palette = vec![info.color];
    let base = |palette: &mut Vec<[u8; 3]>, c: [u8; 3]| match palette.iter().position(|&p| p == c) {
        Some(k) => k,
        None => {
            palette.push(c);
            palette.len() - 1
        }
    };
    let mut per_triangle = Vec::new();
    for (i, (_, m)) in objects.iter().enumerate() {
        let faces = info.colors.get(i).map_or(&[][..], Vec::as_slice);
        let ks: Vec<usize> = m
            .face_of
            .iter()
            .map(|&f| {
                faces
                    .get(f as usize)
                    .copied()
                    .flatten()
                    .map_or(0, |c| base(&mut palette, c))
            })
            .collect();
        per_triangle.push(ks);
    }
    s.push_str("<resources>\n<basematerials id=\"1\">");
    for (k, [r, g, b]) in palette.iter().enumerate() {
        let name = if k == 0 {
            escape(&info.material)
        } else {
            format!("{} #{r:02X}{g:02X}{b:02X}", escape(&info.material))
        };
        let _ = write!(s, "<base name=\"{name}\" displaycolor=\"#{r:02X}{g:02X}{b:02X}\"/>");
    }
    s.push_str("</basematerials>\n");
    for (i, (name, m)) in objects.iter().enumerate() {
        let _ = writeln!(
            s,
            "<object id=\"{}\" type=\"model\" name=\"{}\" pid=\"1\" pindex=\"0\"><mesh><vertices>",
            i + 2,
            escape(name)
        );
        for v in &m.vertices {
            let _ = writeln!(s, "<vertex x=\"{}\" y=\"{}\" z=\"{}\"/>", v[0], v[1], v[2]);
        }
        s.push_str("</vertices><triangles>\n");
        for (j, t) in m.triangles.iter().enumerate() {
            let _ = write!(s, "<triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"", t[0], t[1], t[2]);
            match per_triangle[i].get(j) {
                Some(&k) if k > 0 => {
                    let _ = writeln!(s, " pid=\"1\" p1=\"{k}\"/>");
                }
                _ => s.push_str("/>\n"),
            }
        }
        s.push_str("</triangles></mesh></object>\n");
    }
    s.push_str("</resources>\n<build>\n");
    for i in 0..objects.len() {
        let _ = writeln!(s, "<item objectid=\"{}\"/>", i + 2);
    }
    s.push_str("</build>\n</model>\n");
    s
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    #[test]
    fn objects_carry_names_material_and_settings() {
        let tri = PrintMesh {
            vertices: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
            face_of: vec![0, 1, 2, 3],
        };
        let info = Info {
            title: "Bracket <v2>".into(),
            material: "PETG".into(),
            color: [96, 160, 200],
            settings: vec![
                ("Printer".into(), "Prusa MK4".into()),
                ("Nozzle diameter".into(), "0.4".into()),
            ],
            // The second object's slanted face is red.
            colors: vec![Vec::new(), vec![None, None, None, Some([255, 0, 0])]],
        };
        let path = std::env::temp_dir().join(format!("3bcad-rich-{}.3mf", std::process::id()));
        write_3mf(&path, &[("Body 1", &tri), ("Pins & clips", &tri)], &info).unwrap();
        let mut zip = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();
        let mut xml = String::new();
        zip.by_name("3D/3dmodel.model")
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();
        assert!(xml.contains("<metadata name=\"Title\">Bracket &lt;v2&gt;</metadata>"));
        assert!(xml.contains(
            "<base name=\"PETG\" displaycolor=\"#60A0C8\"/><base name=\"PETG #FF0000\" displaycolor=\"#FF0000\"/>"
        ));
        // Only the last triangle (v1 2 3) of the second object takes base 1.
        assert_eq!(xml.matches("p1=\"1\"").count(), 1);
        assert!(xml.contains("<triangle v1=\"1\" v2=\"2\" v3=\"3\" pid=\"1\" p1=\"1\"/>"));
        assert!(xml.rfind("p1=").unwrap() > xml.find("name=\"Pins").unwrap());
        assert!(xml.contains("name=\"Pins &amp; clips\" pid=\"1\" pindex=\"0\""));
        assert!(xml.contains("<metadata name=\"threebcad:Nozzlediameter\">0.4</metadata>"));
        assert_eq!(xml.matches("<item objectid=").count(), 2);
        // Our own importer reads both objects back.
        let back = crate::mesh::read_mesh(&path).unwrap();
        assert_eq!(back.triangles.len(), 8);
        std::fs::remove_file(path).ok();
    }
}
