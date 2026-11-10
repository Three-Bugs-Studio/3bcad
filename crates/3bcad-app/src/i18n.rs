//! UI translations. Every string the UI shows goes through `tr` (or `trf` when it has values in
//! it); English is the key, and `i18n/<language>.txt` maps it to the translation. A string with
//! no translation shows in English. How to add a language: `docs/guide/translating.md`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    English,
    Vietnamese,
}

impl Language {
    pub const ALL: [Self; 2] = [Self::English, Self::Vietnamese];

    /// In the language itself, so anyone can find theirs.
    pub fn name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Vietnamese => "Tiếng Việt",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Vietnamese => "vi",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.id() == id.trim())
    }

    fn table(self) -> Option<&'static HashMap<&'static str, &'static str>> {
        static VI: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
        match self {
            Self::English => None,
            Self::Vietnamese => Some(VI.get_or_init(|| parse(include_str!("../i18n/vi.txt")))),
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// A system font with Vietnamese letters (egui's own font has none of ạ, ế, ộ…): Segoe UI on
/// Windows, Arial on macOS, DejaVu / Noto / Liberation Sans on Linux.
fn system_font() -> Option<Vec<u8>> {
    let windows = std::env::var_os("WINDIR").map_or_else(|| "C:\\Windows".into(), std::path::PathBuf::from);
    let candidates = [
        windows.join("Fonts").join("segoeui.ttf"),
        windows.join("Fonts").join("arial.ttf"),
        "/System/Library/Fonts/Supplemental/Arial.ttf".into(),
        "/Library/Fonts/Arial Unicode.ttf".into(),
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".into(),
        "/usr/share/fonts/TTF/DejaVuSans.ttf".into(),
        "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf".into(),
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf".into(),
        "/usr/share/fonts/noto/NotoSans-Regular.ttf".into(),
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf".into(),
    ];
    candidates
        .iter()
        .find_map(|p: &std::path::PathBuf| std::fs::read(p).ok())
}

/// Adds the system font: first in line for Vietnamese, else behind egui's font for the letters it
/// lacks (a part may be named in Vietnamese whatever the interface's language).
pub fn install_fonts(ctx: &eframe::egui::Context, lang: Language) {
    use eframe::egui::{FontData, FontDefinitions, FontFamily};
    let mut fonts = FontDefinitions::default();
    let Some(bytes) = system_font() else {
        log::warn!("no system font with Vietnamese letters was found");
        return ctx.set_fonts(fonts);
    };
    fonts
        .font_data
        .insert("system".into(), std::sync::Arc::new(FontData::from_owned(bytes)));
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let list = fonts.families.entry(family.clone()).or_default();
        if lang == Language::Vietnamese && family == FontFamily::Proportional {
            list.insert(0, "system".into());
        } else {
            list.push("system".into());
        }
    }
    ctx.set_fonts(fonts);
}

pub fn set(lang: Language) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Language {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Language::Vietnamese,
        _ => Language::English,
    }
}

/// `en: …` / `vi: …` line pairs; `#` starts a comment. `\n` in a line is a line break.
fn parse(text: &'static str) -> HashMap<&'static str, &'static str> {
    // A few texts have line breaks: those are unescaped once and kept for the program's life.
    let unescape = |s: &'static str| -> &'static str {
        if s.contains("\\n") {
            Box::leak(s.replace("\\n", "\n").into_boxed_str())
        } else {
            s
        }
    };
    let mut out = HashMap::new();
    let mut en = None;
    for line in text.lines().map(str::trim_end) {
        if line.starts_with('#') {
            continue;
        }
        if let Some(e) = line.strip_prefix("en: ") {
            en = Some(e);
        } else if let Some((_, t)) = line.split_once(": ") {
            if let Some(e) = en.take() {
                out.insert(unescape(e), unescape(t));
            }
        }
    }
    out
}

/// `text` in the chosen language.
pub fn tr(text: &str) -> &str {
    current().table().and_then(|t| t.get(text).copied()).unwrap_or(text)
}

/// `tr` for text with values: `{}` in the English template and its translation take `args` in
/// order (a translation may not reorder them).
pub fn trf(template: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::new();
    let mut args = args.iter();
    let mut parts = tr(template).split("{}");
    if let Some(first) = parts.next() {
        out += first;
    }
    for part in parts {
        if let Some(a) = args.next() {
            out += &a.to_string();
        }
        out += part;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translations_load_and_fill_their_values() {
        let table = parse("# comment\nen: Open\nvi: Mở\n\nen: Opened {}\nvi: Đã mở {}\nen: Untranslated\nvi:\n");
        assert_eq!(table.get("Open"), Some(&"Mở"));
        assert_eq!(table.get("Untranslated"), None);
        // The real table is well formed: every value keeps its {} slots.
        let vi = Language::Vietnamese.table().unwrap();
        assert!(vi.len() > 300, "{} entries", vi.len());
        for (en, t) in vi {
            assert_eq!(en.matches("{}").count(), t.matches("{}").count(), "{en} -> {t}");
        }
        set(Language::Vietnamese);
        assert_eq!(tr("Open…"), *vi.get("Open…").unwrap());
        assert_eq!(tr("no such text"), "no such text");
        set(Language::English);
        assert_eq!(trf("Opened {}", &[&"part.step"]), "Opened part.step");
        assert_eq!(Language::from_id("vi"), Some(Language::Vietnamese));
    }

    /// Windows and macOS always have the font; Linux depends on the distribution.
    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn a_system_font_with_vietnamese_letters_is_found() {
        assert!(system_font().is_some_and(|b| b.len() > 10_000));
    }
}
