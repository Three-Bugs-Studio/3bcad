//! Drawing sheets as vector files at true scale: one PDF with a page per sheet, or an SVG or DXF
//! file per sheet (DXF with a layer per kind of line). Coordinates are paper millimetres, y up.

use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};

use crate::dxf::{put, xy};

pub type P2 = [f64; 2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dash {
    Solid,
    Dashed,
    Chain,
}

/// What a mark is; each layer has its own line width and dash pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Layer {
    Visible,
    Hidden,
    Center,
    Hatch,
    Dimensions,
    Notes,
    /// The border and title block.
    Format,
}

impl Layer {
    pub const ALL: [Layer; 7] = [
        Layer::Visible,
        Layer::Hidden,
        Layer::Center,
        Layer::Hatch,
        Layer::Dimensions,
        Layer::Notes,
        Layer::Format,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Layer::Visible => "VISIBLE",
            Layer::Hidden => "HIDDEN",
            Layer::Center => "CENTER",
            Layer::Hatch => "HATCH",
            Layer::Dimensions => "DIMENSIONS",
            Layer::Notes => "NOTES",
            Layer::Format => "FORMAT",
        }
    }

    /// Line width (mm), after ISO 128: thick 0.5, thin 0.25, hatching 0.18.
    pub fn width(self) -> f64 {
        match self {
            Layer::Visible | Layer::Format => 0.5,
            Layer::Hatch => 0.18,
            _ => 0.25,
        }
    }

    pub fn dash(self) -> Dash {
        match self {
            Layer::Hidden => Dash::Dashed,
            Layer::Center => Dash::Chain,
            _ => Dash::Solid,
        }
    }

    /// AutoCAD colour index, so layers tell apart in a DXF viewer.
    fn color(self) -> u8 {
        match self {
            Layer::Visible | Layer::Format => 7,
            Layer::Hidden => 8,
            Layer::Center => 1,
            Layer::Hatch => 9,
            Layer::Dimensions => 4,
            Layer::Notes => 3,
        }
    }
}

/// On and off lengths (mm) of a dash pattern, repeating.
pub fn dash_pattern(d: Dash) -> &'static [f64] {
    match d {
        Dash::Solid => &[],
        Dash::Dashed => &[3.0, 1.5],
        Dash::Chain => &[10.0, 1.5, 1.5, 1.5],
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Line(Vec<P2>),
    /// A filled polygon (arrowheads).
    Fill(Vec<P2>),
    /// `at` is the baseline point the text is aligned on; `height` the capital height.
    Text {
        at: P2,
        height: f64,
        text: String,
        align: Align,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub layer: Layer,
    pub shape: Shape,
}

/// One sheet: its name, size (mm) and marks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Page {
    pub name: String,
    pub size: P2,
    pub marks: Vec<Mark>,
}

/// Helvetica advance widths (1/1000 em) of the printable ASCII characters, from Adobe's AFM.
const HELVETICA: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556, 556, 556, 556,
    556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833,
    722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556,
    556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334,
    260, 334, 584,
];
/// Helvetica's capital height (em): text heights on drawings are capital heights.
const CAP: f64 = 0.718;

/// How wide `text` is at capital height `height` (mm), in Helvetica.
pub fn text_width(text: &str, height: f64) -> f64 {
    let em: u32 = text
        .chars()
        .map(|c| {
            let i = c as usize;
            if (32..127).contains(&i) {
                u32::from(HELVETICA[i - 32])
            } else {
                556
            }
        })
        .sum();
    f64::from(em) / 1000.0 * height / CAP
}

fn left_of(text: &str, at: P2, height: f64, align: Align) -> f64 {
    match align {
        Align::Left => at[0],
        Align::Center => at[0] - text_width(text, height) / 2.0,
        Align::Right => at[0] - text_width(text, height),
    }
}

fn n(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.into()
    }
}

