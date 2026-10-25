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

/// 3MF package (millimetres), one object per body so slicers see separate parts.
pub fn write_3mf(path: &Path, meshes: &[PrintMesh]) -> io::Result<()> {
    let mut zip = ZipWriter::new(File::create(path)?);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file("[Content_Types].xml", opts)?;
    zip.write_all(CONTENT_TYPES.as_bytes())?;
    zip.start_file("_rels/.rels", opts)?;
    zip.write_all(RELS.as_bytes())?;
    zip.start_file("3D/3dmodel.model", opts)?;
    zip.write_all(model_xml(meshes).as_bytes())?;
    zip.finish()?;
    Ok(())
}

fn model_xml(meshes: &[PrintMesh]) -> String {
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<model unit=\"millimeter\" xml:lang=\"en-US\" \
         xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\">\n<resources>\n",
    );
    for (i, m) in meshes.iter().enumerate() {
        let _ = writeln!(s, "<object id=\"{}\" type=\"model\"><mesh><vertices>", i + 1);
        for v in &m.vertices {
            let _ = writeln!(s, "<vertex x=\"{}\" y=\"{}\" z=\"{}\"/>", v[0], v[1], v[2]);
        }
        s.push_str("</vertices><triangles>\n");
        for t in &m.triangles {
            let _ = writeln!(s, "<triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>", t[0], t[1], t[2]);
        }
        s.push_str("</triangles></mesh></object>\n");
    }
    s.push_str("</resources>\n<build>\n");
    for i in 0..meshes.len() {
        let _ = writeln!(s, "<item objectid=\"{}\"/>", i + 1);
    }
    s.push_str("</build>\n</model>\n");
    s
}
