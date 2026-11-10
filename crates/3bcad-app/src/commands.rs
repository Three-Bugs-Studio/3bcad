//! Commands with a keyboard shortcut the user can change (Settings > Shortcuts). Esc and Delete
//! keep their meaning wherever they are pressed, so they are not listed.

use std::collections::HashMap;

use eframe::egui::{Key, KeyboardShortcut, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    New,
    Open,
    Save,
    SaveAs,
    Undo,
    Redo,
    Rebuild,
    Rename,
    ZoomToFit,
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
    Isometric,
    NormalTo,
    Perspective,
    DisplayStyle,
    SectionView,
    ShortcutBar,
    Settings,
    ShortcutsWindow,
}

const fn ctrl(key: Key) -> Option<KeyboardShortcut> {
    Some(KeyboardShortcut::new(Modifiers::COMMAND, key))
}

const fn plain(key: Key) -> Option<KeyboardShortcut> {
    Some(KeyboardShortcut::new(Modifiers::NONE, key))
}

impl Command {
    pub const ALL: [Command; 23] = [
        Command::New,
        Command::Open,
        Command::Save,
        Command::SaveAs,
        Command::Undo,
        Command::Redo,
        Command::Rebuild,
        Command::Rename,
        Command::ZoomToFit,
        Command::Front,
        Command::Back,
        Command::Left,
        Command::Right,
        Command::Top,
        Command::Bottom,
        Command::Isometric,
        Command::NormalTo,
        Command::Perspective,
        Command::DisplayStyle,
        Command::SectionView,
        Command::ShortcutBar,
        Command::Settings,
        Command::ShortcutsWindow,
    ];

    /// Settings-file name.
    pub fn id(self) -> &'static str {
        match self {
            Command::New => "new",
            Command::Open => "open",
            Command::Save => "save",
            Command::SaveAs => "save_as",
            Command::Undo => "undo",
            Command::Redo => "redo",
            Command::Rebuild => "rebuild",
            Command::Rename => "rename",
            Command::ZoomToFit => "zoom_to_fit",
            Command::Front => "view_front",
            Command::Back => "view_back",
            Command::Left => "view_left",
            Command::Right => "view_right",
            Command::Top => "view_top",
            Command::Bottom => "view_bottom",
            Command::Isometric => "view_isometric",
            Command::NormalTo => "normal_to",
            Command::Perspective => "perspective",
            Command::DisplayStyle => "display_style",
            Command::SectionView => "section_view",
            Command::ShortcutBar => "shortcut_bar",
            Command::Settings => "settings",
            Command::ShortcutsWindow => "shortcuts_window",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Command::New => "New part",
            Command::Open => "Open",
            Command::Save => "Save",
            Command::SaveAs => "Save As",
            Command::Undo => "Undo",
            Command::Redo => "Redo",
            Command::Rebuild => "Rebuild",
            Command::Rename => "Rename the selected feature",
            Command::ZoomToFit => "Zoom to fit",
            Command::Front => "Front view",
            Command::Back => "Back view",
            Command::Left => "Left view",
            Command::Right => "Right view",
            Command::Top => "Top view",
            Command::Bottom => "Bottom view",
            Command::Isometric => "Isometric view",
            Command::NormalTo => "Normal to the selected face or plane",
            Command::Perspective => "Perspective / orthographic",
            Command::DisplayStyle => "Next display style",
            Command::SectionView => "Section view",
            Command::ShortcutBar => "Shortcut bar at the mouse",
            Command::Settings => "Settings",
            Command::ShortcutsWindow => "Keyboard shortcuts window",
        }
    }

    /// The SolidWorks-like defaults (Ctrl+1..8 for the views).
    pub fn default_key(self) -> Option<KeyboardShortcut> {
        match self {
            Command::New => ctrl(Key::N),
            Command::Open => ctrl(Key::O),
            Command::Save => ctrl(Key::S),
            Command::SaveAs => Some(KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S)),
            Command::Undo => ctrl(Key::Z),
            Command::Redo => ctrl(Key::Y),
            Command::Rebuild => ctrl(Key::B),
            Command::Rename => plain(Key::F2),
            Command::ZoomToFit => plain(Key::F),
            Command::Front => ctrl(Key::Num1),
            Command::Back => ctrl(Key::Num2),
            Command::Left => ctrl(Key::Num3),
            Command::Right => ctrl(Key::Num4),
            Command::Top => ctrl(Key::Num5),
            Command::Bottom => ctrl(Key::Num6),
            Command::Isometric => ctrl(Key::Num7),
            Command::NormalTo => ctrl(Key::Num8),
            Command::Perspective => plain(Key::P),
            Command::DisplayStyle => plain(Key::D),
            Command::SectionView => None,
            Command::ShortcutBar => plain(Key::S),
            Command::Settings => ctrl(Key::Comma),
            Command::ShortcutsWindow => plain(Key::F1),
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.id() == id)
    }
}