/// The files a drawing exports to: `path` itself for one page or a PDF, else one per sheet named
/// after it ("Bracket - Sheet2.svg").
pub fn sheet_paths(path: &Path, pages: &[Page]) -> Vec<PathBuf> {
    let pdf = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
    if pdf || pages.len() <= 1 {
        return vec![path.to_path_buf()];
    }
    let stem = path
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let ext = path
        .extension()
        .map_or_else(String::new, |e| e.to_string_lossy().into_owned());
    pages
        .iter()
        .map(|p| {
            let name: String = p
                .name
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || " -_".contains(c) {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            path.with_file_name(format!("{stem} - {name}.{ext}"))
        })
        .collect()
}

/// Writes every page to `path` as PDF, SVG or DXF (by its extension) and returns the files written.
pub fn export(path: &Path, pages: &[Page]) -> io::Result<Vec<PathBuf>> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let paths = sheet_paths(path, pages);
    match ext.as_str() {
        "pdf" => std::fs::write(path, pdf(pages))?,
        "svg" => {
            for (p, page) in paths.iter().zip(pages) {
                std::fs::write(p, svg(page))?;
            }
        }
        "dxf" => {
            for (p, page) in paths.iter().zip(pages) {
                std::fs::write(p, dxf(page))?;
            }
        }
        _ => return Err(io::Error::other("export a drawing as .pdf, .svg or .dxf")),
    }
    Ok(paths)
}

/// PDF text: WinAnsi bytes (Latin-1 covers Ø ° ±), brackets and backslashes escaped.
fn pdf_string(text: &str) -> String {
    let mut s = String::from("(");
    for c in text.chars() {
        match c {
            '(' | ')' | '\\' => {
                s.push('\\');
                s.push(c);
            }
            ' '..='~' => s.push(c),
            c if (c as u32) >= 0xA0 && (c as u32) <= 0xFF => {
                let _ = write!(s, "\\{:03o}", c as u32);
            }
            _ => s.push('?'),
        }
    }
    s.push(')');
    s
}

/// A vector PDF with one page per sheet, 1 mm on paper = 1 mm printed.
pub fn pdf(pages: &[Page]) -> Vec<u8> {
    const PT: f64 = 72.0 / 25.4;
    let mut objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        String::new(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
    ];
    let mut kids = Vec::new();
    for page in pages {
        // Six places: rounded to n()'s three, the scale would be off by 0.05 mm across an A3 sheet.
        let mut c = format!("{PT:.6} 0 0 {PT:.6} 0 0 cm 1 J 1 j\n");
        let mut layer = None;
        for m in &page.marks {
            if layer != Some(m.layer) {
                layer = Some(m.layer);
                let dash: Vec<String> = dash_pattern(m.layer.dash()).iter().map(|&d| n(d)).collect();
                let _ = writeln!(c, "{} w [{}] 0 d", n(m.layer.width()), dash.join(" "));
            }
            match &m.shape {
                Shape::Line(pts) | Shape::Fill(pts) if pts.len() >= 2 => {
                    for (i, p) in pts.iter().enumerate() {
                        let _ = write!(c, "{} {} {} ", n(p[0]), n(p[1]), if i == 0 { "m" } else { "l" });
                    }
                    c += if matches!(m.shape, Shape::Fill(_)) {
                        "h f\n"
                    } else {
                        "S\n"
                    };
                }
                Shape::Text {
                    at,
                    height,
                    text,
                    align,
                } => {
                    let x = left_of(text, *at, *height, *align);
                    let _ = writeln!(
                        c,
                        "BT /F1 {} Tf {} {} Td {} Tj ET",
                        n(height / CAP),
                        n(x),
                        n(at[1]),
                        pdf_string(text)
                    );
                }
                _ => {}
            }
        }
        kids.push(objects.len() + 1);
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
            n(page.size[0] * PT),
            n(page.size[1] * PT),
            objects.len() + 2
        ));
        objects.push(format!("<< /Length {} >>\nstream\n{c}endstream", c.len()));
    }
    let kids: Vec<String> = kids.iter().map(|k| format!("{k} 0 R")).collect();
    objects[1] = format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len());
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        let _ = write!(out, "{} 0 obj\n{o}\nendobj\n", i + 1);
    }
    let xref = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for o in offsets {
        let _ = writeln!(out, "{o:010} 00000 n ");
    }
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    out.into_bytes()
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// An SVG of one sheet, sized in millimetres, one group per layer.
pub fn svg(page: &Page) -> String {
    let [w, h] = page.size;
    let pt = |p: &P2| format!("{},{}", n(p[0]), n(h - p[1]));
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}mm\" height=\"{h}mm\" viewBox=\"0 0 {w} {h}\">\n<title>{}</title>\n<rect width=\"{w}\" height=\"{h}\" fill=\"white\"/>\n",
        xml(&page.name),
        w = n(w),
        h = n(h)
    );
    for layer in Layer::ALL {
        let marks: Vec<&Mark> = page.marks.iter().filter(|m| m.layer == layer).collect();
        if marks.is_empty() {
            continue;
        }
        let dash: Vec<String> = dash_pattern(layer.dash()).iter().map(|&d| n(d)).collect();
        let _ = writeln!(
            s,
            "<g id=\"{}\" fill=\"none\" stroke=\"black\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"{}>",
            layer.name(),
            n(layer.width()),
            if dash.is_empty() {
                String::new()
            } else {
                format!(" stroke-dasharray=\"{}\"", dash.join(" "))
            }
        );
        for m in marks {
            match &m.shape {
                Shape::Line(p) => {
                    let pts: Vec<String> = p.iter().map(pt).collect();
                    let _ = writeln!(s, "<polyline points=\"{}\"/>", pts.join(" "));
                }
                Shape::Fill(p) => {
                    let pts: Vec<String> = p.iter().map(pt).collect();
                    let _ = writeln!(
                        s,
                        "<polygon points=\"{}\" fill=\"black\" stroke=\"none\"/>",
                        pts.join(" ")
                    );
                }
                Shape::Text {
                    at,
                    height,
                    text,
                    align,
                } => {
                    let anchor = match align {
                        Align::Left => "start",
                        Align::Center => "middle",
                        Align::Right => "end",
                    };
                    let _ = writeln!(
                        s,
                        "<text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" text-anchor=\"{anchor}\" fill=\"black\" stroke=\"none\">{}</text>",
                        n(at[0]),
                        n(h - at[1]),
                        n(height / CAP),
                        xml(text)
                    );
                }
            }
        }
        s += "</g>\n";
    }
    s + "</svg>\n"
}