/// Shortcuts that differ from the defaults (`None`: the user cleared it).
pub type Overrides = HashMap<Command, Option<KeyboardShortcut>>;

pub fn key_of(overrides: &Overrides, cmd: Command) -> Option<KeyboardShortcut> {
    overrides.get(&cmd).copied().unwrap_or_else(|| cmd.default_key())
}

/// The command other than `cmd` that already has `key`.
pub fn conflict(overrides: &Overrides, cmd: Command, key: KeyboardShortcut) -> Option<Command> {
    Command::ALL
        .into_iter()
        .find(|&c| c != cmd && key_of(overrides, c) == Some(key))
}

/// "Ctrl+Shift+S", as the settings file and the menus show it. Ctrl is Cmd on a Mac.
pub fn text(key: &KeyboardShortcut) -> String {
    let m = key.modifiers;
    let mut out = String::new();
    for (on, name) in [(m.command || m.ctrl, "Ctrl"), (m.alt, "Alt"), (m.shift, "Shift")] {
        if on {
            out += name;
            out.push('+');
        }
    }
    out + key.logical_key.name()
}

pub fn parse(text: &str) -> Option<KeyboardShortcut> {
    let mut modifiers = Modifiers::NONE;
    let mut parts: Vec<&str> = text.trim().split('+').map(str::trim).collect();
    // "Ctrl++" is the plus key.
    if text.trim().ends_with("++") {
        parts.pop();
        parts.pop();
        parts.push("+");
    }
    let key = Key::from_name(parts.pop()?)?;
    for p in parts {
        match p.to_ascii_lowercase().as_str() {
            "ctrl" | "cmd" | "command" => modifiers = modifiers.plus(Modifiers::COMMAND),
            "alt" | "option" => modifiers = modifiers.plus(Modifiers::ALT),
            "shift" => modifiers = modifiers.plus(Modifiers::SHIFT),
            _ => return None,
        }
    }
    Some(KeyboardShortcut::new(modifiers, key))
}

/// The commands' shortcuts in the order to check them: more modifiers first, so Ctrl+Shift+S
/// is not taken for Ctrl+S.
pub fn in_order(overrides: &Overrides) -> Vec<(Command, KeyboardShortcut)> {
    let mut keys: Vec<_> = Command::ALL
        .into_iter()
        .filter_map(|c| Some((c, key_of(overrides, c)?)))
        .collect();
    let weight = |k: &KeyboardShortcut| {
        let m = k.modifiers;
        u8::from(m.command || m.ctrl) + u8::from(m.alt) + u8::from(m.shift)
    };
    keys.sort_by_key(|(_, k)| std::cmp::Reverse(weight(k)));
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_do_not_clash_and_shortcuts_round_trip_as_text() {
        let none = Overrides::new();
        for c in Command::ALL {
            assert_eq!(Command::from_id(c.id()), Some(c));
            if let Some(k) = c.default_key() {
                assert_eq!(conflict(&none, c, k), None, "{c:?}");
                assert_eq!(parse(&text(&k)), Some(k), "{}", text(&k));
            }
        }
        assert_eq!(text(&Command::SaveAs.default_key().unwrap()), "Ctrl+Shift+S");
        assert_eq!(
            parse("ctrl + alt + F5").map(|k| text(&k)).as_deref(),
            Some("Ctrl+Alt+F5")
        );
        assert_eq!(parse("Hyper+S"), None);
    }

    #[test]
    fn a_taken_shortcut_is_reported_and_more_modifiers_are_checked_first() {
        let mut o = Overrides::new();
        let f = KeyboardShortcut::new(Modifiers::NONE, Key::F);
        // F is Zoom to fit's: giving it to the display style clashes.
        assert_eq!(conflict(&o, Command::DisplayStyle, f), Some(Command::ZoomToFit));
        // Clear Zoom to fit's and it is free.
        o.insert(Command::ZoomToFit, None);
        assert_eq!(conflict(&o, Command::DisplayStyle, f), None);
        assert_eq!(key_of(&o, Command::ZoomToFit), None);
        let order = in_order(&Overrides::new());
        let at = |c| order.iter().position(|(x, _)| *x == c).unwrap();
        assert!(at(Command::SaveAs) < at(Command::Save));
    }
}