/// An R12 DXF of one sheet in millimetres, with a layer (colour and line type) per kind of line.
pub fn dxf(page: &Page) -> String {
    let mut s = String::new();
    for (c, v) in [
        (0, "SECTION"),
        (2, "HEADER"),
        (9, "$ACADVER"),
        (1, "AC1009"),
        (9, "$INSUNITS"),
        (70, "4"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "TABLES"),
        (0, "TABLE"),
        (2, "LTYPE"),
        (70, "3"),
    ] {
        put(&mut s, c, v);
    }
    for (name, dash) in [
        ("CONTINUOUS", Dash::Solid),
        ("DASHED", Dash::Dashed),
        ("CENTER", Dash::Chain),
    ] {
        let p = dash_pattern(dash);
        put(&mut s, 0, "LTYPE");
        put(&mut s, 2, name);
        put(&mut s, 70, 0);
        put(&mut s, 3, name);
        put(&mut s, 72, 65);
        put(&mut s, 73, p.len());
        put(&mut s, 40, n(p.iter().sum()));
        for (i, d) in p.iter().enumerate() {
            put(&mut s, 49, n(if i % 2 == 0 { *d } else { -d }));
        }
    }
    put(&mut s, 0, "ENDTAB");
    put(&mut s, 0, "TABLE");
    put(&mut s, 2, "LAYER");
    put(&mut s, 70, Layer::ALL.len());
    for l in Layer::ALL {
        put(&mut s, 0, "LAYER");
        put(&mut s, 2, l.name());
        put(&mut s, 70, 0);
        put(&mut s, 62, l.color());
        put(
            &mut s,
            6,
            match l.dash() {
                Dash::Solid => "CONTINUOUS",
                Dash::Dashed => "DASHED",
                Dash::Chain => "CENTER",
            },
        );
    }
    for (c, v) in [(0, "ENDTAB"), (0, "ENDSEC"), (0, "SECTION"), (2, "ENTITIES")] {
        put(&mut s, c, v);
    }
    for m in &page.marks {
        let layer = m.layer.name();
        match &m.shape {
            Shape::Line(p) if p.len() == 2 => {
                put(&mut s, 0, "LINE");
                put(&mut s, 8, layer);
                xy(&mut s, 10, p[0]);
                xy(&mut s, 11, p[1]);
            }
            Shape::Line(p) if p.len() > 2 => {
                for (c, v) in [(0, "POLYLINE"), (8, layer), (66, "1"), (70, "0")] {
                    put(&mut s, c, v);
                }
                for &q in p {
                    put(&mut s, 0, "VERTEX");
                    put(&mut s, 8, layer);
                    xy(&mut s, 10, q);
                }
                put(&mut s, 0, "SEQEND");
            }
            // Arrowheads are triangles: a SOLID with its last corner repeated.
            Shape::Fill(p) if p.len() >= 3 => {
                put(&mut s, 0, "SOLID");
                put(&mut s, 8, layer);
                for (i, code) in [10, 11, 12, 13].into_iter().enumerate() {
                    xy(&mut s, code, p[i.min(p.len() - 1)]);
                }
            }
            Shape::Text {
                at,
                height,
                text,
                align,
            } => {
                put(&mut s, 0, "TEXT");
                put(&mut s, 8, layer);
                xy(&mut s, 10, *at);
                put(&mut s, 40, n(*height));
                put(&mut s, 1, text.replace(['\n', '\r'], " "));
                let h = match align {
                    Align::Left => 0,
                    Align::Center => 1,
                    Align::Right => 2,
                };
                if h != 0 {
                    put(&mut s, 72, h);
                    xy(&mut s, 11, *at);
                }
            }
            _ => {}
        }
    }
    put(&mut s, 0, "ENDSEC");
    put(&mut s, 0, "EOF");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An A4 sheet with a 100 mm line, a hidden line, an arrowhead and a centred note.
    fn sheets() -> Vec<Page> {
        let mark = |layer, shape| Mark { layer, shape };
        let a = Page {
            name: "Sheet1".into(),
            size: [297.0, 210.0],
            marks: vec![
                mark(Layer::Visible, Shape::Line(vec![[20.0, 30.0], [120.0, 30.0]])),
                mark(
                    Layer::Hidden,
                    Shape::Line(vec![[20.0, 40.0], [60.0, 40.0], [60.0, 80.0]]),
                ),
                mark(
                    Layer::Dimensions,
                    Shape::Fill(vec![[0.0, 0.0], [3.0, 1.0], [3.0, -1.0]]),
                ),
                mark(
                    Layer::Notes,
                    Shape::Text {
                        at: [148.5, 100.0],
                        height: 3.5,
                        text: "Ø10 (THRU) & <done>".into(),
                        align: Align::Center,
                    },
                ),
            ],
        };
        let b = Page {
            name: "Sheet2".into(),
            size: [420.0, 297.0],
            marks: Vec::new(),
        };
        vec![a, b]
    }

    #[test]
    fn the_pdf_has_a_page_per_sheet_at_true_scale() {
        let pdf = String::from_utf8(pdf(&sheets())).unwrap();
        assert!(pdf.starts_with("%PDF-1.4") && pdf.ends_with("%%EOF\n"));
        assert_eq!(pdf.matches("/Type /Page ").count(), 2);
        // A4 and A3 in points.
        assert!(pdf.contains("/MediaBox [0 0 841.89 595.276]"), "{pdf}");
        assert!(pdf.contains("/MediaBox [0 0 1190.551 841.89]"));
        // Drawn in mm under a mm-to-point matrix: the line is 100 mm long on paper.
        assert!(pdf.contains("2.834646 0 0 2.834646 0 0 cm") && pdf.contains("20 30 m 120 30 l S"));
        assert!(pdf.contains("[3 1.5] 0 d"));
        assert!(
            pdf.contains("(\\33010 \\(THRU\\) & <done>) Tj"),
            "Ø as WinAnsi, brackets escaped"
        );
        // Every xref offset points at its object.
        let xref = pdf.rfind("xref\n").unwrap();
        for (i, line) in pdf[xref..].lines().skip(3).take(7).enumerate() {
            let at: usize = line[..10].parse().unwrap();
            assert!(pdf[at..].starts_with(&format!("{} 0 obj", i + 1)), "object {}", i + 1);
        }
    }

    #[test]
    fn svg_and_dxf_keep_layers_and_millimetres() {
        let pages = sheets();
        let svg = svg(&pages[0]);
        assert!(svg.contains("width=\"297mm\" height=\"210mm\" viewBox=\"0 0 297 210\""));
        assert!(svg.contains("<g id=\"HIDDEN\"") && svg.contains("stroke-dasharray=\"3 1.5\""));
        // y flips: paper y = 30 is 180 from the top.
        assert!(svg.contains("points=\"20,180 120,180\""));
        assert!(svg.contains("&amp; &lt;done&gt;</text>"));
        let dxf = dxf(&pages[0]);
        for l in Layer::ALL {
            assert!(dxf.contains(&format!("LAYER\n2\n{}\n", l.name())));
        }
        assert!(dxf.contains("LINE\n8\nVISIBLE\n10\n20\n20\n30\n11\n120\n21\n30\n"));
        assert!(dxf.contains("POLYLINE\n8\nHIDDEN\n"));
        assert!(dxf.contains("TEXT\n8\nNOTES\n"));
        // 3BCAD's own reader takes it back: the line and the polyline.
        let back = crate::read_dxf(dxf.as_bytes(), None).unwrap();
        assert_eq!(back.entities().len(), 3);
    }

    #[test]
    fn each_sheet_gets_its_own_svg_or_dxf_file() {
        let dir = std::env::temp_dir().join(format!("3bcad-drawing-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pages = sheets();
        let svgs = export(&dir.join("Bracket.svg"), &pages).unwrap();
        assert_eq!(
            svgs,
            vec![dir.join("Bracket - Sheet1.svg"), dir.join("Bracket - Sheet2.svg")]
        );
        assert!(svgs.iter().all(|p| p.exists()));
        let pdfs = export(&dir.join("Bracket.pdf"), &pages).unwrap();
        assert_eq!(pdfs, vec![dir.join("Bracket.pdf")]);
        assert_eq!(
            export(&dir.join("Bracket.dxf"), &pages[..1]).unwrap(),
            vec![dir.join("Bracket.dxf")]
        );
        assert!(export(&dir.join("Bracket.png"), &pages).is_err());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn text_width_follows_helvetica() {
        // "0" is 556/1000 em; at 3.5 mm capital height the em is 3.5 / 0.718 mm.
        assert!((text_width("00", 3.5) - 2.0 * 0.556 * 3.5 / CAP).abs() < 1e-12);
        assert!(text_width("W", 5.0) > text_width("i", 5.0));
    }
}
